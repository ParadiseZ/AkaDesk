use crate::diagnostics::ShellDiagnosticState;
use crate::host::HostSupervisor;
use host_ipc::types::{
    CapabilitySnapshot, DiagnosticEntry, HostIdentitySnapshot, HostOverviewSnapshot,
    HostSettingsSnapshot, PendingRemoteRequestSnapshot, RuntimeStatusSnapshot, VideoStreamSnapshot,
};
use serde::{Deserialize, Serialize};
use std::fs;
use std::io;
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::Mutex;
use std::time::{SystemTime, UNIX_EPOCH};

#[derive(Debug)]
pub struct AppState {
    shutting_down: AtomicBool,
    shell_store: Mutex<ShellStore>,
    supervisor: Mutex<HostSupervisor>,
    shell_messages: Mutex<Vec<ShellMessage>>,
    diagnostics: Mutex<ShellDiagnosticState>,
}

impl AppState {
    pub fn load(app_data_dir: PathBuf, resource_dir: PathBuf) -> io::Result<Self> {
        fs::create_dir_all(&app_data_dir)?;
        // Keep existing preferences when correcting the previously invalid bundle identifier.
        let settings_path = app_data_dir.join("desktop-shell.json");
        if !settings_path.exists() {
            if let Some(parent) = app_data_dir.parent() {
                let legacy_path = parent
                    .join("com.akadesk.desktop_shell")
                    .join("desktop-shell.json");
                if legacy_path.is_file() {
                    fs::copy(legacy_path, &settings_path)?;
                }
            }
        }
        let shell_store = ShellStore::load(app_data_dir.join("desktop-shell.json"))?;
        let state = Self {
            shutting_down: AtomicBool::new(false),
            shell_store: Mutex::new(shell_store),
            supervisor: Mutex::new(HostSupervisor::with_resources(resource_dir)),
            shell_messages: Mutex::new(Vec::new()),
            diagnostics: Mutex::new(ShellDiagnosticState::new(app_data_dir)?),
        };
        let _ = state.push_shell_message(ShellMessage::info(
            "DESKTOP_SHELL_READY",
            "Desktop shell command bridge initialized.",
        ));
        Ok(state)
    }

    pub fn shutdown_host(&self) {
        self.shutting_down.store(true, Ordering::Relaxed);
        if let Ok(mut supervisor) = self.supervisor.lock() {
            supervisor.stop();
        }
    }

    pub fn ensure_host_started(&self) -> Result<(), String> {
        self.supervisor
            .lock()
            .map_err(lock_err)?
            .ensure_started()
            .map_err(|err| err.to_string())
    }

    pub fn bootstrap_snapshot(&self) -> Result<ShellSnapshot, String> {
        if self.shutting_down.load(Ordering::Relaxed) {
            return Err("应用正在退出".to_string());
        }
        let host_bundle = self
            .supervisor
            .lock()
            .map_err(lock_err)?
            .fetch_snapshot()
            .map_err(|err| err.to_string())?;
        let shell_settings = self.shell_store.lock().map_err(lock_err)?.settings.clone();
        let mut messages = host_bundle
            .diagnostics
            .entries
            .into_iter()
            .map(ShellMessage::from_host)
            .collect::<Vec<_>>();
        messages.extend(
            self.shell_messages
                .lock()
                .map_err(lock_err)?
                .clone()
                .into_iter(),
        );
        messages.sort_by_key(|entry| entry.created_at_ms);
        if messages.len() > 40 {
            messages = messages.split_off(messages.len() - 40);
        }

        let runtime_status = host_bundle.runtime_status.status.unwrap_or_default();
        let capability = host_bundle.capability_snapshot.snapshot.unwrap_or_default();
        let host_overview = host_bundle.host_overview.overview.unwrap_or_default();
        let incoming_request = host_overview
            .pending_remote_request
            .clone()
            .map(IncomingRequestPreview::from);
        let active_session = runtime_status
            .active_session_id
            .as_ref()
            .filter(|_| {
                matches!(
                    runtime_status.state_label.as_str(),
                    "SESSION_PREPARING" | "STREAMING" | "STOPPING"
                )
            })
            .map(|session_id| ActiveSessionPresentation {
                session_id: session_id.clone(),
                role: "controlled_host".to_string(),
                peer_name: host_overview
                    .active_controller_display_name
                    .clone()
                    .unwrap_or_else(|| "远程设备".to_string()),
                peer_subtitle: if runtime_status.state_label == "STREAMING" {
                    "正在共享屏幕"
                } else {
                    "正在建立屏幕共享"
                }
                .to_string(),
                started_at_ms: runtime_status.active_started_at_ms,
            });
        let desktop_ui_state = if active_session.is_some() {
            "SESSION_ACTIVE_VIEW".to_string()
        } else {
            map_desktop_ui_state(&runtime_status.state_label)
        };
        let supports_input = capability
            .input
            .as_ref()
            .map(|input| input.supports_pointer && input.supports_keyboard)
            .unwrap_or(false);

        Ok(ShellSnapshot {
            ready: true,
            runtime_status_label: runtime_status.state_label.clone(),
            desktop_ui_state,
            supports_input,
            runtime_status: RuntimeStatusView::from(runtime_status),
            capability: CapabilityView::from(capability),
            host_overview: HostOverviewView::from(host_overview),
            shell_settings,
            messages,
            incoming_request,
            active_session,
            updated_at_ms: now_ms(),
        })
    }

    pub fn save_shell_settings(&self, next: ShellSettings) -> Result<ShellSnapshot, String> {
        self.shell_store
            .lock()
            .map_err(lock_err)?
            .update(next)
            .map_err(|err| err.to_string())?;
        self.push_shell_message(ShellMessage::info(
            "SHELL_SETTINGS_SAVED",
            "Desktop shell settings saved.",
        ))?;
        self.bootstrap_snapshot()
    }

    pub fn update_host_settings(&self, next: HostSettingsInput) -> Result<ShellSnapshot, String> {
        let snapshot = HostSettingsSnapshot {
            allow_remote_assistance: next.allow_remote_assistance,
            verification_mode: next.verification_mode,
            connection_method: next.connection_method.as_wire_value().to_string(),
            notify_on_incoming_request: next.notify_on_incoming_request,
            auto_accept_trusted_controller: next.auto_accept_trusted_controller,
            updated_at_ms: now_ms(),
            temporary_verification_code: next.temporary_verification_code,
            custom_verification_code: next.custom_verification_code,
            refresh_temporary_code_after_controlled_end: next
                .refresh_temporary_code_after_controlled_end,
        };
        let response = self
            .supervisor
            .lock()
            .map_err(lock_err)?
            .update_host_settings(snapshot)
            .map_err(|err| err.to_string())?;
        if let Some(error) = response.error {
            return Err(error.message);
        }
        self.bootstrap_snapshot()
    }

    pub fn refresh_temporary_verification_code(&self) -> Result<ShellSnapshot, String> {
        let response = self
            .supervisor
            .lock()
            .map_err(lock_err)?
            .refresh_temporary_verification_code()
            .map_err(|err| err.to_string())?;
        if let Some(error) = response.error {
            return Err(error.message);
        }
        self.push_shell_message(ShellMessage::info(
            "HOST_TEMP_CODE_REFRESHED",
            "Temporary verification code refreshed by host runtime.",
        ))?;
        self.bootstrap_snapshot()
    }

    pub fn queue_remote_connect(
        &self,
        _request: RemoteConnectRequest,
    ) -> Result<ShellActionReceipt, String> {
        Err("当前版本仅支持由 Android 连接此电脑，桌面主控尚未开放".to_string())
    }

    pub fn preview_incoming_request(&self) -> Result<ShellSnapshot, String> {
        Err("请从 Android 发起真实远控申请".to_string())
    }

    pub fn respond_incoming_request(
        &self,
        request_id: &str,
        allow: bool,
    ) -> Result<ShellSnapshot, String> {
        let response = self
            .supervisor
            .lock()
            .map_err(lock_err)?
            .respond_pending_remote_request(request_id.to_owned(), allow)
            .map_err(|err| err.to_string())?;
        if let Some(error) = response.error {
            return Err(error.message);
        }
        self.bootstrap_snapshot()
    }

    pub fn disconnect_session(&self) -> Result<ShellSnapshot, String> {
        let mut supervisor = self.supervisor.lock().map_err(lock_err)?;
        let status = supervisor
            .fetch_snapshot()
            .map_err(|err| err.to_string())?
            .runtime_status
            .status;
        if let Some(session_id) = status.and_then(|status| status.active_session_id) {
            let response = supervisor
                .stop_session(session_id)
                .map_err(|err| err.to_string())?;
            if let Some(error) = response.error {
                return Err(error.message);
            }
        }
        drop(supervisor);
        self.bootstrap_snapshot()
    }

    fn push_shell_message(&self, message: ShellMessage) -> Result<(), String> {
        let level = message.level.clone();
        let code = message.code.clone();
        let content = message.message.clone();
        let mut buffer = self.shell_messages.lock().map_err(lock_err)?;
        buffer.push(message);
        if buffer.len() > 20 {
            buffer.remove(0);
        }
        self.append_log(&level, &code, &content);
        Ok(())
    }

    pub fn log_command_error(&self, code: &str, message: &str) {
        let _ = self.push_shell_message(ShellMessage::error(code, message));
    }

    fn append_log(&self, level: &str, code: &str, message: &str) {
        if let Ok(mut diagnostics) = self.diagnostics.lock() {
            let _ = diagnostics.append(level, code, message);
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ShellSettings {
    pub launch_on_startup: bool,
    pub minimize_to_tray: bool,
    pub notifications_enabled: bool,
    pub last_route: String,
    pub last_remote_device_id: String,
    pub last_access_code: String,
}

impl Default for ShellSettings {
    fn default() -> Self {
        Self {
            launch_on_startup: false,
            minimize_to_tray: false,
            notifications_enabled: false,
            last_route: "/remote".to_string(),
            last_remote_device_id: String::new(),
            last_access_code: String::new(),
        }
    }
}

#[derive(Debug)]
struct ShellStore {
    path: PathBuf,
    settings: ShellSettings,
}

impl ShellStore {
    fn load(path: PathBuf) -> io::Result<Self> {
        let settings = if path.exists() {
            let text = fs::read_to_string(&path)?;
            serde_json::from_str(&text)
                .map_err(|err| io::Error::new(io::ErrorKind::InvalidData, err.to_string()))?
        } else {
            let settings = ShellSettings::default();
            write_json(&path, &settings)?;
            settings
        };
        let mut settings = settings;
        settings.launch_on_startup = false;
        settings.minimize_to_tray = false;
        settings.notifications_enabled = false;
        settings.last_access_code.clear();
        if !matches!(settings.last_route.as_str(), "/remote" | "/settings") {
            settings.last_route = "/remote".to_string();
        }
        Ok(Self { path, settings })
    }

    fn update(&mut self, next: ShellSettings) -> io::Result<()> {
        if next.launch_on_startup || next.minimize_to_tray || next.notifications_enabled {
            return Err(io::Error::new(
                io::ErrorKind::Unsupported,
                "开机自启、托盘和系统通知尚未实现",
            ));
        }
        if !matches!(next.last_route.as_str(), "/remote" | "/settings") {
            return Err(io::Error::new(io::ErrorKind::InvalidInput, "无效的启动页"));
        }
        let mut next = next;
        next.last_access_code.clear();
        write_json(&self.path, &next)?;
        self.settings = next;
        Ok(())
    }
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ShellSnapshot {
    pub ready: bool,
    pub runtime_status_label: String,
    pub desktop_ui_state: String,
    pub supports_input: bool,
    pub runtime_status: RuntimeStatusView,
    pub capability: CapabilityView,
    pub host_overview: HostOverviewView,
    pub shell_settings: ShellSettings,
    pub messages: Vec<ShellMessage>,
    pub incoming_request: Option<IncomingRequestPreview>,
    pub active_session: Option<ActiveSessionPresentation>,
    pub updated_at_ms: i64,
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct RuntimeStatusView {
    pub state_label: String,
    pub active_session_id: Option<String>,
    pub updated_at_ms: i64,
}

impl From<RuntimeStatusSnapshot> for RuntimeStatusView {
    fn from(value: RuntimeStatusSnapshot) -> Self {
        Self {
            state_label: value.state_label,
            active_session_id: value.active_session_id,
            updated_at_ms: value.updated_at_ms,
        }
    }
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct CapabilityView {
    pub supports_audio: bool,
    pub supports_input: bool,
    pub supports_display_control: bool,
    pub supports_pointer: bool,
    pub supports_scroll: bool,
    pub supports_keyboard: bool,
    pub updated_at_ms: i64,
}

impl From<CapabilitySnapshot> for CapabilityView {
    fn from(value: CapabilitySnapshot) -> Self {
        let input = value.input.unwrap_or_default();
        Self {
            supports_audio: value.supports_audio,
            supports_input: value.supports_input,
            supports_display_control: value.supports_display_control,
            supports_pointer: input.supports_pointer,
            supports_scroll: input.supports_scroll,
            supports_keyboard: input.supports_keyboard,
            updated_at_ms: value.updated_at_ms,
        }
    }
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct HostOverviewView {
    pub identity: HostIdentityView,
    pub settings: HostSettingsView,
    pub paired_controller_count: u32,
    pub control_port: u32,
    pub active_controller_display_name: Option<String>,
    pub video_stream: VideoStreamView,
    pub updated_at_ms: i64,
}

impl From<HostOverviewSnapshot> for HostOverviewView {
    fn from(value: HostOverviewSnapshot) -> Self {
        Self {
            identity: HostIdentityView::from(value.identity.unwrap_or_default()),
            settings: HostSettingsView::from(value.settings.unwrap_or_default()),
            paired_controller_count: value.paired_controller_count,
            control_port: value.control_port,
            active_controller_display_name: value.active_controller_display_name,
            video_stream: VideoStreamView::from(value.video_stream.unwrap_or_default()),
            updated_at_ms: value.updated_at_ms,
        }
    }
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct VideoStreamView {
    pub session_id: String,
    pub packet_count: u64,
    pub keyframe_count: u64,
    pub latest_packet_len: u64,
    pub latest_frame_source_label: String,
    pub latest_error: String,
    pub updated_at_ms: i64,
}

impl From<VideoStreamSnapshot> for VideoStreamView {
    fn from(value: VideoStreamSnapshot) -> Self {
        Self {
            session_id: value.session_id,
            packet_count: value.packet_count,
            keyframe_count: value.keyframe_count,
            latest_packet_len: value.latest_packet_len,
            latest_frame_source_label: value.latest_frame_source_label,
            latest_error: value.latest_error,
            updated_at_ms: value.updated_at_ms,
        }
    }
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct HostIdentityView {
    pub device_id: String,
    pub display_name: String,
    pub hostname: String,
    pub app_version: String,
    pub platform: String,
}

impl From<HostIdentitySnapshot> for HostIdentityView {
    fn from(value: HostIdentitySnapshot) -> Self {
        Self {
            device_id: value.device_id,
            display_name: value.display_name,
            hostname: value.hostname,
            app_version: value.app_version,
            platform: value.platform,
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub enum ConnectionMethodView {
    #[serde(rename = "ACCESS_CODE")]
    AccessCode,
    #[serde(rename = "TRUST_ONLY")]
    TrustOnly,
    #[serde(rename = "ACCESS_CODE_AND_CONSENT")]
    AccessCodeAndConsent,
}

impl ConnectionMethodView {
    fn as_wire_value(&self) -> &'static str {
        match self {
            Self::AccessCode => "ACCESS_CODE",
            Self::TrustOnly => "TRUST_ONLY",
            Self::AccessCodeAndConsent => "ACCESS_CODE_AND_CONSENT",
        }
    }

    fn from_wire_value(value: impl AsRef<str>) -> Self {
        match value.as_ref() {
            "TRUST_ONLY" => Self::TrustOnly,
            "ACCESS_CODE_AND_CONSENT" => Self::AccessCodeAndConsent,
            _ => Self::AccessCode,
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct HostSettingsInput {
    pub allow_remote_assistance: bool,
    pub verification_mode: String,
    pub connection_method: ConnectionMethodView,
    pub notify_on_incoming_request: bool,
    pub auto_accept_trusted_controller: bool,
    pub temporary_verification_code: String,
    pub custom_verification_code: String,
    pub refresh_temporary_code_after_controlled_end: bool,
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct HostSettingsView {
    pub allow_remote_assistance: bool,
    pub verification_mode: String,
    pub connection_method: ConnectionMethodView,
    pub notify_on_incoming_request: bool,
    pub auto_accept_trusted_controller: bool,
    pub temporary_verification_code: String,
    pub custom_verification_code: String,
    pub refresh_temporary_code_after_controlled_end: bool,
    pub updated_at_ms: i64,
}

impl From<HostSettingsSnapshot> for HostSettingsView {
    fn from(value: HostSettingsSnapshot) -> Self {
        Self {
            allow_remote_assistance: value.allow_remote_assistance,
            verification_mode: value.verification_mode,
            connection_method: ConnectionMethodView::from_wire_value(value.connection_method),
            notify_on_incoming_request: value.notify_on_incoming_request,
            auto_accept_trusted_controller: value.auto_accept_trusted_controller,
            temporary_verification_code: value.temporary_verification_code,
            custom_verification_code: value.custom_verification_code,
            refresh_temporary_code_after_controlled_end: value
                .refresh_temporary_code_after_controlled_end,
            updated_at_ms: value.updated_at_ms,
        }
    }
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ShellMessage {
    pub source: String,
    pub level: String,
    pub code: String,
    pub message: String,
    pub created_at_ms: i64,
}

impl ShellMessage {
    fn info(code: &str, message: impl Into<String>) -> Self {
        Self {
            source: "shell".to_string(),
            level: "info".to_string(),
            code: code.to_string(),
            message: message.into(),
            created_at_ms: now_ms(),
        }
    }

    fn warning(code: &str, message: impl Into<String>) -> Self {
        Self {
            source: "shell".to_string(),
            level: "warning".to_string(),
            code: code.to_string(),
            message: message.into(),
            created_at_ms: now_ms(),
        }
    }

    fn error(code: &str, message: impl Into<String>) -> Self {
        Self {
            source: "shell".to_string(),
            level: "error".to_string(),
            code: code.to_string(),
            message: message.into(),
            created_at_ms: now_ms(),
        }
    }

    fn from_host(entry: DiagnosticEntry) -> Self {
        Self {
            source: "host-runtime".to_string(),
            level: entry.level,
            code: entry.code,
            message: entry.message,
            created_at_ms: entry.created_at_ms,
        }
    }
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct IncomingRequestPreview {
    pub request_id: String,
    pub controller_name: String,
    pub device_label: String,
    pub expires_at_ms: i64,
}

impl From<PendingRemoteRequestSnapshot> for IncomingRequestPreview {
    fn from(value: PendingRemoteRequestSnapshot) -> Self {
        Self {
            request_id: value.request_id,
            controller_name: value.controller_display_name,
            device_label: value.controller_device_id,
            expires_at_ms: value.expires_at_ms,
        }
    }
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ActiveSessionPresentation {
    pub session_id: String,
    pub role: String,
    pub peer_name: String,
    pub peer_subtitle: String,
    pub started_at_ms: i64,
}

#[derive(Debug, Clone, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct RemoteConnectRequest {
    pub remote_device_id: String,
    pub access_code: String,
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ShellActionReceipt {
    pub accepted: bool,
    pub message: String,
    pub created_at_ms: i64,
}

fn write_json(path: &Path, value: &ShellSettings) -> io::Result<()> {
    let text = serde_json::to_string_pretty(value)
        .map_err(|err| io::Error::new(io::ErrorKind::InvalidData, err.to_string()))?;
    host_ipc::storage::atomic_write(path, text.as_bytes())
}

fn map_desktop_ui_state(runtime_state_label: &str) -> String {
    let normalized = runtime_state_label.to_ascii_uppercase();
    if normalized.contains("STARTING") {
        "BOOTING".to_string()
    } else if normalized.contains("PREPARING") {
        "CONNECTING_VIEW".to_string()
    } else if normalized.contains("STREAMING") {
        "SESSION_ACTIVE_VIEW".to_string()
    } else if normalized.contains("ERROR") {
        "ERROR_VIEW".to_string()
    } else if normalized.contains("STOPPING") {
        "STOPPING".to_string()
    } else {
        "READY".to_string()
    }
}

fn now_ms() -> i64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|duration| duration.as_millis() as i64)
        .unwrap_or_default()
}

fn lock_err<T>(_: std::sync::PoisonError<T>) -> String {
    "desktop shell state lock poisoned".to_string()
}

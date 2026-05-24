use crate::diagnostics::ShellDiagnosticState;
use crate::host::HostSupervisor;
use host_ipc::types::{
    CapabilitySnapshot, DiagnosticEntry, HostIdentitySnapshot, HostOverviewSnapshot,
    HostSettingsSnapshot, RuntimeStatusSnapshot,
};
use serde::{Deserialize, Serialize};
use std::fs;
use std::io;
use std::path::{Path, PathBuf};
use std::sync::Mutex;
use std::time::{SystemTime, UNIX_EPOCH};

#[derive(Debug)]
pub struct AppState {
    shell_store: Mutex<ShellStore>,
    supervisor: Mutex<HostSupervisor>,
    shell_messages: Mutex<Vec<ShellMessage>>,
    incoming_request: Mutex<Option<IncomingRequestPreview>>,
    active_session: Mutex<Option<ActiveSessionPresentation>>,
    diagnostics: Mutex<ShellDiagnosticState>,
}

impl AppState {
    pub fn load(app_data_dir: PathBuf) -> io::Result<Self> {
        fs::create_dir_all(&app_data_dir)?;
        let shell_store = ShellStore::load(app_data_dir.join("desktop-shell.json"))?;
        let state = Self {
            shell_store: Mutex::new(shell_store),
            supervisor: Mutex::new(HostSupervisor::default()),
            shell_messages: Mutex::new(Vec::new()),
            incoming_request: Mutex::new(None),
            active_session: Mutex::new(None),
            diagnostics: Mutex::new(ShellDiagnosticState::new(app_data_dir)?),
        };
        let _ = state.push_shell_message(ShellMessage::info(
            "DESKTOP_SHELL_READY",
            "Desktop shell command bridge initialized.",
        ));
        Ok(state)
    }

    pub fn ensure_host_started(&self) -> Result<(), String> {
        self.supervisor
            .lock()
            .map_err(lock_err)?
            .ensure_started()
            .map_err(|err| err.to_string())
    }

    pub fn bootstrap_snapshot(&self) -> Result<ShellSnapshot, String> {
        let host_bundle = self
            .supervisor
            .lock()
            .map_err(lock_err)?
            .fetch_snapshot()
            .map_err(|err| err.to_string())?;
        let shell_settings = self.shell_store.lock().map_err(lock_err)?.settings.clone();
        let incoming_request = self
            .incoming_request
            .lock()
            .map_err(lock_err)?
            .clone()
            .filter(|request| request.expires_at_ms > now_ms());
        let active_session = self.active_session.lock().map_err(lock_err)?.clone();
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
            connection_method: next.connection_method,
            notify_on_incoming_request: next.notify_on_incoming_request,
            auto_accept_trusted_controller: next.auto_accept_trusted_controller,
            updated_at_ms: now_ms(),
            temporary_verification_code: next.temporary_verification_code,
            custom_verification_code: next.custom_verification_code,
            refresh_temporary_code_after_controlled_end: next
                .refresh_temporary_code_after_controlled_end,
        };
        self.supervisor
            .lock()
            .map_err(lock_err)?
            .update_host_settings(snapshot)
            .map_err(|err| err.to_string())?;
        self.bootstrap_snapshot()
    }

    pub fn refresh_temporary_verification_code(&self) -> Result<ShellSnapshot, String> {
        self.supervisor
            .lock()
            .map_err(lock_err)?
            .refresh_temporary_verification_code()
            .map_err(|err| err.to_string())?;
        self.push_shell_message(ShellMessage::info(
            "HOST_TEMP_CODE_REFRESHED",
            "Temporary verification code refreshed by host runtime.",
        ))?;
        self.bootstrap_snapshot()
    }

    pub fn queue_remote_connect(
        &self,
        request: RemoteConnectRequest,
    ) -> Result<ShellActionReceipt, String> {
        {
            let mut store = self.shell_store.lock().map_err(lock_err)?;
            store.settings.last_remote_device_id = request.remote_device_id.clone();
            store.settings.last_access_code = request.access_code.clone();
            store.persist().map_err(|err| err.to_string())?;
        }
        self.push_shell_message(ShellMessage::info(
            "REMOTE_CONNECT_QUEUED",
            format!(
                "Queued remote assist request for device {}. Control-plane session wiring stays on the M3/M4 path.",
                request.remote_device_id
            ),
        ))?;
        *self.active_session.lock().map_err(lock_err)? = Some(ActiveSessionPresentation {
            role: "controller".to_string(),
            peer_name: if request.remote_device_id.trim().is_empty() {
                "Remote Device".to_string()
            } else {
                request.remote_device_id.clone()
            },
            peer_subtitle: if request.access_code.trim().is_empty() {
                "正在建立远程控制连接".to_string()
            } else {
                format!("验证码 {}", request.access_code)
            },
            started_at_ms: now_ms(),
        });
        Ok(ShellActionReceipt {
            accepted: true,
            message: "连接请求已记录，主窗口已切到主控视图。".to_string(),
            created_at_ms: now_ms(),
        })
    }

    pub fn preview_incoming_request(&self) -> Result<ShellSnapshot, String> {
        let request = IncomingRequestPreview {
            controller_name: "Android Controller".to_string(),
            device_label: "Pixel 04".to_string(),
            expires_at_ms: now_ms() + 39_000,
        };
        *self.incoming_request.lock().map_err(lock_err)? = Some(request);
        self.push_shell_message(ShellMessage::warning(
            "REMOTE_ASSIST_REQUEST_PREVIEW",
            "Incoming remote assist request preview emitted to the desktop shell.",
        ))?;
        self.bootstrap_snapshot()
    }

    pub fn respond_incoming_request(&self, allow: bool) -> Result<ShellSnapshot, String> {
        *self.incoming_request.lock().map_err(lock_err)? = None;
        if allow {
            *self.active_session.lock().map_err(lock_err)? = Some(ActiveSessionPresentation {
                role: "controlled_host".to_string(),
                peer_name: "Android Controller".to_string(),
                peer_subtitle: "1 台设备正在控制此电脑".to_string(),
                started_at_ms: now_ms(),
            });
            self.push_shell_message(ShellMessage::info(
                "REMOTE_ASSIST_REQUEST_ALLOWED",
                "Incoming remote assist request preview marked as allowed.",
            ))?;
        } else {
            self.push_shell_message(ShellMessage::warning(
                "REMOTE_ASSIST_REQUEST_REJECTED",
                "Incoming remote assist request preview marked as rejected.",
            ))?;
        }
        self.bootstrap_snapshot()
    }

    pub fn disconnect_session(&self) -> Result<ShellSnapshot, String> {
        let active_role = self
            .active_session
            .lock()
            .map_err(lock_err)?
            .as_ref()
            .map(|session| session.role.clone());
        *self.active_session.lock().map_err(lock_err)? = None;
        if active_role.as_deref() == Some("controlled_host") {
            let refresh_after_end = self
                .supervisor
                .lock()
                .map_err(lock_err)?
                .fetch_snapshot()
                .ok()
                .and_then(|bundle| bundle.host_overview.overview)
                .and_then(|overview| overview.settings)
                .map(|settings| settings.refresh_temporary_code_after_controlled_end)
                .unwrap_or(false);
            if refresh_after_end {
                match self
                    .supervisor
                    .lock()
                    .map_err(lock_err)?
                    .refresh_temporary_verification_code()
                {
                    Ok(_) => {
                        let _ = self.push_shell_message(ShellMessage::info(
                            "HOST_TEMP_CODE_REFRESHED_AFTER_SESSION",
                            "Temporary verification code refreshed after controlled session ended.",
                        ));
                    }
                    Err(err) => {
                        let _ = self.push_shell_message(ShellMessage::warning(
                            "HOST_TEMP_CODE_REFRESH_FAILED",
                            format!(
                                "Controlled session ended, but temporary verification code refresh failed: {}",
                                err
                            ),
                        ));
                    }
                }
            }
        }
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
            minimize_to_tray: true,
            notifications_enabled: true,
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
        Ok(Self { path, settings })
    }

    fn update(&mut self, next: ShellSettings) -> io::Result<()> {
        self.settings = next;
        self.persist()
    }

    fn persist(&self) -> io::Result<()> {
        write_json(&self.path, &self.settings)
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
pub struct HostSettingsInput {
    pub allow_remote_assistance: bool,
    pub verification_mode: String,
    pub connection_method: String,
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
    pub connection_method: String,
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
            connection_method: value.connection_method,
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
    pub controller_name: String,
    pub device_label: String,
    pub expires_at_ms: i64,
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ActiveSessionPresentation {
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
    fs::write(path, text)
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

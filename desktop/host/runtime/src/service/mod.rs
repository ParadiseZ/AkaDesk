use crate::control::{
    default_capability_flags, default_host_identity, ControlPlaneState, DEFAULT_CONTROL_PORT,
};
use crate::diagnostics::{DiagnosticBuffer, SharedDiagnosticBuffer};
use crate::generated::v1::DevicePresence;
use crate::pairing::{load_runtime_base_dir, HostIdentity, PairingStore, SharedPairingStore};
use crate::session::{SessionRegistry, SharedSessionRegistry};
use crate::settings::HostSettingsStore;
use crate::state::RuntimeStatus;
use host_ipc::types::{
    CapabilitySnapshot, CapabilitySnapshotChanged, DiagnosticsChanged, DiagnosticsResponse,
    RefreshTemporaryVerificationCodeResponse,
    HostIdentitySnapshot, HostOverviewChanged, HostOverviewResponse, HostOverviewSnapshot,
    HostSettingsSnapshot, InputCapabilitySnapshot, RuntimeStatusChanged, RuntimeStatusResponse,
    RuntimeStatusSnapshot, UpdateHostSettingsResponse,
};
use std::io;
use std::sync::{Arc, Mutex};

#[derive(Debug)]
pub struct RuntimeService {
    host_identity: HostIdentity,
    status: RuntimeStatus,
    sessions: SharedSessionRegistry,
    control_state: ControlPlaneState,
    settings: Arc<Mutex<HostSettingsStore>>,
    diagnostics: SharedDiagnosticBuffer,
}

impl RuntimeService {
    pub fn new() -> io::Result<Self> {
        let base_dir = load_runtime_base_dir();
        let host_identity = HostIdentity::load_or_create(&base_dir)?;
        let settings = Arc::new(Mutex::new(HostSettingsStore::load(&base_dir)?));
        let diagnostics: SharedDiagnosticBuffer = Arc::new(Mutex::new(DiagnosticBuffer::default()));
        let pairing_store: SharedPairingStore =
            Arc::new(Mutex::new(PairingStore::load(&base_dir)?));
        let sessions: SharedSessionRegistry = Arc::new(Mutex::new(SessionRegistry::default()));
        let device_identity = default_host_identity(
            host_identity.device_id.clone(),
            host_identity.display_name.clone(),
            host_identity.hostname.clone(),
            host_identity.app_version.clone(),
        );
        let control_state = ControlPlaneState {
            host_identity: device_identity.clone(),
            advertised_presence: DevicePresence {
                identity: Some(device_identity),
                lan_address: String::new(),
                control_port: DEFAULT_CONTROL_PORT as u32,
                paired: false,
                reachable: true,
                last_seen_ms: crate::pairing::now_ms(),
            },
            capability_flags: default_capability_flags(),
            pairing_store,
            sessions: Arc::clone(&sessions),
        };
        if let Ok(mut buffer) = diagnostics.lock() {
            buffer.push(
                "info",
                "HOST_RUNTIME_READY",
                format!(
                    "Host runtime ready on control port {}.",
                    control_state.advertised_presence.control_port
                ),
            );
        }
        Ok(Self {
            host_identity,
            status: RuntimeStatus::default(),
            sessions,
            control_state,
            settings,
            diagnostics,
        })
    }

    pub fn status(&self) -> &RuntimeStatus {
        &self.status
    }

    pub fn sessions(&self) -> &SharedSessionRegistry {
        &self.sessions
    }

    pub fn control_state(&self) -> &ControlPlaneState {
        &self.control_state
    }

    pub fn runtime_status_response(&self) -> RuntimeStatusResponse {
        RuntimeStatusResponse {
            status: self.ipc_runtime_status_changed().status,
            error: None,
        }
    }

    pub fn capability_snapshot_response(&self) -> host_ipc::types::CapabilitySnapshotResponse {
        host_ipc::types::CapabilitySnapshotResponse {
            snapshot: self.ipc_capability_snapshot_changed().snapshot,
            error: None,
        }
    }

    pub fn ipc_runtime_status_changed(&self) -> RuntimeStatusChanged {
        let (active_session_id, updated_at_ms) = match self.sessions.lock() {
            Ok(sessions) => (
                sessions
                    .active_session_id()
                    .map(|session_id| session_id.0.clone()),
                sessions
                    .active
                    .as_ref()
                    .map(|session| session.updated_at_ms)
                    .unwrap_or(self.status.updated_at_ms),
            ),
            Err(_) => (
                self.status
                    .active_session_id
                    .as_ref()
                    .map(|session_id| session_id.0.clone()),
                self.status.updated_at_ms,
            ),
        };
        RuntimeStatusChanged {
            status: Some(RuntimeStatusSnapshot {
                state_label: self.status.state_label.to_string(),
                active_session_id,
                updated_at_ms,
            }),
        }
    }

    pub fn ipc_capability_snapshot_changed(&self) -> CapabilitySnapshotChanged {
        CapabilitySnapshotChanged {
            snapshot: Some(CapabilitySnapshot {
                supports_audio: self.control_state.capability_flags.supports_audio,
                supports_input: self.control_state.capability_flags.supports_input,
                supports_display_control: self
                    .control_state
                    .capability_flags
                    .supports_display_control,
                input: Some(InputCapabilitySnapshot {
                    supports_pointer: true,
                    supports_scroll: true,
                    supports_keyboard: true,
                }),
                updated_at_ms: self.status.updated_at_ms,
            }),
        }
    }

    pub fn host_overview_response(&self) -> HostOverviewResponse {
        HostOverviewResponse {
            overview: self.host_overview_snapshot(),
            error: None,
        }
    }

    pub fn ipc_host_overview_changed(&self) -> HostOverviewChanged {
        HostOverviewChanged {
            overview: self.host_overview_snapshot(),
        }
    }

    pub fn update_host_settings(
        &self,
        settings: HostSettingsSnapshot,
    ) -> io::Result<UpdateHostSettingsResponse> {
        let updated_settings = self.settings.lock().map_err(lock_err)?.update(settings)?;
        if let Ok(mut buffer) = self.diagnostics.lock() {
            buffer.push(
                "info",
                "HOST_SETTINGS_UPDATED",
                format!(
                    "Remote assistance updated: allow_remote_assistance={}, verification_mode={}",
                    updated_settings.allow_remote_assistance, updated_settings.verification_mode
                ),
            );
        }
        Ok(UpdateHostSettingsResponse {
            overview: self.host_overview_snapshot(),
            error: None,
        })
    }

    pub fn refresh_temporary_verification_code(
        &self,
    ) -> io::Result<RefreshTemporaryVerificationCodeResponse> {
        let updated_settings = self
            .settings
            .lock()
            .map_err(lock_err)?
            .refresh_temporary_code()?;
        if let Ok(mut buffer) = self.diagnostics.lock() {
            buffer.push(
                "info",
                "HOST_TEMP_CODE_REFRESHED",
                format!(
                    "Temporary verification code refreshed to {}.",
                    updated_settings.temporary_verification_code
                ),
            );
        }
        Ok(RefreshTemporaryVerificationCodeResponse {
            overview: self.host_overview_snapshot(),
            error: None,
        })
    }

    pub fn diagnostics_response(&self) -> DiagnosticsResponse {
        DiagnosticsResponse {
            entries: self.diagnostic_entries(),
            error: None,
        }
    }

    pub fn ipc_diagnostics_changed(&self) -> DiagnosticsChanged {
        DiagnosticsChanged {
            entries: self.diagnostic_entries(),
        }
    }

    fn host_overview_snapshot(&self) -> Option<HostOverviewSnapshot> {
        let settings = self.settings.lock().ok().map(|store| store.snapshot());
        let paired_controller_count = self
            .control_state
            .pairing_store
            .lock()
            .ok()
            .map(|store| store.count() as u32)
            .unwrap_or_default();
        let active_controller_display_name = self.sessions.lock().ok().and_then(|sessions| {
            sessions
                .active
                .as_ref()
                .map(|session| session.controller_device_id.clone())
        });
        Some(HostOverviewSnapshot {
            identity: Some(HostIdentitySnapshot {
                device_id: self.host_identity.device_id.clone(),
                display_name: self.host_identity.display_name.clone(),
                hostname: self.host_identity.hostname.clone(),
                app_version: self.host_identity.app_version.clone(),
                platform: self.host_identity.platform.clone(),
            }),
            settings,
            paired_controller_count,
            control_port: self.control_state.advertised_presence.control_port as u32,
            active_controller_display_name,
            updated_at_ms: crate::pairing::now_ms(),
        })
    }

    fn diagnostic_entries(&self) -> Vec<host_ipc::types::DiagnosticEntry> {
        self.diagnostics
            .lock()
            .map(|buffer| buffer.entries())
            .unwrap_or_default()
    }
}

impl Default for RuntimeService {
    fn default() -> Self {
        Self::new().expect("failed to initialize runtime service")
    }
}

fn lock_err<T>(_: std::sync::PoisonError<T>) -> io::Error {
    io::Error::new(io::ErrorKind::Other, "runtime state lock poisoned")
}

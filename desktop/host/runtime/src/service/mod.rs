use crate::approval::{
    PendingRemoteRequestSnapshot as RuntimePendingRemoteRequestSnapshot,
    PendingRemoteRequestStore, SharedPendingRemoteRequestStore,
};
use crate::control::{
    default_capability_flags, default_host_identity, ControlPlaneState, DEFAULT_CONTROL_PORT,
};
use crate::diagnostics::{DiagnosticBuffer, SharedDiagnosticBuffer};
use crate::generated::v1::DevicePresence;
use crate::pairing::{load_runtime_base_dir, HostIdentity, PairingStore, SharedPairingStore};
use crate::session::{SessionRegistry, SharedSessionRegistry};
use crate::signaling::{SharedSignalingState, SignalingState};
use crate::settings::HostSettingsStore;
use crate::state::RuntimeStatus;
use crate::transport::{SharedTransportStatusState, TransportStatusState};
use crate::video::{SharedVideoStreamState, VideoStreamState};
use host_ipc::types::{
    CapabilitySnapshot, CapabilitySnapshotChanged, DiagnosticsChanged, DiagnosticsResponse,
    RefreshTemporaryVerificationCodeResponse, RespondPendingRemoteRequestResponse,
    HostIdentitySnapshot, HostOverviewChanged, HostOverviewResponse, HostOverviewSnapshot,
    HostSettingsSnapshot, InputCapabilitySnapshot,
    PendingRemoteRequestSnapshot as IpcPendingRemoteRequestSnapshot, RuntimeStatusChanged,
    RuntimeStatusResponse, RuntimeStatusSnapshot, UpdateHostSettingsResponse,
    VideoStreamSnapshot as IpcVideoStreamSnapshot,
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
    pending_remote_requests: SharedPendingRemoteRequestStore,
    video_streams: SharedVideoStreamState,
    signaling: SharedSignalingState,
    transport_status: SharedTransportStatusState,
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
        let pending_remote_requests: SharedPendingRemoteRequestStore =
            Arc::new(Mutex::new(PendingRemoteRequestStore::default()));
        let video_streams: SharedVideoStreamState =
            Arc::new(Mutex::new(VideoStreamState::default()));
        let signaling: SharedSignalingState = Arc::new(Mutex::new(SignalingState::default()));
        let transport_status: SharedTransportStatusState =
            Arc::new(Mutex::new(TransportStatusState::default()));
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
            settings: Arc::clone(&settings),
            pending_remote_requests: Arc::clone(&pending_remote_requests),
            video_streams: Arc::clone(&video_streams),
            signaling: Arc::clone(&signaling),
            transport_status: Arc::clone(&transport_status),
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
            pending_remote_requests,
            video_streams,
            signaling,
            transport_status,
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

    pub fn video_streams(&self) -> &SharedVideoStreamState {
        &self.video_streams
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

    pub fn respond_pending_remote_request(
        &self,
        allow: bool,
    ) -> io::Result<RespondPendingRemoteRequestResponse> {
        let handled = self
            .pending_remote_requests
            .lock()
            .map_err(lock_err)?
            .respond(allow);
        if let Ok(mut buffer) = self.diagnostics.lock() {
            if handled {
                buffer.push(
                    if allow { "info" } else { "warning" },
                    if allow {
                        "REMOTE_ASSIST_REQUEST_ALLOWED"
                    } else {
                        "REMOTE_ASSIST_REQUEST_REJECTED"
                    },
                    if allow {
                        "Desktop shell allowed the pending remote assistance request."
                    } else {
                        "Desktop shell rejected the pending remote assistance request."
                    },
                );
            }
        }
        Ok(RespondPendingRemoteRequestResponse {
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
        let pending_remote_request = self
            .pending_remote_requests
            .lock()
            .ok()
            .and_then(|mut store| {
                store.clear_expired();
                store.snapshot()
            })
            .map(IpcPendingRemoteRequestSnapshot::from);
        let settings = self.settings.lock().ok().map(|store| store.snapshot());
        let video_stream = self
            .video_streams
            .lock()
            .ok()
            .map(|streams| streams.snapshot())
            .map(IpcVideoStreamSnapshot::from);
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
            pending_remote_request,
            video_stream,
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

impl From<RuntimePendingRemoteRequestSnapshot> for IpcPendingRemoteRequestSnapshot {
    fn from(value: RuntimePendingRemoteRequestSnapshot) -> Self {
        Self {
            request_id: value.request_id,
            controller_device_id: value.controller_device_id,
            controller_display_name: value.controller_display_name,
            connection_method: value.connection_method,
            requested_at_ms: value.requested_at_ms,
            expires_at_ms: value.expires_at_ms,
        }
    }
}

impl From<crate::video::VideoStreamSnapshot> for IpcVideoStreamSnapshot {
    fn from(value: crate::video::VideoStreamSnapshot) -> Self {
        Self {
            session_id: value.session_id,
            packet_count: value.packet_count,
            keyframe_count: value.keyframe_count,
            latest_packet_len: value.latest_packet_len as u64,
            latest_frame_source_label: value.latest_frame_source_label,
            latest_error: value.latest_error,
            updated_at_ms: value.updated_at_ms,
        }
    }
}

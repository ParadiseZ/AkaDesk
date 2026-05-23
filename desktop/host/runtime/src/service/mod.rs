use crate::control::{
    default_capability_flags, default_host_identity, ControlPlaneState, DEFAULT_CONTROL_PORT,
};
use crate::generated::v1::DevicePresence;
use crate::pairing::{load_runtime_base_dir, HostIdentity, PairingStore, SharedPairingStore};
use crate::session::{SessionRegistry, SharedSessionRegistry};
use crate::state::RuntimeStatus;
use host_ipc::types::{
    CapabilitySnapshot, CapabilitySnapshotChanged, InputCapabilitySnapshot, RuntimeStatusChanged,
    RuntimeStatusSnapshot,
};
use std::io;
use std::sync::{Arc, Mutex};

#[derive(Debug)]
pub struct RuntimeService {
    status: RuntimeStatus,
    sessions: SharedSessionRegistry,
    control_state: ControlPlaneState,
}

impl RuntimeService {
    pub fn new() -> io::Result<Self> {
        let base_dir = load_runtime_base_dir();
        let host_identity = HostIdentity::load_or_create(&base_dir)?;
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
        Ok(Self {
            status: RuntimeStatus::default(),
            sessions,
            control_state,
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
}

impl Default for RuntimeService {
    fn default() -> Self {
        Self::new().expect("failed to initialize runtime service")
    }
}

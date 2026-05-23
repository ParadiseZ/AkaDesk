use crate::session::SessionRegistry;
use crate::state::RuntimeStatus;
use host_ipc::types::{
    CapabilitySnapshot, InputCapabilitySnapshot, RuntimeStatusChanged, RuntimeStatusSnapshot,
    CapabilitySnapshotChanged,
};

#[derive(Debug, Default)]
pub struct RuntimeService {
    status: RuntimeStatus,
    sessions: SessionRegistry,
}

impl RuntimeService {
    pub fn new() -> Self {
        Self::default()
    }

    pub fn status(&self) -> &RuntimeStatus {
        &self.status
    }

    pub fn sessions(&self) -> &SessionRegistry {
        &self.sessions
    }

    pub fn ipc_runtime_status_changed(&self) -> RuntimeStatusChanged {
        RuntimeStatusChanged {
            status: Some(RuntimeStatusSnapshot {
                state_label: self.status.state_label.to_string(),
                active_session_id: self
                    .status
                    .active_session_id
                    .as_ref()
                    .map(|session_id| session_id.0.clone()),
                updated_at_ms: self.status.updated_at_ms,
            }),
        }
    }

    pub fn ipc_capability_snapshot_changed(&self) -> CapabilitySnapshotChanged {
        CapabilitySnapshotChanged {
            snapshot: Some(CapabilitySnapshot {
                supports_audio: false,
                supports_input: true,
                supports_display_control: false,
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

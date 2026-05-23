use host_contracts::common::{HealthStatus, SessionId, TimestampMs};

#[derive(Debug, Clone)]
pub struct RuntimeStatus {
    pub state_label: &'static str,
    pub health: HealthStatus,
    pub active_session_id: Option<SessionId>,
    pub updated_at_ms: TimestampMs,
}

impl Default for RuntimeStatus {
    fn default() -> Self {
        Self {
            state_label: "READY_FOR_M4",
            health: HealthStatus::Healthy,
            active_session_id: None,
            updated_at_ms: crate::pairing::now_ms(),
        }
    }
}

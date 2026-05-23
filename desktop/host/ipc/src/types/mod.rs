use host_contracts::common::TimestampMs;
use host_contracts::input::InputCapabilitySnapshot;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum IpcMessageKind {
    Request,
    Response,
    Event,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct IpcEnvelope {
    pub request_id: String,
    pub kind: IpcMessageKind,
    pub message_type: String,
    pub payload: Vec<u8>,
    pub timestamp_ms: TimestampMs,
}

#[derive(Debug, Clone, Default)]
pub struct RuntimeStatusSnapshot {
    pub state_label: String,
    pub active_session_id: Option<String>,
    pub updated_at_ms: TimestampMs,
}

#[derive(Debug, Clone, Default)]
pub struct CapabilitySnapshot {
    pub supports_audio: bool,
    pub supports_input: bool,
    pub supports_display_control: bool,
    pub input: InputCapabilitySnapshot,
    pub updated_at_ms: TimestampMs,
}

pub const GET_RUNTIME_STATUS: &str = "GetRuntimeStatus";
pub const RUNTIME_STATUS_CHANGED: &str = "RuntimeStatusChanged";
pub const GET_CAPABILITY_SNAPSHOT: &str = "GetCapabilitySnapshot";
pub const CAPABILITY_SNAPSHOT_CHANGED: &str = "CapabilitySnapshotChanged";
pub const START_SESSION: &str = "StartSession";
pub const STOP_SESSION: &str = "StopSession";
pub const SESSION_CHANGED: &str = "SessionChanged";
pub const DIAGNOSTIC_EVENT: &str = "DiagnosticEvent";

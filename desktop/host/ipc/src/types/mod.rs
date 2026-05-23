use prost::{Enumeration, Message};

pub type TimestampMs = i64;

#[derive(Clone, Copy, Debug, PartialEq, Eq, Enumeration)]
#[repr(i32)]
pub enum IpcMessageKind {
    Unspecified = 0,
    Request = 1,
    Response = 2,
    Event = 3,
}

#[derive(Clone, PartialEq, Message)]
pub struct ErrorInfo {
    #[prost(string, tag = "1")]
    pub code: String,
    #[prost(string, tag = "2")]
    pub message: String,
    #[prost(bool, tag = "3")]
    pub retryable: bool,
    #[prost(string, tag = "4")]
    pub detail: String,
}

#[derive(Clone, PartialEq, Message)]
pub struct IpcEnvelope {
    #[prost(string, tag = "1")]
    pub request_id: String,
    #[prost(enumeration = "IpcMessageKind", tag = "2")]
    pub kind: i32,
    #[prost(string, tag = "3")]
    pub message_type: String,
    #[prost(bytes = "vec", tag = "4")]
    pub payload: Vec<u8>,
    #[prost(int64, tag = "5")]
    pub timestamp_ms: TimestampMs,
}

#[derive(Clone, PartialEq, Message)]
pub struct RuntimeStatusSnapshot {
    #[prost(string, tag = "1")]
    pub state_label: String,
    #[prost(string, optional, tag = "2")]
    pub active_session_id: Option<String>,
    #[prost(int64, tag = "3")]
    pub updated_at_ms: TimestampMs,
}

#[derive(Clone, PartialEq, Message)]
pub struct InputCapabilitySnapshot {
    #[prost(bool, tag = "1")]
    pub supports_pointer: bool,
    #[prost(bool, tag = "2")]
    pub supports_scroll: bool,
    #[prost(bool, tag = "3")]
    pub supports_keyboard: bool,
}

#[derive(Clone, PartialEq, Message)]
pub struct CapabilitySnapshot {
    #[prost(bool, tag = "1")]
    pub supports_audio: bool,
    #[prost(bool, tag = "2")]
    pub supports_input: bool,
    #[prost(bool, tag = "3")]
    pub supports_display_control: bool,
    #[prost(message, optional, tag = "4")]
    pub input: Option<InputCapabilitySnapshot>,
    #[prost(int64, tag = "5")]
    pub updated_at_ms: TimestampMs,
}

#[derive(Clone, PartialEq, Message)]
pub struct GetRuntimeStatusRequest {}

#[derive(Clone, PartialEq, Message)]
pub struct RuntimeStatusResponse {
    #[prost(message, optional, tag = "1")]
    pub status: Option<RuntimeStatusSnapshot>,
    #[prost(message, optional, tag = "2")]
    pub error: Option<ErrorInfo>,
}

#[derive(Clone, PartialEq, Message)]
pub struct RuntimeStatusChanged {
    #[prost(message, optional, tag = "1")]
    pub status: Option<RuntimeStatusSnapshot>,
}

#[derive(Clone, PartialEq, Message)]
pub struct GetCapabilitySnapshotRequest {}

#[derive(Clone, PartialEq, Message)]
pub struct CapabilitySnapshotResponse {
    #[prost(message, optional, tag = "1")]
    pub snapshot: Option<CapabilitySnapshot>,
    #[prost(message, optional, tag = "2")]
    pub error: Option<ErrorInfo>,
}

#[derive(Clone, PartialEq, Message)]
pub struct CapabilitySnapshotChanged {
    #[prost(message, optional, tag = "1")]
    pub snapshot: Option<CapabilitySnapshot>,
}

pub const GET_RUNTIME_STATUS: &str = "GetRuntimeStatus";
pub const RUNTIME_STATUS_RESPONSE: &str = "RuntimeStatusResponse";
pub const RUNTIME_STATUS_CHANGED: &str = "RuntimeStatusChanged";
pub const GET_CAPABILITY_SNAPSHOT: &str = "GetCapabilitySnapshot";
pub const CAPABILITY_SNAPSHOT_RESPONSE: &str = "CapabilitySnapshotResponse";
pub const CAPABILITY_SNAPSHOT_CHANGED: &str = "CapabilitySnapshotChanged";

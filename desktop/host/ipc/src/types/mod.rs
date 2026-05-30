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

#[derive(Clone, PartialEq, Message)]
pub struct HostIdentitySnapshot {
    #[prost(string, tag = "1")]
    pub device_id: String,
    #[prost(string, tag = "2")]
    pub display_name: String,
    #[prost(string, tag = "3")]
    pub hostname: String,
    #[prost(string, tag = "4")]
    pub app_version: String,
    #[prost(string, tag = "5")]
    pub platform: String,
}

#[derive(Clone, PartialEq, Message)]
pub struct HostSettingsSnapshot {
    #[prost(bool, tag = "1")]
    pub allow_remote_assistance: bool,
    #[prost(string, tag = "2")]
    pub verification_mode: String,
    #[prost(string, tag = "3")]
    pub connection_method: String,
    #[prost(bool, tag = "4")]
    pub notify_on_incoming_request: bool,
    #[prost(bool, tag = "5")]
    pub auto_accept_trusted_controller: bool,
    #[prost(int64, tag = "6")]
    pub updated_at_ms: TimestampMs,
    #[prost(string, tag = "7")]
    pub temporary_verification_code: String,
    #[prost(string, tag = "8")]
    pub custom_verification_code: String,
    #[prost(bool, tag = "9")]
    pub refresh_temporary_code_after_controlled_end: bool,
}

#[derive(Clone, PartialEq, Message)]
pub struct HostOverviewSnapshot {
    #[prost(message, optional, tag = "1")]
    pub identity: Option<HostIdentitySnapshot>,
    #[prost(message, optional, tag = "2")]
    pub settings: Option<HostSettingsSnapshot>,
    #[prost(uint32, tag = "3")]
    pub paired_controller_count: u32,
    #[prost(uint32, tag = "4")]
    pub control_port: u32,
    #[prost(string, optional, tag = "5")]
    pub active_controller_display_name: Option<String>,
    #[prost(int64, tag = "6")]
    pub updated_at_ms: TimestampMs,
    #[prost(message, optional, tag = "7")]
    pub pending_remote_request: Option<PendingRemoteRequestSnapshot>,
    #[prost(message, optional, tag = "8")]
    pub video_stream: Option<VideoStreamSnapshot>,
}

#[derive(Clone, PartialEq, Message)]
pub struct PendingRemoteRequestSnapshot {
    #[prost(string, tag = "1")]
    pub request_id: String,
    #[prost(string, tag = "2")]
    pub controller_device_id: String,
    #[prost(string, tag = "3")]
    pub controller_display_name: String,
    #[prost(string, tag = "4")]
    pub connection_method: String,
    #[prost(int64, tag = "5")]
    pub requested_at_ms: TimestampMs,
    #[prost(int64, tag = "6")]
    pub expires_at_ms: TimestampMs,
}

#[derive(Clone, PartialEq, Message)]
pub struct VideoStreamSnapshot {
    #[prost(string, tag = "1")]
    pub session_id: String,
    #[prost(uint64, tag = "2")]
    pub packet_count: u64,
    #[prost(uint64, tag = "3")]
    pub keyframe_count: u64,
    #[prost(uint64, tag = "4")]
    pub latest_packet_len: u64,
    #[prost(string, tag = "5")]
    pub latest_frame_source_label: String,
    #[prost(string, tag = "6")]
    pub latest_error: String,
    #[prost(int64, tag = "7")]
    pub updated_at_ms: TimestampMs,
}

#[derive(Clone, PartialEq, Message)]
pub struct DiagnosticEntry {
    #[prost(string, tag = "1")]
    pub level: String,
    #[prost(string, tag = "2")]
    pub code: String,
    #[prost(string, tag = "3")]
    pub message: String,
    #[prost(int64, tag = "4")]
    pub created_at_ms: TimestampMs,
}

#[derive(Clone, PartialEq, Message)]
pub struct GetHostOverviewRequest {}

#[derive(Clone, PartialEq, Message)]
pub struct HostOverviewResponse {
    #[prost(message, optional, tag = "1")]
    pub overview: Option<HostOverviewSnapshot>,
    #[prost(message, optional, tag = "2")]
    pub error: Option<ErrorInfo>,
}

#[derive(Clone, PartialEq, Message)]
pub struct HostOverviewChanged {
    #[prost(message, optional, tag = "1")]
    pub overview: Option<HostOverviewSnapshot>,
}

#[derive(Clone, PartialEq, Message)]
pub struct UpdateHostSettingsRequest {
    #[prost(message, optional, tag = "1")]
    pub settings: Option<HostSettingsSnapshot>,
}

#[derive(Clone, PartialEq, Message)]
pub struct UpdateHostSettingsResponse {
    #[prost(message, optional, tag = "1")]
    pub overview: Option<HostOverviewSnapshot>,
    #[prost(message, optional, tag = "2")]
    pub error: Option<ErrorInfo>,
}

#[derive(Clone, PartialEq, Message)]
pub struct RefreshTemporaryVerificationCodeRequest {}

#[derive(Clone, PartialEq, Message)]
pub struct RefreshTemporaryVerificationCodeResponse {
    #[prost(message, optional, tag = "1")]
    pub overview: Option<HostOverviewSnapshot>,
    #[prost(message, optional, tag = "2")]
    pub error: Option<ErrorInfo>,
}

#[derive(Clone, PartialEq, Message)]
pub struct RespondPendingRemoteRequestRequest {
    #[prost(bool, tag = "1")]
    pub allow: bool,
}

#[derive(Clone, PartialEq, Message)]
pub struct RespondPendingRemoteRequestResponse {
    #[prost(message, optional, tag = "1")]
    pub overview: Option<HostOverviewSnapshot>,
    #[prost(message, optional, tag = "2")]
    pub error: Option<ErrorInfo>,
}

#[derive(Clone, PartialEq, Message)]
pub struct GetDiagnosticsRequest {}

#[derive(Clone, PartialEq, Message)]
pub struct DiagnosticsResponse {
    #[prost(message, repeated, tag = "1")]
    pub entries: Vec<DiagnosticEntry>,
    #[prost(message, optional, tag = "2")]
    pub error: Option<ErrorInfo>,
}

#[derive(Clone, PartialEq, Message)]
pub struct DiagnosticsChanged {
    #[prost(message, repeated, tag = "1")]
    pub entries: Vec<DiagnosticEntry>,
}

pub const GET_RUNTIME_STATUS: &str = "GetRuntimeStatus";
pub const RUNTIME_STATUS_RESPONSE: &str = "RuntimeStatusResponse";
pub const RUNTIME_STATUS_CHANGED: &str = "RuntimeStatusChanged";
pub const GET_CAPABILITY_SNAPSHOT: &str = "GetCapabilitySnapshot";
pub const CAPABILITY_SNAPSHOT_RESPONSE: &str = "CapabilitySnapshotResponse";
pub const CAPABILITY_SNAPSHOT_CHANGED: &str = "CapabilitySnapshotChanged";
pub const GET_HOST_OVERVIEW: &str = "GetHostOverview";
pub const HOST_OVERVIEW_RESPONSE: &str = "HostOverviewResponse";
pub const HOST_OVERVIEW_CHANGED: &str = "HostOverviewChanged";
pub const UPDATE_HOST_SETTINGS: &str = "UpdateHostSettings";
pub const UPDATE_HOST_SETTINGS_RESPONSE: &str = "UpdateHostSettingsResponse";
pub const REFRESH_TEMPORARY_VERIFICATION_CODE: &str = "RefreshTemporaryVerificationCode";
pub const REFRESH_TEMPORARY_VERIFICATION_CODE_RESPONSE: &str =
    "RefreshTemporaryVerificationCodeResponse";
pub const RESPOND_PENDING_REMOTE_REQUEST: &str = "RespondPendingRemoteRequest";
pub const RESPOND_PENDING_REMOTE_REQUEST_RESPONSE: &str = "RespondPendingRemoteRequestResponse";
pub const GET_DIAGNOSTICS: &str = "GetDiagnostics";
pub const DIAGNOSTICS_RESPONSE: &str = "DiagnosticsResponse";
pub const DIAGNOSTICS_CHANGED: &str = "DiagnosticsChanged";

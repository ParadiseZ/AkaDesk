pub type TimestampMs = i64;

include!(concat!(env!("OUT_DIR"), "/akadesk.v1.rs"));
pub type DiagnosticEntry = DiagnosticEvent;

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
pub const STOP_SESSION: &str = "StopSession";
pub const STOP_SESSION_RESPONSE: &str = "StopSessionResponse";

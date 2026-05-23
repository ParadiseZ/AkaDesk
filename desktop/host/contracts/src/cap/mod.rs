use crate::common::{CaptureTargetId, DisplayId, TimestampMs};
use crate::error::HostResult;

#[derive(Debug, Clone, Default)]
pub struct CaptureTargetSnapshot {
    pub target_id: CaptureTargetId,
    pub display_id: Option<DisplayId>,
    pub name: String,
    pub primary: bool,
}

#[derive(Debug, Clone, Default)]
pub struct CaptureCapabilitySnapshot {
    pub supports_wgc: bool,
    pub supports_dxgi: bool,
    pub supports_cursor_capture: bool,
}

#[derive(Debug, Clone, Default)]
pub struct CapturePrepareRequest {
    pub target_id: CaptureTargetId,
    pub requested_width: u32,
    pub requested_height: u32,
    pub requested_fps: u32,
}

#[derive(Debug, Clone, Default)]
pub struct CapturePrepareResult {
    pub target_id: CaptureTargetId,
    pub accepted_width: u32,
    pub accepted_height: u32,
    pub accepted_fps: u32,
}

#[derive(Debug, Clone, Default)]
pub struct CaptureStartRequest {
    pub target_id: CaptureTargetId,
    pub session_id: String,
}

#[derive(Debug, Clone, Default)]
pub struct CaptureSessionHandle {
    pub handle_id: String,
    pub started_at_ms: TimestampMs,
}

#[derive(Debug, Clone, Default)]
pub struct RawVideoFrame {
    pub width: u32,
    pub height: u32,
    pub timestamp_ms: TimestampMs,
    pub bytes: Vec<u8>,
}

pub trait CaptureQueryPort {
    async fn list_capture_targets(&self) -> HostResult<Vec<CaptureTargetSnapshot>>;
    async fn get_capture_capabilities(
        &self,
        target_id: &CaptureTargetId,
    ) -> HostResult<CaptureCapabilitySnapshot>;
}

pub trait CaptureControlPort {
    async fn prepare_capture(
        &self,
        request: CapturePrepareRequest,
    ) -> HostResult<CapturePrepareResult>;
    async fn start_capture(
        &self,
        request: CaptureStartRequest,
    ) -> HostResult<CaptureSessionHandle>;
    async fn stop_capture(&self, handle: &CaptureSessionHandle) -> HostResult<()>;
}

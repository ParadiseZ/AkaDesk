use crate::common::{CodecImplementation, TimestampMs, VideoCodec};
use crate::error::HostResult;

#[derive(Debug, Clone, Default)]
pub struct VideoEncoderDescriptor {
    pub encoder_id: String,
    pub codec: VideoCodec,
    pub implementation: CodecImplementation,
    pub display_name: String,
}

#[derive(Debug, Clone, Default)]
pub struct VideoEncoderCapabilitySnapshot {
    pub encoder_id: String,
    pub max_width: u32,
    pub max_height: u32,
    pub max_fps: u32,
    pub supports_reconfigure: bool,
}

#[derive(Debug, Clone, Default)]
pub struct VideoEncoderPrepareRequest {
    pub codec: VideoCodec,
    pub width: u32,
    pub height: u32,
    pub fps: u32,
    pub bitrate_kbps: u32,
}

#[derive(Debug, Clone, Default)]
pub struct VideoEncoderPrepareResult {
    pub encoder_id: String,
    pub implementation: CodecImplementation,
}

#[derive(Debug, Clone, Default)]
pub struct VideoEncoderStartRequest {
    pub session_id: String,
}

#[derive(Debug, Clone, Default)]
pub struct VideoEncoderReconfigureRequest {
    pub width: u32,
    pub height: u32,
    pub fps: u32,
    pub bitrate_kbps: u32,
}

#[derive(Debug, Clone, Default)]
pub struct VideoEncoderHandle {
    pub handle_id: String,
    pub started_at_ms: TimestampMs,
}

#[derive(Debug, Clone, Default)]
pub struct EncodedVideoPacket {
    pub codec: VideoCodec,
    pub timestamp_ms: TimestampMs,
    pub is_keyframe: bool,
    pub bytes: Vec<u8>,
}

pub trait VideoEncoderQueryPort {
    async fn list_video_encoders(&self) -> HostResult<Vec<VideoEncoderDescriptor>>;
    async fn get_video_encoder_capabilities(
        &self,
        encoder_id: &str,
    ) -> HostResult<VideoEncoderCapabilitySnapshot>;
}

pub trait VideoEncodePort {
    async fn prepare_video_encoder(
        &self,
        request: VideoEncoderPrepareRequest,
    ) -> HostResult<VideoEncoderPrepareResult>;
    async fn start_video_encoder(
        &self,
        request: VideoEncoderStartRequest,
    ) -> HostResult<VideoEncoderHandle>;
    async fn reconfigure_video_encoder(
        &self,
        handle: &VideoEncoderHandle,
        request: VideoEncoderReconfigureRequest,
    ) -> HostResult<()>;
    async fn stop_video_encoder(&self, handle: &VideoEncoderHandle) -> HostResult<()>;
}

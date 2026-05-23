use crate::generated::v1::{
    CodecImplementation, SessionIntent, VideoCodec, VideoPreference, VideoSessionSpec,
};

#[derive(Debug, Clone, Copy, Default)]
pub struct VideoPipelineState {
    pub codec: &'static str,
    pub width: u32,
    pub height: u32,
    pub fps: u32,
    pub bitrate_kbps: u32,
}

#[derive(Debug, Clone, Copy, Default)]
pub struct VideoRuntimeSupport {
    pub has_h264_decoder: bool,
    pub has_h264_encoder: bool,
    pub has_h264_software_encoder: bool,
    pub has_h264_hardware_encoder: bool,
}

#[derive(Debug, Clone, PartialEq)]
pub struct PreparedVideoSession {
    pub spec: VideoSessionSpec,
    pub encoder_name: String,
}

const DEFAULT_H264_WIDTH: u32 = 1280;
const DEFAULT_H264_HEIGHT: u32 = 720;
const DEFAULT_H264_FPS: u32 = 30;
const DEFAULT_H264_BITRATE_KBPS: u32 = 4_000;

#[cfg(windows)]
pub fn detect_runtime_video_support() -> VideoRuntimeSupport {
    match host_windows::ffmpeg::probe_codec_inventory() {
        Ok(inventory) => VideoRuntimeSupport {
            has_h264_decoder: inventory.h264.decoder,
            has_h264_encoder: inventory.h264_encoder.any,
            has_h264_software_encoder: inventory.h264_encoder.software,
            has_h264_hardware_encoder: inventory.h264_encoder.hardware,
        },
        Err(err) => {
            eprintln!("failed to probe local ffmpeg sdk: {err}");
            VideoRuntimeSupport::default()
        }
    }
}

#[cfg(not(windows))]
pub fn detect_runtime_video_support() -> VideoRuntimeSupport {
    VideoRuntimeSupport::default()
}

#[cfg(windows)]
pub fn prepare_h264_session(intent: &SessionIntent) -> Result<PreparedVideoSession, String> {
    let video = intent.video.clone().unwrap_or_default();
    let requested_codec = VideoCodec::try_from(video.codec).unwrap_or(VideoCodec::Unspecified);
    if requested_codec != VideoCodec::Unspecified && requested_codec != VideoCodec::H264 {
        return Err(format!(
            "V1 only accepts H.264 video requests; got {:?}",
            requested_codec
        ));
    }

    let request = host_windows::ffmpeg::H264EncoderRequest {
        width: non_zero_or(video.width, DEFAULT_H264_WIDTH),
        height: non_zero_or(video.height, DEFAULT_H264_HEIGHT),
        fps: non_zero_or(video.fps, DEFAULT_H264_FPS),
        bitrate_kbps: non_zero_or(video.bitrate_kbps, DEFAULT_H264_BITRATE_KBPS),
        allow_software_fallback: allow_software_fallback(&video),
    };
    let prepared = host_windows::ffmpeg::prepare_h264_encoder(&request)?;
    let host_encoder = match prepared.implementation {
        host_windows::ffmpeg::PreparedEncoderImplementation::Hardware => {
            CodecImplementation::Hardware as i32
        }
        host_windows::ffmpeg::PreparedEncoderImplementation::Software => {
            CodecImplementation::Software as i32
        }
    };

    Ok(PreparedVideoSession {
        spec: VideoSessionSpec {
            codec: VideoCodec::H264 as i32,
            width: request.width,
            height: request.height,
            fps: request.fps,
            bitrate_kbps: request.bitrate_kbps,
            host_encoder,
            controller_decoder: CodecImplementation::Software as i32,
        },
        encoder_name: prepared.encoder_name,
    })
}

#[cfg(not(windows))]
pub fn prepare_h264_session(_: &SessionIntent) -> Result<PreparedVideoSession, String> {
    Err("Video session preparation is only wired for the Windows host runtime.".to_string())
}

fn allow_software_fallback(video: &VideoPreference) -> bool {
    if video.codec == VideoCodec::Unspecified as i32 {
        return true;
    }
    video.allow_same_codec_software_fallback
}

fn non_zero_or(value: u32, fallback: u32) -> u32 {
    if value == 0 {
        fallback
    } else {
        value
    }
}

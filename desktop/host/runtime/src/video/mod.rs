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

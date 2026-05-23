#[derive(Debug, Default)]
pub struct VideoPipelineState {
    pub codec: &'static str,
    pub width: u32,
    pub height: u32,
    pub fps: u32,
    pub bitrate_kbps: u32,
}

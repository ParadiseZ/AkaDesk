#[derive(Debug, Default)]
pub struct AudioPipelineState {
    pub codec: &'static str,
    pub sample_rate: u32,
    pub channels: u32,
    pub bitrate_kbps: u32,
}

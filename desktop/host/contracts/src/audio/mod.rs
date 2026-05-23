use crate::common::{AudioCodec, AudioDeviceId, TimestampMs};
use crate::error::HostResult;

#[derive(Debug, Clone, Default)]
pub struct AudioDeviceSnapshot {
    pub device_id: AudioDeviceId,
    pub name: String,
    pub is_input: bool,
    pub is_output: bool,
    pub is_default: bool,
}

#[derive(Debug, Clone, Default)]
pub struct AudioCapabilitySnapshot {
    pub supports_loopback_capture: bool,
    pub supported_sample_rates: Vec<u32>,
    pub supported_channel_counts: Vec<u32>,
}

#[derive(Debug, Clone, Default)]
pub struct AudioOutputSettingSnapshot {
    pub default_output_device_id: Option<AudioDeviceId>,
    pub muted: bool,
    pub volume_percent: Option<u32>,
}

#[derive(Debug, Clone, Default)]
pub struct AudioPrepareRequest {
    pub device_id: Option<AudioDeviceId>,
    pub codec: AudioCodec,
    pub sample_rate: u32,
    pub channels: u32,
    pub bitrate_kbps: u32,
}

#[derive(Debug, Clone, Default)]
pub struct AudioPrepareResult {
    pub device_id: Option<AudioDeviceId>,
    pub sample_rate: u32,
    pub channels: u32,
    pub bitrate_kbps: u32,
}

#[derive(Debug, Clone, Default)]
pub struct AudioStartRequest {
    pub session_id: String,
}

#[derive(Debug, Clone, Default)]
pub struct AudioCaptureHandle {
    pub handle_id: String,
    pub started_at_ms: TimestampMs,
}

#[derive(Debug, Clone, Default)]
pub struct PcmAudioChunk {
    pub sample_rate: u32,
    pub channels: u32,
    pub timestamp_ms: TimestampMs,
    pub samples: Vec<i16>,
}

pub trait AudioDeviceQueryPort {
    async fn list_audio_devices(&self) -> HostResult<Vec<AudioDeviceSnapshot>>;
    async fn get_default_output_device(&self) -> HostResult<Option<AudioDeviceId>>;
    async fn get_audio_capabilities(
        &self,
        device_id: &AudioDeviceId,
    ) -> HostResult<AudioCapabilitySnapshot>;
    async fn capture_audio_output_setting(&self) -> HostResult<AudioOutputSettingSnapshot>;
}

pub trait AudioCapturePort {
    async fn prepare_audio_capture(
        &self,
        request: AudioPrepareRequest,
    ) -> HostResult<AudioPrepareResult>;
    async fn start_audio_capture(
        &self,
        request: AudioStartRequest,
    ) -> HostResult<AudioCaptureHandle>;
    async fn stop_audio_capture(&self, handle: &AudioCaptureHandle) -> HostResult<()>;
    async fn set_controlled_local_playback_muted(&self, muted: bool) -> HostResult<()>;
    async fn restore_audio_output_setting(
        &self,
        snapshot: AudioOutputSettingSnapshot,
    ) -> HostResult<()>;
}

#[derive(Debug, Clone, PartialEq, Eq, Hash, Default)]
pub struct DeviceId(pub String);

#[derive(Debug, Clone, PartialEq, Eq, Hash, Default)]
pub struct SessionId(pub String);

#[derive(Debug, Clone, PartialEq, Eq, Hash, Default)]
pub struct DisplayId(pub String);

#[derive(Debug, Clone, PartialEq, Eq, Hash, Default)]
pub struct AudioDeviceId(pub String);

#[derive(Debug, Clone, PartialEq, Eq, Hash, Default)]
pub struct CaptureTargetId(pub String);

pub type TimestampMs = i64;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum VideoCodec {
    #[default]
    H264,
    Hevc,
    Vp8,
    Vp9,
    Av1,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum AudioCodec {
    #[default]
    Opus,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum CodecImplementation {
    #[default]
    Hardware,
    Software,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum HealthStatus {
    #[default]
    Unknown,
    Healthy,
    Degraded,
    Failed,
}

#[derive(Debug, Clone, Default)]
pub struct DeviceRuntimeSnapshot {
    pub device_id: DeviceId,
    pub health: HealthStatus,
    pub active_session_id: Option<SessionId>,
    pub updated_at_ms: TimestampMs,
}

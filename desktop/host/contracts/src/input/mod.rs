use crate::common::TimestampMs;
use crate::error::HostResult;

#[derive(Debug, Clone, Default)]
pub struct InputCapabilitySnapshot {
    pub supports_pointer: bool,
    pub supports_scroll: bool,
    pub supports_keyboard: bool,
}

#[derive(Debug, Clone, Default)]
pub struct InputPrepareRequest {
    pub session_id: String,
}

#[derive(Debug, Clone, Default)]
pub struct InputPrepareResult {
    pub prepared: bool,
}

#[derive(Debug, Clone, Default)]
pub struct InputReleaseRequest {
    pub session_id: String,
}

#[derive(Debug, Clone, Default)]
pub struct PointerInputCommand {
    pub normalized_x: f32,
    pub normalized_y: f32,
    pub button: u8,
    pub pressed: bool,
    pub timestamp_ms: TimestampMs,
}

#[derive(Debug, Clone, Default)]
pub struct ScrollInputCommand {
    pub delta_x: f32,
    pub delta_y: f32,
    pub timestamp_ms: TimestampMs,
}

#[derive(Debug, Clone, Default)]
pub struct KeyInputCommand {
    pub key_code: u32,
    pub action: u8,
    pub alt: bool,
    pub ctrl: bool,
    pub shift: bool,
    pub meta: bool,
    pub timestamp_ms: TimestampMs,
}

pub trait InputCapabilityQueryPort {
    async fn get_input_capabilities(&self) -> HostResult<InputCapabilitySnapshot>;
}

pub trait InputInjectionPort {
    async fn prepare_input(&self, request: InputPrepareRequest) -> HostResult<InputPrepareResult>;
    async fn inject_pointer_event(&self, event: PointerInputCommand) -> HostResult<()>;
    async fn inject_scroll_event(&self, event: ScrollInputCommand) -> HostResult<()>;
    async fn inject_key_event(&self, event: KeyInputCommand) -> HostResult<()>;
    async fn release_input(&self, request: InputReleaseRequest) -> HostResult<()>;
}

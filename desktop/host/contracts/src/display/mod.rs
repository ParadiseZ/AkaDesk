use crate::common::{DisplayId, TimestampMs};
use crate::error::HostResult;

#[derive(Debug, Clone, Default)]
pub struct DisplaySnapshot {
    pub display_id: DisplayId,
    pub name: String,
    pub width: u32,
    pub height: u32,
    pub refresh_rate: u32,
    pub scale_percent: u32,
    pub primary: bool,
    pub hdr: bool,
}

#[derive(Debug, Clone, Default)]
pub struct DisplayMode {
    pub width: u32,
    pub height: u32,
    pub refresh_rate: u32,
    pub scale_percent: u32,
}

#[derive(Debug, Clone, Default)]
pub struct DisplayPrepareRequest {
    pub display_id: DisplayId,
}

#[derive(Debug, Clone, Default)]
pub struct DisplayPrepareResult {
    pub display_id: DisplayId,
    pub history_display_mode: Option<DisplayMode>,
}

#[derive(Debug, Clone, Default)]
pub struct DisplaySettingIntent {
    pub display_id: DisplayId,
    pub requested_display_mode: DisplayMode,
    pub history_display_mode: DisplayMode,
}

#[derive(Debug, Clone, Default)]
pub struct DisplayApplyResult {
    pub applied: bool,
    pub current_mode: DisplayMode,
    pub recoverable: bool,
}

#[derive(Debug, Clone, Default)]
pub struct DisplayRestoreToken {
    pub display_id: DisplayId,
    pub history_display_mode: DisplayMode,
    pub created_at_ms: TimestampMs,
}

pub trait DisplayQueryPort {
    async fn list_displays(&self) -> HostResult<Vec<DisplaySnapshot>>;
    async fn get_display(&self, display_id: &DisplayId) -> HostResult<DisplaySnapshot>;
    async fn list_display_modes(&self, display_id: &DisplayId) -> HostResult<Vec<DisplayMode>>;
}

pub trait DisplayControlPort {
    async fn prepare_display(
        &self,
        request: DisplayPrepareRequest,
    ) -> HostResult<DisplayPrepareResult>;
    async fn apply_display_settings(
        &self,
        intent: DisplaySettingIntent,
    ) -> HostResult<DisplayApplyResult>;
    async fn restore_display_settings(&self, token: DisplayRestoreToken) -> HostResult<()>;
}

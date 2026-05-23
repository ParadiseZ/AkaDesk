use host_contracts::common::SessionId;

#[derive(Debug, Default)]
pub struct SessionContext {
    pub session_id: SessionId,
    pub target_display_id: Option<String>,
}

#[derive(Debug, Default)]
pub struct SessionRegistry {
    pub active: Option<SessionContext>,
    pub history_count: usize,
}

use crate::session::SessionRegistry;
use crate::state::RuntimeStatus;

#[derive(Debug, Default)]
pub struct RuntimeService {
    status: RuntimeStatus,
    sessions: SessionRegistry,
}

impl RuntimeService {
    pub fn new() -> Self {
        Self::default()
    }

    pub fn status(&self) -> &RuntimeStatus {
        &self.status
    }

    pub fn sessions(&self) -> &SessionRegistry {
        &self.sessions
    }
}

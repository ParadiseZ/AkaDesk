#[derive(Debug, Default)]
pub struct DiagnosticBuffer {
    pub recent_events: Vec<String>,
}

impl DiagnosticBuffer {
    pub fn push(&mut self, message: impl Into<String>) {
        self.recent_events.push(message.into());
    }
}

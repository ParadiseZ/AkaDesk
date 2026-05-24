use crate::pairing::now_ms;
use host_ipc::types::DiagnosticEntry;
use std::sync::{Arc, Mutex};

#[derive(Debug, Default)]
pub struct DiagnosticBuffer {
    recent_entries: Vec<DiagnosticEntry>,
}

pub type SharedDiagnosticBuffer = Arc<Mutex<DiagnosticBuffer>>;

impl DiagnosticBuffer {
    pub fn push(&mut self, level: &str, code: &str, message: impl Into<String>) {
        if self.recent_entries.len() >= 40 {
            self.recent_entries.remove(0);
        }
        self.recent_entries.push(DiagnosticEntry {
            level: level.to_string(),
            code: code.to_string(),
            message: message.into(),
            created_at_ms: now_ms(),
        });
    }

    pub fn entries(&self) -> Vec<DiagnosticEntry> {
        self.recent_entries.clone()
    }
}

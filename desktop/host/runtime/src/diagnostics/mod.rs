use crate::events::SharedStateSignal;
use crate::pairing::now_ms;
use host_ipc::types::{DiagnosticEntry, DiagnosticSnapshot};
use std::collections::BTreeSet;
use std::sync::{Arc, Mutex};

#[derive(Debug, Default)]
pub struct DiagnosticBuffer {
    recent_entries: Vec<DiagnosticEntry>,
    signal: Option<SharedStateSignal>,
    snapshot: DiagnosticSnapshot,
    sequence: u64,
    active_session: Option<String>,
    faults: BTreeSet<String>,
}

pub type SharedDiagnosticBuffer = Arc<Mutex<DiagnosticBuffer>>;

impl DiagnosticBuffer {
    pub fn set_signal(&mut self, signal: SharedStateSignal) {
        self.signal = Some(signal);
    }

    pub fn push(&mut self, level: &str, code: &str, _raw: impl Into<String>) {
        self.record(level, code, None);
    }
    pub fn record(&mut self, level: &str, code: &str, session: Option<&str>) {
        self.record_at(level, code, session, now_ms());
    }
    fn record_at(&mut self, level: &str, code: &str, session: Option<&str>, now: i64) {
        let level = if level == "warn" { "warning" } else { level };
        let existing = self.recent_entries.iter_mut().rev().find(|e| {
            e.code == code
                && e.level == level
                && e.session_id.as_deref() == session
                && now.saturating_sub(e.last_seen_ms) < 30_000
        });
        let entry = if let Some(entry) = existing {
            entry.repeat_count = entry.repeat_count.saturating_add(1);
            entry.last_seen_ms = now;
            entry.clone()
        } else {
            self.sequence += 1;
            let entry = DiagnosticEntry {
                level: level.into(),
                code: code.into(),
                message: host_ipc::diagnostics::presentation(code).0.into(),
                created_at_ms: now,
                last_seen_ms: now,
                event_id: format!("{now}-{}", self.sequence),
                category: host_ipc::diagnostics::category(code).into(),
                session_id: session.map(str::to_owned),
                repeat_count: 1,
            };
            if self.recent_entries.len() == 40 {
                self.recent_entries.remove(0);
            }
            self.recent_entries.push(entry.clone());
            entry
        };
        if level == "error" {
            self.snapshot.last_error = Some(entry.clone());
        }
        if level == "warning" {
            self.snapshot.last_warning = Some(entry.clone());
        }
        if code.ends_with("RESTORE_FAILED")
            || code.ends_with("INTERRUPTED")
            || code == "MEDIA_STREAM_FAILED"
        {
            self.snapshot.last_degraded_event = Some(entry.clone());
        }
        if code.starts_with("DISPLAY_RESTORE_") {
            self.snapshot.last_display_restore_result = Some(entry.clone());
        }
        if code.starts_with("AUDIO_OUTPUT_RESTORE_") {
            self.snapshot.last_audio_restore_result = Some(entry.clone());
        }
        if code == "SESSION_CLOSED" || code == "SESSION_FAILED" {
            self.snapshot.last_session_closed_event = Some(entry.clone());
        }
        self.snapshot.updated_at_ms = now;
        // Repeated cleanup failures arrive every 250 ms; update counts quietly.
        if entry.repeat_count == 1 {
            if let Some(signal) = &self.signal {
                signal.notify();
            }
        }
    }
    pub fn begin_session(&mut self, id: &str) {
        self.active_session = Some(id.into());
        self.faults.retain(|source| source.ends_with("restore"));
        self.snapshot.degraded = !self.faults.is_empty();
        self.record("info", "SESSION_PREPARING", Some(id));
    }
    pub fn end_session(&mut self, id: &str) {
        if self.active_session.as_deref() != Some(id) {
            return;
        }
        self.active_session = None;
        self.faults.retain(|source| source.ends_with("restore"));
        self.snapshot.degraded = !self.faults.is_empty();
        self.record("info", "SESSION_CLOSED", Some(id));
    }
    pub fn health(&mut self, id: Option<&str>, source: &str, healthy: bool, code: &str) {
        if id.is_some() && id != self.active_session.as_deref() {
            return;
        }
        let changed = if healthy {
            self.faults.remove(source)
        } else {
            self.faults.insert(source.into())
        };
        if changed {
            self.snapshot.degraded = !self.faults.is_empty();
            self.record(if healthy { "info" } else { "error" }, code, id);
            if let Some(signal) = &self.signal {
                signal.notify();
            }
        } else if !healthy {
            self.record("error", code, id);
        }
    }

    pub fn entries(&self) -> Vec<DiagnosticEntry> {
        self.recent_entries.clone()
    }
    pub fn snapshot(&self) -> DiagnosticSnapshot {
        self.snapshot.clone()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn repeated_failures_are_bounded_and_pinned_summaries_survive_eviction() {
        let mut b = DiagnosticBuffer::default();
        for i in 0..1000 {
            b.record_at("error", "DISPLAY_RESTORE_FAILED", None, 1000 + i);
        }
        assert_eq!(b.entries().len(), 1);
        assert_eq!(b.snapshot().last_error.unwrap().repeat_count, 1000);
        for i in 0..60 {
            b.record_at(
                "info",
                "HOST_SETTINGS_UPDATED",
                Some(&format!("{i}")),
                2000 + i,
            );
        }
        assert_eq!(b.entries().len(), 40);
        assert_eq!(
            b.snapshot().last_display_restore_result.unwrap().code,
            "DISPLAY_RESTORE_FAILED"
        );
        b.push(
            "error",
            "DISPLAY_RESTORE_FAILED",
            "pair_token=secret SDP=candidate:private",
        );
        assert!(b
            .entries()
            .iter()
            .all(|e| !e.message.contains("secret") && !e.message.contains("candidate:")));
    }
    #[test]
    fn recovery_requires_all_sources_and_ignores_old_sessions() {
        let mut b = DiagnosticBuffer::default();
        b.begin_session("one");
        b.health(
            Some("one"),
            "transport",
            false,
            "MEDIA_CONNECTION_INTERRUPTED",
        );
        b.health(Some("one"), "video", false, "MEDIA_STREAM_FAILED");
        b.health(Some("one"), "transport", true, "MEDIA_CONNECTION_RECOVERED");
        assert!(b.snapshot().degraded);
        b.health(Some("one"), "video", true, "MEDIA_STREAM_RECOVERED");
        assert!(!b.snapshot().degraded);
        b.end_session("one");
        b.begin_session("two");
        b.health(
            Some("one"),
            "transport",
            false,
            "MEDIA_CONNECTION_INTERRUPTED",
        );
        assert!(!b.snapshot().degraded);
    }
}

use crate::events::SharedStateSignal;
use crate::pairing::now_ms;
use std::sync::mpsc::{self, Receiver, Sender};
use std::sync::{Arc, Mutex};
use std::thread;
use std::time::Duration;

const DEFAULT_PENDING_REQUEST_TTL_MS: i64 = 39_000;

#[derive(Debug, Clone)]
pub struct PendingRemoteRequestSnapshot {
    pub request_id: String,
    pub controller_device_id: String,
    pub controller_display_name: String,
    pub connection_method: String,
    pub requested_at_ms: i64,
    pub expires_at_ms: i64,
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn approval_requires_exact_current_request_and_cannot_replace_it() {
        let mut store = PendingRemoteRequestStore::default();
        let (first, response) = store
            .begin_request("a".into(), "Phone A".into(), "TRUST_ONLY".into())
            .unwrap();
        assert!(store
            .begin_request("b".into(), "Phone B".into(), "TRUST_ONLY".into())
            .is_none());
        assert!(!store.respond("stale-id", true));
        assert!(matches!(
            response.try_recv(),
            Err(mpsc::TryRecvError::Empty)
        ));
        assert!(store.respond(&first, false));
        assert!(!response.recv().unwrap());
        let (second, response) = store
            .begin_request("b".into(), "Phone B".into(), "TRUST_ONLY".into())
            .unwrap();
        assert!(!store.respond(&first, true));
        assert!(store.respond(&second, true));
        assert!(response.recv().unwrap());
    }

    #[test]
    fn expired_request_cannot_be_approved() {
        let mut store = PendingRemoteRequestStore::default();
        let (id, response) = store
            .begin_request("a".into(), "Phone".into(), "TRUST_ONLY".into())
            .unwrap();
        store.current.as_mut().unwrap().snapshot.expires_at_ms = now_ms() - 1;
        assert!(!store.respond(&id, true));
        assert!(response.recv().is_err());
    }
}

#[derive(Debug)]
struct PendingRemoteRequestEntry {
    snapshot: PendingRemoteRequestSnapshot,
    response_tx: Sender<bool>,
}

#[derive(Debug, Default)]
pub struct PendingRemoteRequestStore {
    current: Option<PendingRemoteRequestEntry>,
    signal: Option<SharedStateSignal>,
}

pub type SharedPendingRemoteRequestStore = Arc<Mutex<PendingRemoteRequestStore>>;

impl PendingRemoteRequestStore {
    pub fn set_signal(&mut self, signal: SharedStateSignal) {
        self.signal = Some(signal);
    }

    pub fn begin_request(
        &mut self,
        controller_device_id: String,
        controller_display_name: String,
        connection_method: String,
    ) -> Option<(String, Receiver<bool>)> {
        self.clear_expired();
        if self.current.is_some() {
            return None;
        }
        let now = now_ms();
        let snapshot = PendingRemoteRequestSnapshot {
            request_id: format!("pending-{}", crate::pairing::generate_pair_token()),
            controller_device_id,
            controller_display_name,
            connection_method,
            requested_at_ms: now,
            expires_at_ms: now + DEFAULT_PENDING_REQUEST_TTL_MS,
        };
        let (response_tx, response_rx) = mpsc::channel();
        let request_id = snapshot.request_id.clone();
        self.current = Some(PendingRemoteRequestEntry {
            snapshot,
            response_tx,
        });
        if let Some(signal) = self.signal.as_ref() {
            signal.notify();
            let signal = Arc::clone(signal);
            thread::spawn(move || {
                thread::sleep(Duration::from_millis(DEFAULT_PENDING_REQUEST_TTL_MS as u64));
                signal.notify();
            });
        }
        Some((request_id, response_rx))
    }

    pub fn snapshot(&self) -> Option<PendingRemoteRequestSnapshot> {
        self.current.as_ref().map(|entry| entry.snapshot.clone())
    }

    pub fn respond(&mut self, request_id: &str, allow: bool) -> bool {
        self.clear_expired();
        if !self
            .current
            .as_ref()
            .is_some_and(|entry| entry.snapshot.request_id == request_id)
        {
            return false;
        }
        let Some(entry) = self.current.take() else {
            return false;
        };
        let _ = entry.response_tx.send(allow);
        if let Some(signal) = self.signal.as_ref() {
            signal.notify();
        }
        true
    }

    pub fn cancel(&mut self, request_id: &str) {
        let _ = self.respond(request_id, false);
    }

    pub fn clear_expired(&mut self) {
        let should_clear = self
            .current
            .as_ref()
            .map(|entry| entry.snapshot.expires_at_ms <= now_ms())
            .unwrap_or(false);
        if should_clear {
            let _ = self.current.take();
            if let Some(signal) = self.signal.as_ref() {
                signal.notify();
            }
        }
    }
}

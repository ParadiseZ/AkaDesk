use crate::pairing::now_ms;
use std::sync::mpsc::{self, Receiver, Sender};
use std::sync::{Arc, Mutex};

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

#[derive(Debug)]
struct PendingRemoteRequestEntry {
    snapshot: PendingRemoteRequestSnapshot,
    response_tx: Sender<bool>,
}

#[derive(Debug, Default)]
pub struct PendingRemoteRequestStore {
    current: Option<PendingRemoteRequestEntry>,
}

pub type SharedPendingRemoteRequestStore = Arc<Mutex<PendingRemoteRequestStore>>;

impl PendingRemoteRequestStore {
    pub fn begin_request(
        &mut self,
        controller_device_id: String,
        controller_display_name: String,
        connection_method: String,
    ) -> Receiver<bool> {
        if let Some(previous) = self.current.take() {
            let _ = previous.response_tx.send(false);
        }
        let now = now_ms();
        let snapshot = PendingRemoteRequestSnapshot {
            request_id: format!("pending-{}-{controller_device_id}", now),
            controller_device_id,
            controller_display_name,
            connection_method,
            requested_at_ms: now,
            expires_at_ms: now + DEFAULT_PENDING_REQUEST_TTL_MS,
        };
        let (response_tx, response_rx) = mpsc::channel();
        self.current = Some(PendingRemoteRequestEntry {
            snapshot,
            response_tx,
        });
        response_rx
    }

    pub fn snapshot(&self) -> Option<PendingRemoteRequestSnapshot> {
        self.current.as_ref().map(|entry| entry.snapshot.clone())
    }

    pub fn respond(&mut self, allow: bool) -> bool {
        let Some(entry) = self.current.take() else {
            return false;
        };
        let _ = entry.response_tx.send(allow);
        true
    }

    pub fn clear_expired(&mut self) {
        let should_clear = self
            .current
            .as_ref()
            .map(|entry| entry.snapshot.expires_at_ms <= now_ms())
            .unwrap_or(false);
        if should_clear {
            let _ = self.current.take();
        }
    }
}

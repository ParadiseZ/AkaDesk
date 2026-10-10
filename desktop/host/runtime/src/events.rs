use std::collections::HashMap;
use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::mpsc::{self, Receiver, SyncSender, TrySendError};
use std::sync::{Arc, Mutex, Weak};
use std::time::Duration;

#[derive(Debug, Default)]
pub struct StateSignal {
    listeners: Mutex<HashMap<u64, SyncSender<()>>>,
    next_id: AtomicU64,
}

pub type SharedStateSignal = Arc<StateSignal>;

impl StateSignal {
    pub fn subscribe(self: &Arc<Self>) -> StateSubscription {
        let (tx, rx) = mpsc::sync_channel(1);
        let id = self.next_id.fetch_add(1, Ordering::Relaxed);
        if let Ok(mut listeners) = self.listeners.lock() {
            listeners.insert(id, tx);
        }
        StateSubscription {
            id,
            signal: Arc::downgrade(self),
            rx,
        }
    }

    pub fn notify(&self) {
        if let Ok(mut listeners) = self.listeners.lock() {
            listeners.retain(|_, listener| match listener.try_send(()) {
                Ok(()) | Err(TrySendError::Full(())) => true,
                Err(TrySendError::Disconnected(())) => false,
            });
        }
    }
}

pub struct StateSubscription {
    id: u64,
    signal: Weak<StateSignal>,
    rx: Receiver<()>,
}

impl StateSubscription {
    pub fn recv_timeout(&self, timeout: Duration) -> Result<(), mpsc::RecvTimeoutError> {
        self.rx.recv_timeout(timeout)
    }
}

impl Drop for StateSubscription {
    fn drop(&mut self) {
        if let Some(signal) = self.signal.upgrade() {
            if let Ok(mut listeners) = signal.listeners.lock() {
                listeners.remove(&self.id);
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn subscription_is_removed_even_when_runtime_is_idle() {
        let signal = Arc::new(StateSignal::default());
        let subscription = signal.subscribe();
        for _ in 0..100 {
            signal.notify();
        }
        assert!(subscription.recv_timeout(Duration::from_millis(1)).is_ok());
        assert!(subscription.recv_timeout(Duration::from_millis(1)).is_err());
        drop(subscription);
        assert!(signal.listeners.lock().unwrap().is_empty());
    }
}

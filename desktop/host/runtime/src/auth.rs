use std::collections::HashMap;

const MAX_FAILURES: u32 = 5;
const WINDOW_MS: i64 = 60_000;

#[derive(Debug, Default)]
pub struct PairingAttemptLimiter {
    failures: HashMap<String, (i64, u32)>,
}

impl PairingAttemptLimiter {
    pub fn permits(&mut self, peer: &str, now: i64) -> bool {
        self.failures
            .retain(|_, (started, _)| now - *started < WINDOW_MS);
        self.failures
            .get(peer)
            .map_or(self.failures.len() < 4096, |(_, count)| {
                *count < MAX_FAILURES
            })
    }

    pub fn record_failure(&mut self, peer: &str, now: i64) {
        self.permits(peer, now);
        // Refuse additional unknown sources if the bounded table is full.
        if self.failures.len() >= 4096 && !self.failures.contains_key(peer) {
            return;
        }
        let entry = self.failures.entry(peer.to_owned()).or_insert((now, 0));
        entry.1 = entry.1.saturating_add(1);
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn repeated_failures_are_limited_by_source_and_expire() {
        let mut limiter = PairingAttemptLimiter::default();
        for _ in 0..MAX_FAILURES {
            assert!(limiter.permits("192.0.2.1", 100));
            limiter.record_failure("192.0.2.1", 100);
        }
        assert!(!limiter.permits("192.0.2.1", 101));
        assert!(limiter.permits("192.0.2.2", 101));
        assert!(limiter.permits("192.0.2.1", 100 + WINDOW_MS));
    }
}

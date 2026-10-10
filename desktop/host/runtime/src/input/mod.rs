use crate::generated::v1::{
    input_event::Payload, ErrorInfo, InputEvent, InputResult, SessionState,
};
use crate::session::SharedSessionRegistry;
use host_contracts::input::*;
use prost::Message;
use std::sync::{Arc, Mutex};

pub trait InputSink: Send + std::fmt::Debug {
    fn prepare(&self, id: &str) -> Result<(), String>;
    fn inject(&self, payload: &Payload) -> Result<(), String>;
    fn release_held(&self) -> Result<(), String>;
    fn finish(&self, id: &str) -> Result<(), String>;
}

#[cfg(windows)]
impl InputSink for host_windows::input::WindowsInputBackend {
    fn prepare(&self, id: &str) -> Result<(), String> {
        self.prepare(id).map_err(|e| e.to_string())
    }
    fn inject(&self, payload: &Payload) -> Result<(), String> {
        match payload {
            Payload::Pointer(p) => self.pointer(PointerInputCommand {
                normalized_x: p.normalized_x,
                normalized_y: p.normalized_y,
                button: p.button as u8,
                pressed: p.pressed,
                timestamp_ms: p.timestamp_ms,
            }),
            Payload::Scroll(p) => self.scroll(ScrollInputCommand {
                delta_x: p.delta_x,
                delta_y: p.delta_y,
                timestamp_ms: p.timestamp_ms,
            }),
            Payload::Key(p) => self.keyboard(KeyInputCommand {
                key_code: p.key_code,
                action: p.action as u8,
                alt: p.alt,
                ctrl: p.ctrl,
                shift: p.shift,
                meta: p.meta,
                timestamp_ms: p.timestamp_ms,
            }),
            Payload::Text(p) => self.text(TextInputCommand {
                text: p.text.clone(),
                timestamp_ms: p.timestamp_ms,
            }),
            Payload::Release(_) => self.release_held(),
        }
        .map_err(|e| e.to_string())
    }
    fn release_held(&self) -> Result<(), String> {
        self.release_held().map_err(|e| e.to_string())
    }
    fn finish(&self, id: &str) -> Result<(), String> {
        self.release(id).map_err(|e| e.to_string())
    }
}

#[derive(Debug)]
pub struct InputState {
    sink: Option<Box<dyn InputSink>>,
    active: Option<String>,
    seq: u64,
    failed: bool,
}
pub type SharedInputState = Arc<Mutex<InputState>>;
impl Default for InputState {
    fn default() -> Self {
        Self {
            #[cfg(windows)]
            sink: Some(Box::new(host_windows::input::WindowsInputBackend::default())),
            #[cfg(not(windows))]
            sink: None,
            active: None,
            seq: 0,
            failed: false,
        }
    }
}
impl InputState {
    pub fn with_sink(sink: Box<dyn InputSink>) -> Self {
        Self {
            sink: Some(sink),
            active: None,
            seq: 0,
            failed: false,
        }
    }
    pub fn prepare(&mut self, id: &str) -> Result<(), String> {
        if self.active.is_some() {
            return Err("Input is already bound to a session".into());
        }
        self.sink
            .as_ref()
            .ok_or("Input is unavailable on this platform")?
            .prepare(id)?;
        self.active = Some(id.into());
        self.seq = 0;
        self.failed = false;
        Ok(())
    }
    pub fn release_held(&mut self, id: &str) -> Result<(), String> {
        if self.active.as_deref() == Some(id) {
            self.sink.as_ref().unwrap().release_held()?;
        }
        Ok(())
    }
    pub fn finish(&mut self, id: &str) -> Result<(), String> {
        if self.active.as_deref() == Some(id) {
            self.sink.as_ref().unwrap().finish(id)?;
            self.active = None;
        }
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::generated::v1::{KeyEvent, PointerEvent, ReleaseInputEvent, VideoSessionSpec};
    use crate::session::SessionRegistry;
    #[derive(Debug)]
    struct Mock {
        calls: Arc<Mutex<Vec<String>>>,
        fail: bool,
    }
    impl InputSink for Mock {
        fn prepare(&self, id: &str) -> Result<(), String> {
            self.calls.lock().unwrap().push(format!("prepare:{id}"));
            Ok(())
        }
        fn inject(&self, p: &Payload) -> Result<(), String> {
            self.calls
                .lock()
                .unwrap()
                .push(format!("inject:{}", sequence(p)));
            if self.fail {
                Err("native failure".into())
            } else {
                Ok(())
            }
        }
        fn release_held(&self) -> Result<(), String> {
            self.calls.lock().unwrap().push("release".into());
            Ok(())
        }
        fn finish(&self, id: &str) -> Result<(), String> {
            self.calls.lock().unwrap().push(format!("finish:{id}"));
            Ok(())
        }
    }
    fn fixture(
        fail: bool,
    ) -> (
        SharedSessionRegistry,
        SharedInputState,
        String,
        Arc<Mutex<Vec<String>>>,
    ) {
        let calls = Arc::new(Mutex::new(Vec::new()));
        let mut registry = SessionRegistry::default();
        let id = registry
            .activate_preparing(
                "client".into(),
                "controller".into(),
                "host".into(),
                String::new(),
                VideoSessionSpec::default(),
            )
            .unwrap()
            .session_id;
        registry.active.as_mut().unwrap().input_enabled = true;
        registry.mark_streaming(&id);
        let mut input = InputState::with_sink(Box::new(Mock {
            calls: calls.clone(),
            fail,
        }));
        input.prepare(&id).unwrap();
        (
            Arc::new(Mutex::new(registry)),
            Arc::new(Mutex::new(input)),
            id,
            calls,
        )
    }
    fn packet(id: &str, seq: u64) -> Vec<u8> {
        InputEvent {
            session_id: id.into(),
            payload: Some(Payload::Key(KeyEvent {
                seq,
                key_code: 0x41,
                action: 1,
                ..Default::default()
            })),
        }
        .encode_to_vec()
    }
    #[test]
    fn only_active_enabled_bound_session_can_inject() {
        let (sessions, input, id, calls) = fixture(false);
        assert!(process(&sessions, &input, &id, &packet("foreign", 1))
            .error
            .is_some());
        sessions
            .lock()
            .unwrap()
            .active
            .as_mut()
            .unwrap()
            .input_enabled = false;
        assert!(process(&sessions, &input, &id, &packet(&id, 1))
            .error
            .is_some());
        sessions
            .lock()
            .unwrap()
            .active
            .as_mut()
            .unwrap()
            .input_enabled = true;
        assert!(process(&sessions, &input, &id, &packet(&id, 1))
            .error
            .is_none());
        sessions.lock().unwrap().mark_disconnected(&id);
        assert!(process(&sessions, &input, &id, &packet(&id, 2))
            .error
            .is_some());
        assert_eq!(
            calls
                .lock()
                .unwrap()
                .iter()
                .filter(|v| v.starts_with("inject"))
                .count(),
            1
        );
    }
    #[test]
    fn replay_invalid_numbers_unknown_enums_and_oversized_packets_are_rejected() {
        let (sessions, input, id, calls) = fixture(false);
        assert!(process(&sessions, &input, &id, &packet(&id, 0))
            .error
            .is_some());
        let invalid = InputEvent {
            session_id: id.clone(),
            payload: Some(Payload::Pointer(PointerEvent {
                seq: 1,
                normalized_x: f32::NAN,
                ..Default::default()
            })),
        };
        assert!(process(&sessions, &input, &id, &invalid.encode_to_vec())
            .error
            .is_some());
        let invalid = InputEvent {
            session_id: id.clone(),
            payload: Some(Payload::Key(KeyEvent {
                seq: 1,
                key_code: 0x41,
                action: 257,
                ..Default::default()
            })),
        };
        assert!(process(&sessions, &input, &id, &invalid.encode_to_vec())
            .error
            .is_some());
        assert!(process(&sessions, &input, &id, &vec![0; 16_385])
            .error
            .is_some());
        assert!(process(&sessions, &input, &id, &packet(&id, 1))
            .error
            .is_none());
        assert!(process(&sessions, &input, &id, &packet(&id, 1))
            .error
            .is_some());
        assert_eq!(calls.lock().unwrap().len(), 2);
    }
    #[test]
    fn native_failure_releases_keys_and_blocks_subsequent_injection() {
        let (sessions, input, id, calls) = fixture(true);
        assert!(process(&sessions, &input, &id, &packet(&id, 1))
            .error
            .is_some());
        assert!(process(&sessions, &input, &id, &packet(&id, 2))
            .error
            .is_some());
        assert_eq!(
            &*calls.lock().unwrap(),
            &[format!("prepare:{id}"), "inject:1".into(), "release".into()]
        );
    }
    #[test]
    fn late_cleanup_cannot_release_a_new_session_and_release_uses_ordered_sequence() {
        let (sessions, input, id, calls) = fixture(false);
        let release = InputEvent {
            session_id: id.clone(),
            payload: Some(Payload::Release(ReleaseInputEvent {
                seq: 1,
                timestamp_ms: 0,
            })),
        };
        assert!(process(&sessions, &input, &id, &release.encode_to_vec())
            .error
            .is_none());
        input.lock().unwrap().finish("old").unwrap();
        input.lock().unwrap().finish(&id).unwrap();
        input.lock().unwrap().prepare("new").unwrap();
        input.lock().unwrap().release_held(&id).unwrap();
        input.lock().unwrap().finish(&id).unwrap();
        assert_eq!(input.lock().unwrap().active.as_deref(), Some("new"));
        assert_eq!(calls.lock().unwrap().len(), 4);
    }
}
pub fn sequence(p: &Payload) -> u64 {
    match p {
        Payload::Pointer(p) => p.seq,
        Payload::Scroll(p) => p.seq,
        Payload::Key(p) => p.seq,
        Payload::Text(p) => p.seq,
        Payload::Release(p) => p.seq,
    }
}
fn validate(p: &Payload) -> bool {
    match p {
        Payload::Pointer(p) => {
            p.normalized_x.is_finite()
                && p.normalized_y.is_finite()
                && (0.0..=1.0).contains(&p.normalized_x)
                && (0.0..=1.0).contains(&p.normalized_y)
                && (0..=3).contains(&p.button)
                && !(p.button == 0 && p.pressed)
        }
        Payload::Scroll(p) => [p.delta_x, p.delta_y]
            .iter()
            .all(|v| v.is_finite() && v.abs() <= 10.0),
        Payload::Key(p) => (8..=254).contains(&p.key_code) && [1, 2].contains(&p.action),
        Payload::Text(p) => !p.text.is_empty() && p.text.len() <= 4096 && !p.text.contains('\0'),
        Payload::Release(_) => true,
    }
}
// Keep the session lock until injection completes: a stopped/replaced session cannot inject late.
// Cleanup uses the same session -> input lock order, with no async work under either lock.
pub fn process(
    sessions: &SharedSessionRegistry,
    input: &SharedInputState,
    bound_id: &str,
    bytes: &[u8],
) -> InputResult {
    let mut seq = 0;
    let result = (|| -> Result<(), String> {
        if bytes.len() > 16_384 {
            return Err("Input message is too large".into());
        }
        let event = InputEvent::decode(bytes).map_err(|e| e.to_string())?;
        let payload = event.payload.ok_or("Input payload is missing")?;
        seq = sequence(&payload);
        let registry = sessions.lock().map_err(|_| "Session lock poisoned")?;
        let allowed = registry.active.as_ref().is_some_and(|s| {
            s.session_id.0 == bound_id
                && event.session_id == bound_id
                && s.input_enabled
                && s.state == SessionState::Streaming as i32
                && registry.should_keep_running(bound_id)
        });
        if !allowed {
            return Err("Input session is not active or authorized".into());
        }
        let mut state = input.lock().map_err(|_| "Input lock poisoned")?;
        if state.active.as_deref() != Some(bound_id) || state.failed {
            return Err("Input channel is unavailable".into());
        }
        if seq == 0 || seq <= state.seq {
            return Err("Input sequence is stale".into());
        }
        if !validate(&payload) {
            return Err("Invalid input payload".into());
        }
        state.seq = seq;
        if let Err(error) = state.sink.as_ref().unwrap().inject(&payload) {
            state.failed = true;
            let cleanup = state.release_held(bound_id);
            return Err(format!("{error}; release: {cleanup:?}"));
        }
        Ok(())
    })();
    InputResult {
        session_id: bound_id.into(),
        seq,
        error: result.err().map(|message| ErrorInfo {
            code: "INPUT_REJECTED".into(),
            message,
            retryable: false,
            detail: String::new(),
        }),
    }
}

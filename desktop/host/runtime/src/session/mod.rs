use crate::events::SharedStateSignal;
use crate::generated::v1::{AudioSessionSpec, SessionSnapshot, SessionState, VideoSessionSpec};
use crate::pairing::{generate_pair_token, now_ms};
use host_contracts::common::SessionId;
use std::sync::{Arc, Mutex};

#[derive(Debug, Clone, Default)]
pub struct SessionContext {
    pub session_id: SessionId,
    pub controller_device_id: String,
    pub controlled_device_id: String,
    pub target_display_id: Option<String>,
    pub state: i32,
    pub video: Option<VideoSessionSpec>,
    pub requested_video: Option<VideoSessionSpec>,
    pub video_change_error: String,
    pub audio: Option<AudioSessionSpec>,
    pub input_enabled: bool,
    pub view_only: bool,
    pub history_display_mode: Option<crate::generated::v1::DisplayMode>,
    pub requested_display_mode: Option<crate::generated::v1::DisplayMode>,
    pub mute_controlled_local_playback: bool,
    pub created_at_ms: i64,
    pub negotiating_started_at_ms: i64,
    pub preparing_started_at_ms: i64,
    pub streaming_started_at_ms: i64,
    pub disconnecting_started_at_ms: i64,
    pub updated_at_ms: i64,
    pub closed_at_ms: i64,
    control_connection_id: Option<String>,
    control_last_seen_ms: i64,
}

#[derive(Debug, Default)]
pub struct SessionRegistry {
    pub active: Option<SessionContext>,
    pub history_count: usize,
    signal: Option<SharedStateSignal>,
    diagnostics: Option<crate::diagnostics::SharedDiagnosticBuffer>,
}

pub type SharedSessionRegistry = Arc<Mutex<SessionRegistry>>;

impl SessionContext {
    pub fn snapshot(&self) -> SessionSnapshot {
        SessionSnapshot {
            session_id: self.session_id.0.clone(),
            state: self.state,
            controller_device_id: self.controller_device_id.clone(),
            controlled_device_id: self.controlled_device_id.clone(),
            target_display_id: self.target_display_id.clone().unwrap_or_default(),
            video: self.video.clone(),
            audio: self.audio.clone(),
            input_enabled: self.input_enabled,
            history_display_mode: self.history_display_mode.clone(),
            requested_display_mode: self.requested_display_mode.clone(),
            created_at_ms: self.created_at_ms,
            negotiating_started_at_ms: self.negotiating_started_at_ms,
            preparing_started_at_ms: self.preparing_started_at_ms,
            streaming_started_at_ms: self.streaming_started_at_ms,
            disconnecting_started_at_ms: self.disconnecting_started_at_ms,
            updated_at_ms: self.updated_at_ms,
            closed_at_ms: self.closed_at_ms,
            mute_controlled_local_playback: self.mute_controlled_local_playback,
        }
    }
}

impl SessionRegistry {
    pub fn attach_control(&mut self, id: &str, connection: &str) {
        if !self.should_keep_running(id) {
            return;
        }
        let s = self.active.as_mut().unwrap();
        s.control_connection_id = Some(connection.into());
        s.control_last_seen_ms = now_ms();
        if let Some(d) = &self.diagnostics {
            if let Ok(mut d) = d.lock() {
                d.health(Some(id), "control", true, "SESSION_CONTROL_RECOVERED");
            }
        }
    }
    pub fn lose_control(&mut self, controller: &str, connection: &str) -> Option<String> {
        let s = self.active.as_mut()?;
        if s.closed_at_ms > 0
            || s.controller_device_id != controller
            || s.control_connection_id.as_deref() != Some(connection)
        {
            return None;
        }
        s.control_connection_id = None;
        // Keep media for a bounded reconnect window; never extend it on repeated drops.
        if let Some(d) = &self.diagnostics {
            if let Ok(mut d) = d.lock() {
                d.health(
                    Some(&s.session_id.0),
                    "control",
                    false,
                    "SESSION_CONTROL_INTERRUPTED",
                );
            }
        }
        Some(s.session_id.0.clone())
    }
    pub fn touch_control(&mut self, controller: &str, connection: &str) {
        if let Some(s) = &mut self.active {
            if s.controller_device_id == controller
                && s.control_connection_id.as_deref() == Some(connection)
            {
                s.control_last_seen_ms = now_ms();
            }
        }
    }
    pub fn expire_control_at(&mut self, now: i64) {
        let expired = self
            .active
            .as_ref()
            .filter(|s| {
                s.closed_at_ms == 0
                    && s.control_last_seen_ms > 0
                    && now.saturating_sub(s.control_last_seen_ms) > 30_000
            })
            .map(|s| s.session_id.0.clone());
        if let Some(id) = expired {
            if let Some(d) = &self.diagnostics {
                if let Ok(mut d) = d.lock() {
                    d.record("error", "SESSION_CONTROL_TIMEOUT", Some(&id));
                }
            }
            self.mark_failed(&id);
        }
    }
    pub fn set_diagnostics(&mut self, diagnostics: crate::diagnostics::SharedDiagnosticBuffer) {
        self.diagnostics = Some(diagnostics);
    }
    pub fn set_signal(&mut self, signal: SharedStateSignal) {
        self.signal = Some(signal);
    }

    fn notify(&self) {
        if let Some(signal) = self.signal.as_ref() {
            signal.notify();
        }
    }

    pub fn active_session_id(&self) -> Option<&SessionId> {
        self.active.as_ref().map(|session| &session.session_id)
    }

    pub fn active_snapshot_for_controller(
        &self,
        controller_device_id: &str,
    ) -> Option<SessionSnapshot> {
        let session = self.active.as_ref()?;
        if session.controller_device_id != controller_device_id {
            return None;
        }
        if session.state == SessionState::Disconnecting as i32
            || session.state == SessionState::Disconnected as i32
            || session.state == SessionState::Failed as i32
        {
            return None;
        }
        Some(session.snapshot())
    }

    pub fn activate_preparing(
        &mut self,
        session_id: String,
        controller_device_id: String,
        controlled_device_id: String,
        target_display_id: String,
        video: VideoSessionSpec,
    ) -> Option<SessionSnapshot> {
        if self
            .active
            .as_ref()
            .is_some_and(|session| self.should_keep_running(&session.session_id.0))
        {
            return None;
        }
        if self.active.is_some() {
            self.history_count += 1;
        }

        let now = now_ms();
        // A client-supplied ID must not revive an earlier session's late callbacks.
        let _ = session_id;
        let next_session_id = format!("session-{}", generate_pair_token());
        let context = SessionContext {
            session_id: SessionId(next_session_id),
            controller_device_id,
            controlled_device_id,
            target_display_id: (!target_display_id.trim().is_empty()).then_some(target_display_id),
            state: SessionState::Preparing as i32,
            video: Some(video),
            requested_video: None,
            video_change_error: String::new(),
            audio: None,
            input_enabled: false,
            view_only: false,
            history_display_mode: None,
            requested_display_mode: None,
            mute_controlled_local_playback: false,
            created_at_ms: now,
            negotiating_started_at_ms: now,
            preparing_started_at_ms: now,
            streaming_started_at_ms: 0,
            disconnecting_started_at_ms: 0,
            updated_at_ms: now,
            closed_at_ms: 0,
            control_connection_id: None,
            control_last_seen_ms: 0,
        };
        let snapshot = context.snapshot();
        self.active = Some(context);
        if let Some(diagnostics) = &self.diagnostics {
            if let Ok(mut d) = diagnostics.lock() {
                d.begin_session(&snapshot.session_id);
            }
        }
        self.notify();
        Some(snapshot)
    }

    pub fn should_keep_running(&self, session_id: &str) -> bool {
        self.active
            .as_ref()
            .map(|session| {
                session.session_id.0 == session_id
                    && session.state != SessionState::Disconnecting as i32
                    && session.state != SessionState::Disconnected as i32
                    && session.state != SessionState::Failed as i32
            })
            .unwrap_or(false)
    }

    pub fn mark_streaming(&mut self, session_id: &str) -> Option<SessionSnapshot> {
        if !self.should_keep_running(session_id) {
            return None;
        }
        let now = now_ms();
        let session = self.active.as_mut()?;
        if session.session_id.0 != session_id {
            return None;
        }
        let changed = session.state != SessionState::Streaming as i32;
        session.state = SessionState::Streaming as i32;
        if session.streaming_started_at_ms == 0 {
            session.streaming_started_at_ms = now;
        }
        session.updated_at_ms = now;
        let snapshot = session.snapshot();
        if changed {
            if let Some(diagnostics) = &self.diagnostics {
                if let Ok(mut d) = diagnostics.lock() {
                    d.record("info", "SESSION_STREAMING", Some(session_id));
                }
            }
        }
        self.notify();
        Some(snapshot)
    }

    pub fn set_audio(&mut self, session_id: &str, audio: AudioSessionSpec, muted: bool) -> bool {
        if !self.should_keep_running(session_id) {
            return false;
        }
        let session = self.active.as_mut().unwrap();
        session.audio = Some(audio);
        session.mute_controlled_local_playback = muted;
        session.updated_at_ms = now_ms();
        self.notify();
        true
    }
    pub fn set_audio_muted(&mut self, session_id: &str, muted: bool) -> bool {
        if !self.should_keep_running(session_id) {
            return false;
        }
        let session = self.active.as_mut().unwrap();
        if session.audio.is_none() {
            return false;
        }
        session.mute_controlled_local_playback = muted;
        session.updated_at_ms = now_ms();
        self.notify();
        true
    }

    pub fn touch_active(&mut self, session_id: &str) -> bool {
        if !self.should_keep_running(session_id) {
            return false;
        }
        let Some(session) = self.active.as_mut() else {
            return false;
        };
        if session.session_id.0 != session_id {
            return false;
        }
        session.updated_at_ms = now_ms();
        true
    }

    pub fn mark_failed(&mut self, session_id: &str) -> Option<SessionSnapshot> {
        if !self.should_keep_running(session_id) {
            return None;
        }
        let now = now_ms();
        let session = self.active.as_mut()?;
        if session.session_id.0 != session_id {
            return None;
        }
        session.state = SessionState::Failed as i32;
        session.updated_at_ms = now;
        session.closed_at_ms = now;
        let snapshot = session.snapshot();
        if let Some(diagnostics) = &self.diagnostics {
            if let Ok(mut d) = diagnostics.lock() {
                d.record("error", "SESSION_FAILED", Some(session_id));
            }
        }
        self.notify();
        Some(snapshot)
    }

    pub fn mark_disconnected(&mut self, session_id: &str) -> Option<SessionSnapshot> {
        if !self.should_keep_running(session_id) {
            return None;
        }
        let now = now_ms();
        let session = self.active.as_mut()?;
        if session.session_id.0 != session_id {
            return None;
        }
        if session.disconnecting_started_at_ms == 0 {
            session.disconnecting_started_at_ms = now;
        }
        session.state = SessionState::Disconnected as i32;
        session.updated_at_ms = now;
        session.closed_at_ms = now;
        let snapshot = session.snapshot();
        self.notify();
        Some(snapshot)
    }
}

#[cfg(test)]
mod control_recovery_tests {
    use super::*;
    #[test]
    fn old_connection_cannot_drop_replacement_and_missing_heartbeats_expire() {
        let diagnostics = Arc::new(Mutex::new(crate::diagnostics::DiagnosticBuffer::default()));
        let mut r = SessionRegistry::default();
        r.set_diagnostics(diagnostics.clone());
        let id = r
            .activate_preparing(
                "".into(),
                "owner".into(),
                "host".into(),
                "".into(),
                Default::default(),
            )
            .unwrap()
            .session_id;
        r.attach_control(&id, "first");
        r.mark_streaming(&id);
        assert!(r.lose_control("owner", "first").is_some());
        assert!(diagnostics.lock().unwrap().snapshot().degraded);
        r.attach_control(&id, "replacement");
        assert!(r.lose_control("owner", "first").is_none());
        assert!(!diagnostics.lock().unwrap().snapshot().degraded);
        let last = r.active.as_ref().unwrap().control_last_seen_ms;
        r.expire_control_at(last + 30_000);
        assert!(r.should_keep_running(&id));
        r.expire_control_at(last + 30_001);
        assert!(!r.should_keep_running(&id));
        assert!(diagnostics
            .lock()
            .unwrap()
            .entries()
            .iter()
            .any(|e| e.code == "SESSION_CONTROL_TIMEOUT"));
    }
}

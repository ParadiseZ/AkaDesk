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
    pub audio: Option<AudioSessionSpec>,
    pub created_at_ms: i64,
    pub negotiating_started_at_ms: i64,
    pub preparing_started_at_ms: i64,
    pub streaming_started_at_ms: i64,
    pub disconnecting_started_at_ms: i64,
    pub updated_at_ms: i64,
    pub closed_at_ms: i64,
}

#[derive(Debug, Default)]
pub struct SessionRegistry {
    pub active: Option<SessionContext>,
    pub history_count: usize,
    signal: Option<SharedStateSignal>,
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
            created_at_ms: self.created_at_ms,
            negotiating_started_at_ms: self.negotiating_started_at_ms,
            preparing_started_at_ms: self.preparing_started_at_ms,
            streaming_started_at_ms: self.streaming_started_at_ms,
            disconnecting_started_at_ms: self.disconnecting_started_at_ms,
            updated_at_ms: self.updated_at_ms,
            closed_at_ms: self.closed_at_ms,
        }
    }
}

impl SessionRegistry {
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
            audio: None,
            created_at_ms: now,
            negotiating_started_at_ms: now,
            preparing_started_at_ms: now,
            streaming_started_at_ms: 0,
            disconnecting_started_at_ms: 0,
            updated_at_ms: now,
            closed_at_ms: 0,
        };
        let snapshot = context.snapshot();
        self.active = Some(context);
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
        session.state = SessionState::Streaming as i32;
        if session.streaming_started_at_ms == 0 {
            session.streaming_started_at_ms = now;
        }
        session.updated_at_ms = now;
        let snapshot = session.snapshot();
        self.notify();
        Some(snapshot)
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

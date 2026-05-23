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
    pub fn active_session_id(&self) -> Option<&SessionId> {
        self.active.as_ref().map(|session| &session.session_id)
    }

    pub fn activate_preparing(
        &mut self,
        session_id: String,
        controller_device_id: String,
        controlled_device_id: String,
        target_display_id: String,
        video: VideoSessionSpec,
    ) -> SessionSnapshot {
        if self.active.is_some() {
            self.history_count += 1;
        }

        let now = now_ms();
        let next_session_id = if session_id.trim().is_empty() {
            format!("session-{}", generate_pair_token())
        } else {
            session_id
        };
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
        snapshot
    }
}

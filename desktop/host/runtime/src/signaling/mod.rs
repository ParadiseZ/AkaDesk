use crate::generated::v1::{IceCandidate, WebRtcAnswer, WebRtcOffer};
use std::collections::VecDeque;
use std::sync::{Arc, Mutex};

#[derive(Debug, Default)]
pub struct SignalingState {
    active_session_id: Option<String>,
    local_offer: Option<WebRtcOffer>,
    local_candidates: VecDeque<IceCandidate>,
    sent_local_offer: Option<WebRtcOffer>,
    sent_local_candidates: Vec<IceCandidate>,
    remote_answer: Option<WebRtcAnswer>,
    remote_candidates: Vec<IceCandidate>,
}

pub type SharedSignalingState = Arc<Mutex<SignalingState>>;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum RemoteAnswerStoreResult {
    InactiveSession,
    Stored,
    Duplicate,
    IgnoredReplacement,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum RemoteCandidateStoreResult {
    InactiveSession,
    Stored,
    Duplicate,
}

impl SignalingState {
    pub fn begin_session(&mut self, session_id: String) {
        self.active_session_id = Some(session_id);
        self.local_offer = None;
        self.local_candidates.clear();
        self.sent_local_offer = None;
        self.sent_local_candidates.clear();
        self.remote_answer = None;
        self.remote_candidates.clear();
    }

    pub fn finish_session(&mut self, session_id: &str) {
        if self.active_session_id.as_deref() != Some(session_id) {
            return;
        }
        self.active_session_id = None;
        self.local_offer = None;
        self.local_candidates.clear();
        self.sent_local_offer = None;
        self.sent_local_candidates.clear();
        self.remote_answer = None;
        self.remote_candidates.clear();
    }

    pub fn set_local_offer(&mut self, offer: WebRtcOffer) -> bool {
        if self.active_session_id.as_deref() != Some(offer.session_id.as_str()) {
            return false;
        }
        self.local_offer = Some(offer);
        true
    }

    pub fn take_local_offer(&mut self, session_id: &str) -> Option<WebRtcOffer> {
        if self.active_session_id.as_deref() != Some(session_id) {
            return None;
        }
        self.local_offer.take()
    }

    pub fn local_offer(&self, session_id: &str) -> Option<WebRtcOffer> {
        if self.active_session_id.as_deref() != Some(session_id) {
            return None;
        }
        self.local_offer.clone()
    }

    pub fn clear_local_offer(&mut self, session_id: &str, sent_offer: &WebRtcOffer) -> bool {
        if self.active_session_id.as_deref() != Some(session_id) {
            return false;
        }
        if self
            .local_offer
            .as_ref()
            .is_some_and(|current| current == sent_offer)
        {
            self.sent_local_offer = Some(sent_offer.clone());
            self.local_offer = None;
            return true;
        }
        false
    }

    pub fn push_local_candidate(&mut self, candidate: IceCandidate) -> bool {
        if self.active_session_id.as_deref() != Some(candidate.session_id.as_str()) {
            return false;
        }
        if self
            .local_candidates
            .iter()
            .any(|existing| same_candidate(existing, &candidate))
        {
            return true;
        }
        self.local_candidates.push_back(candidate);
        true
    }

    pub fn drain_local_candidates(&mut self, session_id: &str) -> Vec<IceCandidate> {
        if self.active_session_id.as_deref() != Some(session_id) {
            return Vec::new();
        }
        self.local_candidates.drain(..).collect()
    }

    pub fn local_candidates_snapshot(&self, session_id: &str) -> Vec<IceCandidate> {
        if self.active_session_id.as_deref() != Some(session_id) {
            return Vec::new();
        }
        self.local_candidates.iter().cloned().collect()
    }

    pub fn discard_local_candidates(&mut self, session_id: &str, count: usize) -> usize {
        if self.active_session_id.as_deref() != Some(session_id) {
            return 0;
        }
        let remove_count = count.min(self.local_candidates.len());
        self.local_candidates.drain(..remove_count);
        remove_count
    }

    pub fn local_candidate_count(&self, session_id: &str) -> usize {
        if self.active_session_id.as_deref() != Some(session_id) {
            return 0;
        }
        self.local_candidates.len()
    }

    pub fn mark_local_candidate_sent(&mut self, session_id: &str, candidate: &IceCandidate) -> bool {
        if self.active_session_id.as_deref() != Some(session_id) {
            return false;
        }
        if self
            .local_candidates
            .front()
            .is_some_and(|current| same_candidate(current, candidate))
        {
            self.local_candidates.pop_front();
            if !self
                .sent_local_candidates
                .iter()
                .any(|existing| same_candidate(existing, candidate))
            {
                self.sent_local_candidates.push(candidate.clone());
            }
            return true;
        }
        false
    }

    pub fn restore_replayable_local_signaling(&mut self, session_id: &str) -> bool {
        if self.active_session_id.as_deref() != Some(session_id) {
            return false;
        }
        let mut restored = false;
        if self.local_offer.is_none() {
            if let Some(offer) = self.sent_local_offer.clone() {
                self.local_offer = Some(offer);
                restored = true;
            }
        }
        for candidate in self.sent_local_candidates.iter().rev() {
            if self
                .local_candidates
                .iter()
                .any(|existing| same_candidate(existing, candidate))
            {
                continue;
            }
            self.local_candidates.push_front(candidate.clone());
            restored = true;
        }
        restored
    }

    pub fn set_remote_answer(&mut self, answer: WebRtcAnswer) -> RemoteAnswerStoreResult {
        if self.active_session_id.as_deref() != Some(answer.session_id.as_str()) {
            return RemoteAnswerStoreResult::InactiveSession;
        }
        if let Some(current) = self.remote_answer.as_ref() {
            if current == &answer {
                return RemoteAnswerStoreResult::Duplicate;
            }
            return RemoteAnswerStoreResult::IgnoredReplacement;
        }
        self.remote_answer = Some(answer);
        RemoteAnswerStoreResult::Stored
    }

    pub fn push_remote_candidate(&mut self, candidate: IceCandidate) -> RemoteCandidateStoreResult {
        if self.active_session_id.as_deref() != Some(candidate.session_id.as_str()) {
            return RemoteCandidateStoreResult::InactiveSession;
        }
        if self
            .remote_candidates
            .iter()
            .any(|existing| same_candidate(existing, &candidate))
        {
            return RemoteCandidateStoreResult::Duplicate;
        }
        self.remote_candidates.push(candidate);
        RemoteCandidateStoreResult::Stored
    }

    pub fn remote_answer(&self) -> Option<WebRtcAnswer> {
        self.remote_answer.clone()
    }

    pub fn remote_candidates(&self) -> Vec<IceCandidate> {
        self.remote_candidates.clone()
    }
}

fn same_candidate(left: &IceCandidate, right: &IceCandidate) -> bool {
    left.session_id == right.session_id
        && left.candidate == right.candidate
        && left.sdp_mid == right.sdp_mid
        && left.sdp_mline_index == right.sdp_mline_index
}

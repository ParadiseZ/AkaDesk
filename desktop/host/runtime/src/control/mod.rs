use crate::approval::SharedPendingRemoteRequestStore;
use crate::generated::v1::{
    CapabilityFlags, ControlEnvelope, ControlHello, ControlHelloAck, ControlMessageKind,
    DeviceIdentity, DevicePresence, DeviceSnapshot, DeviceSnapshotResponse, ErrorInfo,
    GetDeviceSnapshot, HeartbeatPing, HeartbeatPong, HostTransportStatusEvent,
    HostVideoStreamEvent, IceCandidate, PairingChallenge, PairingChallengeType, PairingConfirm,
    PairingRequest, PairingResult, PlatformKind, SessionChangedEvent, SessionClosedEvent,
    SessionState, StartSessionAccepted, StartSessionRejected, StartSessionRequest, WebRtcAnswer,
};
use crate::pairing::{generate_pair_token, now_ms, PendingChallenge, SharedPairingStore};
use crate::session::SharedSessionRegistry;
use crate::settings::{ConnectionMethod, HostSettingsStore};
use crate::signaling::{RemoteAnswerStoreResult, RemoteCandidateStoreResult, SharedSignalingState};
use crate::transport::SharedTransportStatusState;
use crate::video::SharedVideoStreamState;
use host_ipc::types::StopSessionRequest;
use prost::Message as ProstMessage;
use std::io;
use std::io::ErrorKind;
use std::net::{SocketAddr, TcpListener, TcpStream};
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::mpsc::{Receiver, RecvTimeoutError, SyncSender, TryRecvError};
use std::sync::{Arc, Mutex};
use std::thread;
use std::time::Duration;
use tungstenite::{accept, Error as WsError, Message as WsMessage};

pub const DEFAULT_CONTROL_PORT: u16 = 46661;

pub fn reap_finished_session(state: &ControlPlaneState) -> io::Result<()> {
    let sessions = state.sessions.lock().map_err(session_lock_err)?;
    let Some(snapshot) = sessions
        .active
        .as_ref()
        .filter(|session| session.closed_at_ms > 0)
        .map(|session| session.snapshot())
    else {
        return Ok(());
    };
    state
        .video_streams
        .lock()
        .map_err(video_stream_lock_err)?
        .finish_session(&snapshot.session_id);
    state
        .signaling
        .lock()
        .map_err(signaling_lock_err)?
        .finish_session(&snapshot.session_id);
    state
        .transport_status
        .lock()
        .map_err(transport_status_lock_err)?
        .finish_session(&snapshot.session_id);
    state
        .settings
        .lock()
        .map_err(settings_lock_err)?
        .refresh_after_session_end(&snapshot.session_id)
}

type SharedControlWebSocket = Arc<SyncSender<WsMessage>>;

#[cfg(test)]
mod tests {
    use super::*;
    use crate::service::RuntimeService;
    use tungstenite::connect;

    fn fixture() -> (std::path::PathBuf, Arc<ControlPlaneState>) {
        let directory =
            std::env::temp_dir().join(format!("akadesk-control-{}", generate_pair_token()));
        let service = RuntimeService::new_in(&directory).unwrap();
        let state = Arc::new(service.control_state().clone());
        (directory, state)
    }

    #[test]
    fn consent_wait_keeps_heartbeats_alive_and_connection_close_joins_workers() {
        let (directory, state) = fixture();
        let mut settings = state.settings.lock().unwrap();
        let mut snapshot = settings.snapshot();
        snapshot.connection_method = "TRUST_ONLY".into();
        settings.update(snapshot).unwrap();
        drop(settings);
        let listener = TcpListener::bind("127.0.0.1:0").unwrap();
        let address = listener.local_addr().unwrap();
        let server_state = Arc::clone(&state);
        let (done_tx, done_rx) = std::sync::mpsc::channel();
        let worker = thread::spawn(move || {
            let (stream, _) = listener.accept().unwrap();
            done_tx
                .send(handle_connection(stream, server_state))
                .unwrap();
        });
        let (mut socket, _) = connect(format!("ws://{address}")).unwrap();
        if let tungstenite::stream::MaybeTlsStream::Plain(stream) = socket.get_mut() {
            stream
                .set_read_timeout(Some(Duration::from_secs(2)))
                .unwrap();
        }
        let send =
            |socket: &mut tungstenite::WebSocket<_>, message_type: &str, payload: Vec<u8>| {
                socket
                    .send(WsMessage::Binary(
                        ControlEnvelope {
                            request_id: "test".into(),
                            kind: ControlMessageKind::Request as i32,
                            r#type: message_type.into(),
                            payload,
                            timestamp_ms: now_ms(),
                        }
                        .encode_to_vec(),
                    ))
                    .unwrap();
            };
        send(
            &mut socket,
            "PairingRequest",
            PairingRequest {
                request_id: "pair".into(),
                controller_device_id: "phone".into(),
                controller_display_name: "Phone".into(),
                version: None,
            }
            .encode_to_vec(),
        );
        let challenge = socket.read().unwrap();
        assert!(matches!(challenge, WsMessage::Binary(_)));
        let id = state
            .pending_remote_requests
            .lock()
            .unwrap()
            .snapshot()
            .unwrap()
            .request_id;
        assert!(!state
            .pending_remote_requests
            .lock()
            .unwrap()
            .respond("old-popup", true));
        for index in 0..8 {
            thread::sleep(Duration::from_secs(1));
            send(
                &mut socket,
                "HeartbeatPing",
                HeartbeatPing {
                    request_id: format!("ping-{index}"),
                    timestamp_ms: index,
                }
                .encode_to_vec(),
            );
            let WsMessage::Binary(frame) = socket.read().unwrap() else {
                panic!("missing heartbeat response")
            };
            assert_eq!(
                ControlEnvelope::decode(&frame[..]).unwrap().r#type,
                "HeartbeatPong"
            );
        }
        assert!(state
            .pending_remote_requests
            .lock()
            .unwrap()
            .respond(&id, true));
        let WsMessage::Binary(frame) = socket.read().unwrap() else {
            panic!("missing pairing result")
        };
        let result = ControlEnvelope::decode(&frame[..]).unwrap();
        assert!(PairingResult::decode(&result.payload[..]).unwrap().accepted);
        socket.close(None).unwrap();
        assert!(done_rx
            .recv_timeout(Duration::from_secs(2))
            .unwrap()
            .is_ok());
        worker.join().unwrap();
        assert!(state
            .pending_remote_requests
            .lock()
            .unwrap()
            .snapshot()
            .is_none());
        drop(state);
        std::fs::remove_dir_all(directory).unwrap();
    }

    #[test]
    fn signaling_rejects_other_controllers_and_expired_challenges() {
        let (directory, state) = fixture();
        let active = state
            .sessions
            .lock()
            .unwrap()
            .activate_preparing(
                String::new(),
                "owner".into(),
                "host".into(),
                String::new(),
                Default::default(),
            )
            .unwrap();
        let (tx, rx) = std::sync::mpsc::sync_channel(64);
        let outbound = Arc::new(tx);
        let mut session = ConnectionSession {
            peer: "127.0.0.1".into(),
            authenticated_controller_device_id: Some("intruder".into()),
            ..Default::default()
        };
        let envelope = ControlEnvelope {
            request_id: "answer".into(),
            kind: ControlMessageKind::Request as i32,
            r#type: "WebRtcAnswer".into(),
            payload: WebRtcAnswer {
                session_id: active.session_id,
                sdp: "unauthorized".into(),
            }
            .encode_to_vec(),
            timestamp_ms: now_ms(),
        };
        assert_eq!(
            handle_envelope(
                &outbound,
                &state,
                &mut session,
                &Arc::new(Mutex::new(None)),
                envelope
            )
            .unwrap_err()
            .kind(),
            io::ErrorKind::PermissionDenied
        );
        session.pending_challenge = Some(PendingChallenge {
            request_id: "pair".into(),
            controller_device_id: "phone".into(),
            controller_display_name: "Phone".into(),
            challenge_id: "expired".into(),
            expires_at_ms: now_ms() - 1,
        });
        let confirm = PairingConfirm {
            request_id: "confirm".into(),
            challenge_id: "expired".into(),
            verification_code: state
                .settings
                .lock()
                .unwrap()
                .current()
                .temporary_verification_code,
        };
        handle_envelope(
            &outbound,
            &state,
            &mut session,
            &Arc::new(Mutex::new(None)),
            ControlEnvelope {
                request_id: "confirm".into(),
                kind: ControlMessageKind::Request as i32,
                r#type: "PairingConfirm".into(),
                payload: confirm.encode_to_vec(),
                timestamp_ms: now_ms(),
            },
        )
        .unwrap();
        let WsMessage::Binary(frame) = rx.recv().unwrap() else {
            panic!("missing rejection")
        };
        let result = ControlEnvelope::decode(&frame[..]).unwrap();
        assert!(!PairingResult::decode(&result.payload[..]).unwrap().accepted);
        assert_eq!(state.pairing_store.lock().unwrap().count(), 0);
        drop(state);
        std::fs::remove_dir_all(directory).unwrap();
    }
}
type SharedActiveSessionId = Arc<Mutex<Option<String>>>;
type SharedTerminalSessionState = Arc<Mutex<Option<(String, i32)>>>;
type SharedVideoStreamEventState = Arc<Mutex<Option<SentVideoStreamEventState>>>;
type SharedTransportEventState = Arc<Mutex<Option<SentTransportEventState>>>;

#[derive(Debug, Clone, PartialEq, Eq)]
struct SentVideoStreamEventState {
    session_id: String,
    packet_count: u64,
    keyframe_count: u64,
    latest_packet_len: usize,
    latest_error: String,
    updated_at_ms: i64,
    emitted_at_ms: i64,
}

#[derive(Debug, Clone, PartialEq, Eq)]
struct SentTransportEventState {
    session_id: String,
    peer_connection_state: String,
    remote_answer_applied: bool,
    applied_remote_candidate_count: u32,
    sent_sample_count: u64,
    latest_sent_packet_len: usize,
    latest_error: String,
    updated_at_ms: i64,
    emitted_at_ms: i64,
}

#[derive(Debug, Clone)]
pub struct ControlPlaneState {
    pub host_identity: DeviceIdentity,
    pub advertised_presence: DevicePresence,
    pub capability_flags: CapabilityFlags,
    pub pairing_store: SharedPairingStore,
    pub sessions: SharedSessionRegistry,
    pub settings: Arc<Mutex<HostSettingsStore>>,
    pub pending_remote_requests: SharedPendingRemoteRequestStore,
    pub video_streams: SharedVideoStreamState,
    pub signaling: SharedSignalingState,
    pub transport_status: SharedTransportStatusState,
    pub pairing_attempts: Arc<Mutex<crate::auth::PairingAttemptLimiter>>,
}

pub fn spawn_control_server(
    state: ControlPlaneState,
) -> io::Result<thread::JoinHandle<io::Result<()>>> {
    let bind_addr = SocketAddr::from(([0, 0, 0, 0], state.advertised_presence.control_port as u16));
    let listener = TcpListener::bind(bind_addr)?;
    let shared_state = Arc::new(state);
    let handle = thread::spawn(move || -> io::Result<()> {
        for stream in listener.incoming() {
            let stream = stream?;
            let connection_state = Arc::clone(&shared_state);
            thread::spawn(move || {
                if let Err(err) = handle_connection(stream, connection_state) {
                    eprintln!("control websocket connection error: {err}");
                }
            });
        }
        Ok(())
    });
    Ok(handle)
}

fn handle_connection(stream: TcpStream, state: Arc<ControlPlaneState>) -> io::Result<()> {
    let peer = stream.peer_addr()?.ip().to_string();
    stream.set_read_timeout(Some(Duration::from_secs(5)))?;
    stream.set_write_timeout(Some(Duration::from_secs(2)))?;
    let mut websocket =
        accept(stream).map_err(|err| io::Error::new(io::ErrorKind::Other, err.to_string()))?;
    websocket
        .get_mut()
        .set_read_timeout(Some(Duration::from_millis(150)))?;
    let (outbound_tx, outbound_rx) = std::sync::mpsc::sync_channel(64);
    let outbound = Arc::new(outbound_tx);
    let active_session_id: SharedActiveSessionId = Arc::new(Mutex::new(None));
    let stopping = Arc::new(AtomicBool::new(false));
    let signaling_pump = spawn_signaling_pump(
        Arc::clone(&outbound),
        Arc::clone(&active_session_id),
        Arc::clone(&state),
        Arc::clone(&stopping),
    );
    let terminal_session_state: SharedTerminalSessionState = Arc::new(Mutex::new(None));
    let terminal_session_pump = spawn_terminal_session_pump(
        Arc::clone(&outbound),
        Arc::clone(&active_session_id),
        Arc::clone(&terminal_session_state),
        Arc::clone(&state),
        Arc::clone(&stopping),
    );
    let video_stream_event_state: SharedVideoStreamEventState = Arc::new(Mutex::new(None));
    let video_stream_pump = spawn_video_stream_event_pump(
        Arc::clone(&outbound),
        Arc::clone(&active_session_id),
        Arc::clone(&video_stream_event_state),
        Arc::clone(&state),
        Arc::clone(&stopping),
    );
    let transport_event_state: SharedTransportEventState = Arc::new(Mutex::new(None));
    let transport_status_pump = spawn_transport_status_event_pump(
        Arc::clone(&outbound),
        Arc::clone(&active_session_id),
        Arc::clone(&transport_event_state),
        Arc::clone(&state),
        Arc::clone(&stopping),
    );
    let mut session = ConnectionSession {
        peer,
        ..ConnectionSession::default()
    };
    // Always run cleanup, including decode errors and rejected messages.
    let loop_result = (|| -> io::Result<()> {
        loop {
            poll_local_confirmation(&outbound, &state, &mut session)?;
            for _ in 0..64 {
                match outbound_rx.try_recv() {
                    Ok(message) => websocket.send(message).map_err(ws_err)?,
                    Err(TryRecvError::Empty) => break,
                    Err(TryRecvError::Disconnected) => return Ok(()),
                }
            }
            let frame = {
                match websocket.read() {
                    Ok(frame) => Some(frame),
                    Err(WsError::Io(err))
                        if matches!(err.kind(), ErrorKind::WouldBlock | ErrorKind::TimedOut) =>
                    {
                        None
                    }
                    Err(err) => break Err(ws_err(err)),
                }
            };
            let Some(frame) = frame else {
                continue;
            };
            match frame {
                WsMessage::Binary(bytes) => {
                    let envelope = ControlEnvelope::decode(&bytes[..]).map_err(decode_err)?;
                    handle_envelope(
                        &outbound,
                        &state,
                        &mut session,
                        &active_session_id,
                        envelope,
                    )?
                }
                WsMessage::Close(_) => break Ok(()),
                WsMessage::Ping(payload) => {
                    websocket.send(WsMessage::Pong(payload)).map_err(ws_err)?
                }
                _ => {}
            }
        }
    })();
    stopping.store(true, Ordering::Relaxed);
    if let Some(pending) = session.pending_confirmation.take() {
        if let Ok(mut requests) = state.pending_remote_requests.lock() {
            requests.cancel(&pending.request_id);
        }
    }
    if let Ok(mut session_id) = active_session_id.lock() {
        *session_id = None;
    }
    let _ = signaling_pump.join();
    let _ = terminal_session_pump.join();
    let _ = video_stream_pump.join();
    let _ = transport_status_pump.join();
    loop_result
}

#[derive(Debug, Default)]
struct ConnectionSession {
    peer: String,
    authenticated_controller_device_id: Option<String>,
    pending_challenge: Option<PendingChallenge>,
    pending_confirmation: Option<PendingLocalConfirmation>,
}

#[derive(Debug)]
struct PendingLocalConfirmation {
    request_id: String,
    response_rx: Receiver<bool>,
    envelope_request_id: String,
    result_request_id: String,
    controller_device_id: String,
    controller_display_name: String,
    connection_method: ConnectionMethod,
    after_access_code: bool,
    expires_at_ms: i64,
}

fn handle_envelope(
    websocket: &SharedControlWebSocket,
    state: &ControlPlaneState,
    session: &mut ConnectionSession,
    active_session_id: &SharedActiveSessionId,
    envelope: ControlEnvelope,
) -> io::Result<()> {
    match envelope.r#type.as_str() {
        "ControlHello" => {
            let hello = ControlHello::decode(&envelope.payload[..]).map_err(decode_err)?;
            let host_settings = state.settings.lock().map_err(settings_lock_err)?.current();
            let has_active_session_for_controller = state
                .sessions
                .lock()
                .map_err(session_lock_err)?
                .active_snapshot_for_controller(&hello.controller_device_id)
                .is_some();
            let mut invalid_token = false;
            let accepted = if !host_settings.allow_remote_assistance
                || hello.pair_token.is_empty()
                || !state
                    .pairing_attempts
                    .lock()
                    .map_err(lock_err)?
                    .permits(&session.peer, now_ms())
            {
                false
            } else if host_settings.connection_method() != ConnectionMethod::AccessCode
                && !has_active_session_for_controller
            {
                false
            } else {
                let valid = state
                    .pairing_store
                    .lock()
                    .map_err(lock_err)?
                    .verify_pair_token(&hello.controller_device_id, &hello.pair_token)?;
                invalid_token = !valid;
                valid
            };
            if invalid_token {
                state
                    .pairing_attempts
                    .lock()
                    .map_err(lock_err)?
                    .record_failure(&session.peer, now_ms());
            }
            session.authenticated_controller_device_id = None;
            if accepted {
                let controller_device_id = hello.controller_device_id;
                session.authenticated_controller_device_id = Some(controller_device_id.clone());
                restore_active_session_for_controller(
                    state,
                    active_session_id,
                    &controller_device_id,
                )?;
            }
            let ack = ControlHelloAck {
                request_id: hello.request_id.clone(),
                accepted,
                identity: Some(state.host_identity.clone()),
                capability_flags: Some(state.capability_flags.clone()),
                error: (!accepted).then(|| pairing_required_error(&host_settings)),
            };
            send_payload_shared(
                websocket,
                &envelope.request_id,
                ControlMessageKind::Response,
                "ControlHelloAck",
                ack,
            )
        }
        "PairingRequest" => {
            let request = PairingRequest::decode(&envelope.payload[..]).map_err(decode_err)?;
            if session.pending_confirmation.is_some() {
                return send_pairing_error(
                    websocket,
                    &envelope.request_id,
                    &request.request_id,
                    "AUTH_REQUEST_PENDING",
                    "A local confirmation request is already pending.",
                );
            }
            if !state
                .pairing_attempts
                .lock()
                .map_err(lock_err)?
                .permits(&session.peer, now_ms())
            {
                return send_pairing_error(
                    websocket,
                    &envelope.request_id,
                    &request.request_id,
                    "AUTH_RATE_LIMITED",
                    "Too many failed attempts. Please retry in one minute.",
                );
            }
            let host_settings = state.settings.lock().map_err(settings_lock_err)?.current();
            if !host_settings.allow_remote_assistance {
                let result = PairingResult {
                    request_id: request.request_id,
                    accepted: false,
                    pair_token: String::new(),
                    error: Some(simple_error(
                        "AUTH_REMOTE_ASSISTANCE_DISABLED",
                        "Remote assistance is currently disabled on this host.",
                    )),
                };
                return send_payload_shared(
                    websocket,
                    &envelope.request_id,
                    ControlMessageKind::Response,
                    "PairingResult",
                    result,
                );
            }
            if host_settings.connection_method() != ConnectionMethod::AccessCode {
                if host_settings.connection_method() == ConnectionMethod::TrustOnly {
                    return wait_for_local_confirmation(
                        websocket,
                        state,
                        session,
                        &envelope.request_id,
                        request.request_id,
                        request.controller_device_id,
                        request.controller_display_name,
                        host_settings
                            .connection_method()
                            .as_wire_value()
                            .to_string(),
                        false,
                        None,
                    );
                }
            }
            let challenge = PendingChallenge {
                request_id: request.request_id.clone(),
                controller_device_id: request.controller_device_id.clone(),
                controller_display_name: request.controller_display_name.clone(),
                challenge_id: format!("challenge-{}", crate::pairing::generate_pair_token()),
                expires_at_ms: now_ms() + 60_000,
            };
            session.pending_challenge = Some(challenge.clone());
            let message = PairingChallenge {
                request_id: request.request_id,
                challenge_id: challenge.challenge_id,
                challenge_type: PairingChallengeType::OneTimeCode as i32,
                verification_code: String::new(),
            };
            send_payload_shared(
                websocket,
                &envelope.request_id,
                ControlMessageKind::Response,
                "PairingChallenge",
                message,
            )
        }
        "PairingConfirm" => {
            let confirm = PairingConfirm::decode(&envelope.payload[..]).map_err(decode_err)?;
            if session.pending_confirmation.is_some() {
                return send_pairing_error(
                    websocket,
                    &envelope.request_id,
                    &confirm.request_id,
                    "AUTH_REQUEST_PENDING",
                    "A local confirmation request is already pending.",
                );
            }
            if !state
                .pairing_attempts
                .lock()
                .map_err(lock_err)?
                .permits(&session.peer, now_ms())
            {
                session.pending_challenge = None;
                return send_pairing_error(
                    websocket,
                    &envelope.request_id,
                    &confirm.request_id,
                    "AUTH_RATE_LIMITED",
                    "Too many failed attempts. Please retry in one minute.",
                );
            }
            let Some(challenge) = session.pending_challenge.as_ref() else {
                let result = PairingResult {
                    request_id: confirm.request_id,
                    accepted: false,
                    pair_token: String::new(),
                    error: Some(simple_error(
                        "PAIRING_CHALLENGE_MISSING",
                        "No pairing challenge is active.",
                    )),
                };
                return send_payload_shared(
                    websocket,
                    &envelope.request_id,
                    ControlMessageKind::Response,
                    "PairingResult",
                    result,
                );
            };

            if challenge.challenge_id != confirm.challenge_id || challenge.expires_at_ms <= now_ms()
            {
                let result = PairingResult {
                    request_id: confirm.request_id,
                    accepted: false,
                    pair_token: String::new(),
                    error: Some(simple_error(
                        "PAIRING_CHALLENGE_INVALID",
                        "The pairing challenge is no longer valid.",
                    )),
                };
                return send_payload_shared(
                    websocket,
                    &envelope.request_id,
                    ControlMessageKind::Response,
                    "PairingResult",
                    result,
                );
            }

            let host_settings = state.settings.lock().map_err(settings_lock_err)?.current();
            if !host_settings.allow_remote_assistance {
                session.pending_challenge = None;
                return send_pairing_error(
                    websocket,
                    &envelope.request_id,
                    &confirm.request_id,
                    "AUTH_REMOTE_ASSISTANCE_DISABLED",
                    "Remote assistance is currently disabled.",
                );
            }
            if !host_settings.accepts_access_code(&confirm.verification_code) {
                state
                    .pairing_attempts
                    .lock()
                    .map_err(lock_err)?
                    .record_failure(&session.peer, now_ms());
                let result = PairingResult {
                    request_id: confirm.request_id,
                    accepted: false,
                    pair_token: String::new(),
                    error: Some(simple_error(
                        "PAIRING_CODE_INVALID",
                        "The submitted access code does not match the current host verification settings.",
                    )),
                };
                return send_payload_shared(
                    websocket,
                    &envelope.request_id,
                    ControlMessageKind::Response,
                    "PairingResult",
                    result,
                );
            }

            if host_settings.connection_method() == ConnectionMethod::AccessCodeAndConsent {
                return wait_for_local_confirmation(
                    websocket,
                    state,
                    session,
                    &envelope.request_id,
                    confirm.request_id,
                    challenge.controller_device_id.clone(),
                    challenge.controller_display_name.clone(),
                    host_settings
                        .connection_method()
                        .as_wire_value()
                        .to_string(),
                    true,
                    Some(challenge.challenge_id.clone()),
                );
            }

            if host_settings.connection_method() != ConnectionMethod::AccessCode {
                let result = PairingResult {
                    request_id: confirm.request_id,
                    accepted: false,
                    pair_token: String::new(),
                    error: Some(simple_error(
                        "AUTH_CONNECTION_METHOD_NOT_READY",
                        "The current connection method is not ready to complete pairing.",
                    )),
                };
                return send_payload_shared(
                    websocket,
                    &envelope.request_id,
                    ControlMessageKind::Response,
                    "PairingResult",
                    result,
                );
            }

            let pair_token = generate_pair_token();
            state
                .pairing_store
                .lock()
                .map_err(lock_err)?
                .upsert_pairing(
                    &challenge.controller_device_id,
                    &challenge.controller_display_name,
                    &pair_token,
                )?;
            session.authenticated_controller_device_id =
                Some(challenge.controller_device_id.clone());
            session.pending_challenge = None;

            let result = PairingResult {
                request_id: confirm.request_id,
                accepted: true,
                pair_token,
                error: None,
            };
            send_payload_shared(
                websocket,
                &envelope.request_id,
                ControlMessageKind::Response,
                "PairingResult",
                result,
            )
        }
        "GetDeviceSnapshot" => {
            let request = GetDeviceSnapshot::decode(&envelope.payload[..]).map_err(decode_err)?;
            if session.authenticated_controller_device_id.is_none() {
                let host_settings = state.settings.lock().map_err(settings_lock_err)?.current();
                let response = DeviceSnapshotResponse {
                    request_id: request.request_id,
                    snapshot: None,
                    error: Some(pairing_required_error(&host_settings)),
                };
                return send_payload_shared(
                    websocket,
                    &envelope.request_id,
                    ControlMessageKind::Response,
                    "DeviceSnapshotResponse",
                    response,
                );
            }

            let response = DeviceSnapshotResponse {
                request_id: request.request_id,
                snapshot: Some(DeviceSnapshot {
                    identity: Some(state.host_identity.clone()),
                    capability_flags: Some(state.capability_flags.clone()),
                    displays: Vec::new(),
                    audio_devices: Vec::new(),
                }),
                error: None,
            };
            send_payload_shared(
                websocket,
                &envelope.request_id,
                ControlMessageKind::Response,
                "DeviceSnapshotResponse",
                response,
            )
        }
        "HeartbeatPing" => {
            let ping = HeartbeatPing::decode(&envelope.payload[..]).map_err(decode_err)?;
            let pong = HeartbeatPong {
                request_id: ping.request_id,
                timestamp_ms: ping.timestamp_ms,
            };
            send_payload_shared(
                websocket,
                &envelope.request_id,
                ControlMessageKind::Event,
                "HeartbeatPong",
                pong,
            )
        }
        "StartSessionRequest" => {
            if session.authenticated_controller_device_id.is_none() {
                let host_settings = state.settings.lock().map_err(settings_lock_err)?.current();
                return send_start_session_rejected(
                    websocket,
                    &envelope.request_id,
                    String::new(),
                    pairing_required_error(&host_settings),
                );
            }

            let request = StartSessionRequest::decode(&envelope.payload[..]).map_err(decode_err)?;
            let host_settings = state.settings.lock().map_err(settings_lock_err)?.current();
            if !host_settings.allow_remote_assistance {
                return send_start_session_rejected(
                    websocket,
                    &envelope.request_id,
                    String::new(),
                    simple_error(
                        "AUTH_REMOTE_ASSISTANCE_DISABLED",
                        "Remote assistance is currently disabled on this host.",
                    ),
                );
            }
            let Some(mut intent) = request.intent else {
                return send_start_session_rejected(
                    websocket,
                    &envelope.request_id,
                    String::new(),
                    simple_error(
                        "MEDIA_SESSION_INTENT_MISSING",
                        "StartSessionRequest must include a SessionIntent payload.",
                    ),
                );
            };

            let authenticated_controller = session
                .authenticated_controller_device_id
                .as_ref()
                .expect("checked authenticated controller")
                .clone();
            if intent.controller_device_id.trim().is_empty() {
                intent.controller_device_id = authenticated_controller.clone();
            }
            if intent.controller_device_id != authenticated_controller {
                return send_start_session_rejected(
                    websocket,
                    &envelope.request_id,
                    intent.session_id,
                    simple_error(
                        "AUTH_CONTROLLER_ID_MISMATCH",
                        "Session intent controller_device_id does not match the authenticated control channel.",
                    ),
                );
            }
            if intent.controlled_device_id.trim().is_empty() {
                intent.controlled_device_id = state.host_identity.device_id.clone();
            }
            if intent.controlled_device_id != state.host_identity.device_id {
                return send_start_session_rejected(
                    websocket,
                    &envelope.request_id,
                    intent.session_id,
                    simple_error(
                        "AUTH_HOST_ID_MISMATCH",
                        "The requested host does not match this device.",
                    ),
                );
            }

            if intent.enable_input || intent.audio.is_some() || !intent.target_display_id.is_empty()
            {
                return send_start_session_rejected(
                    websocket,
                    &envelope.request_id,
                    intent.session_id,
                    simple_error(
                        "SESSION_FEATURE_UNSUPPORTED",
                        "This version supports viewing the primary display only.",
                    ),
                );
            }
            reap_finished_session(state)?;
            if state
                .sessions
                .lock()
                .map_err(session_lock_err)?
                .active
                .as_ref()
                .is_some_and(|s| s.closed_at_ms == 0)
            {
                return send_start_session_rejected(
                    websocket,
                    &envelope.request_id,
                    intent.session_id,
                    simple_error(
                        "SESSION_HOST_BUSY",
                        "Another remote assistance session is active.",
                    ),
                );
            }
            let prepared = match crate::video::prepare_h264_session(&intent) {
                Ok(prepared) => prepared,
                Err(err) => {
                    return send_start_session_rejected(
                        websocket,
                        &envelope.request_id,
                        intent.session_id,
                        simple_error("MEDIA_H264_PREPARE_FAILED", &err),
                    )
                }
            };
            eprintln!(
                "prepared H.264 session encoder={} packet_len={} keyframe={} fmtp={} frame_source={}",
                prepared.encoder_name,
                prepared.smoke_packet_len,
                prepared.smoke_keyframe,
                prepared.rtp_fmtp_line,
                prepared.frame_source_label
            );
            let prepared_spec = prepared.spec.clone();

            let snapshot = state
                .sessions
                .lock()
                .map_err(session_lock_err)?
                .activate_preparing(
                    intent.session_id,
                    intent.controller_device_id,
                    intent.controlled_device_id,
                    intent.target_display_id,
                    prepared_spec.clone(),
                );
            let Some(snapshot) = snapshot else {
                return send_start_session_rejected(
                    websocket,
                    &envelope.request_id,
                    String::new(),
                    simple_error(
                        "SESSION_HOST_BUSY",
                        "Another remote assistance session is active.",
                    ),
                );
            };
            state
                .signaling
                .lock()
                .map_err(signaling_lock_err)?
                .begin_session(snapshot.session_id.clone());
            state
                .video_streams
                .lock()
                .map_err(video_stream_lock_err)?
                .begin_session(snapshot.session_id.clone());
            let streaming_startup = crate::video::spawn_h264_streaming_loop(
                Arc::clone(&state.sessions),
                Arc::clone(&state.video_streams),
                snapshot.session_id.clone(),
                prepared_spec,
            )
            .map_err(|err| io::Error::new(io::ErrorKind::Other, err))?;
            let streaming = match streaming_startup.recv_timeout(Duration::from_millis(1_500)) {
                Ok(Ok(streaming)) => streaming,
                Ok(Err(err)) => {
                    let _ = state
                        .sessions
                        .lock()
                        .ok()
                        .and_then(|mut registry| registry.mark_failed(&snapshot.session_id));
                    if let Ok(mut active) = active_session_id.lock() {
                        active.take();
                    }
                    if let Ok(mut video_streams) = state.video_streams.lock() {
                        video_streams.finish_session(&snapshot.session_id);
                    }
                    if let Ok(mut signaling) = state.signaling.lock() {
                        signaling.finish_session(&snapshot.session_id);
                    }
                    return send_start_session_rejected(
                        websocket,
                        &envelope.request_id,
                        snapshot.session_id,
                        simple_error("MEDIA_H264_STREAMING_FAILED", &err),
                    );
                }
                Err(RecvTimeoutError::Timeout) => {
                    let _ = state
                        .sessions
                        .lock()
                        .ok()
                        .and_then(|mut registry| registry.mark_failed(&snapshot.session_id));
                    if let Ok(mut active) = active_session_id.lock() {
                        active.take();
                    }
                    if let Ok(mut video_streams) = state.video_streams.lock() {
                        video_streams.finish_session(&snapshot.session_id);
                    }
                    if let Ok(mut signaling) = state.signaling.lock() {
                        signaling.finish_session(&snapshot.session_id);
                    }
                    return send_start_session_rejected(
                        websocket,
                        &envelope.request_id,
                        snapshot.session_id,
                        simple_error(
                            "MEDIA_H264_STREAMING_TIMEOUT",
                            "The continuous H.264 streaming loop did not produce its first frame before timeout.",
                        ),
                    );
                }
                Err(RecvTimeoutError::Disconnected) => {
                    let _ = state
                        .sessions
                        .lock()
                        .ok()
                        .and_then(|mut registry| registry.mark_failed(&snapshot.session_id));
                    if let Ok(mut active) = active_session_id.lock() {
                        active.take();
                    }
                    if let Ok(mut video_streams) = state.video_streams.lock() {
                        video_streams.finish_session(&snapshot.session_id);
                    }
                    if let Ok(mut signaling) = state.signaling.lock() {
                        signaling.finish_session(&snapshot.session_id);
                    }
                    return send_start_session_rejected(
                        websocket,
                        &envelope.request_id,
                        snapshot.session_id,
                        simple_error(
                            "MEDIA_H264_STREAMING_DISCONNECTED",
                            "The continuous H.264 streaming loop exited before reporting its first frame.",
                        ),
                    );
                }
            };
            eprintln!(
                "continuous H.264 stream prepared session={} packet_len={} fmtp={} frame_source={}",
                streaming.snapshot.session_id,
                streaming.first_packet_len,
                streaming.rtp_fmtp_line,
                streaming.first_frame_source_label
            );
            let transport_startup = crate::transport::spawn_h264_webrtc_transport_loop(
                Arc::clone(&state.sessions),
                Arc::clone(&state.video_streams),
                Arc::clone(&state.signaling),
                Arc::clone(&state.transport_status),
                snapshot.session_id.clone(),
                snapshot
                    .video
                    .as_ref()
                    .map(|video| video.fps)
                    .unwrap_or_default(),
                streaming.rtp_fmtp_line.clone(),
            )
            .map_err(|err| io::Error::new(io::ErrorKind::Other, err))?;
            match transport_startup.recv_timeout(Duration::from_millis(1_500)) {
                Ok(Ok(transport)) => {
                    eprintln!(
                        "webrtc transport prepared session={} local_offer_sdp_len={} local_candidates={} fmtp={}",
                        snapshot.session_id,
                        transport.local_offer_sdp_len,
                        transport.local_candidate_count,
                        streaming.rtp_fmtp_line
                    );
                }
                Ok(Err(err)) => {
                    let _ = state
                        .sessions
                        .lock()
                        .ok()
                        .and_then(|mut registry| registry.mark_failed(&snapshot.session_id));
                    if let Ok(mut active) = active_session_id.lock() {
                        active.take();
                    }
                    if let Ok(mut video_streams) = state.video_streams.lock() {
                        video_streams.finish_session(&snapshot.session_id);
                    }
                    if let Ok(mut signaling) = state.signaling.lock() {
                        signaling.finish_session(&snapshot.session_id);
                    }
                    return send_start_session_rejected(
                        websocket,
                        &envelope.request_id,
                        snapshot.session_id,
                        simple_error("MEDIA_WEBRTC_TRANSPORT_FAILED", &err),
                    );
                }
                Err(RecvTimeoutError::Timeout) => {
                    let _ = state
                        .sessions
                        .lock()
                        .ok()
                        .and_then(|mut registry| registry.mark_failed(&snapshot.session_id));
                    if let Ok(mut active) = active_session_id.lock() {
                        active.take();
                    }
                    if let Ok(mut video_streams) = state.video_streams.lock() {
                        video_streams.finish_session(&snapshot.session_id);
                    }
                    if let Ok(mut signaling) = state.signaling.lock() {
                        signaling.finish_session(&snapshot.session_id);
                    }
                    return send_start_session_rejected(
                        websocket,
                        &envelope.request_id,
                        snapshot.session_id,
                        simple_error(
                            "MEDIA_WEBRTC_TRANSPORT_TIMEOUT",
                            "WebRTC transport did not produce a local offer before timeout.",
                        ),
                    );
                }
                Err(RecvTimeoutError::Disconnected) => {
                    let _ = state
                        .sessions
                        .lock()
                        .ok()
                        .and_then(|mut registry| registry.mark_failed(&snapshot.session_id));
                    if let Ok(mut active) = active_session_id.lock() {
                        active.take();
                    }
                    if let Ok(mut video_streams) = state.video_streams.lock() {
                        video_streams.finish_session(&snapshot.session_id);
                    }
                    if let Ok(mut signaling) = state.signaling.lock() {
                        signaling.finish_session(&snapshot.session_id);
                    }
                    return send_start_session_rejected(
                        websocket,
                        &envelope.request_id,
                        snapshot.session_id,
                        simple_error(
                            "MEDIA_WEBRTC_TRANSPORT_DISCONNECTED",
                            "WebRTC transport startup channel disconnected unexpectedly.",
                        ),
                    );
                }
            }

            let session_id = snapshot.session_id.clone();

            send_payload_shared(
                websocket,
                &envelope.request_id,
                ControlMessageKind::Response,
                "StartSessionAccepted",
                StartSessionAccepted {
                    session_id: session_id.clone(),
                    state: streaming.snapshot.state,
                },
            )?;
            send_payload_shared(
                websocket,
                &envelope.request_id,
                ControlMessageKind::Event,
                "SessionChangedEvent",
                SessionChangedEvent {
                    snapshot: Some(streaming.snapshot.clone()),
                },
            )?;
            flush_local_signaling_events(websocket, &envelope.request_id, state, &session_id)?;

            if let Ok(mut active) = active_session_id.lock() {
                *active = Some(session_id);
            }
            Ok(())
        }
        "WebRtcAnswer" => {
            let answer = WebRtcAnswer::decode(&envelope.payload[..]).map_err(decode_err)?;
            if session.authenticated_controller_device_id.is_none() {
                return Err(io::Error::new(
                    io::ErrorKind::PermissionDenied,
                    "WebRtcAnswer received before control channel authentication",
                ));
            }
            require_session_owner(state, session, &answer.session_id)?;
            let store_result = state
                .signaling
                .lock()
                .map_err(signaling_lock_err)?
                .set_remote_answer(answer.clone());
            match store_result {
                RemoteAnswerStoreResult::Stored => {
                    eprintln!(
                        "stored remote WebRTC answer for session {} sdp_len={}",
                        answer.session_id,
                        answer.sdp.len()
                    );
                    flush_local_signaling_events(
                        websocket,
                        &envelope.request_id,
                        state,
                        &answer.session_id,
                    )?;
                }
                RemoteAnswerStoreResult::Duplicate => {
                    eprintln!(
                        "ignoring duplicate remote WebRTC answer for session {}",
                        answer.session_id
                    );
                }
                RemoteAnswerStoreResult::IgnoredReplacement => {
                    eprintln!(
                        "ignoring replacement remote WebRTC answer for session {} after first answer was already stored",
                        answer.session_id
                    );
                }
                RemoteAnswerStoreResult::InactiveSession => {
                    eprintln!(
                        "ignoring WebRtcAnswer for inactive session {}",
                        answer.session_id
                    );
                }
            }
            Ok(())
        }
        "IceCandidate" => {
            let candidate = IceCandidate::decode(&envelope.payload[..]).map_err(decode_err)?;
            if session.authenticated_controller_device_id.is_none() {
                return Err(io::Error::new(
                    io::ErrorKind::PermissionDenied,
                    "IceCandidate received before control channel authentication",
                ));
            }
            require_session_owner(state, session, &candidate.session_id)?;
            let store_result = state
                .signaling
                .lock()
                .map_err(signaling_lock_err)?
                .push_remote_candidate(candidate.clone());
            match store_result {
                RemoteCandidateStoreResult::Stored => {
                    eprintln!(
                        "stored remote ICE candidate for session {}",
                        candidate.session_id
                    );
                    flush_local_signaling_events(
                        websocket,
                        &envelope.request_id,
                        state,
                        &candidate.session_id,
                    )?;
                }
                RemoteCandidateStoreResult::Duplicate => {
                    eprintln!(
                        "ignoring duplicate remote ICE candidate for session {}",
                        candidate.session_id
                    );
                }
                RemoteCandidateStoreResult::InactiveSession => {
                    eprintln!(
                        "ignoring IceCandidate for inactive session {}",
                        candidate.session_id
                    );
                }
            }
            Ok(())
        }
        "StopSessionRequest" => {
            let request = StopSessionRequest::decode(&envelope.payload[..]).map_err(decode_err)?;
            let Some(controller_device_id) = session.authenticated_controller_device_id.as_deref()
            else {
                return Err(io::Error::new(
                    io::ErrorKind::PermissionDenied,
                    "StopSessionRequest received before control channel authentication",
                ));
            };
            let snapshot = state
                .sessions
                .lock()
                .map_err(session_lock_err)?
                .active_snapshot_for_controller(controller_device_id);
            let Some(active_snapshot) = snapshot else {
                return Ok(());
            };
            if active_snapshot.session_id != request.session_id {
                return Ok(());
            }
            let Some(disconnected_snapshot) = state
                .sessions
                .lock()
                .map_err(session_lock_err)?
                .mark_disconnected(&request.session_id)
            else {
                return Ok(());
            };
            if let Ok(mut active) = active_session_id.lock() {
                if active.as_deref() == Some(request.session_id.as_str()) {
                    active.take();
                }
            }
            let _ = state
                .video_streams
                .lock()
                .map_err(video_stream_lock_err)?
                .finish_session(&request.session_id);
            state
                .signaling
                .lock()
                .map_err(signaling_lock_err)?
                .finish_session(&request.session_id);
            state
                .transport_status
                .lock()
                .map_err(transport_status_lock_err)?
                .finish_session(&request.session_id);
            state
                .settings
                .lock()
                .map_err(settings_lock_err)?
                .refresh_after_session_end(&request.session_id)?;
            send_payload_shared(
                websocket,
                &envelope.request_id,
                ControlMessageKind::Event,
                "SessionClosedEvent",
                SessionClosedEvent {
                    session_id: disconnected_snapshot.session_id,
                    reason: crate::generated::v1::DisconnectReason::ControllerRequest as i32,
                    error: None,
                    closed_at_ms: disconnected_snapshot.closed_at_ms,
                },
            )
        }
        other => Err(io::Error::new(
            io::ErrorKind::InvalidInput,
            format!("unsupported control message type: {other}"),
        )),
    }
}

fn spawn_signaling_pump(
    websocket: SharedControlWebSocket,
    active_session_id: SharedActiveSessionId,
    state: Arc<ControlPlaneState>,
    stopping: Arc<AtomicBool>,
) -> thread::JoinHandle<()> {
    thread::spawn(move || loop {
        if stopping.load(Ordering::Relaxed) {
            break;
        }
        let maybe_session_id = match active_session_id.lock() {
            Ok(active) => active.clone(),
            Err(_) => break,
        };
        if let Some(session_id) = maybe_session_id {
            let request_id = format!("server-signal-{}", now_ms());
            let result = flush_local_signaling_events(&websocket, &request_id, &state, &session_id);
            if let Err(err) = result {
                match err.kind() {
                    io::ErrorKind::BrokenPipe
                    | io::ErrorKind::ConnectionReset
                    | io::ErrorKind::ConnectionAborted
                    | io::ErrorKind::UnexpectedEof => break,
                    _ => eprintln!(
                        "control signaling pump failed for session {}: {}",
                        session_id, err
                    ),
                }
            }
        }
        thread::sleep(Duration::from_millis(100));
    })
}

fn spawn_terminal_session_pump(
    websocket: SharedControlWebSocket,
    active_session_id: SharedActiveSessionId,
    terminal_session_state: SharedTerminalSessionState,
    state: Arc<ControlPlaneState>,
    stopping: Arc<AtomicBool>,
) -> thread::JoinHandle<()> {
    thread::spawn(move || loop {
        if stopping.load(Ordering::Relaxed) {
            break;
        }
        let maybe_session_id = match active_session_id.lock() {
            Ok(active) => active.clone(),
            Err(_) => break,
        };
        let Some(session_id) = maybe_session_id else {
            if let Ok(mut sent_terminal) = terminal_session_state.lock() {
                sent_terminal.take();
            }
            thread::sleep(Duration::from_millis(100));
            continue;
        };

        let snapshot = {
            let registry = match state.sessions.lock() {
                Ok(registry) => registry,
                Err(_) => break,
            };
            registry.active.as_ref().and_then(|session| {
                (session.session_id.0 == session_id).then(|| session.snapshot())
            })
        };

        let Some(snapshot) = snapshot else {
            if let Ok(mut sent_terminal) = terminal_session_state.lock() {
                sent_terminal.take();
            }
            thread::sleep(Duration::from_millis(100));
            continue;
        };

        let should_send = match terminal_session_state.lock() {
            Ok(sent_terminal) => sent_terminal
                .as_ref()
                .map(|(sent_session_id, sent_state)| {
                    sent_session_id != &snapshot.session_id || *sent_state != snapshot.state
                })
                .unwrap_or(true),
            Err(_) => break,
        };
        if !should_send {
            thread::sleep(Duration::from_millis(100));
            continue;
        }

        let request_id = format!("server-session-{}", now_ms());
        let result = send_payload_shared(
            &websocket,
            &request_id,
            ControlMessageKind::Event,
            "SessionChangedEvent",
            SessionChangedEvent {
                snapshot: Some(snapshot.clone()),
            },
        );
        match result {
            Ok(()) => {
                if let Ok(mut sent_terminal) = terminal_session_state.lock() {
                    *sent_terminal = Some((snapshot.session_id.clone(), snapshot.state));
                }
                if matches!(
                    snapshot.state,
                    x if x == SessionState::Failed as i32 || x == SessionState::Disconnected as i32
                ) {
                    if let Ok(mut active) = active_session_id.lock() {
                        active.take();
                    }
                }
            }
            Err(err) => match err.kind() {
                io::ErrorKind::BrokenPipe
                | io::ErrorKind::ConnectionReset
                | io::ErrorKind::ConnectionAborted
                | io::ErrorKind::UnexpectedEof => break,
                _ => eprintln!(
                    "control session-state pump failed for session {}: {}",
                    snapshot.session_id, err
                ),
            },
        }

        thread::sleep(Duration::from_millis(100));
    })
}

fn spawn_video_stream_event_pump(
    websocket: SharedControlWebSocket,
    active_session_id: SharedActiveSessionId,
    sent_state: SharedVideoStreamEventState,
    state: Arc<ControlPlaneState>,
    stopping: Arc<AtomicBool>,
) -> thread::JoinHandle<()> {
    thread::spawn(move || loop {
        if stopping.load(Ordering::Relaxed) {
            break;
        }
        let maybe_session_id = match active_session_id.lock() {
            Ok(active) => active.clone(),
            Err(_) => break,
        };
        let Some(session_id) = maybe_session_id else {
            if let Ok(mut sent) = sent_state.lock() {
                sent.take();
            }
            thread::sleep(Duration::from_millis(500));
            continue;
        };

        let snapshot = {
            let stream_state = match state.video_streams.lock() {
                Ok(stream_state) => stream_state,
                Err(_) => break,
            };
            let snapshot = stream_state.snapshot();
            (snapshot.session_id == session_id).then_some(snapshot)
        };
        let Some(snapshot) = snapshot else {
            if let Ok(mut sent) = sent_state.lock() {
                sent.take();
            }
            thread::sleep(Duration::from_millis(500));
            continue;
        };

        let now = now_ms();
        let should_send = match sent_state.lock() {
            Ok(sent) => sent
                .as_ref()
                .map(|previous| {
                    previous.session_id != snapshot.session_id
                        || previous.packet_count != snapshot.packet_count
                        || previous.keyframe_count != snapshot.keyframe_count
                        || previous.latest_packet_len != snapshot.latest_packet_len
                        || previous.latest_error != snapshot.latest_error
                        || previous.updated_at_ms != snapshot.updated_at_ms
                        || now - previous.emitted_at_ms >= 1_000
                })
                .unwrap_or(true),
            Err(_) => break,
        };
        if !should_send {
            thread::sleep(Duration::from_millis(500));
            continue;
        }

        let event = HostVideoStreamEvent {
            session_id: snapshot.session_id.clone(),
            packet_count: snapshot.packet_count,
            keyframe_count: snapshot.keyframe_count,
            latest_packet_len: snapshot.latest_packet_len as u64,
            latest_frame_source_label: snapshot.latest_frame_source_label.clone(),
            latest_error: snapshot.latest_error.clone(),
            updated_at_ms: snapshot.updated_at_ms,
        };
        let request_id = format!("server-video-{}", now);
        match send_payload_shared(
            &websocket,
            &request_id,
            ControlMessageKind::Event,
            "HostVideoStreamEvent",
            event,
        ) {
            Ok(()) => {
                if let Ok(mut sent) = sent_state.lock() {
                    *sent = Some(SentVideoStreamEventState {
                        session_id: snapshot.session_id,
                        packet_count: snapshot.packet_count,
                        keyframe_count: snapshot.keyframe_count,
                        latest_packet_len: snapshot.latest_packet_len,
                        latest_error: snapshot.latest_error,
                        updated_at_ms: snapshot.updated_at_ms,
                        emitted_at_ms: now,
                    });
                }
            }
            Err(err) => match err.kind() {
                io::ErrorKind::BrokenPipe
                | io::ErrorKind::ConnectionReset
                | io::ErrorKind::ConnectionAborted
                | io::ErrorKind::UnexpectedEof => break,
                _ => eprintln!(
                    "control video-stream pump failed for session {}: {}",
                    session_id, err
                ),
            },
        }

        thread::sleep(Duration::from_millis(500));
    })
}

fn spawn_transport_status_event_pump(
    websocket: SharedControlWebSocket,
    active_session_id: SharedActiveSessionId,
    sent_state: SharedTransportEventState,
    state: Arc<ControlPlaneState>,
    stopping: Arc<AtomicBool>,
) -> thread::JoinHandle<()> {
    thread::spawn(move || loop {
        if stopping.load(Ordering::Relaxed) {
            break;
        }
        let maybe_session_id = match active_session_id.lock() {
            Ok(active) => active.clone(),
            Err(_) => break,
        };
        let Some(session_id) = maybe_session_id else {
            if let Ok(mut sent) = sent_state.lock() {
                sent.take();
            }
            thread::sleep(Duration::from_millis(500));
            continue;
        };

        let snapshot = {
            let transport_state = match state.transport_status.lock() {
                Ok(transport_state) => transport_state,
                Err(_) => break,
            };
            let snapshot = transport_state.snapshot();
            (snapshot.session_id == session_id).then_some(snapshot)
        };
        let Some(snapshot) = snapshot else {
            if let Ok(mut sent) = sent_state.lock() {
                sent.take();
            }
            thread::sleep(Duration::from_millis(500));
            continue;
        };

        let now = now_ms();
        let should_send = match sent_state.lock() {
            Ok(sent) => sent
                .as_ref()
                .map(|previous| {
                    previous.session_id != snapshot.session_id
                        || previous.peer_connection_state != snapshot.peer_connection_state
                        || previous.remote_answer_applied != snapshot.remote_answer_applied
                        || previous.applied_remote_candidate_count
                            != snapshot.applied_remote_candidate_count
                        || previous.sent_sample_count != snapshot.sent_sample_count
                        || previous.latest_sent_packet_len != snapshot.latest_sent_packet_len
                        || previous.latest_error != snapshot.latest_error
                        || previous.updated_at_ms != snapshot.updated_at_ms
                        || now - previous.emitted_at_ms >= 1_000
                })
                .unwrap_or(true),
            Err(_) => break,
        };
        if !should_send {
            thread::sleep(Duration::from_millis(500));
            continue;
        }

        let event = HostTransportStatusEvent {
            session_id: snapshot.session_id.clone(),
            peer_connection_state: snapshot.peer_connection_state.clone(),
            remote_answer_applied: snapshot.remote_answer_applied,
            applied_remote_candidate_count: snapshot.applied_remote_candidate_count,
            sent_sample_count: snapshot.sent_sample_count,
            latest_sent_packet_len: snapshot.latest_sent_packet_len as u64,
            latest_error: snapshot.latest_error.clone(),
            updated_at_ms: snapshot.updated_at_ms,
        };
        let request_id = format!("server-transport-{}", now);
        match send_payload_shared(
            &websocket,
            &request_id,
            ControlMessageKind::Event,
            "HostTransportStatusEvent",
            event,
        ) {
            Ok(()) => {
                if let Ok(mut sent) = sent_state.lock() {
                    *sent = Some(SentTransportEventState {
                        session_id: snapshot.session_id,
                        peer_connection_state: snapshot.peer_connection_state,
                        remote_answer_applied: snapshot.remote_answer_applied,
                        applied_remote_candidate_count: snapshot.applied_remote_candidate_count,
                        sent_sample_count: snapshot.sent_sample_count,
                        latest_sent_packet_len: snapshot.latest_sent_packet_len,
                        latest_error: snapshot.latest_error,
                        updated_at_ms: snapshot.updated_at_ms,
                        emitted_at_ms: now,
                    });
                }
            }
            Err(err) => match err.kind() {
                io::ErrorKind::BrokenPipe
                | io::ErrorKind::ConnectionReset
                | io::ErrorKind::ConnectionAborted
                | io::ErrorKind::UnexpectedEof => break,
                _ => eprintln!(
                    "control transport-status pump failed for session {}: {}",
                    session_id, err
                ),
            },
        }

        thread::sleep(Duration::from_millis(500));
    })
}

fn send_payload_shared<T: ProstMessage>(
    websocket: &SharedControlWebSocket,
    request_id: &str,
    kind: ControlMessageKind,
    message_type: &str,
    payload: T,
) -> io::Result<()> {
    let envelope = ControlEnvelope {
        request_id: request_id.to_string(),
        kind: kind as i32,
        r#type: message_type.to_string(),
        payload: payload.encode_to_vec(),
        timestamp_ms: now_ms(),
    };
    websocket
        .try_send(WsMessage::Binary(envelope.encode_to_vec()))
        .map_err(|err| match err {
            std::sync::mpsc::TrySendError::Full(_) => {
                io::Error::new(io::ErrorKind::WouldBlock, "control event queue full")
            }
            std::sync::mpsc::TrySendError::Disconnected(_) => {
                io::Error::new(io::ErrorKind::BrokenPipe, "control connection closed")
            }
        })
}

fn pairing_required_error(settings: &crate::settings::HostSettings) -> ErrorInfo {
    match settings.connection_method() {
        ConnectionMethod::TrustOnly => simple_error(
            "AUTH_TRUST_ONLY_CONSENT_REQUIRED",
            "This host requires local desktop approval before the controller can connect.",
        ),
        ConnectionMethod::AccessCodeAndConsent => simple_error(
            "AUTH_ACCESS_CODE_AND_CONSENT_REQUIRED",
            "This host requires an access code and local desktop approval before the controller can connect.",
        ),
        ConnectionMethod::AccessCode => simple_error(
            "AUTH_ACCESS_CODE_REQUIRED",
            "Pairing with the current host access code is required before this controller can connect.",
        ),
    }
}

fn send_start_session_rejected(
    websocket: &SharedControlWebSocket,
    request_id: &str,
    session_id: String,
    error: ErrorInfo,
) -> io::Result<()> {
    send_payload_shared(
        websocket,
        request_id,
        ControlMessageKind::Response,
        "StartSessionRejected",
        StartSessionRejected {
            session_id,
            error: Some(error),
        },
    )
}

fn simple_error(code: &str, message: &str) -> ErrorInfo {
    ErrorInfo {
        code: code.to_string(),
        message: message.to_string(),
        retryable: true,
        detail: String::new(),
    }
}

fn ws_err(err: WsError) -> io::Error {
    match err {
        WsError::Io(err) => err,
        WsError::ConnectionClosed | WsError::AlreadyClosed => {
            io::Error::new(io::ErrorKind::BrokenPipe, "control connection closed")
        }
        err => io::Error::new(io::ErrorKind::Other, err.to_string()),
    }
}

fn decode_err(err: prost::DecodeError) -> io::Error {
    io::Error::new(io::ErrorKind::InvalidData, err.to_string())
}

fn lock_err<T>(_: std::sync::PoisonError<T>) -> io::Error {
    io::Error::new(io::ErrorKind::Other, "pairing store lock poisoned")
}

fn session_lock_err<T>(_: std::sync::PoisonError<T>) -> io::Error {
    io::Error::new(io::ErrorKind::Other, "session registry lock poisoned")
}

fn settings_lock_err<T>(_: std::sync::PoisonError<T>) -> io::Error {
    io::Error::new(io::ErrorKind::Other, "host settings lock poisoned")
}

fn restore_active_session_for_controller(
    state: &ControlPlaneState,
    active_session_id: &SharedActiveSessionId,
    controller_device_id: &str,
) -> io::Result<()> {
    let snapshot = state
        .sessions
        .lock()
        .map_err(session_lock_err)?
        .active_snapshot_for_controller(controller_device_id);
    let Some(snapshot) = snapshot else {
        return Ok(());
    };
    state
        .signaling
        .lock()
        .map_err(signaling_lock_err)?
        .restore_replayable_local_signaling(&snapshot.session_id);
    let mut active = active_session_id.lock().map_err(lock_err)?;
    *active = Some(snapshot.session_id);
    Ok(())
}

#[allow(clippy::too_many_arguments)]
fn wait_for_local_confirmation(
    websocket: &SharedControlWebSocket,
    state: &ControlPlaneState,
    session: &mut ConnectionSession,
    envelope_request_id: &str,
    result_request_id: String,
    controller_device_id: String,
    controller_display_name: String,
    connection_method: String,
    after_access_code: bool,
    challenge_id: Option<String>,
) -> io::Result<()> {
    let pending = state
        .pending_remote_requests
        .lock()
        .map_err(pending_request_lock_err)?
        .begin_request(
            controller_device_id.clone(),
            controller_display_name.clone(),
            connection_method.clone(),
        );
    let Some((request_id, response_rx)) = pending else {
        return send_pairing_error(
            websocket,
            envelope_request_id,
            &result_request_id,
            "AUTH_HOST_BUSY",
            "Another local confirmation request is pending.",
        );
    };
    session.pending_confirmation = Some(PendingLocalConfirmation {
        request_id,
        response_rx,
        envelope_request_id: envelope_request_id.to_owned(),
        result_request_id: result_request_id.clone(),
        controller_device_id,
        controller_display_name,
        connection_method: ConnectionMethod::from_wire_value(connection_method),
        after_access_code,
        expires_at_ms: now_ms() + 39_000,
    });
    send_payload_shared(
        websocket,
        envelope_request_id,
        ControlMessageKind::Response,
        "PairingChallenge",
        PairingChallenge {
            request_id: result_request_id,
            challenge_id: challenge_id
                .unwrap_or_else(|| format!("local-confirm-{}", generate_pair_token())),
            challenge_type: PairingChallengeType::LocalConfirm as i32,
            verification_code: String::new(),
        },
    )
}

fn poll_local_confirmation(
    websocket: &SharedControlWebSocket,
    state: &ControlPlaneState,
    session: &mut ConnectionSession,
) -> io::Result<()> {
    let Some(pending) = session.pending_confirmation.as_ref() else {
        return Ok(());
    };
    let expired = now_ms() >= pending.expires_at_ms;
    let allowed = match pending.response_rx.try_recv() {
        Ok(allow) if !expired => allow,
        Err(TryRecvError::Empty) if !expired => return Ok(()),
        _ => false,
    };
    let pending = session
        .pending_confirmation
        .take()
        .expect("pending confirmation exists");
    state
        .pending_remote_requests
        .lock()
        .map_err(pending_request_lock_err)?
        .cancel(&pending.request_id);
    session.pending_challenge = None;
    let settings = state.settings.lock().map_err(settings_lock_err)?.current();
    if !settings.allow_remote_assistance
        || settings.connection_method() != pending.connection_method
    {
        return send_pairing_error(
            websocket,
            &pending.envelope_request_id,
            &pending.result_request_id,
            "AUTH_POLICY_CHANGED",
            "Remote assistance settings changed while awaiting approval.",
        );
    }
    if !allowed {
        let code = if expired {
            "AUTH_LOCAL_CONSENT_TIMEOUT"
        } else if pending.after_access_code {
            "AUTH_LOCAL_CONSENT_REJECTED_AFTER_ACCESS_CODE"
        } else {
            "AUTH_LOCAL_CONSENT_REJECTED"
        };
        return send_pairing_error(
            websocket,
            &pending.envelope_request_id,
            &pending.result_request_id,
            code,
            "The remote assistance request was rejected or expired.",
        );
    }
    let pair_token = generate_pair_token();
    state
        .pairing_store
        .lock()
        .map_err(lock_err)?
        .upsert_pairing(
            &pending.controller_device_id,
            &pending.controller_display_name,
            &pair_token,
        )?;
    session.authenticated_controller_device_id = Some(pending.controller_device_id);
    send_payload_shared(
        websocket,
        &pending.envelope_request_id,
        ControlMessageKind::Response,
        "PairingResult",
        PairingResult {
            request_id: pending.result_request_id,
            accepted: true,
            pair_token,
            error: None,
        },
    )
}

fn send_pairing_error(
    websocket: &SharedControlWebSocket,
    envelope_id: &str,
    request_id: &str,
    code: &str,
    message: &str,
) -> io::Result<()> {
    send_payload_shared(
        websocket,
        envelope_id,
        ControlMessageKind::Response,
        "PairingResult",
        PairingResult {
            request_id: request_id.to_owned(),
            accepted: false,
            pair_token: String::new(),
            error: Some(simple_error(code, message)),
        },
    )
}

fn require_session_owner(
    state: &ControlPlaneState,
    connection: &ConnectionSession,
    session_id: &str,
) -> io::Result<()> {
    let owned = connection
        .authenticated_controller_device_id
        .as_deref()
        .and_then(|id| {
            state
                .sessions
                .lock()
                .ok()?
                .active_snapshot_for_controller(id)
        })
        .is_some_and(|snapshot| snapshot.session_id == session_id);
    if owned {
        Ok(())
    } else {
        Err(io::Error::new(
            io::ErrorKind::PermissionDenied,
            "signaling does not belong to this controller",
        ))
    }
}

fn pending_request_lock_err<T>(_: std::sync::PoisonError<T>) -> io::Error {
    io::Error::new(io::ErrorKind::Other, "pending request store lock poisoned")
}

fn signaling_lock_err<T>(_: std::sync::PoisonError<T>) -> io::Error {
    io::Error::new(io::ErrorKind::Other, "signaling state lock poisoned")
}

fn video_stream_lock_err<T>(_: std::sync::PoisonError<T>) -> io::Error {
    io::Error::new(io::ErrorKind::Other, "video stream state lock poisoned")
}

fn transport_status_lock_err<T>(_: std::sync::PoisonError<T>) -> io::Error {
    io::Error::new(io::ErrorKind::Other, "transport status lock poisoned")
}

fn flush_local_signaling_events(
    websocket: &SharedControlWebSocket,
    request_id: &str,
    state: &ControlPlaneState,
    session_id: &str,
) -> io::Result<()> {
    let (offer, candidates) = {
        let signaling = state.signaling.lock().map_err(signaling_lock_err)?;
        (
            signaling.local_offer(session_id),
            signaling.local_candidates_snapshot(session_id),
        )
    };
    if let Some(offer) = offer {
        send_payload_shared(
            websocket,
            request_id,
            ControlMessageKind::Event,
            "WebRtcOffer",
            offer.clone(),
        )?;
        state
            .signaling
            .lock()
            .map_err(signaling_lock_err)?
            .clear_local_offer(session_id, &offer);
    }
    for candidate in candidates {
        send_payload_shared(
            websocket,
            request_id,
            ControlMessageKind::Event,
            "IceCandidate",
            candidate.clone(),
        )?;
        state
            .signaling
            .lock()
            .map_err(signaling_lock_err)?
            .mark_local_candidate_sent(session_id, &candidate);
    }
    Ok(())
}

pub fn default_capability_flags() -> CapabilityFlags {
    let video_support = crate::video::detect_runtime_video_support();
    CapabilityFlags {
        supports_audio: false,
        supports_input: false,
        supports_display_control: false,
        supports_hardware_encode: video_support.has_h264_hardware_encoder,
        supports_hardware_decode: false,
        supports_software_encode: video_support.has_h264_software_encoder,
        supports_software_decode: video_support.has_h264_decoder,
    }
}

pub fn default_host_identity(
    device_id: String,
    display_name: String,
    hostname: String,
    app_version: String,
) -> DeviceIdentity {
    DeviceIdentity {
        device_id,
        display_name,
        platform: PlatformKind::Windows as i32,
        hostname,
        app_version,
    }
}

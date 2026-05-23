use crate::generated::v1::{
    CapabilityFlags, ControlEnvelope, ControlHello, ControlHelloAck, ControlMessageKind,
    DeviceIdentity, DevicePresence, DeviceSnapshot, DeviceSnapshotResponse, ErrorInfo,
    GetDeviceSnapshot, HeartbeatPing, HeartbeatPong, PairingChallenge, PairingChallengeType,
    PairingConfirm, PairingRequest, PairingResult, PlatformKind, SessionChangedEvent,
    StartSessionAccepted, StartSessionRejected, StartSessionRequest,
};
use crate::pairing::{
    generate_pair_token, generate_verification_code, now_ms, PendingChallenge, SharedPairingStore,
};
use crate::session::SharedSessionRegistry;
use prost::Message as ProstMessage;
use std::io;
use std::net::{SocketAddr, TcpListener, TcpStream};
use std::sync::Arc;
use std::thread;
use tungstenite::{accept, Error as WsError, Message as WsMessage, WebSocket};

pub const DEFAULT_CONTROL_PORT: u16 = 46661;

#[derive(Debug, Clone)]
pub struct ControlPlaneState {
    pub host_identity: DeviceIdentity,
    pub advertised_presence: DevicePresence,
    pub capability_flags: CapabilityFlags,
    pub pairing_store: SharedPairingStore,
    pub sessions: SharedSessionRegistry,
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
    let mut websocket =
        accept(stream).map_err(|err| io::Error::new(io::ErrorKind::Other, err.to_string()))?;
    let mut session = ConnectionSession::default();
    loop {
        let frame = websocket.read().map_err(ws_err)?;
        match frame {
            WsMessage::Binary(bytes) => {
                let envelope = ControlEnvelope::decode(&bytes[..]).map_err(decode_err)?;
                handle_envelope(&mut websocket, &state, &mut session, envelope)?;
            }
            WsMessage::Close(_) => break,
            WsMessage::Ping(payload) => websocket.send(WsMessage::Pong(payload)).map_err(ws_err)?,
            _ => {}
        }
    }
    Ok(())
}

#[derive(Debug, Default)]
struct ConnectionSession {
    authenticated_controller_device_id: Option<String>,
    pending_challenge: Option<PendingChallenge>,
}

fn handle_envelope(
    websocket: &mut WebSocket<TcpStream>,
    state: &ControlPlaneState,
    session: &mut ConnectionSession,
    envelope: ControlEnvelope,
) -> io::Result<()> {
    match envelope.r#type.as_str() {
        "ControlHello" => {
            let hello = ControlHello::decode(&envelope.payload[..]).map_err(decode_err)?;
            let accepted = if hello.pair_token.is_empty() {
                false
            } else {
                state
                    .pairing_store
                    .lock()
                    .map_err(lock_err)?
                    .verify_pair_token(&hello.controller_device_id, &hello.pair_token)?
            };
            if accepted {
                session.authenticated_controller_device_id = Some(hello.controller_device_id);
            }
            let ack = ControlHelloAck {
                request_id: hello.request_id.clone(),
                accepted,
                identity: Some(state.host_identity.clone()),
                capability_flags: Some(state.capability_flags.clone()),
                error: (!accepted).then(|| pairing_required_error()),
            };
            send_payload(
                websocket,
                &envelope.request_id,
                ControlMessageKind::Response,
                "ControlHelloAck",
                ack,
            )
        }
        "PairingRequest" => {
            let request = PairingRequest::decode(&envelope.payload[..]).map_err(decode_err)?;
            let challenge = PendingChallenge {
                request_id: request.request_id.clone(),
                controller_device_id: request.controller_device_id.clone(),
                controller_display_name: request.controller_display_name.clone(),
                challenge_id: format!("challenge-{}", crate::pairing::generate_pair_token()),
                verification_code: generate_verification_code(),
            };
            eprintln!(
                "pairing challenge for {} ({}) => {}",
                challenge.controller_display_name,
                challenge.controller_device_id,
                challenge.verification_code
            );
            session.pending_challenge = Some(challenge.clone());
            let message = PairingChallenge {
                request_id: request.request_id,
                challenge_id: challenge.challenge_id,
                challenge_type: PairingChallengeType::OneTimeCode as i32,
                verification_code: challenge.verification_code,
            };
            send_payload(
                websocket,
                &envelope.request_id,
                ControlMessageKind::Response,
                "PairingChallenge",
                message,
            )
        }
        "PairingConfirm" => {
            let confirm = PairingConfirm::decode(&envelope.payload[..]).map_err(decode_err)?;
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
                return send_payload(
                    websocket,
                    &envelope.request_id,
                    ControlMessageKind::Response,
                    "PairingResult",
                    result,
                );
            };

            if challenge.challenge_id != confirm.challenge_id
                || challenge.verification_code != confirm.verification_code.trim().to_uppercase()
            {
                let result = PairingResult {
                    request_id: confirm.request_id,
                    accepted: false,
                    pair_token: String::new(),
                    error: Some(simple_error(
                        "PAIRING_CODE_INVALID",
                        "The verification code is invalid.",
                    )),
                };
                return send_payload(
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
            send_payload(
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
                let response = DeviceSnapshotResponse {
                    request_id: request.request_id,
                    snapshot: None,
                    error: Some(pairing_required_error()),
                };
                return send_payload(
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
            send_payload(
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
            send_payload(
                websocket,
                &envelope.request_id,
                ControlMessageKind::Event,
                "HeartbeatPong",
                pong,
            )
        }
        "StartSessionRequest" => {
            if session.authenticated_controller_device_id.is_none() {
                return send_start_session_rejected(
                    websocket,
                    &envelope.request_id,
                    String::new(),
                    pairing_required_error(),
                );
            }

            let request = StartSessionRequest::decode(&envelope.payload[..]).map_err(decode_err)?;
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

            let snapshot = state
                .sessions
                .lock()
                .map_err(session_lock_err)?
                .activate_preparing(
                    intent.session_id,
                    intent.controller_device_id,
                    intent.controlled_device_id,
                    intent.target_display_id,
                    prepared.spec,
                );

            send_payload(
                websocket,
                &envelope.request_id,
                ControlMessageKind::Response,
                "StartSessionAccepted",
                StartSessionAccepted {
                    session_id: snapshot.session_id.clone(),
                    state: snapshot.state,
                },
            )?;
            send_payload(
                websocket,
                &envelope.request_id,
                ControlMessageKind::Event,
                "SessionChangedEvent",
                SessionChangedEvent {
                    snapshot: Some(snapshot),
                },
            )
        }
        other => Err(io::Error::new(
            io::ErrorKind::InvalidInput,
            format!("unsupported control message type: {other}"),
        )),
    }
}

fn send_payload<T: ProstMessage>(
    websocket: &mut WebSocket<TcpStream>,
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
        .send(WsMessage::Binary(envelope.encode_to_vec()))
        .map_err(ws_err)
}

fn pairing_required_error() -> ErrorInfo {
    simple_error(
        "AUTH_PAIRING_REQUIRED",
        "Pairing is required before this controller can connect.",
    )
}

fn send_start_session_rejected(
    websocket: &mut WebSocket<TcpStream>,
    request_id: &str,
    session_id: String,
    error: ErrorInfo,
) -> io::Result<()> {
    send_payload(
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
    io::Error::new(io::ErrorKind::Other, err.to_string())
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

pub fn default_capability_flags() -> CapabilityFlags {
    let video_support = crate::video::detect_runtime_video_support();
    CapabilityFlags {
        supports_audio: false,
        supports_input: true,
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

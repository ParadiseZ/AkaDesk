use host_runtime::approval::{PendingRemoteRequestStore, SharedPendingRemoteRequestStore};
use host_runtime::control::{
    default_capability_flags, default_host_identity, spawn_control_server, ControlPlaneState,
};
use host_runtime::discovery::{publish_presence, AKADESK_SERVICE_TYPE};
use host_runtime::generated::v1::{
    ControlEnvelope, ControlHello, ControlHelloAck, ControlMessageKind, DevicePresence,
    DeviceSnapshotResponse, GetDeviceSnapshot, PairingChallenge, PairingConfirm, PairingRequest,
    PairingResult, PlatformKind, SessionChangedEvent, SessionIntent, StartSessionAccepted,
    StartSessionRequest, VersionInfo, VideoCodec, VideoPreference,
};
use host_runtime::pairing::{now_ms, PairingStore};
use host_runtime::session::SessionRegistry;
use host_runtime::signaling::{SharedSignalingState, SignalingState};
use host_runtime::settings::HostSettingsStore;
use host_runtime::transport::{SharedTransportStatusState, TransportStatusState};
use host_runtime::video::{SharedVideoStreamState, VideoStreamState};
use mdns_sd::{Receiver, ServiceDaemon, ServiceEvent};
use prost::Message;
use std::fs;
use std::net::TcpListener;
use std::sync::{Arc, Mutex};
use std::thread;
use std::time::{Duration, Instant};
use tungstenite::{connect, Message as WsMessage};

#[test]
fn discovery_publish_pairing_and_reauth_work_together() {
    let port = allocate_port();
    let state = test_control_state(port);
    let _control_server =
        spawn_control_server(state.clone()).expect("failed to start control server");
    let _mdns = publish_presence(&state.advertised_presence).expect("failed to publish mdns");
    thread::sleep(Duration::from_millis(500));

    let browser = ServiceDaemon::new().expect("failed to create mdns browser");
    let receiver = browser
        .browse(AKADESK_SERVICE_TYPE)
        .expect("failed to browse services");

    let resolved = wait_for_resolved_service(receiver, "host-test-device");
    assert_eq!(resolved.get_port(), port);
    assert_eq!(
        resolved.get_property_val_str("device_id"),
        Some("host-test-device")
    );

    let mut socket = connect_ws(port);

    let hello = ControlHello {
        request_id: "hello-1".to_string(),
        controller_device_id: "android-controller-1".to_string(),
        pair_token: String::new(),
        version: Some(test_version(PlatformKind::Android)),
        supported_features: vec!["pairing".to_string()],
    };
    send_envelope(
        &mut socket,
        "hello-1",
        ControlMessageKind::Request,
        "ControlHello",
        hello,
    );
    let hello_ack: ControlHelloAck = recv_typed(&mut socket, "ControlHelloAck");
    assert!(!hello_ack.accepted);

    let pairing_request = PairingRequest {
        request_id: "pairing-1".to_string(),
        controller_device_id: "android-controller-1".to_string(),
        controller_display_name: "Android Test".to_string(),
        version: Some(test_version(PlatformKind::Android)),
    };
    send_envelope(
        &mut socket,
        "pairing-1",
        ControlMessageKind::Request,
        "PairingRequest",
        pairing_request,
    );
    let challenge: PairingChallenge = recv_typed(&mut socket, "PairingChallenge");
    assert!(!challenge.verification_code.is_empty());

    let confirm = PairingConfirm {
        request_id: "pairing-confirm-1".to_string(),
        challenge_id: challenge.challenge_id.clone(),
        verification_code: challenge.verification_code.clone(),
    };
    send_envelope(
        &mut socket,
        "pairing-confirm-1",
        ControlMessageKind::Request,
        "PairingConfirm",
        confirm,
    );
    let pairing_result: PairingResult = recv_typed(&mut socket, "PairingResult");
    assert!(pairing_result.accepted);
    assert!(!pairing_result.pair_token.is_empty());

    drop(socket);

    let mut reconnect_socket = connect_ws(port);
    let reconnect_hello = ControlHello {
        request_id: "hello-2".to_string(),
        controller_device_id: "android-controller-1".to_string(),
        pair_token: pairing_result.pair_token.clone(),
        version: Some(test_version(PlatformKind::Android)),
        supported_features: vec!["pairing".to_string()],
    };
    send_envelope(
        &mut reconnect_socket,
        "hello-2",
        ControlMessageKind::Request,
        "ControlHello",
        reconnect_hello,
    );
    let reconnect_ack: ControlHelloAck = recv_typed(&mut reconnect_socket, "ControlHelloAck");
    assert!(reconnect_ack.accepted);

    let snapshot_request = GetDeviceSnapshot {
        request_id: "snapshot-1".to_string(),
    };
    send_envelope(
        &mut reconnect_socket,
        "snapshot-1",
        ControlMessageKind::Request,
        "GetDeviceSnapshot",
        snapshot_request,
    );
    let snapshot_response: DeviceSnapshotResponse =
        recv_typed(&mut reconnect_socket, "DeviceSnapshotResponse");
    assert!(snapshot_response.snapshot.is_some());

    #[cfg(windows)]
    {
        let start_session = StartSessionRequest {
            intent: Some(SessionIntent {
                session_id: String::new(),
                controller_device_id: "android-controller-1".to_string(),
                controlled_device_id: "host-test-device".to_string(),
                target_display_id: String::new(),
                enable_input: true,
                video: Some(VideoPreference {
                    codec: VideoCodec::H264 as i32,
                    width: 1280,
                    height: 720,
                    fps: 30,
                    bitrate_kbps: 4_000,
                    allow_same_codec_software_fallback: true,
                }),
                audio: None,
            }),
        };
        send_envelope(
            &mut reconnect_socket,
            "start-session-1",
            ControlMessageKind::Request,
            "StartSessionRequest",
            start_session,
        );
        let accepted: StartSessionAccepted =
            recv_typed(&mut reconnect_socket, "StartSessionAccepted");
        assert!(!accepted.session_id.is_empty());
        let changed: SessionChangedEvent = recv_typed(&mut reconnect_socket, "SessionChangedEvent");
        let snapshot = changed.snapshot.expect("missing session snapshot");
        assert_eq!(snapshot.session_id, accepted.session_id);
        let video = snapshot.video.expect("missing video spec");
        assert_eq!(video.codec, VideoCodec::H264 as i32);
        assert_eq!(video.width, 1280);
        assert_eq!(video.height, 720);
    }
}

fn test_control_state(port: u16) -> ControlPlaneState {
    let base_dir = std::env::temp_dir().join(format!("akadesk-m3-test-{}", now_ms()));
    fs::create_dir_all(&base_dir).expect("failed to create temp test dir");
    let pairing_store = PairingStore::load(&base_dir).expect("failed to create pairing store");
    let settings = Arc::new(Mutex::new(
        HostSettingsStore::load(&base_dir).expect("failed to create host settings store"),
    ));
    let pending_remote_requests: SharedPendingRemoteRequestStore =
        Arc::new(Mutex::new(PendingRemoteRequestStore::default()));
    let video_streams: SharedVideoStreamState =
        Arc::new(Mutex::new(VideoStreamState::default()));
    let signaling: SharedSignalingState =
        Arc::new(Mutex::new(SignalingState::default()));
    let transport_status: SharedTransportStatusState =
        Arc::new(Mutex::new(TransportStatusState::default()));
    let host_identity = default_host_identity(
        "host-test-device".to_string(),
        "AkaDesk Test Host".to_string(),
        "akadesk-test-host".to_string(),
        "0.1.0-test".to_string(),
    );
    ControlPlaneState {
        host_identity: host_identity.clone(),
        advertised_presence: DevicePresence {
            identity: Some(host_identity),
            lan_address: "127.0.0.1".to_string(),
            control_port: port as u32,
            paired: false,
            reachable: true,
            last_seen_ms: now_ms(),
        },
        capability_flags: default_capability_flags(),
        pairing_store: Arc::new(Mutex::new(pairing_store)),
        sessions: Arc::new(Mutex::new(SessionRegistry::default())),
        settings,
        pending_remote_requests,
        video_streams,
        signaling,
        transport_status,
    }
}

fn wait_for_resolved_service(
    receiver: Receiver<ServiceEvent>,
    expected_device_id: &str,
) -> mdns_sd::ServiceInfo {
    let started = Instant::now();
    while started.elapsed() < Duration::from_secs(15) {
        if let Ok(ServiceEvent::ServiceResolved(info)) =
            receiver.recv_timeout(Duration::from_secs(1))
        {
            if info.get_property_val_str("device_id") == Some(expected_device_id) {
                return info;
            }
        }
    }
    panic!("timed out waiting for resolved mdns service");
}

fn allocate_port() -> u16 {
    let listener = TcpListener::bind("127.0.0.1:0").expect("failed to allocate port");
    let port = listener.local_addr().expect("missing local addr").port();
    drop(listener);
    port
}

fn connect_ws(
    port: u16,
) -> tungstenite::WebSocket<tungstenite::stream::MaybeTlsStream<std::net::TcpStream>> {
    let (socket, _) =
        connect(format!("ws://127.0.0.1:{port}/")).expect("failed to connect websocket");
    socket
}

fn send_envelope<T: Message>(
    socket: &mut tungstenite::WebSocket<tungstenite::stream::MaybeTlsStream<std::net::TcpStream>>,
    request_id: &str,
    kind: ControlMessageKind,
    message_type: &str,
    payload: T,
) {
    let envelope = ControlEnvelope {
        request_id: request_id.to_string(),
        kind: kind as i32,
        r#type: message_type.to_string(),
        payload: payload.encode_to_vec(),
        timestamp_ms: now_ms(),
    };
    socket
        .send(WsMessage::Binary(envelope.encode_to_vec()))
        .expect("failed to send envelope");
}

fn recv_typed<T: Message + Default>(
    socket: &mut tungstenite::WebSocket<tungstenite::stream::MaybeTlsStream<std::net::TcpStream>>,
    expected_type: &str,
) -> T {
    loop {
        let frame = socket.read().expect("failed to read websocket frame");
        if let WsMessage::Binary(bytes) = frame {
            let envelope = ControlEnvelope::decode(&bytes[..]).expect("failed to decode envelope");
            if envelope.r#type == expected_type {
                return T::decode(&envelope.payload[..]).expect("failed to decode payload");
            }
        }
    }
}

fn test_version(platform: PlatformKind) -> VersionInfo {
    VersionInfo {
        schema_version: 1,
        app_version: "1.0".to_string(),
        build_channel: "test".to_string(),
        platform: platform as i32,
    }
}

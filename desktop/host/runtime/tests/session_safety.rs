use host_runtime::generated::v1::{SessionState, VideoSessionSpec};
use host_runtime::service::RuntimeService;
use host_runtime::session::SessionRegistry;
use host_runtime::video::EncodedVideoPacket;
use std::sync::mpsc::TryRecvError;

#[test]
fn busy_sessions_and_late_callbacks_cannot_replace_or_revive_a_session() {
    let mut registry = SessionRegistry::default();
    let first = registry
        .activate_preparing(
            "client-id".into(),
            "a".into(),
            "host".into(),
            String::new(),
            VideoSessionSpec::default(),
        )
        .unwrap();
    assert!(registry
        .activate_preparing(
            String::new(),
            "b".into(),
            "host".into(),
            String::new(),
            VideoSessionSpec::default()
        )
        .is_none());
    assert!(registry.active_snapshot_for_controller("b").is_none());
    registry.mark_disconnected(&first.session_id).unwrap();
    assert!(registry.mark_streaming(&first.session_id).is_none());
    assert!(registry.mark_failed(&first.session_id).is_none());
    let second = registry
        .activate_preparing(
            first.session_id.clone(),
            "b".into(),
            "host".into(),
            String::new(),
            VideoSessionSpec::default(),
        )
        .unwrap();
    assert_ne!(first.session_id, second.session_id);
    assert!(registry.mark_failed(&first.session_id).is_none());
    assert_eq!(
        registry.active.unwrap().state,
        SessionState::Preparing as i32
    );
}

#[test]
fn desktop_stop_clears_media_and_refreshes_code_only_once() {
    let directory = std::env::temp_dir().join(format!(
        "akadesk-stop-{}",
        host_runtime::pairing::generate_pair_token()
    ));
    let service = RuntimeService::new_in(&directory).unwrap();
    let session = service
        .sessions()
        .lock()
        .unwrap()
        .activate_preparing(
            String::new(),
            "phone".into(),
            "host".into(),
            String::new(),
            VideoSessionSpec::default(),
        )
        .unwrap();
    let receiver = {
        let mut streams = service.video_streams().lock().unwrap();
        streams.begin_session(session.session_id.clone());
        streams.subscribe_packets(&session.session_id).unwrap()
    };
    assert!(!service.stop_session("wrong-id").unwrap().stopped);
    assert!(service.stop_session(&session.session_id).unwrap().stopped);
    assert!(matches!(
        receiver.try_recv(),
        Err(TryRecvError::Disconnected)
    ));
    let first = service.control_state().settings.lock().unwrap().snapshot();
    service.reap_finished_session().unwrap();
    assert!(!service.stop_session(&session.session_id).unwrap().stopped);
    assert_eq!(
        first,
        service.control_state().settings.lock().unwrap().snapshot()
    );
    assert_eq!(
        service
            .sessions()
            .lock()
            .unwrap()
            .active
            .as_ref()
            .unwrap()
            .state,
        SessionState::Disconnected as i32
    );
    service
        .video_streams()
        .lock()
        .unwrap()
        .push_packet(EncodedVideoPacket {
            session_id: session.session_id,
            bytes: vec![1],
            packet_len: 1,
            is_keyframe: true,
            frame_source_label: "late".into(),
            created_at_ms: 1,
        });
    assert_eq!(
        service
            .video_streams()
            .lock()
            .unwrap()
            .snapshot()
            .packet_count,
        0
    );
    drop(service);
    std::fs::remove_dir_all(directory).unwrap();
}

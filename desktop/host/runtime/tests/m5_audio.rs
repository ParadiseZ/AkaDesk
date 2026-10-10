#![cfg(windows)]

use host_contracts::audio::{AudioCapturePort, AudioStartRequest};
use host_runtime::audio::{
    finish_audio_session, recover_audio_output, set_controlled_muted, spawn_audio_stream,
    AudioStreamState,
};
use host_runtime::generated::v1::{AudioPreference, VideoSessionSpec};
use host_runtime::session::SessionRegistry;
use host_windows::audio::WindowsAudioBackend;
use std::sync::{Arc, Mutex};
use std::time::{Duration, Instant};

#[test]
#[ignore = "Uses the real Windows output endpoint and plays a short tone; restores original mute"]
fn real_loopback_survives_endpoint_mute_and_restores_session_output() {
    use std::os::windows::process::CommandExt;
    let runtime = tokio::runtime::Builder::new_current_thread()
        .build()
        .unwrap();
    let backend = WindowsAudioBackend::new().unwrap();
    let history = backend.history();
    let handle = runtime
        .block_on(backend.start_audio_capture(AudioStartRequest {
            session_id: "loopback-probe".into(),
        }))
        .unwrap();
    let ffplay = std::path::Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("../../../third_party/ffmpe-win64-gpl-shared/bin/ffplay.exe");
    let mut tone = std::process::Command::new(ffplay)
        .args([
            "-nodisp",
            "-autoexit",
            "-loglevel",
            "error",
            "-f",
            "lavfi",
            "-i",
            "sine=frequency=440:sample_rate=48000:duration=3",
        ])
        .creation_flags(0x08000000)
        .stdin(std::process::Stdio::null())
        .stdout(std::process::Stdio::null())
        .spawn()
        .unwrap();
    for muted in [false, true] {
        runtime
            .block_on(backend.set_controlled_local_playback_muted(muted))
            .unwrap();
        let end = Instant::now() + Duration::from_millis(800);
        let mut peak = 0.0f32;
        while Instant::now() < end {
            for sample in backend.read_samples().unwrap() {
                peak = peak.max(sample.abs());
            }
            std::thread::sleep(Duration::from_millis(5));
        }
        eprintln!("loopback muted={muted} peak={peak}");
        assert!(
            peak > 0.001,
            "Loopback lost system sound when muted={muted}"
        );
    }
    tone.kill().ok();
    tone.wait().ok();
    runtime
        .block_on(backend.stop_audio_capture(&handle))
        .unwrap();
    runtime
        .block_on(backend.restore_audio_output_setting(history.clone()))
        .unwrap();
    assert_eq!(backend.muted().unwrap(), history.muted);

    let directory = std::env::temp_dir().join(format!(
        "akadesk-audio-{}",
        host_runtime::pairing::generate_pair_token()
    ));
    std::fs::create_dir_all(&directory).unwrap();
    // Simulate a persisted recovery record after an interrupted mute change.
    std::fs::write(
        directory.join("audio_restore.json"),
        serde_json::to_vec(&serde_json::json!({
            "device_id": history.default_output_device_id.as_ref().unwrap().0,
            "muted": history.muted, "volume_percent": history.volume_percent
        }))
        .unwrap(),
    )
    .unwrap();
    runtime
        .block_on(backend.set_controlled_local_playback_muted(!history.muted))
        .unwrap();
    recover_audio_output(&directory).unwrap();
    assert_eq!(backend.muted().unwrap(), history.muted);
    assert!(!directory.join("audio_restore.json").exists());

    let sessions = Arc::new(Mutex::new(SessionRegistry::default()));
    let id = sessions
        .lock()
        .unwrap()
        .activate_preparing(
            String::new(),
            "phone".into(),
            "pc".into(),
            String::new(),
            VideoSessionSpec::default(),
        )
        .unwrap()
        .session_id;
    let streams = Arc::new(Mutex::new(AudioStreamState::new(&directory)));
    let ready = spawn_audio_stream(
        Arc::clone(&sessions),
        Arc::clone(&streams),
        id.clone(),
        AudioPreference {
            mute_controlled_local_playback: true,
            ..Default::default()
        },
    )
    .unwrap();
    struct RestoreOnExit(host_runtime::audio::SharedAudioStreamState, String);
    impl Drop for RestoreOnExit {
        fn drop(&mut self) { let _ = finish_audio_session(&self.0, &self.1); }
    }
    let _restore = RestoreOnExit(Arc::clone(&streams), id.clone());
    ready.recv_timeout(Duration::from_secs(3)).unwrap().unwrap();
    assert!(directory.join("audio_restore.json").exists());
    assert!(set_controlled_muted(&streams, "stale-session", false).is_err());
    assert!(!set_controlled_muted(&streams, &id, false).unwrap());
    assert!(set_controlled_muted(&streams, &id, true).unwrap());
    std::thread::sleep(Duration::from_millis(200));
    let count = streams.lock().unwrap().packet_count;
    assert!(
        count >= 5,
        "Audio worker did not continuously encode silence"
    );
    assert!(!streams
        .lock()
        .unwrap()
        .take_next_packet(&id)
        .unwrap()
        .bytes
        .is_empty());
    finish_audio_session(&streams, &id).unwrap();
    finish_audio_session(&streams, &id).unwrap();
    assert!(!directory.join("audio_restore.json").exists());
    assert_eq!(backend.muted().unwrap(), history.muted);
    assert_eq!(
        WindowsAudioBackend::new().unwrap().history().volume_percent,
        history.volume_percent
    );
    std::fs::remove_dir_all(directory).unwrap();
}

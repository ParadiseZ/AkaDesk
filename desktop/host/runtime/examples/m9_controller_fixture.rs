//! Isolated Windows fixture for the Android controller's real tools/media tests.
fn main() -> Result<(), Box<dyn std::error::Error>> {
    use host_runtime::{control::spawn_control_server, service::RuntimeService};
    let directory = std::path::PathBuf::from(
        std::env::args()
            .nth(1)
            .ok_or("Pass a fresh isolated directory")?,
    );
    if directory.exists() {
        return Err("Use a fresh fixture directory".into());
    }
    let service = RuntimeService::new_in(&directory)?;
    let mut state = service.control_state().clone();
    state.advertised_presence.control_port = 46671;
    let _server = spawn_control_server(state.clone())?;
    let settings = state.settings.lock().unwrap().snapshot();
    std::fs::write(
        directory.join("android_fixture.json"),
        serde_json::to_vec_pretty(&serde_json::json!({
            "hostId":state.host_identity.device_id,"pairCode":settings.temporary_verification_code
        }))?,
    )?;
    println!("Controller fixture ready; auto-stop 600s");
    let started = std::time::Instant::now();
    let mut last_snapshot = std::time::Instant::now();
    while started.elapsed().as_secs() < 600 && !directory.join("stop").exists() {
        let _ = service.reap_finished_session();
        if last_snapshot.elapsed().as_secs() >= 1 {
            let snapshot = state.transport_status.lock().unwrap().snapshot();
            let video = state.video_streams.lock().unwrap().snapshot();
            let events = service
                .diagnostics_response()
                .entries
                .into_iter()
                .map(|e| serde_json::json!({"code":e.code,"message":e.message}))
                .collect::<Vec<_>>();
            std::fs::write(
                directory.join("media_status.json"),
                serde_json::to_vec_pretty(
                    &serde_json::json!({"transport":format!("{snapshot:?}"),"video":format!("{video:?}"),"events":events}),
                )?,
            )?;
            last_snapshot = std::time::Instant::now();
        }
        std::thread::sleep(std::time::Duration::from_millis(100));
    }
    let active_id = state
        .sessions
        .lock()
        .unwrap()
        .active_session_id()
        .map(|s| s.0.clone());
    if let Some(id) = active_id {
        state.sessions.lock().unwrap().mark_disconnected(&id);
    }
    service.reap_finished_session()?;
    Ok(())
}

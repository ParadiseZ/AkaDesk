fn main() -> Result<(), Box<dyn std::error::Error>> {
    use host_runtime::{control::spawn_control_server, service::RuntimeService};
    let directory = std::path::PathBuf::from(
        std::env::args()
            .nth(1)
            .expect("Pass a fresh isolated directory"),
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
    println!("M8 fixture ready; auto-stop 120s");
    let started = std::time::Instant::now();
    while started.elapsed().as_secs() < 120 && !directory.join("stop").exists() {
        let _ = service.reap_finished_session();
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
    let response = service.diagnostics_response();
    let summary = response.snapshot.unwrap_or_default();
    let events = response
        .entries
        .iter()
        .map(
            |e| serde_json::json!({"code":e.code,"message":e.message,"repeatCount":e.repeat_count}),
        )
        .collect::<Vec<_>>();
    let codes = events
        .iter()
        .filter_map(|e| e["code"].as_str())
        .collect::<Vec<_>>();
    let recovered = codes.contains(&"SESSION_CONTROL_INTERRUPTED")
        && codes.contains(&"SESSION_CONTROL_RECOVERED")
        && codes.contains(&"SESSION_CLOSED")
        && !summary.degraded;
    std::fs::write(
        directory.join("final_diagnostics.json"),
        serde_json::to_vec_pretty(&serde_json::json!({
            "recovered":recovered,"degraded":summary.degraded,"events":events,
            "audioJournalRemaining":directory.join("audio_restore.json").exists(),"displayJournalRemaining":directory.join("display_restore.json").exists()
        }))?,
    )?;
    println!("M8 fixture finished; recovered={recovered}");
    if !recovered {
        return Err("Control recovery was not verified".into());
    }
    Ok(())
}

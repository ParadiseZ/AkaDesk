#[cfg(windows)]
fn main() -> Result<(), Box<dyn std::error::Error>> {
    use host_runtime::{control::spawn_control_server, service::RuntimeService};
    use host_windows::display::WindowsDisplayBackend;
    use std::time::{Duration, Instant};
    let directory = std::path::PathBuf::from(
        std::env::args()
            .nth(1)
            .expect("Pass an isolated new fixture directory"),
    );
    if directory.exists() {
        return Err("Use a fresh fixture directory".into());
    }
    std::fs::create_dir_all(&directory)?;
    let backend = WindowsDisplayBackend;
    let display = backend
        .list()?
        .into_iter()
        .find(|d| d.primary)
        .ok_or("No primary display")?;
    let original = backend.read(&display.display_id.0)?;
    let service = RuntimeService::new_in(&directory)?;
    let mut state = service.control_state().clone();
    state.advertised_presence.control_port = 46671;
    let server = spawn_control_server(state.clone())?;
    let settings = state.settings.lock().unwrap().snapshot();
    std::fs::write(
        directory.join("android_fixture.json"),
        serde_json::to_vec_pretty(&serde_json::json!({
            "hostId":state.host_identity.device_id,"pairCode":settings.temporary_verification_code,"port":46671,
            "original":{"width":original.width,"height":original.height,"refreshRate":original.refresh_rate,"scalePercent":original.scale_percent}
        }))?,
    )?;
    println!("M7 fixture ready; original={original:?}; auto-stop 180s");
    let started = Instant::now();
    while started.elapsed() < Duration::from_secs(180) && !directory.join("stop").exists() {
        let _ = service.reap_finished_session();
        std::thread::sleep(Duration::from_millis(100));
    }
    let active_id = state
        .sessions
        .lock()
        .unwrap()
        .active_session_id()
        .map(|id| id.0.clone());
    if let Some(id) = active_id {
        state.sessions.lock().unwrap().mark_disconnected(&id);
    }
    service.reap_finished_session()?;
    drop(server);
    let after = backend.read(&display.display_id.0)?;
    let restored = after == original;
    std::fs::write(
        directory.join("final_display.json"),
        serde_json::to_vec_pretty(&serde_json::json!({
            "restored":restored,"journalRemaining":directory.join("display_restore.json").exists(),
            "current":{"width":after.width,"height":after.height,"refreshRate":after.refresh_rate,"scalePercent":after.scale_percent}
        }))?,
    )?;
    if !restored {
        return Err("Original display was not restored; retain recovery directory".into());
    }
    println!("Original display restored: {after:?}");
    Ok(())
}
#[cfg(not(windows))]
fn main() {
    panic!("Windows desktop required")
}

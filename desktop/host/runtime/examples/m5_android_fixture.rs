//! Isolated, time-bounded real-device fixture. Never loads the installed app's settings.
#[cfg(windows)]
fn main() -> std::io::Result<()> {
    use host_runtime::{control::spawn_control_server, service::RuntimeService};
    use std::time::{Duration, Instant};
    let directory = std::env::args()
        .nth(1)
        .expect("Pass an isolated fixture directory");
    let service = RuntimeService::new_in(std::path::Path::new(&directory))?;
    let mut state = service.control_state().clone();
    state.advertised_presence.control_port = 46671;
    let settings = state.settings.lock().unwrap().current();
    std::fs::write(
        std::path::Path::new(&directory).join("android_fixture.json"),
        serde_json::to_vec(&serde_json::json!({
            "hostId": state.host_identity.device_id, "pairCode": settings.temporary_verification_code,
            "port": 46671
        }))?,
    )?;
    let _server = spawn_control_server(state)?;
    eprintln!("M5 fixture listening on 46671; exits after 180 seconds or target stop file");
    let start = Instant::now();
    while start.elapsed() < Duration::from_secs(180)
        && !std::path::Path::new(&directory).join("stop").exists()
    {
        service.reap_finished_session()?;
        std::thread::sleep(Duration::from_millis(100));
    }
    let id = service
        .sessions()
        .lock()
        .unwrap()
        .active_session_id()
        .cloned();
    if let Some(id) = id {
        service.stop_session(&id.0)?;
    }
    Ok(())
}
#[cfg(not(windows))]
fn main() {
    eprintln!("The M5 fixture requires Windows");
}

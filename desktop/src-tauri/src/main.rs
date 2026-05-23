use desktop_shell::host::HostSupervisor;
use desktop_shell::state::AppHostState;
use std::io;

fn main() -> io::Result<()> {
    let mut supervisor = HostSupervisor::default();
    let bootstrap = supervisor.start()?;
    let runtime_status = bootstrap
        .runtime_status
        .and_then(|response| response.status)
        .ok_or_else(|| io::Error::new(io::ErrorKind::InvalidData, "missing runtime status"))?;
    let capability_snapshot = bootstrap
        .capability_snapshot
        .and_then(|response| response.snapshot)
        .ok_or_else(|| io::Error::new(io::ErrorKind::InvalidData, "missing capability snapshot"))?;
    let supports_input = capability_snapshot
        .input
        .map(|input| input.supports_pointer && input.supports_keyboard)
        .unwrap_or(false);
    let state = AppHostState::ready(runtime_status.state_label.clone(), supports_input);
    println!(
        "desktop-shell bootstrap ok: supervisor_ready={}, host_state={}, runtime_status={}, supports_input={}",
        supervisor.ready, state.label, state.runtime_status_label, state.supports_input
    );
    Ok(())
}

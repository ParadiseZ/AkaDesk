use desktop_shell::host::HostSupervisor;
use desktop_shell::state::AppHostState;

fn main() {
    let supervisor = HostSupervisor::default();
    let state = AppHostState::default();
    println!(
        "desktop-shell skeleton initialized: supervisor_ready={}, host_state={}",
        supervisor.ready, state.label
    );
}

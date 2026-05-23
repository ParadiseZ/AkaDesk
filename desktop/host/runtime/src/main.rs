use host_ipc::server::{serve_forever, ServerState};
use host_runtime::control::spawn_control_server;
use host_runtime::discovery::publish_presence;
use host_runtime::service::RuntimeService;
use std::env;
use std::io;

fn main() -> io::Result<()> {
    let service = RuntimeService::new()?;
    let ipc_name = parse_ipc_name();
    let advertised_presence = service.control_state().advertised_presence.clone();
    let _mdns_daemon = publish_presence(&advertised_presence)?;
    let _control_server = spawn_control_server(service.control_state().clone())?;
    eprintln!(
        "host-runtime listening on IPC `{ipc_name}` and control ws `0.0.0.0:{}`",
        advertised_presence.control_port
    );
    let state = ServerState {
        runtime_status: service.ipc_runtime_status_changed(),
        capability_snapshot: service.ipc_capability_snapshot_changed(),
    };
    serve_forever(&ipc_name, state)
}

fn parse_ipc_name() -> String {
    let mut args = env::args().skip(1);
    while let Some(arg) = args.next() {
        if arg == "--ipc-name" {
            if let Some(value) = args.next() {
                return value;
            }
        }
    }
    "akadesk.m2.ipc".to_string()
}

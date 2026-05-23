use host_runtime::service::RuntimeService;
use host_ipc::server::{serve_forever, ServerState};
use std::env;
use std::io;

fn main() -> io::Result<()> {
    let service = RuntimeService::new();
    let ipc_name = parse_ipc_name();
    eprintln!("host-runtime listening on IPC `{ipc_name}`");
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

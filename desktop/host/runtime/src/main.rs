use host_ipc::server::{serve_forever, IpcRequestHandler};
use host_ipc::types::{
    GetCapabilitySnapshotRequest, GetDiagnosticsRequest, GetHostOverviewRequest,
    GetRuntimeStatusRequest, IpcEnvelope, IpcMessageKind,
    RefreshTemporaryVerificationCodeRequest, UpdateHostSettingsRequest,
    CAPABILITY_SNAPSHOT_CHANGED, DIAGNOSTICS_CHANGED, GET_CAPABILITY_SNAPSHOT, GET_DIAGNOSTICS,
    GET_HOST_OVERVIEW, GET_RUNTIME_STATUS, HOST_OVERVIEW_CHANGED,
    REFRESH_TEMPORARY_VERIFICATION_CODE, RUNTIME_STATUS_CHANGED, UPDATE_HOST_SETTINGS,
};
use host_runtime::control::spawn_control_server;
use host_runtime::discovery::publish_presence;
use host_runtime::service::RuntimeService;
use std::env;
use std::io;
use std::sync::Arc;
use std::time::{SystemTime, UNIX_EPOCH};

use prost::Message;

fn main() -> io::Result<()> {
    let service = Arc::new(RuntimeService::new()?);
    let ipc_name = parse_ipc_name();
    let advertised_presence = service.control_state().advertised_presence.clone();
    let _mdns_daemon = publish_presence(&advertised_presence)?;
    let _control_server = spawn_control_server(service.control_state().clone())?;
    eprintln!(
        "host-runtime listening on IPC `{ipc_name}` and control ws `0.0.0.0:{}`",
        advertised_presence.control_port
    );
    serve_forever(&ipc_name, Arc::new(RuntimeIpcHandler { service }))
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

struct RuntimeIpcHandler {
    service: Arc<RuntimeService>,
}

impl IpcRequestHandler for RuntimeIpcHandler {
    fn initial_events(&self) -> io::Result<Vec<IpcEnvelope>> {
        Ok(vec![
            event_envelope(
                "event-runtime-status",
                RUNTIME_STATUS_CHANGED,
                self.service.ipc_runtime_status_changed(),
            ),
            event_envelope(
                "event-capability-snapshot",
                CAPABILITY_SNAPSHOT_CHANGED,
                self.service.ipc_capability_snapshot_changed(),
            ),
            event_envelope(
                "event-host-overview",
                HOST_OVERVIEW_CHANGED,
                self.service.ipc_host_overview_changed(),
            ),
            event_envelope(
                "event-diagnostics",
                DIAGNOSTICS_CHANGED,
                self.service.ipc_diagnostics_changed(),
            ),
        ])
    }

    fn handle_request(&self, envelope: IpcEnvelope) -> io::Result<IpcEnvelope> {
        match envelope.message_type.as_str() {
            GET_RUNTIME_STATUS => {
                let _ =
                    GetRuntimeStatusRequest::decode(&envelope.payload[..]).map_err(decode_err)?;
                response_envelope(
                    envelope.request_id,
                    host_ipc::types::RUNTIME_STATUS_RESPONSE,
                    self.service.runtime_status_response(),
                )
            }
            GET_CAPABILITY_SNAPSHOT => {
                let _ = GetCapabilitySnapshotRequest::decode(&envelope.payload[..])
                    .map_err(decode_err)?;
                response_envelope(
                    envelope.request_id,
                    host_ipc::types::CAPABILITY_SNAPSHOT_RESPONSE,
                    self.service.capability_snapshot_response(),
                )
            }
            GET_HOST_OVERVIEW => {
                let _ =
                    GetHostOverviewRequest::decode(&envelope.payload[..]).map_err(decode_err)?;
                response_envelope(
                    envelope.request_id,
                    host_ipc::types::HOST_OVERVIEW_RESPONSE,
                    self.service.host_overview_response(),
                )
            }
            UPDATE_HOST_SETTINGS => {
                let request =
                    UpdateHostSettingsRequest::decode(&envelope.payload[..]).map_err(decode_err)?;
                let settings = request.settings.ok_or_else(|| {
                    io::Error::new(io::ErrorKind::InvalidInput, "missing host settings payload")
                })?;
                response_envelope(
                    envelope.request_id,
                    host_ipc::types::UPDATE_HOST_SETTINGS_RESPONSE,
                    self.service.update_host_settings(settings)?,
                )
            }
            REFRESH_TEMPORARY_VERIFICATION_CODE => {
                let _ = RefreshTemporaryVerificationCodeRequest::decode(&envelope.payload[..])
                    .map_err(decode_err)?;
                response_envelope(
                    envelope.request_id,
                    host_ipc::types::REFRESH_TEMPORARY_VERIFICATION_CODE_RESPONSE,
                    self.service.refresh_temporary_verification_code()?,
                )
            }
            GET_DIAGNOSTICS => {
                let _ = GetDiagnosticsRequest::decode(&envelope.payload[..]).map_err(decode_err)?;
                response_envelope(
                    envelope.request_id,
                    host_ipc::types::DIAGNOSTICS_RESPONSE,
                    self.service.diagnostics_response(),
                )
            }
            other => Err(io::Error::new(
                io::ErrorKind::InvalidInput,
                format!("unsupported IPC message type: {other}"),
            )),
        }
    }
}

fn event_envelope<T: Message>(request_id: &str, message_type: &str, payload: T) -> IpcEnvelope {
    IpcEnvelope {
        request_id: request_id.to_string(),
        kind: IpcMessageKind::Event as i32,
        message_type: message_type.to_string(),
        payload: payload.encode_to_vec(),
        timestamp_ms: now_ms(),
    }
}

fn response_envelope<T: Message>(
    request_id: String,
    message_type: &str,
    payload: T,
) -> io::Result<IpcEnvelope> {
    Ok(IpcEnvelope {
        request_id,
        kind: IpcMessageKind::Response as i32,
        message_type: message_type.to_string(),
        payload: payload.encode_to_vec(),
        timestamp_ms: now_ms(),
    })
}

fn decode_err(err: prost::DecodeError) -> io::Error {
    io::Error::new(io::ErrorKind::InvalidData, err.to_string())
}

fn now_ms() -> i64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|duration| duration.as_millis() as i64)
        .unwrap_or_default()
}

use crate::codec::{read_envelope, write_envelope};
use crate::types::{
    CapabilitySnapshot, CapabilitySnapshotChanged, CapabilitySnapshotResponse,
    GetCapabilitySnapshotRequest, GetRuntimeStatusRequest, IpcEnvelope, IpcMessageKind,
    RuntimeStatusChanged, RuntimeStatusResponse, CAPABILITY_SNAPSHOT_CHANGED,
    CAPABILITY_SNAPSHOT_RESPONSE, GET_CAPABILITY_SNAPSHOT, GET_RUNTIME_STATUS,
    RUNTIME_STATUS_CHANGED, RUNTIME_STATUS_RESPONSE,
};
use interprocess::local_socket::{prelude::*, GenericFilePath, GenericNamespaced, ListenerOptions};
use prost::Message;
use std::io;
use std::path::PathBuf;
use std::time::{SystemTime, UNIX_EPOCH};

#[derive(Debug, Clone)]
pub struct ServerState {
    pub runtime_status: RuntimeStatusChanged,
    pub capability_snapshot: CapabilitySnapshotChanged,
}

pub fn serve_forever(socket_name: &str, state: ServerState) -> io::Result<()> {
    let listener = if GenericNamespaced::is_supported() {
        ListenerOptions::new()
            .name(socket_name.to_ns_name::<GenericNamespaced>()?)
            .try_overwrite(true)
            .create_sync()?
    } else {
        let path = socket_path(socket_name);
        ListenerOptions::new()
            .name(path.to_fs_name::<GenericFilePath>()?)
            .try_overwrite(true)
            .create_sync()?
    };

    for stream in listener.incoming() {
        let mut stream = stream?;
        let runtime_event = IpcEnvelope {
            request_id: "event-runtime-status".to_string(),
            kind: IpcMessageKind::Event as i32,
            message_type: RUNTIME_STATUS_CHANGED.to_string(),
            payload: state.runtime_status.encode_to_vec(),
            timestamp_ms: now_ms(),
        };
        write_envelope(&mut stream, &runtime_event)?;

        let capability_event = IpcEnvelope {
            request_id: "event-capability-snapshot".to_string(),
            kind: IpcMessageKind::Event as i32,
            message_type: CAPABILITY_SNAPSHOT_CHANGED.to_string(),
            payload: state.capability_snapshot.encode_to_vec(),
            timestamp_ms: now_ms(),
        };
        write_envelope(&mut stream, &capability_event)?;

        loop {
            let envelope = match read_envelope(&mut stream) {
                Ok(envelope) => envelope,
                Err(err) if err.kind() == io::ErrorKind::UnexpectedEof => break,
                Err(err) if err.kind() == io::ErrorKind::ConnectionReset => break,
                Err(err) => return Err(err),
            };

            let response = handle_request(envelope, &state)?;
            write_envelope(&mut stream, &response)?;
        }
    }

    Ok(())
}

fn handle_request(envelope: IpcEnvelope, state: &ServerState) -> io::Result<IpcEnvelope> {
    match envelope.message_type.as_str() {
        GET_RUNTIME_STATUS => {
            let _ = GetRuntimeStatusRequest::decode(&envelope.payload[..])
                .map_err(|err| io::Error::new(io::ErrorKind::InvalidData, err.to_string()))?;
            let response = RuntimeStatusResponse {
                status: state.runtime_status.status.clone(),
                error: None,
            };
            Ok(IpcEnvelope {
                request_id: envelope.request_id,
                kind: IpcMessageKind::Response as i32,
                message_type: RUNTIME_STATUS_RESPONSE.to_string(),
                payload: response.encode_to_vec(),
                timestamp_ms: now_ms(),
            })
        }
        GET_CAPABILITY_SNAPSHOT => {
            let _ = GetCapabilitySnapshotRequest::decode(&envelope.payload[..])
                .map_err(|err| io::Error::new(io::ErrorKind::InvalidData, err.to_string()))?;
            let snapshot: Option<CapabilitySnapshot> = state.capability_snapshot.snapshot.clone();
            let response = CapabilitySnapshotResponse {
                snapshot,
                error: None,
            };
            Ok(IpcEnvelope {
                request_id: envelope.request_id,
                kind: IpcMessageKind::Response as i32,
                message_type: CAPABILITY_SNAPSHOT_RESPONSE.to_string(),
                payload: response.encode_to_vec(),
                timestamp_ms: now_ms(),
            })
        }
        other => Err(io::Error::new(
            io::ErrorKind::InvalidInput,
            format!("unsupported IPC message type: {other}"),
        )),
    }
}

fn socket_path(socket_name: &str) -> PathBuf {
    std::env::temp_dir().join(format!("{socket_name}.sock"))
}

fn now_ms() -> i64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|duration| duration.as_millis() as i64)
        .unwrap_or_default()
}

use crate::codec::{read_envelope, write_envelope};
use crate::types::{
    CapabilitySnapshotChanged, CapabilitySnapshotResponse, GetCapabilitySnapshotRequest,
    GetRuntimeStatusRequest, IpcEnvelope, IpcMessageKind, RuntimeStatusChanged,
    RuntimeStatusResponse, CAPABILITY_SNAPSHOT_CHANGED, CAPABILITY_SNAPSHOT_RESPONSE,
    GET_CAPABILITY_SNAPSHOT, GET_RUNTIME_STATUS, RUNTIME_STATUS_CHANGED, RUNTIME_STATUS_RESPONSE,
};
use interprocess::local_socket::{prelude::*, GenericFilePath, GenericNamespaced, Stream};
use prost::Message;
use std::io;
use std::path::PathBuf;
use std::time::{Duration, SystemTime, UNIX_EPOCH};

#[derive(Debug)]
pub struct IpcClient {
    stream: Stream,
    request_counter: u64,
}

impl IpcClient {
    pub fn connect(socket_name: &str) -> io::Result<Self> {
        let stream = if GenericNamespaced::is_supported() {
            Stream::connect(socket_name.to_ns_name::<GenericNamespaced>()?)?
        } else {
            let path = socket_path(socket_name);
            Stream::connect(path.to_fs_name::<GenericFilePath>()?)?
        };
        Ok(Self {
            stream,
            request_counter: 0,
        })
    }

    pub fn connect_with_retry(socket_name: &str, timeout: Duration) -> io::Result<Self> {
        let started = std::time::Instant::now();
        loop {
            match Self::connect(socket_name) {
                Ok(client) => return Ok(client),
                Err(err) if started.elapsed() < timeout => {
                    std::thread::sleep(Duration::from_millis(100));
                    if err.kind() == io::ErrorKind::NotFound
                        || err.kind() == io::ErrorKind::ConnectionRefused
                    {
                        continue;
                    }
                }
                Err(err) => return Err(err),
            }
        }
    }

    pub fn recv_runtime_status_changed(&mut self) -> io::Result<RuntimeStatusChanged> {
        let envelope = read_envelope(&mut self.stream)?;
        if envelope.message_type != RUNTIME_STATUS_CHANGED {
            return Err(io::Error::new(
                io::ErrorKind::InvalidData,
                format!(
                    "expected {RUNTIME_STATUS_CHANGED}, got {}",
                    envelope.message_type
                ),
            ));
        }
        RuntimeStatusChanged::decode(&envelope.payload[..])
            .map_err(|err| io::Error::new(io::ErrorKind::InvalidData, err.to_string()))
    }

    pub fn recv_capability_snapshot_changed(&mut self) -> io::Result<CapabilitySnapshotChanged> {
        let envelope = read_envelope(&mut self.stream)?;
        if envelope.message_type != CAPABILITY_SNAPSHOT_CHANGED {
            return Err(io::Error::new(
                io::ErrorKind::InvalidData,
                format!(
                    "expected {CAPABILITY_SNAPSHOT_CHANGED}, got {}",
                    envelope.message_type
                ),
            ));
        }
        CapabilitySnapshotChanged::decode(&envelope.payload[..])
            .map_err(|err| io::Error::new(io::ErrorKind::InvalidData, err.to_string()))
    }

    pub fn get_runtime_status(&mut self) -> io::Result<RuntimeStatusResponse> {
        self.request_response(
            GET_RUNTIME_STATUS,
            RUNTIME_STATUS_RESPONSE,
            GetRuntimeStatusRequest {},
        )
    }

    pub fn get_capability_snapshot(&mut self) -> io::Result<CapabilitySnapshotResponse> {
        self.request_response(
            GET_CAPABILITY_SNAPSHOT,
            CAPABILITY_SNAPSHOT_RESPONSE,
            GetCapabilitySnapshotRequest {},
        )
    }

    fn request_response<Req, Res>(
        &mut self,
        request_type: &str,
        response_type: &str,
        payload: Req,
    ) -> io::Result<Res>
    where
        Req: Message,
        Res: Message + Default,
    {
        self.request_counter += 1;
        let envelope = IpcEnvelope {
            request_id: format!("req-{}", self.request_counter),
            kind: IpcMessageKind::Request as i32,
            message_type: request_type.to_string(),
            payload: payload.encode_to_vec(),
            timestamp_ms: now_ms(),
        };
        write_envelope(&mut self.stream, &envelope)?;
        loop {
            let response = read_envelope(&mut self.stream)?;
            if response.message_type == response_type {
                return Res::decode(&response.payload[..])
                    .map_err(|err| io::Error::new(io::ErrorKind::InvalidData, err.to_string()));
            }
        }
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

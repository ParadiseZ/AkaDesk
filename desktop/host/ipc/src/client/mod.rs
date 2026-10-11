use crate::codec::{read_envelope, write_envelope};
use crate::types::{
    CapabilitySnapshotChanged, CapabilitySnapshotResponse, DiagnosticsResponse,
    GetCapabilitySnapshotRequest, GetDiagnosticsRequest, GetHostOverviewRequest,
    GetRuntimeStatusRequest, HostOverviewResponse, IpcEnvelope, IpcMessageKind,
    RefreshTemporaryVerificationCodeRequest, RefreshTemporaryVerificationCodeResponse,
    RespondPendingRemoteRequestRequest, RespondPendingRemoteRequestResponse, RuntimeStatusChanged,
    RuntimeStatusResponse, UpdateHostSettingsRequest, UpdateHostSettingsResponse,
    CAPABILITY_SNAPSHOT_CHANGED, CAPABILITY_SNAPSHOT_RESPONSE, DIAGNOSTICS_RESPONSE,
    GET_CAPABILITY_SNAPSHOT, GET_DIAGNOSTICS, GET_HOST_OVERVIEW, GET_RUNTIME_STATUS,
    HOST_OVERVIEW_RESPONSE, REFRESH_TEMPORARY_VERIFICATION_CODE,
    REFRESH_TEMPORARY_VERIFICATION_CODE_RESPONSE, RESPOND_PENDING_REMOTE_REQUEST,
    RESPOND_PENDING_REMOTE_REQUEST_RESPONSE, RUNTIME_STATUS_CHANGED, RUNTIME_STATUS_RESPONSE,
    UPDATE_HOST_SETTINGS, UPDATE_HOST_SETTINGS_RESPONSE,
};
use interprocess::local_socket::{prelude::*, GenericFilePath, GenericNamespaced, Stream};
use prost::Message;
use std::io::{self, Read, Write};
use std::path::PathBuf;
use std::time::{Duration, SystemTime, UNIX_EPOCH};

#[derive(Debug)]
pub struct IpcClient {
    stream: Stream,
    request_counter: u64,
    unusable: bool,
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
            unusable: false,
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
        loop {
            let envelope = self.recv_event()?;
            if envelope.message_type == RUNTIME_STATUS_CHANGED {
                return RuntimeStatusChanged::decode(&envelope.payload[..])
                    .map_err(|err| io::Error::new(io::ErrorKind::InvalidData, err.to_string()));
            }
        }
    }

    pub fn recv_capability_snapshot_changed(&mut self) -> io::Result<CapabilitySnapshotChanged> {
        loop {
            let envelope = self.recv_event()?;
            if envelope.message_type == CAPABILITY_SNAPSHOT_CHANGED {
                return CapabilitySnapshotChanged::decode(&envelope.payload[..])
                    .map_err(|err| io::Error::new(io::ErrorKind::InvalidData, err.to_string()));
            }
        }
    }

    pub fn recv_event(&mut self) -> io::Result<IpcEnvelope> {
        loop {
            let envelope = read_envelope(&mut self.stream)?;
            if envelope.kind == IpcMessageKind::Event as i32 {
                return Ok(envelope);
            }
        }
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

    pub fn get_host_overview(&mut self) -> io::Result<HostOverviewResponse> {
        self.request_response(
            GET_HOST_OVERVIEW,
            HOST_OVERVIEW_RESPONSE,
            GetHostOverviewRequest {},
        )
    }

    pub fn update_host_settings(
        &mut self,
        request: UpdateHostSettingsRequest,
    ) -> io::Result<UpdateHostSettingsResponse> {
        self.request_response(UPDATE_HOST_SETTINGS, UPDATE_HOST_SETTINGS_RESPONSE, request)
    }

    pub fn refresh_temporary_verification_code(
        &mut self,
    ) -> io::Result<RefreshTemporaryVerificationCodeResponse> {
        self.request_response(
            REFRESH_TEMPORARY_VERIFICATION_CODE,
            REFRESH_TEMPORARY_VERIFICATION_CODE_RESPONSE,
            RefreshTemporaryVerificationCodeRequest {},
        )
    }

    pub fn respond_pending_remote_request(
        &mut self,
        request_id: String,
        allow: bool,
    ) -> io::Result<RespondPendingRemoteRequestResponse> {
        self.request_response(
            RESPOND_PENDING_REMOTE_REQUEST,
            RESPOND_PENDING_REMOTE_REQUEST_RESPONSE,
            RespondPendingRemoteRequestRequest { allow, request_id },
        )
    }

    pub fn stop_session(
        &mut self,
        session_id: String,
    ) -> io::Result<crate::types::StopSessionResponse> {
        self.request_response(
            crate::types::STOP_SESSION,
            crate::types::STOP_SESSION_RESPONSE,
            crate::types::StopSessionRequest { session_id },
        )
    }

    pub fn get_diagnostics(&mut self) -> io::Result<DiagnosticsResponse> {
        self.request_response(
            GET_DIAGNOSTICS,
            DIAGNOSTICS_RESPONSE,
            GetDiagnosticsRequest {},
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
        if self.unusable {
            return Err(io::Error::new(
                io::ErrorKind::BrokenPipe,
                "IPC client must reconnect",
            ));
        }
        self.request_counter += 1;
        let envelope = IpcEnvelope {
            request_id: format!("req-{}", self.request_counter),
            kind: IpcMessageKind::Request as i32,
            message_type: request_type.to_string(),
            payload: payload.encode_to_vec(),
            timestamp_ms: now_ms(),
        };
        self.stream.set_nonblocking(true)?;
        let result = (|| {
            let mut stream = DeadlineIo {
                inner: &mut self.stream,
                deadline: std::time::Instant::now() + Duration::from_secs(5),
            };
            write_envelope(&mut stream, &envelope)?;
            loop {
                let response = read_envelope(&mut stream)?;
                if response.message_type == response_type
                    && response.request_id == envelope.request_id
                    && response.kind == IpcMessageKind::Response as i32
                {
                    return Res::decode(&response.payload[..]).map_err(|err| {
                        io::Error::new(io::ErrorKind::InvalidData, err.to_string())
                    });
                }
            }
        })();
        let reset = self.stream.set_nonblocking(false);
        self.unusable = result.is_err() || reset.is_err();
        reset?;
        result
    }
}

// Preserve partial frames while bounding the entire request, including streams
// that keep delivering unrelated events. After timeout the caller must reconnect.
struct DeadlineIo<T> {
    inner: T,
    deadline: std::time::Instant,
}
fn retry_until<T>(
    deadline: std::time::Instant,
    mut action: impl FnMut() -> io::Result<T>,
) -> io::Result<T> {
    loop {
        if std::time::Instant::now() >= deadline {
            return Err(io::Error::new(
                io::ErrorKind::TimedOut,
                "IPC request timed out",
            ));
        }
        match action() {
            Err(e) if e.kind() == io::ErrorKind::WouldBlock => {
                std::thread::sleep(Duration::from_millis(2))
            }
            Err(e) if e.kind() == io::ErrorKind::Interrupted => continue,
            result => return result,
        }
    }
}
impl<T: Read> Read for DeadlineIo<T> {
    fn read(&mut self, bytes: &mut [u8]) -> io::Result<usize> {
        retry_until(self.deadline, || {
            let count = self.inner.read(bytes)?;
            // Windows byte-mode PIPE_NOWAIT may complete an empty read with zero bytes.
            // A closed pipe is still bounded by the same request deadline.
            if cfg!(windows) && count == 0 && !bytes.is_empty() {
                Err(io::ErrorKind::WouldBlock.into())
            } else {
                Ok(count)
            }
        })
    }
}
impl<T: Write> Write for DeadlineIo<T> {
    fn write(&mut self, bytes: &[u8]) -> io::Result<usize> {
        retry_until(self.deadline, || {
            let count = self.inner.write(bytes)?;
            if cfg!(windows) && count == 0 && !bytes.is_empty() {
                Err(io::ErrorKind::WouldBlock.into())
            } else {
                Ok(count)
            }
        })
    }
    // Named-pipe flush can wait for the remote reader. write_all already handed
    // every byte to the OS; a remote acknowledgement comes in the response.
    fn flush(&mut self) -> io::Result<()> {
        Ok(())
    }
}

#[cfg(test)]
mod timeout_tests {
    use super::*;
    #[test]
    fn partial_reads_survive_would_block_and_silent_reads_expire() {
        struct Fragmented {
            calls: usize,
            cursor: io::Cursor<Vec<u8>>,
        }
        impl Read for Fragmented {
            fn read(&mut self, b: &mut [u8]) -> io::Result<usize> {
                self.calls += 1;
                if self.calls % 2 == 1 {
                    Err(io::ErrorKind::WouldBlock.into())
                } else {
                    self.cursor.read(&mut b[..1])
                }
            }
        }
        let mut stream = DeadlineIo {
            inner: Fragmented {
                calls: 0,
                cursor: io::Cursor::new(vec![1, 2, 3, 4]),
            },
            deadline: std::time::Instant::now() + Duration::from_secs(1),
        };
        let mut bytes = [0; 4];
        stream.read_exact(&mut bytes).unwrap();
        assert_eq!(bytes, [1, 2, 3, 4]);
        let started = std::time::Instant::now();
        let error = retry_until::<()>(started + Duration::from_millis(10), || {
            Err(io::ErrorKind::WouldBlock.into())
        })
        .unwrap_err();
        assert_eq!(error.kind(), io::ErrorKind::TimedOut);
        assert!(started.elapsed() < Duration::from_millis(200));
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

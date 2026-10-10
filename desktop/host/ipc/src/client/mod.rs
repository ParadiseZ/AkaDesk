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
            if response.message_type == response_type
                && response.request_id == envelope.request_id
                && response.kind == IpcMessageKind::Response as i32
            {
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

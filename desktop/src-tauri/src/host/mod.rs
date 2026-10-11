use host_ipc::client::IpcClient;
use host_ipc::types::{
    CapabilitySnapshotResponse, DiagnosticsResponse, HostOverviewResponse, HostSettingsSnapshot,
    RefreshTemporaryVerificationCodeResponse, RespondPendingRemoteRequestResponse,
    RuntimeStatusResponse, UpdateHostSettingsRequest, UpdateHostSettingsResponse,
};
use std::ffi::OsString;
use std::io;
use std::path::{Path, PathBuf};
use std::process::{Child, Command, Stdio};
use std::time::Duration;

#[derive(Debug)]
pub struct HostSnapshotBundle {
    pub generation: u64,
    pub runtime_status: RuntimeStatusResponse,
    pub capability_snapshot: CapabilitySnapshotResponse,
    pub host_overview: HostOverviewResponse,
    pub diagnostics: DiagnosticsResponse,
}

#[derive(Debug, Default)]
pub struct HostSupervisor {
    generation: u64,
    pub ready: bool,
    pub ipc_name: String,
    child: Option<Child>,
    resource_dir: Option<PathBuf>,
}

impl HostSupervisor {
    pub fn with_resources(resource_dir: PathBuf) -> Self {
        Self {
            ready: false,
            ipc_name: String::new(),
            child: None,
            resource_dir: Some(resource_dir),
            generation: 0,
        }
    }
    pub fn ensure_started(&mut self) -> io::Result<()> {
        if self.is_running()? {
            self.ready = true;
            return Ok(());
        }

        self.ipc_name = format!("akadesk.desktop.{}", std::process::id());
        let host_runtime_bin = resolve_host_runtime_bin(self.resource_dir.as_deref())?;
        let ffmpeg_bin_dir = resolve_local_ffmpeg_bin_dir(&host_runtime_bin)?;

        let mut command = Command::new(host_runtime_bin);
        command
            .arg("--ipc-name")
            .arg(&self.ipc_name)
            .stdin(Stdio::null())
            .stdout(Stdio::null())
            .stderr(Stdio::inherit());
        prepend_path_env(&mut command, &ffmpeg_bin_dir);

        self.child = Some(command.spawn()?);
        self.generation += 1;
        let mut client = self.connect_client(Duration::from_secs(5))?;
        let _ = client.get_runtime_status()?;
        self.ready = true;
        Ok(())
    }

    pub fn fetch_snapshot(&mut self) -> io::Result<HostSnapshotBundle> {
        self.ensure_started()?;
        let mut client = self.connect_client(Duration::from_secs(2))?;
        Ok(HostSnapshotBundle {
            generation: self.generation,
            runtime_status: client.get_runtime_status()?,
            capability_snapshot: client.get_capability_snapshot()?,
            host_overview: client.get_host_overview()?,
            diagnostics: client.get_diagnostics()?,
        })
    }

    pub fn update_host_settings(
        &mut self,
        settings: HostSettingsSnapshot,
    ) -> io::Result<UpdateHostSettingsResponse> {
        self.ensure_started()?;
        let mut client = self.connect_client(Duration::from_secs(2))?;
        client.update_host_settings(UpdateHostSettingsRequest {
            settings: Some(settings),
        })
    }

    pub fn refresh_temporary_verification_code(
        &mut self,
    ) -> io::Result<RefreshTemporaryVerificationCodeResponse> {
        self.ensure_started()?;
        let mut client = self.connect_client(Duration::from_secs(2))?;
        client.refresh_temporary_verification_code()
    }

    pub fn respond_pending_remote_request(
        &mut self,
        request_id: String,
        allow: bool,
    ) -> io::Result<RespondPendingRemoteRequestResponse> {
        self.ensure_started()?;
        let mut client = self.connect_client(Duration::from_secs(2))?;
        client.respond_pending_remote_request(request_id, allow)
    }

    pub fn stop_session(
        &mut self,
        session_id: String,
    ) -> io::Result<host_ipc::types::StopSessionResponse> {
        self.ensure_started()?;
        self.connect_client(Duration::from_secs(2))?
            .stop_session(session_id)
    }

    pub fn connect_event_client(&mut self, timeout: Duration) -> io::Result<IpcClient> {
        self.ensure_started()?;
        self.connect_client(timeout)
    }

    pub fn stop(&mut self) {
        if let Some(mut child) = self.child.take() {
            let _ = child.kill();
            let _ = child.wait();
        }
        self.ready = false;
    }

    fn connect_client(&self, timeout: Duration) -> io::Result<IpcClient> {
        IpcClient::connect_with_retry(&self.ipc_name, timeout)
    }

    fn is_running(&mut self) -> io::Result<bool> {
        let Some(child) = self.child.as_mut() else {
            return Ok(false);
        };
        match child.try_wait()? {
            None => Ok(true),
            Some(_) => {
                self.child = None;
                self.ready = false;
                Ok(false)
            }
        }
    }
}

impl Drop for HostSupervisor {
    fn drop(&mut self) {
        self.stop();
    }
}

fn resolve_host_runtime_bin(resource_dir: Option<&Path>) -> io::Result<PathBuf> {
    if let Some(path) = std::env::var_os("AKADESK_HOST_RUNTIME_BIN") {
        return Ok(PathBuf::from(path));
    }

    if let Some(dir) = resource_dir {
        let candidate = dir.join("host").join(host_runtime_executable_name());
        if candidate.is_file() {
            return Ok(candidate);
        }
    }
    if !cfg!(debug_assertions) {
        return Err(io::Error::new(
            io::ErrorKind::NotFound,
            "打包资源缺少 Host runtime",
        ));
    }
    let manifest_dir = Path::new(env!("CARGO_MANIFEST_DIR"));
    let mut candidate = manifest_dir
        .parent()
        .ok_or_else(|| io::Error::new(io::ErrorKind::NotFound, "missing desktop directory"))?
        .join("host")
        .join("target")
        .join("debug")
        .join(host_runtime_executable_name());

    if candidate.exists() {
        return Ok(candidate);
    }

    candidate = manifest_dir
        .parent()
        .ok_or_else(|| io::Error::new(io::ErrorKind::NotFound, "missing desktop directory"))?
        .join("host")
        .join("target")
        .join("debug")
        .join("host-runtime");
    if candidate.exists() {
        return Ok(candidate);
    }

    Err(io::Error::new(
        io::ErrorKind::NotFound,
        format!(
            "host-runtime binary not found, expected {}",
            manifest_dir
                .parent()
                .unwrap_or(manifest_dir)
                .join("host")
                .join("target")
                .join("debug")
                .join(host_runtime_executable_name())
                .display()
        ),
    ))
}

fn host_runtime_executable_name() -> OsString {
    let mut name = OsString::from("host-runtime");
    if cfg!(windows) {
        name.push(".exe");
    }
    name
}

fn resolve_local_ffmpeg_bin_dir(runtime_bin: &Path) -> io::Result<PathBuf> {
    if let Some(path) = std::env::var_os("AKADESK_FFMPEG_BIN") {
        let candidate = PathBuf::from(path);
        if candidate.exists() {
            return Ok(candidate);
        }
        return Err(io::Error::new(
            io::ErrorKind::NotFound,
            format!("AKADESK_FFMPEG_BIN does not exist: {}", candidate.display()),
        ));
    }

    if let Some(dir) = runtime_bin.parent() {
        if dir.join("avcodec-62.dll").is_file() && dir.join("avutil-60.dll").is_file() {
            return Ok(dir.to_path_buf());
        }
    }
    if !cfg!(debug_assertions) {
        return Err(io::Error::new(
            io::ErrorKind::NotFound,
            "打包资源缺少 FFmpeg DLL",
        ));
    }
    let manifest_dir = Path::new(env!("CARGO_MANIFEST_DIR"));
    let candidate = manifest_dir
        .parent()
        .ok_or_else(|| io::Error::new(io::ErrorKind::NotFound, "missing desktop directory"))?
        .parent()
        .ok_or_else(|| io::Error::new(io::ErrorKind::NotFound, "missing workspace root"))?
        .join("third_party")
        .join("ffmpe-win64-gpl-shared")
        .join("bin");

    if candidate.exists() {
        return Ok(candidate);
    }

    Err(io::Error::new(
        io::ErrorKind::NotFound,
        format!(
            "local ffmpeg bin directory not found: {}",
            candidate.display()
        ),
    ))
}

fn prepend_path_env(command: &mut Command, extra_dir: &Path) {
    let mut value = OsString::from(extra_dir.as_os_str());
    if let Some(existing) = std::env::var_os("PATH") {
        value.push(if cfg!(windows) { ";" } else { ":" });
        value.push(existing);
    }
    command.env("PATH", value);
}

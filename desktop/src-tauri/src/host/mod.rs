use host_ipc::client::IpcClient;
use host_ipc::types::{CapabilitySnapshotResponse, RuntimeStatusResponse};
use std::ffi::OsString;
use std::io;
use std::path::{Path, PathBuf};
use std::process::{Child, Command, Stdio};
use std::time::Duration;

#[derive(Debug, Default)]
pub struct HostBootstrapResult {
    pub runtime_status: Option<RuntimeStatusResponse>,
    pub capability_snapshot: Option<CapabilitySnapshotResponse>,
}

#[derive(Debug, Default)]
pub struct HostSupervisor {
    pub ready: bool,
    pub ipc_name: String,
    child: Option<Child>,
}

impl HostSupervisor {
    pub fn start(&mut self) -> io::Result<HostBootstrapResult> {
        self.ipc_name = format!("akadesk.m2.{}", std::process::id());
        let host_runtime_bin = resolve_host_runtime_bin()?;
        let ffmpeg_bin_dir = resolve_local_ffmpeg_bin_dir()?;

        let mut command = Command::new(host_runtime_bin);
        command
            .arg("--ipc-name")
            .arg(&self.ipc_name)
            .stdin(Stdio::null())
            .stdout(Stdio::null())
            .stderr(Stdio::inherit());
        prepend_path_env(&mut command, &ffmpeg_bin_dir);

        let child = command.spawn()?;
        self.child = Some(child);

        let mut client = IpcClient::connect_with_retry(&self.ipc_name, Duration::from_secs(5))?;
        let _ = client.recv_runtime_status_changed()?;
        let _ = client.recv_capability_snapshot_changed()?;
        let runtime_status = client.get_runtime_status()?;
        let capability_snapshot = client.get_capability_snapshot()?;
        self.ready = true;

        Ok(HostBootstrapResult {
            runtime_status: Some(runtime_status),
            capability_snapshot: Some(capability_snapshot),
        })
    }

    pub fn stop(&mut self) {
        if let Some(mut child) = self.child.take() {
            let _ = child.kill();
            let _ = child.wait();
        }
        self.ready = false;
    }
}

impl Drop for HostSupervisor {
    fn drop(&mut self) {
        self.stop();
    }
}

fn resolve_host_runtime_bin() -> io::Result<PathBuf> {
    if let Some(path) = std::env::var_os("AKADESK_HOST_RUNTIME_BIN") {
        return Ok(PathBuf::from(path));
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

fn resolve_local_ffmpeg_bin_dir() -> io::Result<PathBuf> {
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
        format!("local ffmpeg bin directory not found: {}", candidate.display()),
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

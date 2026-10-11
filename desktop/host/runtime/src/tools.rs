//! Session-bound tools carried by an ordered, encrypted WebRTC data channel.
use crate::session::SharedSessionRegistry;
use base64::{engine::general_purpose::STANDARD, Engine};
use serde::Deserialize;
use serde_json::{json, Value};
use std::{
    collections::HashMap,
    fs::{self, File, OpenOptions},
    io::{Read, Seek, SeekFrom, Write},
    path::{Component, Path, PathBuf},
    time::{Duration, Instant},
};

const CHUNK: usize = 16_384;
const MAX_FILE: u64 = 4 * 1024 * 1024 * 1024;
#[derive(Deserialize)]
struct Request {
    id: u64,
    session: String,
    op: String,
    #[serde(default)]
    args: Value,
}
struct Upload {
    path: PathBuf,
    destination: PathBuf,
    file: File,
    size: u64,
    offset: u64,
    touched: Instant,
}
struct Download {
    file: File,
    size: u64,
    offset: u64,
    touched: Instant,
}
pub struct ToolSession {
    id: String,
    root: PathBuf,
    last_request: u64,
    can_control: bool,
    clipboard_enabled: bool,
    lock_after: bool,
    uploads: HashMap<String, Upload>,
    downloads: HashMap<String, Download>,
    input: Option<crate::input::SharedInputState>,
    ended: bool,
}
impl ToolSession {
    pub fn new(id: String, root: PathBuf, can_control: bool) -> Self {
        Self {
            id,
            root,
            last_request: 0,
            can_control,
            clipboard_enabled: false,
            lock_after: false,
            uploads: HashMap::new(),
            downloads: HashMap::new(),
            input: None,
            ended: false,
        }
    }
    pub fn bind_input(&mut self, input: crate::input::SharedInputState) {
        self.input = Some(input);
    }
    pub fn default_root() -> PathBuf {
        if let Some(path) = std::env::var_os("AKADESK_SHARED_DIR") {
            return PathBuf::from(path);
        }
        std::env::var_os("USERPROFILE")
            .or_else(|| std::env::var_os("HOME"))
            .map(PathBuf::from)
            .unwrap_or_else(std::env::temp_dir)
            .join("Downloads")
            .join("AkaDesk")
    }
    pub fn process(&mut self, sessions: &SharedSessionRegistry, bytes: &[u8]) -> Vec<u8> {
        let mut response = json!({"id":0,"session":self.id,"error":"Invalid request"});
        if bytes.len() <= 65_536 {
            if let Ok(request) = serde_json::from_slice::<Request>(bytes) {
                response["id"] = json!(request.id);
                let Ok(mut registry) = sessions.lock() else {
                    return serde_json::to_vec(&response).unwrap_or_default();
                };
                let authorized = registry.should_keep_running(&self.id);
                let result = if self.ended || request.session != self.id || !authorized {
                    Err("Session is unavailable".into())
                } else if request.id == 0 || request.id <= self.last_request {
                    Err("Request was already handled".into())
                } else {
                    self.last_request = request.id;
                    self.expire();
                    self.execute(&mut registry, &request.op, &request.args)
                };
                match result {
                    Ok(data) => {
                        response.as_object_mut().unwrap().remove("error");
                        response["data"] = data;
                    }
                    Err(error) => response["error"] = json!(error),
                }
            }
        }
        serde_json::to_vec(&response).unwrap_or_default()
    }
    fn execute(
        &mut self,
        registry: &mut crate::session::SessionRegistry,
        op: &str,
        args: &Value,
    ) -> Result<Value, String> {
        let text = |key: &str| args[key].as_str().ok_or_else(|| format!("Missing {key}"));
        if op == "status" {
            let active = registry.active.as_ref().unwrap();
            let video = active.video.as_ref().map(
                |v| json!({"width":v.width,"height":v.height,"fps":v.fps,"bitrate":v.bitrate_kbps}),
            );
            return Ok(
                json!({"root":self.root.to_string_lossy(),"clipboard":self.clipboard_enabled,"lockAfter":self.lock_after,"canControl":self.can_control,"video":video,"videoPending":active.requested_video.is_some(),"videoError":active.video_change_error}),
            );
        }
        if !self.can_control {
            return Err("This session only permits viewing".into());
        }
        match op {
            "set_video" => {
                let mut video = registry
                    .active
                    .as_ref()
                    .and_then(|s| s.video.clone())
                    .ok_or("Video is unavailable")?;
                if registry.active.as_ref().unwrap().requested_video.is_some() {
                    return Err("A video change is pending".into());
                }
                video.width = args["width"]
                    .as_u64()
                    .filter(|v| (640..=1920).contains(v) && v % 2 == 0)
                    .ok_or("Invalid video width")? as u32;
                video.height = args["height"]
                    .as_u64()
                    .filter(|v| (360..=1200).contains(v) && v % 2 == 0)
                    .ok_or("Invalid video height")? as u32;
                video.fps = args["fps"]
                    .as_u64()
                    .filter(|v| [30, 60, 90, 120].contains(v))
                    .ok_or("Invalid video frame rate")? as u32;
                video.bitrate_kbps = args["bitrate"]
                    .as_u64()
                    .filter(|v| (500..=50_000).contains(v))
                    .ok_or("Invalid video bitrate")? as u32;
                let active = registry.active.as_mut().unwrap();
                active.requested_video = Some(video);
                active.video_change_error.clear();
                Ok(json!({"pending":true}))
            }
            "clipboard_enable" => {
                self.clipboard_enabled = args["enabled"].as_bool().ok_or("Missing enabled")?;
                Ok(json!({"enabled":self.clipboard_enabled}))
            }
            "clipboard_get" | "clipboard_set" => {
                if !self.clipboard_enabled {
                    return Err("Clipboard sync is disabled".into());
                }
                #[cfg(windows)]
                {
                    if op == "clipboard_get" {
                        return host_windows::tools::clipboard_read()
                            .map(|value| json!({"text":value}));
                    }
                    host_windows::tools::clipboard_write(text("text")?)?;
                    Ok(json!({}))
                }
                #[cfg(not(windows))]
                {
                    Err("Clipboard is unavailable".into())
                }
            }
            "view_only" => {
                let value = args["enabled"].as_bool().ok_or("Missing enabled")?;
                if !registry.should_keep_running(&self.id) {
                    return Err("Session is unavailable".into());
                }
                if let Some(input) = &self.input {
                    input
                        .lock()
                        .map_err(|_| "Input lock failed")?
                        .release_held(&self.id)?;
                }
                registry.active.as_mut().unwrap().view_only = value;
                Ok(json!({"enabled":value}))
            }
            "lock_after" => {
                self.lock_after = args["enabled"].as_bool().ok_or("Missing enabled")?;
                Ok(json!({"enabled":self.lock_after}))
            }
            "system" => {
                if registry.active.as_ref().is_some_and(|s| s.view_only) {
                    return Err("Disable view-only mode before a system operation".into());
                }
                let action = text("action")?;
                if !["lock", "restart", "shutdown", "task_manager"].contains(&action) {
                    return Err("Unknown system operation".into());
                }
                if args["confirmed"] != true {
                    return Err("Confirmation is required".into());
                }
                #[cfg(windows)]
                host_windows::tools::system_action(action)?;
                #[cfg(not(windows))]
                return Err("System operations are unavailable".into());
                #[cfg(windows)]
                Ok(json!({"accepted":true}))
            }
            "list" => {
                let relative = args["path"].as_str().unwrap_or("");
                let path = self.resolve(relative)?;
                let mut entries = Vec::new();
                for entry in fs::read_dir(path).map_err(io_error)?.take(1000) {
                    let entry = entry.map_err(io_error)?;
                    if entry.file_name().to_string_lossy().starts_with(".part-") {
                        continue;
                    }
                    let resolved = entry.path().canonicalize().map_err(io_error)?;
                    if !resolved.starts_with(self.canonical_root()?) {
                        continue;
                    }
                    let metadata = entry.metadata().map_err(io_error)?;
                    entries.push(json!({"name":entry.file_name().to_string_lossy(),"directory":metadata.is_dir(),"size":metadata.len()}));
                }
                Ok(json!({"path":relative,"entries":entries,"root":self.root.to_string_lossy()}))
            }
            "upload_begin" => {
                if self.uploads.len() >= 4 {
                    return Err("Too many transfers".into());
                }
                let name = text("name")?;
                validate_name(name)?;
                let size = args["size"]
                    .as_u64()
                    .filter(|v| *v <= MAX_FILE)
                    .ok_or("File exceeds 4 GB")?;
                let root = self.canonical_root()?;
                let destination = root.join(name);
                if destination.exists() {
                    return Err("A file with this name already exists".into());
                }
                let token = crate::pairing::generate_pair_token();
                let path = root.join(format!(".part-{token}"));
                let file = OpenOptions::new()
                    .write(true)
                    .create_new(true)
                    .open(&path)
                    .map_err(io_error)?;
                self.uploads.insert(
                    token.clone(),
                    Upload {
                        path,
                        destination,
                        file,
                        size,
                        offset: 0,
                        touched: Instant::now(),
                    },
                );
                Ok(json!({"transfer":token,"chunkSize":CHUNK}))
            }
            "upload_chunk" => {
                let token = text("transfer")?;
                let chunk = text("bytes")?;
                if chunk.len() > CHUNK * 2 {
                    return Err("Chunk is too large".into());
                }
                let data = STANDARD.decode(chunk).map_err(|_| "Invalid file chunk")?;
                let upload = self.uploads.get_mut(token).ok_or("Transfer expired")?;
                if data.is_empty()
                    || data.len() > CHUNK
                    || args["offset"].as_u64() != Some(upload.offset)
                    || upload.offset + data.len() as u64 > upload.size
                {
                    return Err("Invalid file offset or size".into());
                }
                upload.file.write_all(&data).map_err(io_error)?;
                upload.offset += data.len() as u64;
                upload.touched = Instant::now();
                Ok(json!({"offset":upload.offset}))
            }
            "upload_commit" => {
                let token = text("transfer")?;
                let upload = self.uploads.get(token).ok_or("Transfer expired")?;
                if upload.offset != upload.size {
                    return Err("Upload is incomplete".into());
                }
                upload.file.sync_all().map_err(io_error)?;
                // Hard-link creation fails if the destination exists; never overwrite.
                fs::hard_link(&upload.path, &upload.destination).map_err(io_error)?;
                let upload = self.uploads.remove(token).unwrap();
                drop(upload.file);
                let _ = fs::remove_file(upload.path);
                Ok(
                    json!({"name":upload.destination.file_name().unwrap().to_string_lossy(),"size":upload.size}),
                )
            }
            "download_begin" => {
                if self.downloads.len() >= 4 {
                    return Err("Too many transfers".into());
                }
                let path = self.resolve(text("path")?)?;
                let file = File::open(&path).map_err(io_error)?;
                let metadata = file.metadata().map_err(io_error)?;
                if !metadata.is_file() || metadata.len() > MAX_FILE {
                    return Err("Not a supported file".into());
                }
                let token = crate::pairing::generate_pair_token();
                let size = metadata.len();
                self.downloads.insert(
                    token.clone(),
                    Download {
                        file,
                        size,
                        offset: 0,
                        touched: Instant::now(),
                    },
                );
                Ok(
                    json!({"transfer":token,"name":path.file_name().unwrap().to_string_lossy(),"size":size}),
                )
            }
            "download_chunk" => {
                let token = text("transfer")?;
                let download = self.downloads.get_mut(token).ok_or("Transfer expired")?;
                if args["offset"].as_u64() != Some(download.offset) {
                    return Err("Invalid file offset".into());
                }
                let mut data = vec![0; CHUNK.min((download.size - download.offset) as usize)];
                download
                    .file
                    .seek(SeekFrom::Start(download.offset))
                    .map_err(io_error)?;
                download.file.read_exact(&mut data).map_err(io_error)?;
                download.offset += data.len() as u64;
                download.touched = Instant::now();
                let result = json!({"bytes":STANDARD.encode(data),"offset":download.offset,"done":download.offset==download.size});
                if download.offset == download.size {
                    self.downloads.remove(token);
                }
                Ok(result)
            }
            "cancel" => {
                self.cancel(text("transfer")?);
                Ok(json!({}))
            }
            _ => Err("This operation is unavailable".into()),
        }
    }
    fn canonical_root(&self) -> Result<PathBuf, String> {
        fs::create_dir_all(&self.root).map_err(io_error)?;
        self.root.canonicalize().map_err(io_error)
    }
    fn resolve(&self, relative: &str) -> Result<PathBuf, String> {
        if relative.contains(['\\', ':', '\0'])
            || Path::new(relative)
                .components()
                .any(|c| !matches!(c, Component::Normal(_)))
                && !relative.is_empty()
        {
            return Err("Invalid file path".into());
        }
        let root = self.canonical_root()?;
        let resolved = root.join(relative).canonicalize().map_err(io_error)?;
        if !resolved.starts_with(&root) {
            return Err("File path leaves the shared folder".into());
        }
        Ok(resolved)
    }
    fn cancel(&mut self, token: &str) {
        if let Some(upload) = self.uploads.remove(token) {
            drop(upload.file);
            let _ = fs::remove_file(upload.path);
        }
        self.downloads.remove(token);
    }
    fn expire(&mut self) {
        let expired: Vec<String> = self
            .uploads
            .iter()
            .filter(|(_, v)| v.touched.elapsed() > Duration::from_secs(60))
            .map(|(k, _)| k.clone())
            .collect();
        for id in expired {
            self.cancel(&id);
        }
        self.downloads
            .retain(|_, v| v.touched.elapsed() <= Duration::from_secs(60));
    }
    pub fn finish(&mut self) {
        self.ended = true;
        for id in self.uploads.keys().cloned().collect::<Vec<_>>() {
            self.cancel(&id);
        }
        self.downloads.clear();
        #[cfg(windows)]
        if self.lock_after {
            let _ = host_windows::tools::system_action("lock");
        }
        self.lock_after = false;
    }
}
impl Drop for ToolSession {
    fn drop(&mut self) {
        self.finish();
    }
}
fn io_error(error: std::io::Error) -> String {
    error.to_string()
}
fn validate_name(name: &str) -> Result<(), String> {
    let stem = name.split('.').next().unwrap_or("").to_ascii_uppercase();
    if name.is_empty()
        || name.len() > 240
        || name.starts_with('.')
        || name.ends_with(['.', ' '])
        || name
            .chars()
            .any(|v| v.is_control() || "\\/:*?\"<>|".contains(v))
        || ["CON", "PRN", "AUX", "NUL"].contains(&stem.as_str())
        || (stem.starts_with("COM") || stem.starts_with("LPT"))
            && stem[3..].parse::<u8>().is_ok_and(|v| v <= 9)
    {
        Err("Invalid file name".into())
    } else {
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    fn fixture() -> (ToolSession, SharedSessionRegistry, PathBuf) {
        let path = std::env::temp_dir().join(format!(
            "akadesk-tools-test-{}",
            crate::pairing::generate_pair_token()
        ));
        let mut registry = crate::session::SessionRegistry::default();
        let id = registry
            .activate_preparing(
                "session".into(),
                "phone".into(),
                "pc".into(),
                String::new(),
                Default::default(),
            )
            .unwrap()
            .session_id;
        (
            ToolSession::new(id, path.clone(), true),
            std::sync::Arc::new(std::sync::Mutex::new(registry)),
            path,
        )
    }
    fn request(
        tool: &mut ToolSession,
        s: &SharedSessionRegistry,
        id: u64,
        op: &str,
        args: Value,
    ) -> Value {
        serde_json::from_slice(&tool.process(
            s,
            &serde_json::to_vec(&json!({"session":tool.id,"id":id,"op":op,"args":args})).unwrap(),
        ))
        .unwrap()
    }
    #[test]
    fn transfer_commits_exact_bytes_never_overwrites_and_cleans_incomplete_data() {
        let (mut t, s, path) = fixture();
        let r = request(
            &mut t,
            &s,
            1,
            "upload_begin",
            json!({"name":"test.txt","size":3}),
        );
        let token = r["data"]["transfer"].as_str().unwrap();
        assert!(request(
            &mut t,
            &s,
            2,
            "upload_chunk",
            json!({"transfer":token,"offset":1,"bytes":"YWJj"})
        )["error"]
            .is_string());
        assert!(
            request(&mut t, &s, 3, "upload_commit", json!({"transfer":token}))["error"].is_string()
        );
        assert!(request(
            &mut t,
            &s,
            4,
            "upload_chunk",
            json!({"transfer":token,"offset":0,"bytes":"YWJj"})
        )["error"]
            .is_null());
        assert!(
            request(&mut t, &s, 5, "upload_commit", json!({"transfer":token}))["error"].is_null()
        );
        assert_eq!(fs::read(path.join("test.txt")).unwrap(), b"abc");
        assert!(request(
            &mut t,
            &s,
            6,
            "upload_begin",
            json!({"name":"test.txt","size":3})
        )["error"]
            .is_string());
        let read = request(&mut t, &s, 7, "download_begin", json!({"path":"test.txt"}));
        let token = read["data"]["transfer"].as_str().unwrap();
        assert_eq!(
            request(
                &mut t,
                &s,
                8,
                "download_chunk",
                json!({"transfer":token,"offset":0})
            )["data"]["bytes"],
            "YWJj"
        );
        request(
            &mut t,
            &s,
            9,
            "upload_begin",
            json!({"name":"partial.txt","size":100}),
        );
        drop(t);
        assert_eq!(fs::read_dir(&path).unwrap().count(), 1);
        fs::remove_dir_all(path).unwrap();
    }
    #[test]
    fn stale_replayed_view_only_and_traversal_requests_do_not_mutate() {
        let (mut t, s, path) = fixture();
        assert!(request(&mut t, &s, 1, "list", json!({"path":"../"}))["error"].is_string());
        assert!(request(
            &mut t,
            &s,
            2,
            "upload_begin",
            json!({"name":"../outside","size":0})
        )["error"]
            .is_string());
        request(&mut t, &s, 3, "status", json!({}));
        assert!(request(
            &mut t,
            &s,
            3,
            "upload_begin",
            json!({"name":"test.txt","size":0})
        )["error"]
            .is_string());
        t.can_control = false;
        assert!(
            request(&mut t, &s, 4, "clipboard_enable", json!({"enabled":true}))["error"]
                .is_string()
        );
        s.lock().unwrap().mark_disconnected(&t.id);
        assert!(request(&mut t, &s, 5, "status", json!({}))["error"].is_string());
        assert!(!path.exists());
    }
}

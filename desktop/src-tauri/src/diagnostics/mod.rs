use chrono::Local;
use std::collections::{HashMap, VecDeque};
use std::fs::{self, OpenOptions};
use std::io::{self, Write};
use std::path::PathBuf;
use std::time::Instant;

#[derive(Debug)]
pub struct ShellDiagnosticState {
    log_dir: PathBuf,
    pub recent_messages: Vec<String>,
    written: HashMap<String, Instant>,
    host_ids: VecDeque<String>,
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn logs_are_concise_deduplicated_and_exports_do_not_overwrite() {
        let path = std::env::temp_dir().join(format!(
            "akadesk-shell-diag-{}",
            std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .unwrap()
                .as_nanos()
        ));
        let mut d = ShellDiagnosticState::new(path.clone()).unwrap();
        for _ in 0..100 {
            d.append(
                "error",
                "DISPLAY_RESTORE_FAILED",
                "secret=123 long raw platform detail",
            )
            .unwrap();
        }
        assert_eq!(d.recent_messages.len(), 1);
        assert!(!d.recent_messages[0].contains("secret"));
        let report = serde_json::json!({"schemaVersion":1});
        let a = d.export(&report).unwrap();
        let b = d.export(&report).unwrap();
        assert_ne!(a, b);
        assert_eq!(
            serde_json::from_slice::<serde_json::Value>(&fs::read(a).unwrap()).unwrap(),
            report
        );
        fs::remove_dir_all(path).unwrap();
    }
}

impl ShellDiagnosticState {
    pub fn new(base_dir: PathBuf) -> io::Result<Self> {
        let log_dir = base_dir.join("logs");
        fs::create_dir_all(&log_dir)?;
        Ok(Self {
            log_dir,
            recent_messages: Vec::new(),
            written: HashMap::new(),
            host_ids: VecDeque::new(),
        })
    }

    pub fn append(&mut self, level: &str, code: &str, _message: &str) -> io::Result<()> {
        self.written.retain(|_, at| at.elapsed().as_secs() < 30);
        let key = format!("{level}:{code}");
        if self.written.contains_key(&key) {
            return Ok(());
        }
        let message = host_ipc::diagnostics::presentation(code).0;
        let now = Local::now();
        let log_path = self.log_dir.join(format!("{}.log", now.format("%Y-%m-%d")));
        if fs::metadata(&log_path).is_ok_and(|m| m.len() >= 1_048_576) {
            let backup = log_path.with_extension("previous.log");
            if backup.exists() {
                fs::remove_file(&backup)?;
            }
            fs::rename(&log_path, &backup)?;
        }
        let mut file = OpenOptions::new()
            .create(true)
            .append(true)
            .open(log_path)?;
        let line = format!(
            "{} [{}] {} {}\n",
            now.format("%Y-%m-%d %H:%M:%S"),
            level.to_ascii_uppercase(),
            code,
            message
        );
        file.write_all(line.as_bytes())?;
        self.written.insert(key, Instant::now());

        self.recent_messages
            .push(format!("[{}] {} {}", level, code, message));
        if self.recent_messages.len() > 50 {
            self.recent_messages.remove(0);
        }
        Ok(())
    }
    pub fn ingest(&mut self, entries: &[host_ipc::types::DiagnosticEntry]) {
        for entry in entries {
            if self.host_ids.contains(&entry.event_id) {
                continue;
            }
            if self.append(&entry.level, &entry.code, "").is_ok() {
                if self.host_ids.len() == 80 {
                    self.host_ids.pop_front();
                }
                self.host_ids.push_back(entry.event_id.clone());
            }
        }
    }
    pub fn export(&self, report: &serde_json::Value) -> io::Result<PathBuf> {
        let directory = self.log_dir.parent().unwrap().join("diagnostics");
        fs::create_dir_all(&directory)?;
        let bytes = serde_json::to_vec_pretty(report).map_err(io::Error::other)?;
        for index in 0..100 {
            let path = directory.join(format!(
                "diagnostics-{}-{index}.json",
                Local::now().format("%Y%m%d-%H%M%S")
            ));
            match OpenOptions::new().write(true).create_new(true).open(&path) {
                Ok(mut file) => {
                    file.write_all(&bytes)?;
                    file.sync_all()?;
                    return Ok(path);
                }
                Err(e) if e.kind() == io::ErrorKind::AlreadyExists => continue,
                Err(e) => return Err(e),
            }
        }
        Err(io::Error::new(
            io::ErrorKind::AlreadyExists,
            "Export filename unavailable",
        ))
    }
}

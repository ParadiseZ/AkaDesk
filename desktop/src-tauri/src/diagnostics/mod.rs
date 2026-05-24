use chrono::Local;
use std::fs::{self, OpenOptions};
use std::io::{self, Write};
use std::path::PathBuf;

#[derive(Debug)]
pub struct ShellDiagnosticState {
    log_dir: PathBuf,
    pub recent_messages: Vec<String>,
}

impl ShellDiagnosticState {
    pub fn new(base_dir: PathBuf) -> io::Result<Self> {
        let log_dir = base_dir.join("logs");
        fs::create_dir_all(&log_dir)?;
        Ok(Self {
            log_dir,
            recent_messages: Vec::new(),
        })
    }

    pub fn append(&mut self, level: &str, code: &str, message: &str) -> io::Result<()> {
        let now = Local::now();
        let log_path = self
            .log_dir
            .join(format!("{}.log", now.format("%Y-%m-%d")));
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

        self.recent_messages
            .push(format!("[{}] {} {}", level, code, message));
        if self.recent_messages.len() > 50 {
            self.recent_messages.remove(0);
        }
        Ok(())
    }
}

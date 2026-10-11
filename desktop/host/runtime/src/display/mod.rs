use crate::generated::v1::{DisplayInfo, DisplayMode as ProtoMode, DisplaySettingsSnapshot};
use host_contracts::display::{DisplayMode, DisplaySnapshot};
use serde::{Deserialize, Serialize};
use std::path::{Path, PathBuf};
use std::sync::{Arc, Mutex};

#[derive(Debug, Clone)]
pub struct Details {
    pub display: DisplaySnapshot,
    pub modes: Vec<DisplayMode>,
    pub scales: Vec<u32>,
}

#[cfg(test)]
mod tests {
    use super::*;
    use host_contracts::common::DisplayId;
    #[derive(Debug)]
    struct MockData {
        current: DisplayMode,
        calls: usize,
        fail: usize,
    }
    #[derive(Debug)]
    struct Mock(Arc<Mutex<MockData>>);
    fn original() -> DisplayMode {
        DisplayMode {
            width: 1920,
            height: 1200,
            refresh_rate: 60,
            scale_percent: 100,
        }
    }
    impl DisplayBackend for Mock {
        fn list(&self) -> Result<Vec<DisplaySnapshot>, String> {
            Ok(vec![DisplaySnapshot {
                display_id: DisplayId("stable-monitor".into()),
                primary: true,
                ..Default::default()
            }])
        }
        fn details(&self, _: &str) -> Result<Details, String> {
            Ok(Details {
                display: self.list()?.remove(0),
                modes: vec![original()],
                scales: vec![100, 125, 150],
            })
        }
        fn read(&self, _: &str) -> Result<DisplayMode, String> {
            Ok(self.0.lock().unwrap().current.clone())
        }
        fn validate(&self, id: &str, m: &DisplayMode) -> Result<(), String> {
            if id != "stable-monitor"
                || m.width < 640
                || ![100, 125, 150].contains(&m.scale_percent)
            {
                Err("unsupported mode".into())
            } else {
                Ok(())
            }
        }
        fn apply(&self, _: &str, m: &DisplayMode) -> Result<(), String> {
            let mut data = self.0.lock().unwrap();
            data.current = m.clone();
            data.calls += 1;
            if data.fail > 0 {
                data.fail -= 1;
                Err("partial native write".into())
            } else {
                Ok(())
            }
        }
    }
    struct Fixture {
        path: PathBuf,
        data: Arc<Mutex<MockData>>,
        state: DisplayState,
    }
    impl Fixture {
        fn new() -> Self {
            let path = std::env::temp_dir().join(format!(
                "akadesk-display-test-{}",
                crate::pairing::generate_pair_token()
            ));
            let data = Arc::new(Mutex::new(MockData {
                current: original(),
                calls: 0,
                fail: 0,
            }));
            let state = DisplayState::with_backend(&path, Box::new(Mock(data.clone())));
            Self { path, data, state }
        }
        fn change(&mut self, scale: u32) -> Result<(), String> {
            self.state.apply(
                "session",
                "stable-monitor",
                DisplayMode {
                    scale_percent: scale,
                    ..original()
                },
            )
        }
    }
    impl Drop for Fixture {
        fn drop(&mut self) {
            let _ = std::fs::remove_dir_all(&self.path);
        }
    }
    #[test]
    fn invalid_and_foreign_requests_never_write_or_create_a_journal() {
        let mut f = Fixture::new();
        f.state.prepare("session").unwrap();
        assert!(f.change(999).is_err());
        assert!(f.state.apply("old", "stable-monitor", original()).is_err());
        assert!(f
            .state
            .apply("session", "different-monitor", original())
            .is_err());
        assert!(!f.state.journal.exists());
        assert_eq!(f.data.lock().unwrap().calls, 0);
    }
    #[test]
    fn original_baseline_is_immutable_and_late_cleanup_does_not_touch_the_new_session() {
        let mut f = Fixture::new();
        f.state.prepare("session").unwrap();
        f.change(125).unwrap();
        let record = std::fs::read(&f.state.journal).unwrap();
        f.change(150).unwrap();
        assert_eq!(record, std::fs::read(&f.state.journal).unwrap());
        assert_eq!(
            f.state
                .snapshot("session")
                .unwrap()
                .history_display_mode
                .unwrap()
                .scale_percent,
            100
        );
        f.state.finish("old").unwrap();
        assert_eq!(f.data.lock().unwrap().current.scale_percent, 150);
        f.state.finish("session").unwrap();
        assert_eq!(f.data.lock().unwrap().current, original());
        assert!(!f.state.journal.exists());
        f.state.prepare("new").unwrap();
        f.state.finish("session").unwrap();
        assert!(f.state.snapshot("new").is_ok());
    }
    #[test]
    fn partial_failure_rolls_back_to_the_previous_successful_request() {
        let mut f = Fixture::new();
        f.state.prepare("session").unwrap();
        f.change(125).unwrap();
        f.data.lock().unwrap().fail = 1;
        assert!(f.change(150).is_err());
        assert_eq!(f.data.lock().unwrap().current.scale_percent, 125);
        let snapshot = f.state.snapshot("session").unwrap();
        assert_eq!(snapshot.requested_display_mode.unwrap().scale_percent, 125);
        f.state.finish("session").unwrap();
        assert_eq!(f.data.lock().unwrap().current, original());
    }
    #[test]
    fn rollback_failure_keeps_journal_and_blocks_new_writes_until_restore() {
        let mut f = Fixture::new();
        f.state.prepare("session").unwrap();
        f.data.lock().unwrap().fail = 2;
        assert!(f.change(125).unwrap_err().contains("rollback failed"));
        assert!(f.state.journal.exists());
        let calls = f.data.lock().unwrap().calls;
        assert!(f.change(150).is_err());
        assert_eq!(f.data.lock().unwrap().calls, calls);
        f.data.lock().unwrap().fail = 1;
        assert!(f.state.finish("session").is_err());
        assert!(f.state.journal.exists());
        f.state.finish("session").unwrap();
        assert!(!f.state.journal.exists());
    }
    #[test]
    fn restart_restores_committed_baseline_and_failed_recovery_preserves_record() {
        let mut f = Fixture::new();
        f.state.prepare("session").unwrap();
        f.change(150).unwrap();
        let mut restarted = DisplayState::with_backend(&f.path, Box::new(Mock(f.data.clone())));
        f.data.lock().unwrap().fail = 1;
        assert!(restarted.recover().is_err());
        assert!(restarted.journal.exists());
        restarted.recover().unwrap();
        assert_eq!(f.data.lock().unwrap().current, original());
        assert!(!restarted.journal.exists());
    }
    #[test]
    fn persistence_failure_prevents_native_mutation() {
        let mut f = Fixture::new();
        f.state.prepare("session").unwrap();
        std::fs::write(&f.path, b"blocks-directory").unwrap();
        assert!(f.change(125).is_err());
        assert_eq!(f.data.lock().unwrap().calls, 0);
        std::fs::remove_file(&f.path).unwrap();
    }
}
pub trait DisplayBackend: Send + std::fmt::Debug {
    fn list(&self) -> Result<Vec<DisplaySnapshot>, String>;
    fn details(&self, id: &str) -> Result<Details, String>;
    fn read(&self, id: &str) -> Result<DisplayMode, String>;
    fn validate(&self, id: &str, mode: &DisplayMode) -> Result<(), String>;
    fn apply(&self, id: &str, mode: &DisplayMode) -> Result<(), String>;
}
#[cfg(windows)]
impl DisplayBackend for host_windows::display::WindowsDisplayBackend {
    fn list(&self) -> Result<Vec<DisplaySnapshot>, String> {
        self.list().map_err(|e| e.to_string())
    }
    fn details(&self, id: &str) -> Result<Details, String> {
        self.details(id)
            .map(|d| Details {
                display: d.snapshot,
                modes: d.modes,
                scales: d.scales,
            })
            .map_err(|e| e.to_string())
    }
    fn read(&self, id: &str) -> Result<DisplayMode, String> {
        self.read(id).map_err(|e| e.to_string())
    }
    fn validate(&self, id: &str, mode: &DisplayMode) -> Result<(), String> {
        self.validate(id, mode).map_err(|e| e.to_string())
    }
    fn apply(&self, id: &str, mode: &DisplayMode) -> Result<(), String> {
        self.apply(id, mode).map_err(|e| e.to_string())
    }
}
#[derive(Debug, Clone, Serialize, Deserialize)]
struct StoredMode {
    width: u32,
    height: u32,
    refresh_rate: u32,
    scale_percent: u32,
}
impl From<&DisplayMode> for StoredMode {
    fn from(m: &DisplayMode) -> Self {
        Self {
            width: m.width,
            height: m.height,
            refresh_rate: m.refresh_rate,
            scale_percent: m.scale_percent,
        }
    }
}
impl From<&StoredMode> for DisplayMode {
    fn from(m: &StoredMode) -> Self {
        Self {
            width: m.width,
            height: m.height,
            refresh_rate: m.refresh_rate,
            scale_percent: m.scale_percent,
        }
    }
}
#[derive(Debug, Clone, Serialize, Deserialize)]
struct Journal {
    display_id: String,
    history: StoredMode,
}
#[derive(Debug)]
struct Active {
    session_id: String,
    display_id: String,
    history: DisplayMode,
    requested: Option<DisplayMode>,
    journaled: bool,
    failed: bool,
}
#[derive(Debug)]
pub struct DisplayState {
    backend: Option<Box<dyn DisplayBackend>>,
    journal: PathBuf,
    active: Option<Active>,
    pub last_restore_error: Option<String>,
}
pub type SharedDisplayState = Arc<Mutex<DisplayState>>;
fn proto(m: &DisplayMode) -> ProtoMode {
    ProtoMode {
        width: m.width,
        height: m.height,
        refresh_rate: m.refresh_rate,
        scale_percent: m.scale_percent,
    }
}
pub fn from_proto(m: &ProtoMode) -> DisplayMode {
    DisplayMode {
        width: m.width,
        height: m.height,
        refresh_rate: m.refresh_rate,
        scale_percent: m.scale_percent,
    }
}
pub fn display_info(s: DisplaySnapshot) -> DisplayInfo {
    DisplayInfo {
        display_id: s.display_id.0,
        name: s.name,
        width: s.width,
        height: s.height,
        refresh_rate: s.refresh_rate,
        scale_percent: s.scale_percent,
        primary: s.primary,
        hdr: s.hdr,
    }
}
impl DisplayState {
    pub fn new(base: &Path) -> Self {
        Self {
            journal: base.join("display_restore.json"),
            active: None,
            last_restore_error: None,
            #[cfg(windows)]
            backend: Some(Box::new(host_windows::display::WindowsDisplayBackend)),
            #[cfg(not(windows))]
            backend: None,
        }
    }
    pub fn with_backend(base: &Path, backend: Box<dyn DisplayBackend>) -> Self {
        Self {
            backend: Some(backend),
            ..Self::new(base)
        }
    }
    fn backend(&self) -> Result<&dyn DisplayBackend, String> {
        self.backend
            .as_deref()
            .ok_or("Display control unavailable".into())
    }
    pub fn list(&self) -> Result<Vec<DisplayInfo>, String> {
        Ok(self
            .backend()?
            .list()?
            .into_iter()
            .map(display_info)
            .collect())
    }
    pub fn recover(&mut self) -> Result<(), String> {
        if !self.journal.exists() {
            self.last_restore_error = None;
            return Ok(());
        }
        if self.active.is_some() {
            return Err("Cannot recover over an active display session".into());
        }
        let result = (|| {
            let journal: Journal =
                serde_json::from_slice(&std::fs::read(&self.journal).map_err(|e| e.to_string())?)
                    .map_err(|e| e.to_string())?;
            let history = DisplayMode::from(&journal.history);
            self.backend()?.apply(&journal.display_id, &history)?;
            if self.backend()?.read(&journal.display_id)? != history {
                return Err("Recovery readback mismatch".into());
            }
            std::fs::remove_file(&self.journal).map_err(|e| e.to_string())
        })();
        self.last_restore_error = result.as_ref().err().cloned();
        result
    }
    pub fn prepare(&mut self, id: &str) -> Result<(), String> {
        if self.active.as_ref().is_some_and(|a| a.session_id == id) {
            return Ok(());
        }
        if self.active.is_some() {
            return Err("Display is already owned by a session".into());
        }
        self.recover()?;
        let display = self
            .backend()?
            .list()?
            .into_iter()
            .find(|s| s.primary)
            .ok_or("No supported primary display")?;
        let history = self.backend()?.read(&display.display_id.0)?;
        self.active = Some(Active {
            session_id: id.into(),
            display_id: display.display_id.0,
            history,
            requested: None,
            journaled: false,
            failed: false,
        });
        Ok(())
    }
    pub fn snapshot(&self, id: &str) -> Result<DisplaySettingsSnapshot, String> {
        let active = self
            .active
            .as_ref()
            .filter(|a| a.session_id == id)
            .ok_or("Display session is not prepared")?;
        let details = self.backend()?.details(&active.display_id)?;
        let current = self.backend()?.read(&active.display_id)?;
        Ok(DisplaySettingsSnapshot {
            display: Some(display_info(details.display)),
            current_mode: Some(proto(&current)),
            history_display_mode: Some(proto(&active.history)),
            requested_display_mode: active.requested.as_ref().map(proto),
            supported_modes: details.modes.iter().map(proto).collect(),
            supported_scale_percents: details.scales,
        })
    }
    pub fn baseline(&self, id: &str) -> Option<(String, ProtoMode)> {
        self.active
            .as_ref()
            .filter(|a| a.session_id == id)
            .map(|a| (a.display_id.clone(), proto(&a.history)))
    }
    fn persist(&self) -> Result<(), String> {
        let active = self.active.as_ref().unwrap();
        let record = Journal {
            display_id: active.display_id.clone(),
            history: StoredMode::from(&active.history),
        };
        let bytes = serde_json::to_vec(&record).map_err(|e| e.to_string())?;
        let parent = self.journal.parent().ok_or("Missing journal parent")?;
        std::fs::create_dir_all(parent).map_err(|e| e.to_string())?;
        let temporary = self.journal.with_extension("pending");
        // A stale temporary contains no applied change: only the committed journal
        // permits mutation. Never overwrite an existing committed recovery record.
        if self.journal.exists() {
            return Err("Display recovery record already exists".into());
        }
        use std::io::Write;
        let mut file = std::fs::File::create(&temporary).map_err(|e| e.to_string())?;
        file.write_all(&bytes)
            .and_then(|_| file.sync_all())
            .map_err(|e| e.to_string())?;
        drop(file);
        std::fs::rename(temporary, &self.journal).map_err(|e| e.to_string())
    }
    pub fn apply(&mut self, id: &str, display_id: &str, mode: DisplayMode) -> Result<(), String> {
        let active = self
            .active
            .as_ref()
            .filter(|a| a.session_id == id && a.display_id == display_id)
            .ok_or("Display/session mismatch")?;
        if active.failed {
            return Err("Display restoration pending; reconnect after recovery".into());
        }
        self.backend()?.validate(display_id, &mode)?;
        let before = self.backend()?.read(display_id)?;
        if before == mode {
            self.active.as_mut().unwrap().requested = Some(mode);
            return Ok(());
        }
        if !active.journaled {
            self.persist()?;
            self.active.as_mut().unwrap().journaled = true
        }
        let result = self.backend()?.apply(display_id, &mode).and_then(|_| {
            if self.backend()?.read(display_id)? == mode {
                Ok(())
            } else {
                Err("Applied display did not match request".into())
            }
        });
        if let Err(error) = result {
            // A partial resolution/DPI write must roll back to the pre-request state.
            // The session's original baseline stays immutable for disconnect recovery.
            let rollback = self.backend()?.apply(display_id, &before).and_then(|_| {
                if self.backend()?.read(display_id)? == before {
                    Ok(())
                } else {
                    Err("Rollback readback mismatch".into())
                }
            });
            if let Err(recovery) = rollback {
                self.active.as_mut().unwrap().failed = true;
                return Err(format!("{error}; rollback failed: {recovery}"));
            }
            return Err(error);
        }
        self.active.as_mut().unwrap().requested = Some(mode);
        Ok(())
    }
    pub fn finish(&mut self, id: &str) -> Result<(), String> {
        let Some(active) = self.active.as_ref().filter(|a| a.session_id == id) else {
            return Ok(());
        };
        if active.journaled {
            let result = self
                .backend()?
                .apply(&active.display_id, &active.history)
                .and_then(|_| {
                    if self.backend()?.read(&active.display_id)? == active.history {
                        Ok(())
                    } else {
                        Err("Restore readback mismatch".into())
                    }
                });
            if let Err(error) = result {
                self.last_restore_error = Some(error.clone());
                return Err(error);
            }
            std::fs::remove_file(&self.journal).map_err(|e| e.to_string())?;
        }
        self.active = None;
        self.last_restore_error = None;
        Ok(())
    }
}

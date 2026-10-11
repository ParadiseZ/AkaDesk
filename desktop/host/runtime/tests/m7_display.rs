#[cfg(windows)]
#[test]
#[ignore = "Temporarily changes primary resolution/refresh/DPI and captures the real desktop; restores the baseline"]
fn real_display_changes_readback_and_restore() {
    use host_runtime::display::DisplayState;
    use host_windows::display::WindowsDisplayBackend;
    let backend = WindowsDisplayBackend;
    let display = backend
        .list()
        .unwrap()
        .into_iter()
        .find(|d| d.primary)
        .unwrap();
    let original = backend.read(&display.display_id.0).unwrap();
    let details = backend.details(&display.display_id.0).unwrap();
    let path = std::env::temp_dir().join(format!(
        "akadesk-m7-real-{}",
        host_runtime::pairing::generate_pair_token()
    ));
    struct Restore {
        state: DisplayState,
        path: std::path::PathBuf,
        id: String,
        original: host_contracts::display::DisplayMode,
    }
    impl Drop for Restore {
        fn drop(&mut self) {
            let session_restored = self.state.finish("acceptance").is_ok();
            let backend = WindowsDisplayBackend;
            let native_restored = backend
                .read(&self.id)
                .is_ok_and(|mode| mode == self.original)
                || backend.apply(&self.id, &self.original).is_ok();
            if session_restored && native_restored {
                let _ = std::fs::remove_dir_all(&self.path);
            } else {
                eprintln!("Display restoration pending at {}", self.path.display());
            }
        }
    }
    let mut guard = Restore {
        state: DisplayState::new(&path),
        path,
        id: display.display_id.0.clone(),
        original: original.clone(),
    };
    guard.state.prepare("acceptance").unwrap();
    println!("Original: {original:?}");
    let mut changes = Vec::new();
    let resolution = details
        .modes
        .iter()
        .filter(|m| {
            m.width >= 1280
                && m.height >= 720
                && m.width <= original.width
                && m.height <= original.height
                && (m.width != original.width || m.height != original.height)
                && m.refresh_rate == original.refresh_rate
        })
        .max_by_key(|m| m.width * m.height)
        .expect("Needs an alternate supported resolution")
        .clone();
    changes.push(resolution);
    let rate = details
        .modes
        .iter()
        .filter(|m| {
            m.width == original.width
                && m.height == original.height
                && m.refresh_rate != original.refresh_rate
        })
        .min_by_key(|m| m.refresh_rate.abs_diff(original.refresh_rate))
        .expect("Needs an alternate refresh rate")
        .clone();
    changes.push(rate);
    let scale = *details
        .scales
        .iter()
        .find(|s| **s != original.scale_percent)
        .expect("Needs alternate DPI scale");
    changes.push(host_contracts::display::DisplayMode {
        scale_percent: scale,
        ..original.clone()
    });
    for changed in changes {
        guard
            .state
            .apply("acceptance", &display.display_id.0, changed.clone())
            .unwrap();
        assert_eq!(backend.read(&display.display_id.0).unwrap(), changed);
        let frame = host_windows::cap::capture_primary_display_bgra().unwrap();
        assert_eq!((frame.width, frame.height), (changed.width, changed.height));
        println!("Applied and captured: {changed:?}");
    }
    guard.state.finish("acceptance").unwrap();
    assert_eq!(backend.read(&display.display_id.0).unwrap(), original);
    // Simulate a process crash with a committed journal, then recover in a fresh state.
    let mut abandoned = DisplayState::new(&guard.path);
    abandoned.prepare("crash").unwrap();
    abandoned
        .apply(
            "crash",
            &display.display_id.0,
            host_contracts::display::DisplayMode {
                scale_percent: scale,
                ..original.clone()
            },
        )
        .unwrap();
    drop(abandoned);
    DisplayState::new(&guard.path).recover().unwrap();
    assert_eq!(backend.read(&display.display_id.0).unwrap(), original);
    assert!(!guard.path.join("display_restore.json").exists());
    println!("Normal stop and persisted recovery restored: {original:?}");
}

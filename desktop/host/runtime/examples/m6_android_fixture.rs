//! Isolated real-device acceptance. Input is restricted to this fixture's native window.
#[cfg(windows)]
mod fixture {
    use host_runtime::generated::v1::input_event::Payload;
    use host_runtime::{
        control::spawn_control_server,
        input::{InputSink, InputState},
        service::RuntimeService,
    };
    use host_windows::input::WindowsInputBackend;
    use std::sync::{mpsc, Mutex, OnceLock};
    use std::time::{Duration, Instant};
    use windows::core::w;
    use windows::Win32::Foundation::*;
    use windows::Win32::System::LibraryLoader::GetModuleHandleW;
    use windows::Win32::UI::HiDpi::*;
    use windows::Win32::UI::Input::KeyboardAndMouse::*;
    use windows::Win32::UI::WindowsAndMessaging::*;
    static PROBE: OnceLock<Mutex<Probe>> = OnceLock::new();
    #[derive(Default, serde::Serialize)]
    struct Probe {
        left_down: u32,
        left_up: u32,
        right_down: u32,
        right_up: u32,
        moves: u32,
        vertical_wheel: i32,
        horizontal_wheel: i32,
        keys_down: Vec<u32>,
        keys_up: Vec<u32>,
        utf16: Vec<u16>,
        ctrl_c: u32,
        pointer_positions: Vec<(i32, i32)>,
    }
    unsafe extern "system" fn window_proc(
        hwnd: HWND,
        message: u32,
        wp: WPARAM,
        lp: LPARAM,
    ) -> LRESULT {
        if let Some(probe) = PROBE.get() {
            let mut p = probe.lock().unwrap();
            match message {
                WM_LBUTTONDOWN => {
                    p.left_down += 1;
                    p.pointer_positions.push((
                        (lp.0 as u16) as i16 as i32,
                        ((lp.0 >> 16) as u16) as i16 as i32,
                    ));
                }
                WM_LBUTTONUP => p.left_up += 1,
                WM_RBUTTONDOWN => p.right_down += 1,
                WM_RBUTTONUP => p.right_up += 1,
                WM_MOUSEMOVE => p.moves += 1,
                WM_MOUSEWHEEL => p.vertical_wheel += (wp.0 >> 16) as u16 as i16 as i32,
                WM_MOUSEHWHEEL => p.horizontal_wheel += (wp.0 >> 16) as u16 as i16 as i32,
                WM_KEYDOWN | WM_SYSKEYDOWN => {
                    p.keys_down.push(wp.0 as u32);
                    if wp.0 == 0x43 && GetKeyState(0x11) < 0 {
                        p.ctrl_c += 1;
                    }
                }
                WM_KEYUP | WM_SYSKEYUP => p.keys_up.push(wp.0 as u32),
                WM_CHAR => p.utf16.push(wp.0 as u16),
                _ => {}
            }
        }
        match message {
            WM_CLOSE => {
                let _ = DestroyWindow(hwnd);
                LRESULT(0)
            }
            WM_DESTROY => {
                PostQuitMessage(0);
                LRESULT(0)
            }
            _ => DefWindowProcW(hwnd, message, wp, lp),
        }
    }
    #[derive(Debug)]
    struct GuardedInput {
        window: usize,
        native: WindowsInputBackend,
    }
    struct ProbeWindow {
        handle: usize,
        thread: Option<std::thread::JoinHandle<()>>,
    }
    impl Drop for ProbeWindow {
        fn drop(&mut self) {
            unsafe {
                let _ = PostMessageW(
                    Some(HWND(self.handle as *mut _)),
                    WM_CLOSE,
                    WPARAM(0),
                    LPARAM(0),
                );
            }
            if let Some(thread) = self.thread.take() {
                let _ = thread.join();
            }
        }
    }
    impl InputSink for GuardedInput {
        fn prepare(&self, id: &str) -> Result<(), String> {
            self.native.prepare(id).map_err(|e| e.to_string())
        }
        fn inject(&self, p: &Payload) -> Result<(), String> {
            if !matches!(p, Payload::Release(_))
                && unsafe { GetForegroundWindow().0 as usize } != self.window
            {
                // Windows may refuse programmatic foreground activation. Permit only the
                // initial click on this visible probe to activate it, after a hit-test.
                let activate = if let Payload::Pointer(event) = p {
                    unsafe {
                        let previous = SetThreadDpiAwarenessContext(
                            DPI_AWARENESS_CONTEXT_PER_MONITOR_AWARE_V2,
                        );
                        let point = POINT {
                            x: (event.normalized_x * (GetSystemMetrics(SM_CXSCREEN) - 1) as f32)
                                .round() as i32,
                            y: (event.normalized_y * (GetSystemMetrics(SM_CYSCREEN) - 1) as f32)
                                .round() as i32,
                        };
                        let hit = WindowFromPoint(point).0 as usize == self.window;
                        if !previous.0.is_null() {
                            SetThreadDpiAwarenessContext(previous);
                        }
                        event.button == 1 && event.pressed && hit
                    }
                } else {
                    false
                };
                if !activate {
                    return Err("Fixture window lost foreground; input blocked".into());
                }
            }
            let result = InputSink::inject(&self.native, p);
            unsafe {
                let previous =
                    SetThreadDpiAwarenessContext(DPI_AWARENESS_CONTEXT_PER_MONITOR_AWARE_V2);
                let mut cursor = POINT::default();
                let _ = GetCursorPos(&mut cursor);
                eprintln!(
                    "probe input seq={} cursor=({}, {}) foreground={:?} probe={:?} hit={:?}",
                    host_runtime::input::sequence(p),
                    cursor.x,
                    cursor.y,
                    GetForegroundWindow(),
                    HWND(self.window as *mut _),
                    WindowFromPoint(cursor)
                );
                if !previous.0.is_null() {
                    SetThreadDpiAwarenessContext(previous);
                }
            }
            result
        }
        fn release_held(&self) -> Result<(), String> {
            self.native.release_held().map_err(|e| e.to_string())
        }
        fn finish(&self, id: &str) -> Result<(), String> {
            self.native.release(id).map_err(|e| e.to_string())
        }
    }
    pub fn run() -> Result<(), Box<dyn std::error::Error>> {
        let directory = std::path::PathBuf::from(
            std::env::args()
                .nth(1)
                .expect("Pass an isolated new fixture directory"),
        );
        std::fs::create_dir_all(&directory)?;
        PROBE.set(Mutex::new(Probe::default())).ok();
        let (ready_tx, ready_rx) = mpsc::channel();
        let window_thread = std::thread::spawn(move || unsafe {
            SetThreadDpiAwarenessContext(DPI_AWARENESS_CONTEXT_PER_MONITOR_AWARE_V2);
            let original = GetForegroundWindow();
            let mut cursor = POINT::default();
            let _ = GetCursorPos(&mut cursor);
            let module = GetModuleHandleW(None).unwrap();
            let class = WNDCLASSW {
                lpfnWndProc: Some(window_proc),
                hInstance: HINSTANCE(module.0),
                lpszClassName: w!("AkaDeskM6Probe"),
                ..Default::default()
            };
            assert_ne!(RegisterClassW(&class), 0);
            let pw = GetSystemMetrics(SM_CXSCREEN);
            let ph = GetSystemMetrics(SM_CYSCREEN);
            let hwnd = CreateWindowExW(
                WINDOW_EX_STYLE::default(),
                w!("AkaDeskM6Probe"),
                w!("AkaDesk M6 input acceptance"),
                WS_OVERLAPPEDWINDOW | WS_VISIBLE,
                pw / 4,
                ph / 4,
                pw / 2,
                ph / 2,
                None,
                None,
                Some(HINSTANCE(module.0)),
                None,
            )
            .unwrap();
            SetWindowPos(
                hwnd,
                Some(HWND_TOPMOST),
                0,
                0,
                0,
                0,
                SWP_NOMOVE | SWP_NOSIZE | SWP_NOACTIVATE,
            )
            .unwrap();
            let _ = SetForegroundWindow(hwnd);
            let _ = SetFocus(Some(hwnd));
            let mut client = RECT::default();
            GetClientRect(hwnd, &mut client).unwrap();
            let mut point = POINT {
                x: client.right / 2,
                y: client.bottom / 2,
            };
            let _ = windows::Win32::Graphics::Gdi::ClientToScreen(hwnd, &mut point);
            ready_tx
                .send((
                    hwnd.0 as usize,
                    point.x as f32 / (pw - 1) as f32,
                    point.y as f32 / (ph - 1) as f32,
                ))
                .unwrap();
            let mut message = MSG::default();
            while GetMessageW(&mut message, None, 0, 0).as_bool() {
                let _ = TranslateMessage(&message);
                DispatchMessageW(&message);
            }
            let _ = SetCursorPos(cursor.x, cursor.y);
            if !original.0.is_null() && !IsIconic(original).as_bool() {
                let _ = SetForegroundWindow(original);
            }
        });
        let (window, x, y) = ready_rx.recv_timeout(Duration::from_secs(5))?;
        let _window_guard = ProbeWindow {
            handle: window,
            thread: Some(window_thread),
        };
        let service = RuntimeService::new_in(&directory)?;
        let mut state = service.control_state().clone();
        state.advertised_presence.control_port = 46671;
        *state.input.lock().unwrap() = InputState::with_sink(Box::new(GuardedInput {
            window,
            native: WindowsInputBackend::default(),
        }));
        let code = state
            .settings
            .lock()
            .unwrap()
            .current()
            .temporary_verification_code;
        std::fs::write(
            directory.join("android_fixture.json"),
            serde_json::to_vec(
                &serde_json::json!({"hostId":state.host_identity.device_id,"pairCode":code,"port":46671,"x":x,"y":y}),
            )?,
        )?;
        let _server = spawn_control_server(state)?;
        eprintln!("M6 fixture ready; native probe center ({x}, {y}); auto-stop 180s");
        let start = Instant::now();
        while start.elapsed() < Duration::from_secs(180) && !directory.join("stop").exists() {
            service.reap_finished_session()?;
            let p = PROBE.get().unwrap().lock().unwrap();
            std::fs::write(directory.join("input_probe.json"), serde_json::to_vec(&*p)?)?;
            drop(p);
            std::thread::sleep(Duration::from_millis(100));
        }
        let id = service
            .sessions()
            .lock()
            .unwrap()
            .active_session_id()
            .cloned();
        if let Some(id) = id {
            service.stop_session(&id.0)?;
        }
        std::thread::sleep(Duration::from_millis(250));
        let p = PROBE.get().unwrap().lock().unwrap();
        let text = String::from_utf16_lossy(&p.utf16);
        let held = unsafe {
            [0x10, 0x11, 0x12, 0x5b, 0x77, 0x01].map(|key| (key, GetAsyncKeyState(key) < 0))
        };
        std::fs::write(
            directory.join("input_probe_final.json"),
            serde_json::to_vec(&serde_json::json!({"probe":&*p,"text":text,"held":held}))?,
        )?;
        drop(p);
        Ok(())
    }
}
#[cfg(windows)]
fn main() -> Result<(), Box<dyn std::error::Error>> {
    fixture::run()
}
#[cfg(not(windows))]
fn main() {
    eprintln!("M6 fixture requires Windows");
}

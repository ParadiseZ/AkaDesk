use host_contracts::error::{HostError, HostErrorCode, HostResult, RetryClass};
use host_contracts::input::*;
use std::collections::{BTreeMap, BTreeSet};
use std::sync::Mutex;
use windows::Win32::UI::HiDpi::{
    SetThreadDpiAwarenessContext, DPI_AWARENESS_CONTEXT_PER_MONITOR_AWARE_V2,
};
use windows::Win32::UI::Input::KeyboardAndMouse::*;
use windows::Win32::UI::WindowsAndMessaging::*;

#[derive(Debug, Default)]
struct HeldInput {
    session_id: Option<String>,
    buttons: BTreeSet<u8>,
    // Each held key owns its chord requirements; overlapping chords stay held.
    keys: BTreeMap<u16, u8>,
    modifiers: BTreeSet<u16>,
}
#[derive(Debug, Default)]
pub struct WindowsInputBackend {
    held: Mutex<HeldInput>,
}

fn error(message: impl Into<String>) -> HostError {
    HostError::new(HostErrorCode::Unavailable, RetryClass::Never, message)
}
fn send(input: INPUT) -> HostResult<()> {
    let count = unsafe { SendInput(&[input], std::mem::size_of::<INPUT>() as i32) };
    if count != 1 {
        return Err(error(format!(
            "SendInput failed (desktop permissions/UIPI): {}",
            windows::core::Error::from_win32()
        )));
    }
    Ok(())
}
fn mouse(flags: MOUSE_EVENT_FLAGS, dx: i32, dy: i32, data: u32) -> INPUT {
    INPUT {
        r#type: INPUT_MOUSE,
        Anonymous: INPUT_0 {
            mi: MOUSEINPUT {
                dx,
                dy,
                mouseData: data,
                dwFlags: flags,
                time: 0,
                dwExtraInfo: 0,
            },
        },
    }
}
fn key(vk: u16, up: bool) -> INPUT {
    let extended = matches!(vk, 0x21..=0x28 | 0x2d..=0x2e | 0x5b..=0x5d | 0x6f | 0xa3 | 0xa5);
    let mut flags = KEYBD_EVENT_FLAGS(0);
    if up {
        flags |= KEYEVENTF_KEYUP;
    }
    if extended {
        flags |= KEYEVENTF_EXTENDEDKEY;
    }
    INPUT {
        r#type: INPUT_KEYBOARD,
        Anonymous: INPUT_0 {
            ki: KEYBDINPUT {
                wVk: VIRTUAL_KEY(vk),
                wScan: 0,
                dwFlags: flags,
                time: 0,
                dwExtraInfo: 0,
            },
        },
    }
}
fn button_flag(button: u8, up: bool) -> HostResult<MOUSE_EVENT_FLAGS> {
    match (button, up) {
        (1, false) => Ok(MOUSEEVENTF_LEFTDOWN),
        (1, true) => Ok(MOUSEEVENTF_LEFTUP),
        (2, false) => Ok(MOUSEEVENTF_RIGHTDOWN),
        (2, true) => Ok(MOUSEEVENTF_RIGHTUP),
        (3, false) => Ok(MOUSEEVENTF_MIDDLEDOWN),
        (3, true) => Ok(MOUSEEVENTF_MIDDLEUP),
        _ => Err(error("Invalid pointer button")),
    }
}
fn modifier_bit(vk: u16) -> u8 {
    match vk {
        0x10 => 1,
        0x11 => 2,
        0x12 => 4,
        0x5b => 8,
        _ => 0,
    }
}
fn sync_modifiers(held: &mut HeldInput) -> HostResult<()> {
    let required = held
        .keys
        .iter()
        .fold(0, |mask, (vk, chord)| mask | chord | modifier_bit(*vk));
    for (vk, bit) in [(0x10, 1), (0x11, 2), (0x12, 4), (0x5b, 8)] {
        let desired = required & bit != 0;
        if desired && !held.modifiers.contains(&vk) {
            send(key(vk, false))?;
            held.modifiers.insert(vk);
        } else if !desired && held.modifiers.contains(&vk) {
            send(key(vk, true))?;
            held.modifiers.remove(&vk);
        }
    }
    Ok(())
}
impl WindowsInputBackend {
    pub fn prepare(&self, session_id: &str) -> HostResult<()> {
        let mut held = self.held.lock().map_err(|_| error("Input lock poisoned"))?;
        if held.session_id.is_some() {
            return Err(error("Input is already prepared"));
        }
        held.session_id = Some(session_id.into());
        Ok(())
    }
    pub fn pointer(&self, event: PointerInputCommand) -> HostResult<()> {
        if !event.normalized_x.is_finite()
            || !event.normalized_y.is_finite()
            || !(0.0..=1.0).contains(&event.normalized_x)
            || !(0.0..=1.0).contains(&event.normalized_y)
            || event.button > 3
        {
            return Err(error("Invalid normalized pointer"));
        }
        let mut held = self.held.lock().map_err(|_| error("Input lock poisoned"))?;
        if held.session_id.is_none() {
            return Err(error("Input is not prepared"));
        }
        // Match the primary-display capture, independent of desktop scaling.
        unsafe {
            let previous = SetThreadDpiAwarenessContext(DPI_AWARENESS_CONTEXT_PER_MONITOR_AWARE_V2);
            let primary_w = GetSystemMetrics(SM_CXSCREEN);
            let primary_h = GetSystemMetrics(SM_CYSCREEN);
            let vx = GetSystemMetrics(SM_XVIRTUALSCREEN);
            let vy = GetSystemMetrics(SM_YVIRTUALSCREEN);
            let vw = GetSystemMetrics(SM_CXVIRTUALSCREEN);
            let vh = GetSystemMetrics(SM_CYVIRTUALSCREEN);
            if !previous.0.is_null() {
                SetThreadDpiAwarenessContext(previous);
            }
            if primary_w <= 0 || primary_h <= 0 || vw <= 1 || vh <= 1 {
                return Err(error("No input desktop geometry"));
            }
            let px = (event.normalized_x * (primary_w - 1) as f32).round() as i32;
            let py = (event.normalized_y * (primary_h - 1) as f32).round() as i32;
            send(mouse(
                MOUSEEVENTF_MOVE | MOUSEEVENTF_ABSOLUTE | MOUSEEVENTF_VIRTUALDESK,
                ((px - vx) as i64 * 65535 / (vw - 1) as i64) as i32,
                ((py - vy) as i64 * 65535 / (vh - 1) as i64) as i32,
                0,
            ))?;
        }
        if event.button != 0 {
            if event.pressed && !held.buttons.contains(&event.button) {
                send(mouse(button_flag(event.button, false)?, 0, 0, 0))?;
                held.buttons.insert(event.button);
            } else if !event.pressed && held.buttons.contains(&event.button) {
                send(mouse(button_flag(event.button, true)?, 0, 0, 0))?;
                held.buttons.remove(&event.button);
            }
        }
        Ok(())
    }
    pub fn scroll(&self, event: ScrollInputCommand) -> HostResult<()> {
        if [event.delta_x, event.delta_y]
            .iter()
            .any(|v| !v.is_finite() || v.abs() > 10.0)
        {
            return Err(error("Invalid wheel delta"));
        }
        let held = self.held.lock().map_err(|_| error("Input lock poisoned"))?;
        if held.session_id.is_none() {
            return Err(error("Input is not prepared"));
        }
        for (value, flag) in [
            (event.delta_x, MOUSEEVENTF_HWHEEL),
            (event.delta_y, MOUSEEVENTF_WHEEL),
        ] {
            let delta = (value * 120.0).round() as i32;
            if delta != 0 {
                send(mouse(flag, 0, 0, delta as u32))?;
            }
        }
        Ok(())
    }
    pub fn keyboard(&self, event: KeyInputCommand) -> HostResult<()> {
        if !(8..=254).contains(&event.key_code) || ![1, 2].contains(&event.action) {
            return Err(error("Invalid virtual key/action"));
        }
        let vk = event.key_code as u16;
        let mut held = self.held.lock().map_err(|_| error("Input lock poisoned"))?;
        if held.session_id.is_none() {
            return Err(error("Input is not prepared"));
        }
        if event.action == 1 {
            let chord = u8::from(event.shift)
                | (u8::from(event.ctrl) << 1)
                | (u8::from(event.alt) << 2)
                | (u8::from(event.meta) << 3);
            held.keys.insert(vk, chord);
            sync_modifiers(&mut held)?;
            if modifier_bit(vk) == 0 {
                send(key(vk, false))?;
            }
        } else if held.keys.contains_key(&vk) {
            if modifier_bit(vk) == 0 {
                send(key(vk, true))?;
            }
            held.keys.remove(&vk);
            sync_modifiers(&mut held)?;
        }
        Ok(())
    }
    pub fn text(&self, event: TextInputCommand) -> HostResult<()> {
        if event.text.is_empty() || event.text.len() > 4096 || event.text.contains('\0') {
            return Err(error("Invalid text input"));
        }
        let held = self.held.lock().map_err(|_| error("Input lock poisoned"))?;
        if held.session_id.is_none() || !held.keys.is_empty() {
            return Err(error("Release keyboard keys before text input"));
        }
        for unit in event.text.encode_utf16() {
            for up in [false, true] {
                send(INPUT {
                    r#type: INPUT_KEYBOARD,
                    Anonymous: INPUT_0 {
                        ki: KEYBDINPUT {
                            wVk: VIRTUAL_KEY(0),
                            wScan: unit,
                            dwFlags: KEYEVENTF_UNICODE
                                | if up {
                                    KEYEVENTF_KEYUP
                                } else {
                                    KEYBD_EVENT_FLAGS(0)
                                },
                            time: 0,
                            dwExtraInfo: 0,
                        },
                    },
                })?;
            }
        }
        Ok(())
    }
    pub fn release_held(&self) -> HostResult<()> {
        let mut held = self.held.lock().map_err(|_| error("Input lock poisoned"))?;
        let mut failed = None;
        for vk in held
            .keys
            .keys()
            .copied()
            .filter(|k| modifier_bit(*k) == 0)
            .collect::<Vec<_>>()
        {
            match send(key(vk, true)) {
                Ok(()) => {
                    held.keys.remove(&vk);
                }
                Err(e) => {
                    failed = Some(e);
                }
            }
        }
        for vk in held.modifiers.clone() {
            match send(key(vk, true)) {
                Ok(()) => {
                    held.modifiers.remove(&vk);
                    held.keys.remove(&vk);
                }
                Err(e) => {
                    failed = Some(e);
                }
            }
        }
        for button in held.buttons.clone() {
            match send(mouse(button_flag(button, true)?, 0, 0, 0)) {
                Ok(()) => {
                    held.buttons.remove(&button);
                }
                Err(e) => {
                    failed = Some(e);
                }
            }
        }
        failed.map_or(Ok(()), Err)
    }
    pub fn release(&self, session_id: &str) -> HostResult<()> {
        if self
            .held
            .lock()
            .map_err(|_| error("Input lock poisoned"))?
            .session_id
            .as_deref()
            != Some(session_id)
        {
            return Ok(());
        }
        self.release_held()?;
        self.held
            .lock()
            .map_err(|_| error("Input lock poisoned"))?
            .session_id = None;
        Ok(())
    }
}
impl Drop for WindowsInputBackend {
    fn drop(&mut self) {
        let _ = self.release_held();
    }
}
impl InputCapabilityQueryPort for WindowsInputBackend {
    async fn get_input_capabilities(&self) -> HostResult<InputCapabilitySnapshot> {
        Ok(InputCapabilitySnapshot {
            supports_pointer: true,
            supports_scroll: true,
            supports_keyboard: true,
        })
    }
}
impl InputInjectionPort for WindowsInputBackend {
    async fn prepare_input(&self, request: InputPrepareRequest) -> HostResult<InputPrepareResult> {
        self.prepare(&request.session_id)?;
        Ok(InputPrepareResult { prepared: true })
    }
    async fn inject_pointer_event(&self, event: PointerInputCommand) -> HostResult<()> {
        self.pointer(event)
    }
    async fn inject_scroll_event(&self, event: ScrollInputCommand) -> HostResult<()> {
        self.scroll(event)
    }
    async fn inject_key_event(&self, event: KeyInputCommand) -> HostResult<()> {
        self.keyboard(event)
    }
    async fn inject_text_event(&self, event: TextInputCommand) -> HostResult<()> {
        self.text(event)
    }
    async fn release_input(&self, request: InputReleaseRequest) -> HostResult<()> {
        self.release(&request.session_id)
    }
}

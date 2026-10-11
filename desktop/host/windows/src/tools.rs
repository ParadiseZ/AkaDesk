//! Interactive-user operations. Never run a shell with controller-provided commands.
use windows::Win32::Foundation::{HANDLE, HGLOBAL};
use windows::Win32::System::DataExchange::*;
use windows::Win32::System::Memory::*;
use windows::Win32::System::Shutdown::LockWorkStation;

pub fn system_action(action: &str) -> Result<(), String> {
    let system_directory = std::path::PathBuf::from(
        std::env::var_os("SystemRoot").ok_or("Windows directory is unavailable")?,
    )
    .join("System32");
    match action {
        "lock" => unsafe { LockWorkStation().map_err(|e| e.to_string()) },
        "restart" | "shutdown" => {
            let status = std::process::Command::new(system_directory.join("shutdown.exe"))
                .args([if action == "restart" { "/r" } else { "/s" }, "/t", "5"])
                .creation_flags(0x08000000)
                .status()
                .map_err(|e| e.to_string())?;
            if status.success() {
                Ok(())
            } else {
                Err("Windows did not accept the power operation".into())
            }
        }
        "task_manager" => {
            std::process::Command::new(system_directory.join("taskmgr.exe"))
                .spawn()
                .map_err(|e| e.to_string())?;
            Ok(())
        }
        _ => Err("Unknown system operation".into()),
    }
}
use std::os::windows::process::CommandExt;

struct Clipboard;
impl Drop for Clipboard {
    fn drop(&mut self) {
        unsafe {
            let _ = CloseClipboard();
        }
    }
}
pub fn clipboard_read() -> Result<String, String> {
    unsafe {
        OpenClipboard(None).map_err(|e| e.to_string())?;
        let _guard = Clipboard;
        if !IsClipboardFormatAvailable(13).is_ok() {
            return Ok(String::new());
        }
        let data = GetClipboardData(13).map_err(|e| e.to_string())?;
        let global = HGLOBAL(data.0);
        let count = (GlobalSize(global) / 2).min(32_769);
        let pointer = GlobalLock(global) as *const u16;
        if pointer.is_null() {
            return Err("Clipboard is unavailable".into());
        }
        let slice = std::slice::from_raw_parts(pointer, count);
        let length = slice.iter().position(|v| *v == 0).unwrap_or(count);
        let result = if length > 32_768 {
            Err("Clipboard text exceeds 32K characters".into())
        } else {
            Ok(String::from_utf16_lossy(&slice[..length]))
        };
        let _ = GlobalUnlock(global);
        result
    }
}
pub fn clipboard_write(value: &str) -> Result<(), String> {
    if value.contains('\0') || value.len() > 65_536 {
        return Err("Invalid clipboard text".into());
    }
    let text: Vec<u16> = value.encode_utf16().chain(Some(0)).collect();
    unsafe {
        OpenClipboard(None).map_err(|e| e.to_string())?;
        let _guard = Clipboard;
        let memory = GlobalAlloc(GMEM_MOVEABLE, text.len() * 2).map_err(|e| e.to_string())?;
        let pointer = GlobalLock(memory) as *mut u16;
        if pointer.is_null() {
            let _ = windows::Win32::Foundation::GlobalFree(Some(memory));
            return Err("Clipboard allocation failed".into());
        }
        std::ptr::copy_nonoverlapping(text.as_ptr(), pointer, text.len());
        let _ = GlobalUnlock(memory);
        if let Err(err) =
            EmptyClipboard().and_then(|_| SetClipboardData(13, Some(HANDLE(memory.0))).map(|_| ()))
        {
            let _ = windows::Win32::Foundation::GlobalFree(Some(memory));
            return Err(err.to_string());
        }
        Ok(()) // The clipboard now owns memory.
    }
}

use host_contracts::common::DisplayId;
use host_contracts::display::DisplayMode;
use host_contracts::display::*;
use host_contracts::error::{HostError, HostErrorCode, HostResult, RetryClass};
use windows::core::PCWSTR;
use windows::Win32::Devices::Display::*;
use windows::Win32::Foundation::LUID;
use windows::Win32::Graphics::Gdi::*;

fn error(message: impl Into<String>) -> HostError {
    HostError::new(HostErrorCode::Unavailable, RetryClass::Never, message)
}
fn string(value: &[u16]) -> String {
    String::from_utf16_lossy(&value[..value.iter().position(|v| *v == 0).unwrap_or(value.len())])
}
fn wide(value: &str) -> Vec<u16> {
    value.encode_utf16().chain(Some(0)).collect()
}

#[derive(Debug, Default)]
pub struct WindowsDisplayBackend;
#[derive(Debug, Clone)]
pub struct DisplayDetails {
    pub snapshot: DisplaySnapshot,
    pub modes: Vec<DisplayMode>,
    pub scales: Vec<u32>,
}
#[derive(Clone)]
struct Source {
    id: String,
    name: String,
    device: String,
    adapter: LUID,
    source: u32,
    primary: bool,
}
// Windows Settings' source DPI packets are not a documented Win32 contract.
// Keep them isolated, validate every range and fail closed on unsupported systems.
// Packet layout reference: lihas/windows-DPI-scaling-sample (original research).
const SCALES: &[u32] = &[100, 125, 150, 175, 200, 225, 250, 300, 350, 400, 450, 500];
#[repr(C)]
#[derive(Default)]
struct ScaleGet {
    header: DISPLAYCONFIG_DEVICE_INFO_HEADER,
    min: i32,
    current: i32,
    max: i32,
}
#[repr(C)]
struct ScaleSet {
    header: DISPLAYCONFIG_DEVICE_INFO_HEADER,
    relative: i32,
}
fn header(source: &Source, kind: i32, size: usize) -> DISPLAYCONFIG_DEVICE_INFO_HEADER {
    DISPLAYCONFIG_DEVICE_INFO_HEADER {
        r#type: DISPLAYCONFIG_DEVICE_INFO_TYPE(kind),
        size: size as u32,
        adapterId: source.adapter,
        id: source.source,
    }
}
fn scale(source: &Source) -> HostResult<(u32, Vec<u32>, i32)> {
    let mut packet = ScaleGet {
        header: header(source, -3, std::mem::size_of::<ScaleGet>()),
        ..Default::default()
    };
    let result = unsafe { DisplayConfigGetDeviceInfo(&mut packet.header) };
    if result != 0
        || packet.min > 0
        || packet.min < -11
        || packet.current < packet.min
        || packet.current > packet.max
    {
        return Err(error(format!("DPI scale query unsupported: {result}")));
    }
    let base = -packet.min;
    let last = base
        .checked_add(packet.max)
        .filter(|v| *v >= 0 && (*v as usize) < SCALES.len())
        .ok_or_else(|| error("Invalid DPI range"))?;
    let current = (base + packet.current) as usize;
    Ok((SCALES[current], SCALES[..=last as usize].to_vec(), base))
}
fn sources() -> HostResult<Vec<Source>> {
    // Display topology can change between buffer sizing and query. Retry boundedly.
    for _ in 0..3 {
        let (mut np, mut nm) = (0, 0);
        let code = unsafe { GetDisplayConfigBufferSizes(QDC_ONLY_ACTIVE_PATHS, &mut np, &mut nm) };
        if code.0 != 0 {
            return Err(error(format!("Display topology unavailable: {}", code.0)));
        }
        if np > 128 || nm > 1024 {
            return Err(error("Unexpected display topology size"));
        }
        let mut paths = vec![DISPLAYCONFIG_PATH_INFO::default(); np as usize];
        let mut modes = vec![DISPLAYCONFIG_MODE_INFO::default(); nm as usize];
        let code = unsafe {
            QueryDisplayConfig(
                QDC_ONLY_ACTIVE_PATHS,
                &mut np,
                paths.as_mut_ptr(),
                &mut nm,
                modes.as_mut_ptr(),
                None,
            )
        };
        if code.0 == 122 {
            continue;
        }
        if code.0 != 0 {
            return Err(error(format!("Display topology query failed: {}", code.0)));
        }
        let mut result = Vec::new();
        for path in &paths[..np as usize] {
            let mut target = DISPLAYCONFIG_TARGET_DEVICE_NAME::default();
            target.header = DISPLAYCONFIG_DEVICE_INFO_HEADER {
                r#type: DISPLAYCONFIG_DEVICE_INFO_GET_TARGET_NAME,
                size: std::mem::size_of_val(&target) as u32,
                adapterId: path.targetInfo.adapterId,
                id: path.targetInfo.id,
            };
            let mut source = DISPLAYCONFIG_SOURCE_DEVICE_NAME::default();
            source.header = DISPLAYCONFIG_DEVICE_INFO_HEADER {
                r#type: DISPLAYCONFIG_DEVICE_INFO_GET_SOURCE_NAME,
                size: std::mem::size_of_val(&source) as u32,
                adapterId: path.sourceInfo.adapterId,
                id: path.sourceInfo.id,
            };
            if unsafe { DisplayConfigGetDeviceInfo(&mut target.header) } != 0
                || unsafe { DisplayConfigGetDeviceInfo(&mut source.header) } != 0
            {
                continue;
            }
            let device = string(&source.viewGdiDeviceName);
            let mut info = DISPLAY_DEVICEW {
                cb: std::mem::size_of::<DISPLAY_DEVICEW>() as u32,
                ..Default::default()
            };
            // The adapter flags identify the current primary display.
            let mut primary = false;
            for index in 0..128 {
                if !unsafe { EnumDisplayDevicesW(PCWSTR::null(), index, &mut info, 0) }.as_bool() {
                    break;
                }
                if string(&info.DeviceName) == device {
                    primary = (info.StateFlags & DISPLAY_DEVICE_PRIMARY_DEVICE).0 != 0;
                    break;
                }
            }
            let id = string(&target.monitorDevicePath);
            if id.is_empty() {
                continue;
            }
            // Clone paths share a source; mutating one would also mutate the other.
            if paths[..np as usize]
                .iter()
                .filter(|p| {
                    p.sourceInfo.adapterId == path.sourceInfo.adapterId
                        && p.sourceInfo.id == path.sourceInfo.id
                })
                .count()
                != 1
            {
                primary = false
            }
            result.push(Source {
                id,
                name: string(&target.monitorFriendlyDeviceName),
                device,
                adapter: path.sourceInfo.adapterId,
                source: path.sourceInfo.id,
                primary,
            });
        }
        return Ok(result);
    }
    Err(error("Display topology kept changing"))
}
fn source(id: &str) -> HostResult<Source> {
    sources()?
        .into_iter()
        .find(|s| s.id == id)
        .ok_or_else(|| error("Display disconnected or identity changed"))
}
fn devmode(device: &str, index: ENUM_DISPLAY_SETTINGS_MODE) -> HostResult<DEVMODEW> {
    let device = wide(device);
    let mut mode = DEVMODEW {
        dmSize: std::mem::size_of::<DEVMODEW>() as u16,
        ..Default::default()
    };
    if !unsafe { EnumDisplaySettingsW(PCWSTR(device.as_ptr()), index, &mut mode) }.as_bool() {
        return Err(error("Display mode query failed"));
    }
    Ok(mode)
}
fn mode(native: &DEVMODEW, scale: u32) -> DisplayMode {
    DisplayMode {
        width: native.dmPelsWidth,
        height: native.dmPelsHeight,
        refresh_rate: native.dmDisplayFrequency,
        scale_percent: scale,
    }
}
fn current(source: &Source) -> HostResult<DisplayMode> {
    let native = devmode(&source.device, ENUM_CURRENT_SETTINGS)?;
    let percent = scale(source)?.0;
    Ok(mode(&native, percent))
}
impl WindowsDisplayBackend {
    pub fn list(&self) -> HostResult<Vec<DisplaySnapshot>> {
        sources()?
            .iter()
            .map(|s| {
                let dm = devmode(&s.device, ENUM_CURRENT_SETTINGS)?;
                let percent = scale(s).map(|v| v.0).unwrap_or(0);
                Ok(DisplaySnapshot {
                    display_id: DisplayId(s.id.clone()),
                    name: s.name.clone(),
                    width: dm.dmPelsWidth,
                    height: dm.dmPelsHeight,
                    refresh_rate: dm.dmDisplayFrequency,
                    scale_percent: percent,
                    primary: s.primary,
                    hdr: false,
                })
            })
            .collect()
    }
    pub fn details(&self, id: &str) -> HostResult<DisplayDetails> {
        let s = source(id)?;
        let now = current(&s)?;
        let scales = scale(&s)?.1;
        let mut modes = Vec::new();
        let orientation = unsafe {
            devmode(&s.device, ENUM_CURRENT_SETTINGS)?
                .Anonymous1
                .Anonymous2
                .dmDisplayOrientation
        };
        for i in 0..4096 {
            let Ok(dm) = devmode(&s.device, ENUM_DISPLAY_SETTINGS_MODE(i)) else {
                break;
            };
            if dm.dmBitsPerPel != 32
                || dm.dmDisplayFrequency < 24
                || unsafe { dm.Anonymous1.Anonymous2.dmDisplayOrientation } != orientation
            {
                continue;
            }
            let value = mode(&dm, now.scale_percent);
            if !modes.contains(&value) {
                modes.push(value)
            }
        }
        modes.sort_by_key(|m| (m.width, m.height, m.refresh_rate));
        Ok(DisplayDetails {
            snapshot: DisplaySnapshot {
                display_id: DisplayId(s.id),
                name: s.name,
                width: now.width,
                height: now.height,
                refresh_rate: now.refresh_rate,
                scale_percent: now.scale_percent,
                primary: s.primary,
                hdr: false,
            },
            modes,
            scales,
        })
    }
    pub fn read(&self, id: &str) -> HostResult<DisplayMode> {
        current(&source(id)?)
    }
    pub fn validate(&self, id: &str, value: &DisplayMode) -> HostResult<()> {
        let s = source(id)?;
        if !s.primary {
            return Err(error(
                "Only the captured primary, non-cloned display can be controlled",
            ));
        }
        if !scale(&s)?.1.contains(&value.scale_percent) {
            return Err(error("Unsupported scale percentage"));
        }
        let dm = self.selected(&s, value)?;
        let device = wide(&s.device);
        let code = unsafe {
            ChangeDisplaySettingsExW(PCWSTR(device.as_ptr()), Some(&dm), None, CDS_TEST, None)
        };
        if code != DISP_CHANGE_SUCCESSFUL {
            return Err(error(format!("Display mode test failed: {}", code.0)));
        }
        Ok(())
    }
    fn selected(&self, s: &Source, value: &DisplayMode) -> HostResult<DEVMODEW> {
        let current = devmode(&s.device, ENUM_CURRENT_SETTINGS)?;
        for i in 0..4096 {
            let Ok(dm) = devmode(&s.device, ENUM_DISPLAY_SETTINGS_MODE(i)) else {
                break;
            };
            if dm.dmBitsPerPel == current.dmBitsPerPel
                && unsafe {
                    dm.Anonymous1.Anonymous2.dmDisplayOrientation
                        == current.Anonymous1.Anonymous2.dmDisplayOrientation
                }
                && dm.dmPelsWidth == value.width
                && dm.dmPelsHeight == value.height
                && dm.dmDisplayFrequency == value.refresh_rate
            {
                // Preserve position/orientation/layout. Change only the requested fields.
                let mut result = current;
                result.dmFields = DM_PELSWIDTH | DM_PELSHEIGHT | DM_DISPLAYFREQUENCY;
                result.dmPelsWidth = value.width;
                result.dmPelsHeight = value.height;
                result.dmDisplayFrequency = value.refresh_rate;
                return Ok(result);
            }
        }
        Err(error(
            "Requested display mode is not enumerated by the driver",
        ))
    }
    pub fn apply(&self, id: &str, value: &DisplayMode) -> HostResult<()> {
        self.validate(id, value)?;
        let s = source(id)?;
        let before = current(&s)?;
        if before.width != value.width
            || before.height != value.height
            || before.refresh_rate != value.refresh_rate
        {
            let dm = self.selected(&s, value)?;
            let device = wide(&s.device);
            // No registry persistence for resolution/refresh; journal owns restoration.
            let code = unsafe {
                ChangeDisplaySettingsExW(
                    PCWSTR(device.as_ptr()),
                    Some(&dm),
                    None,
                    CDS_TYPE(0),
                    None,
                )
            };
            if code != DISP_CHANGE_SUCCESSFUL {
                return Err(error(format!("Display mode apply failed: {}", code.0)));
            }
        }
        let s = source(id)?;
        let (percent, supported, base) = scale(&s)?;
        if percent != value.scale_percent {
            if !supported.contains(&value.scale_percent) {
                return Err(error("Scale no longer supported after resolution change"));
            }
            let index = SCALES
                .iter()
                .position(|p| *p == value.scale_percent)
                .unwrap() as i32;
            let packet = ScaleSet {
                header: header(&s, -4, std::mem::size_of::<ScaleSet>()),
                relative: index - base,
            };
            let code = unsafe { DisplayConfigSetDeviceInfo(&packet.header) };
            if code != 0 {
                return Err(error(format!("DPI scale apply failed: {code}")));
            }
        }
        for _ in 0..20 {
            if self.read(id)? == *value {
                return Ok(());
            }
            std::thread::sleep(std::time::Duration::from_millis(50));
        }
        Err(error("Display readback does not match the requested mode"))
    }
}
impl DisplayQueryPort for WindowsDisplayBackend {
    async fn list_displays(&self) -> HostResult<Vec<DisplaySnapshot>> {
        self.list()
    }
    async fn get_display(&self, id: &DisplayId) -> HostResult<DisplaySnapshot> {
        Ok(self.details(&id.0)?.snapshot)
    }
    async fn list_display_modes(&self, id: &DisplayId) -> HostResult<Vec<DisplayMode>> {
        Ok(self.details(&id.0)?.modes)
    }
}
impl DisplayControlPort for WindowsDisplayBackend {
    async fn prepare_display(
        &self,
        request: DisplayPrepareRequest,
    ) -> HostResult<DisplayPrepareResult> {
        Ok(DisplayPrepareResult {
            history_display_mode: Some(self.read(&request.display_id.0)?),
            display_id: request.display_id,
        })
    }
    async fn apply_display_settings(
        &self,
        intent: DisplaySettingIntent,
    ) -> HostResult<DisplayApplyResult> {
        self.validate(&intent.display_id.0, &intent.requested_display_mode)?;
        if let Err(failure) = self.apply(&intent.display_id.0, &intent.requested_display_mode) {
            self.apply(&intent.display_id.0, &intent.history_display_mode)?;
            return Err(failure);
        }
        Ok(DisplayApplyResult {
            applied: true,
            current_mode: self.read(&intent.display_id.0)?,
            recoverable: true,
        })
    }
    async fn restore_display_settings(&self, token: DisplayRestoreToken) -> HostResult<()> {
        self.apply(&token.display_id.0, &token.history_display_mode)
    }
}

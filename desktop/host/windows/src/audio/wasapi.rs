use host_contracts::audio::*;
use host_contracts::common::{AudioCodec, AudioDeviceId};
use host_contracts::error::{HostError, HostErrorCode, HostResult, RetryClass};
use std::cell::Cell;
use std::sync::Mutex;
use windows::core::{PCWSTR, PWSTR};
use windows::Win32::Devices::FunctionDiscovery::PKEY_Device_FriendlyName;
use windows::Win32::Media::Audio::Endpoints::IAudioEndpointVolume;
use windows::Win32::Media::Audio::*;
use windows::Win32::System::Com::StructuredStorage::{PropVariantClear, PropVariantToStringAlloc};
use windows::Win32::System::Com::*;

struct ComApartment;
impl ComApartment {
    fn new() -> HostResult<Self> {
        unsafe { CoInitializeEx(None, COINIT_MULTITHREADED).ok() }.map_err(win_error)?;
        Ok(Self)
    }
}
impl Drop for ComApartment {
    fn drop(&mut self) {
        unsafe { CoUninitialize() }
    }
}

/// Confined to the audio worker; no COM pointers cross threads.
pub struct WindowsAudioBackend {
    client: IAudioClient,
    capture: IAudioCaptureClient,
    volume: IAudioEndpointVolume,
    history: AudioOutputSettingSnapshot,
    handle: Mutex<Option<String>>,
    changed_mute: Cell<bool>,
    _apartment: ComApartment,
}

impl WindowsAudioBackend {
    pub fn new() -> HostResult<Self> {
        let apartment = ComApartment::new()?;
        unsafe {
            let enumerator: IMMDeviceEnumerator =
                CoCreateInstance(&MMDeviceEnumerator, None, CLSCTX_ALL).map_err(win_error)?;
            let device = enumerator
                .GetDefaultAudioEndpoint(eRender, eConsole)
                .map_err(win_error)?;
            let id = allocated_string(device.GetId().map_err(win_error)?)?;
            let volume: IAudioEndpointVolume =
                device.Activate(CLSCTX_ALL, None).map_err(win_error)?;
            let history = AudioOutputSettingSnapshot {
                default_output_device_id: Some(AudioDeviceId(id)),
                muted: volume.GetMute().map_err(win_error)?.as_bool(),
                volume_percent: Some(
                    (volume.GetMasterVolumeLevelScalar().map_err(win_error)? * 100.0).round()
                        as u32,
                ),
            };
            let client: IAudioClient = device.Activate(CLSCTX_ALL, None).map_err(win_error)?;
            // Shared-engine conversion supports 44.1kHz/multichannel outputs.
            // The default loopback tap is before endpoint volume/mute.
            let format = WAVEFORMATEX {
                wFormatTag: 3,
                nChannels: 2,
                nSamplesPerSec: 48_000,
                nAvgBytesPerSec: 48_000 * 8,
                nBlockAlign: 8,
                wBitsPerSample: 32,
                cbSize: 0,
            };
            client
                .Initialize(
                    AUDCLNT_SHAREMODE_SHARED,
                    AUDCLNT_STREAMFLAGS_LOOPBACK
                        | AUDCLNT_STREAMFLAGS_AUTOCONVERTPCM
                        | AUDCLNT_STREAMFLAGS_SRC_DEFAULT_QUALITY,
                    1_000_000,
                    0,
                    &format,
                    None,
                )
                .map_err(win_error)?;
            let capture: IAudioCaptureClient = client.GetService().map_err(win_error)?;
            Ok(Self {
                client,
                capture,
                volume,
                history,
                handle: Mutex::new(None),
                changed_mute: Cell::new(false),
                _apartment: apartment,
            })
        }
    }
    pub fn history(&self) -> AudioOutputSettingSnapshot {
        self.history.clone()
    }
    pub fn muted(&self) -> HostResult<bool> {
        unsafe {
            self.volume
                .GetMute()
                .map(|v| v.as_bool())
                .map_err(win_error)
        }
    }
    pub fn read_samples(&self) -> HostResult<Vec<f32>> {
        let mut samples = Vec::new();
        unsafe {
            for _ in 0..32 {
                if self.capture.GetNextPacketSize().map_err(win_error)? == 0 {
                    break;
                }
                let mut data = std::ptr::null_mut();
                let mut frames = 0;
                let mut flags = 0;
                self.capture
                    .GetBuffer(&mut data, &mut frames, &mut flags, None, None)
                    .map_err(win_error)?;
                let count = frames as usize * 2;
                if flags & AUDCLNT_BUFFERFLAGS_SILENT.0 as u32 != 0 || data.is_null() {
                    samples.resize(samples.len() + count, 0.0);
                } else {
                    for index in 0..count {
                        samples.push((data as *const f32).add(index).read_unaligned());
                    }
                }
                self.capture.ReleaseBuffer(frames).map_err(win_error)?;
            }
        }
        Ok(samples)
    }
}

impl AudioDeviceQueryPort for WindowsAudioBackend {
    async fn list_audio_devices(&self) -> HostResult<Vec<AudioDeviceSnapshot>> {
        unsafe {
            let enumerator: IMMDeviceEnumerator =
                CoCreateInstance(&MMDeviceEnumerator, None, CLSCTX_ALL).map_err(win_error)?;
            let devices = enumerator
                .EnumAudioEndpoints(eRender, DEVICE_STATE_ACTIVE)
                .map_err(win_error)?;
            let mut result = Vec::new();
            for index in 0..devices.GetCount().map_err(win_error)? {
                let device = devices.Item(index).map_err(win_error)?;
                let id = allocated_string(device.GetId().map_err(win_error)?)?;
                let properties = device.OpenPropertyStore(STGM_READ).map_err(win_error)?;
                let mut value = properties
                    .GetValue(&PKEY_Device_FriendlyName)
                    .map_err(win_error)?;
                let name = PropVariantToStringAlloc(&value)
                    .map_err(win_error)
                    .and_then(|v| allocated_string(v));
                let _ = PropVariantClear(&mut value);
                result.push(AudioDeviceSnapshot {
                    is_default: self
                        .history
                        .default_output_device_id
                        .as_ref()
                        .is_some_and(|v| v.0 == id),
                    device_id: AudioDeviceId(id),
                    name: name?,
                    is_output: true,
                    is_input: false,
                });
            }
            Ok(result)
        }
    }
    async fn get_default_output_device(&self) -> HostResult<Option<AudioDeviceId>> {
        Ok(self.history.default_output_device_id.clone())
    }
    async fn get_audio_capabilities(
        &self,
        device_id: &AudioDeviceId,
    ) -> HostResult<AudioCapabilitySnapshot> {
        if self.history.default_output_device_id.as_ref() != Some(device_id) {
            return Err(invalid("Only the prepared output endpoint is supported"));
        }
        Ok(AudioCapabilitySnapshot {
            supports_loopback_capture: true,
            supported_sample_rates: vec![48_000],
            supported_channel_counts: vec![2],
        })
    }
    async fn capture_audio_output_setting(&self) -> HostResult<AudioOutputSettingSnapshot> {
        Ok(self.history())
    }
}

impl AudioCapturePort for WindowsAudioBackend {
    async fn prepare_audio_capture(
        &self,
        request: AudioPrepareRequest,
    ) -> HostResult<AudioPrepareResult> {
        if request.codec != AudioCodec::Opus
            || request.sample_rate != 48_000
            || request.channels != 2
            || request
                .device_id
                .as_ref()
                .is_some_and(|id| Some(id) != self.history.default_output_device_id.as_ref())
        {
            return Err(invalid(
                "Expected Opus at 48000Hz stereo on the prepared output device",
            ));
        }
        Ok(AudioPrepareResult {
            device_id: self.history.default_output_device_id.clone(),
            sample_rate: 48_000,
            channels: 2,
            bitrate_kbps: request.bitrate_kbps,
        })
    }
    async fn start_audio_capture(
        &self,
        request: AudioStartRequest,
    ) -> HostResult<AudioCaptureHandle> {
        let mut current = self.handle.lock().unwrap();
        if current.is_some() {
            return Err(invalid("Audio capture is already running"));
        }
        unsafe {
            self.client.Start().map_err(win_error)?;
        }
        *current = Some(request.session_id.clone());
        Ok(AudioCaptureHandle {
            handle_id: request.session_id,
            started_at_ms: 0,
        })
    }
    async fn stop_audio_capture(&self, handle: &AudioCaptureHandle) -> HostResult<()> {
        let mut current = self.handle.lock().unwrap();
        if current.as_deref() == Some(handle.handle_id.as_str()) {
            unsafe {
                self.client.Stop().map_err(win_error)?;
            }
            current.take();
        }
        Ok(())
    }
    async fn set_controlled_local_playback_muted(&self, muted: bool) -> HostResult<()> {
        self.changed_mute.set(true);
        unsafe {
            self.volume
                .SetMute(muted, std::ptr::null())
                .map_err(win_error)
        }
    }
    async fn restore_audio_output_setting(
        &self,
        snapshot: AudioOutputSettingSnapshot,
    ) -> HostResult<()> {
        if snapshot.default_output_device_id != self.history.default_output_device_id {
            return Err(invalid("Audio restore endpoint mismatch"));
        }
        unsafe {
            self.volume
                .SetMute(snapshot.muted, std::ptr::null())
                .map_err(win_error)?;
        }
        // AkaDesk never changes volume or the selected default output device.
        self.changed_mute.set(false);
        Ok(())
    }
}

impl Drop for WindowsAudioBackend {
    fn drop(&mut self) {
        unsafe {
            let _ = self.client.Stop();
            if self.changed_mute.get() {
                let _ = self.volume.SetMute(self.history.muted, std::ptr::null());
            }
        }
    }
}

pub fn restore_output_setting(snapshot: &AudioOutputSettingSnapshot) -> HostResult<()> {
    let _apartment = ComApartment::new()?;
    let id = snapshot
        .default_output_device_id
        .as_ref()
        .ok_or_else(|| invalid("Missing restore endpoint"))?;
    let wide: Vec<u16> = id.0.encode_utf16().chain(std::iter::once(0)).collect();
    unsafe {
        let enumerator: IMMDeviceEnumerator =
            CoCreateInstance(&MMDeviceEnumerator, None, CLSCTX_ALL).map_err(win_error)?;
        let device = enumerator
            .GetDevice(PCWSTR(wide.as_ptr()))
            .map_err(win_error)?;
        let volume: IAudioEndpointVolume = device.Activate(CLSCTX_ALL, None).map_err(win_error)?;
        volume
            .SetMute(snapshot.muted, std::ptr::null())
            .map_err(win_error)
    }
}

unsafe fn allocated_string(value: PWSTR) -> HostResult<String> {
    let result = value.to_string().map_err(|e| invalid(&e.to_string()));
    CoTaskMemFree(Some(value.0 as *const _));
    result
}
fn win_error(error: windows::core::Error) -> HostError {
    HostError::new(
        HostErrorCode::Unavailable,
        RetryClass::RetryAfterBackoff,
        error.to_string(),
    )
}
fn invalid(message: &str) -> HostError {
    HostError::new(HostErrorCode::InvalidRequest, RetryClass::Never, message)
}

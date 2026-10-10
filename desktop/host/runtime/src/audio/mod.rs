use crate::generated::v1::{AudioCodec, AudioPreference, AudioSessionSpec};
use crate::session::SharedSessionRegistry;
use host_contracts::audio::AudioOutputSettingSnapshot;
use host_contracts::common::AudioDeviceId;
use serde::{Deserialize, Serialize};
use std::collections::VecDeque;
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::mpsc::{self, Receiver, SyncSender};
use std::sync::{Arc, Mutex};
use std::time::Duration;

#[derive(Debug, Clone)]
pub struct EncodedAudioPacket {
    pub bytes: Vec<u8>,
    pub index: u64,
}
#[derive(Debug)]
struct MuteCommand {
    muted: bool,
    result: SyncSender<Result<bool, String>>,
}
#[derive(Debug)]
struct ActiveAudio {
    session_id: String,
    stop: Arc<AtomicBool>,
    commands: SyncSender<MuteCommand>,
    done: Receiver<Result<(), String>>,
}
#[derive(Debug, Default)]
pub struct AudioStreamState {
    journal_path: PathBuf,
    active: Option<ActiveAudio>,
    session_id: String,
    packets: VecDeque<EncodedAudioPacket>,
    pub packet_count: u64,
    pub latest_error: String,
    pub last_restore_result: String,
}
pub type SharedAudioStreamState = Arc<Mutex<AudioStreamState>>;

impl AudioStreamState {
    pub fn new(base_dir: &Path) -> Self {
        Self {
            journal_path: base_dir.join("audio_restore.json"),
            ..Default::default()
        }
    }
    fn push_packet(&mut self, session_id: &str, packet: EncodedAudioPacket) {
        if self.session_id != session_id
            || self
                .active
                .as_ref()
                .is_none_or(|a| a.stop.load(Ordering::Acquire))
        {
            return;
        }
        self.packet_count += 1;
        if self.packets.len() == 10 {
            self.packets.pop_front();
        }
        self.packets.push_back(packet);
    }
    pub fn take_next_packet(&mut self, session_id: &str) -> Option<EncodedAudioPacket> {
        if self.session_id != session_id {
            return None;
        }
        // Keep ordinary scheduling jitter continuous; trim only a real backlog.
        while self.packets.len() > 3 {
            self.packets.pop_front();
        }
        self.packets.pop_front()
    }
}

pub fn audio_spec(preference: &AudioPreference) -> Result<AudioSessionSpec, String> {
    if !matches!(preference.codec, x if x == AudioCodec::Opus as i32 || x == AudioCodec::Unspecified as i32)
        || ![0, 48_000].contains(&preference.sample_rate)
        || ![0, 2].contains(&preference.channels)
        || preference.bitrate_kbps != 0 && !(32..=128).contains(&preference.bitrate_kbps)
    {
        return Err("Audio requires Opus, 48000Hz stereo and 32-128kbps".into());
    }
    Ok(AudioSessionSpec {
        codec: AudioCodec::Opus as i32,
        sample_rate: 48_000,
        channels: 2,
        bitrate_kbps: if preference.bitrate_kbps == 0 {
            64
        } else {
            preference.bitrate_kbps
        },
    })
}

#[derive(Serialize, Deserialize)]
struct RestoreJournal {
    device_id: String,
    muted: bool,
    volume_percent: Option<u32>,
}
impl RestoreJournal {
    fn from_snapshot(snapshot: &AudioOutputSettingSnapshot) -> Result<Self, String> {
        Ok(Self {
            device_id: snapshot
                .default_output_device_id
                .as_ref()
                .ok_or("Missing audio endpoint")?
                .0
                .clone(),
            muted: snapshot.muted,
            volume_percent: snapshot.volume_percent,
        })
    }
    fn snapshot(&self) -> AudioOutputSettingSnapshot {
        AudioOutputSettingSnapshot {
            default_output_device_id: Some(AudioDeviceId(self.device_id.clone())),
            muted: self.muted,
            volume_percent: self.volume_percent,
        }
    }
}

#[cfg(windows)]
pub fn recover_audio_output(base_dir: &Path) -> Result<(), String> {
    let path = base_dir.join("audio_restore.json");
    if !path.exists() {
        return Ok(());
    }
    // COM initialization must be isolated from Tauri/STA callers.
    std::thread::spawn(move || recover_journal(&path))
        .join()
        .map_err(|_| "Audio recovery worker panicked".to_string())?
}
#[cfg(windows)]
fn recover_journal(path: &Path) -> Result<(), String> {
    if !path.exists() {
        return Ok(());
    }
    let journal: RestoreJournal =
        serde_json::from_slice(&std::fs::read(path).map_err(|e| e.to_string())?)
            .map_err(|e| e.to_string())?;
    host_windows::audio::restore_output_setting(&journal.snapshot()).map_err(|e| e.to_string())?;
    std::fs::remove_file(path).map_err(|e| e.to_string())
}

#[cfg(windows)]
pub fn spawn_audio_stream(
    sessions: SharedSessionRegistry,
    streams: SharedAudioStreamState,
    session_id: String,
    preference: AudioPreference,
) -> Result<Receiver<Result<bool, String>>, String> {
    let spec = audio_spec(&preference)?;
    let (commands, command_rx) = mpsc::sync_channel::<MuteCommand>(8);
    let (done_tx, done) = mpsc::sync_channel(1);
    let (ready_tx, ready) = mpsc::sync_channel(1);
    let stop = Arc::new(AtomicBool::new(false));
    let journal_path = {
        let mut state = streams.lock().map_err(|_| "Audio state lock poisoned")?;
        if state.active.is_some() {
            return Err("Previous audio session has not finished restoring".into());
        }
        state.session_id = session_id.clone();
        state.packets.clear();
        state.packet_count = 0;
        state.latest_error.clear();
        state.active = Some(ActiveAudio {
            session_id: session_id.clone(),
            stop: Arc::clone(&stop),
            commands,
            done,
        });
        state.journal_path.clone()
    };
    std::thread::spawn(move || {
        use host_contracts::audio::{
            AudioCapturePort, AudioDeviceQueryPort, AudioPrepareRequest, AudioStartRequest,
        };
        let result = (|| -> Result<(), String> {
            recover_journal(&journal_path)?;
            let runtime = tokio::runtime::Builder::new_current_thread()
                .build()
                .map_err(|e| e.to_string())?;
            let backend =
                host_windows::audio::WindowsAudioBackend::new().map_err(|e| e.to_string())?;
            let history = runtime
                .block_on(backend.capture_audio_output_setting())
                .map_err(|e| e.to_string())?;
            let journal = RestoreJournal::from_snapshot(&history)?;
            host_ipc::storage::atomic_write(
                &journal_path,
                &serde_json::to_vec(&journal).map_err(|e| e.to_string())?,
            )
            .map_err(|e| e.to_string())?;
            let mut encoder = host_windows::ffmpeg::StreamingOpusEncoder::new(spec.bitrate_kbps)?;
            runtime
                .block_on(backend.prepare_audio_capture(AudioPrepareRequest {
                    device_id: None,
                    codec: host_contracts::common::AudioCodec::Opus,
                    sample_rate: 48_000,
                    channels: 2,
                    bitrate_kbps: spec.bitrate_kbps,
                }))
                .map_err(|e| e.to_string())?;
            let handle = runtime
                .block_on(backend.start_audio_capture(AudioStartRequest {
                    session_id: session_id.clone(),
                }))
                .map_err(|e| e.to_string())?;
            let capture_result = (|| -> Result<(), String> {
                if preference.mute_controlled_local_playback {
                    runtime
                        .block_on(backend.set_controlled_local_playback_muted(true))
                        .map_err(|e| e.to_string())?;
                }
                let _ = ready_tx.send(Ok(backend.muted().map_err(|e| e.to_string())?));
                let mut pcm = VecDeque::new();
                let mut index = 0;
                let mut next_packet = std::time::Instant::now() + Duration::from_millis(20);
                while !stop.load(Ordering::Acquire) {
                    for _ in 0..8 {
                        let Ok(command) = command_rx.try_recv() else {
                            break;
                        };
                        let changed = runtime
                            .block_on(backend.set_controlled_local_playback_muted(command.muted))
                            .map_err(|e| e.to_string())
                            .and_then(|_| backend.muted().map_err(|e| e.to_string()));
                        let _ = command.result.send(changed);
                    }
                    pcm.extend(backend.read_samples().map_err(|e| e.to_string())?);
                    // Preserve bounded latency if the engine delivered a backlog.
                    while pcm.len() > 1920 * 5 {
                        pcm.pop_front();
                    }
                    let now = std::time::Instant::now();
                    if now >= next_packet {
                        let samples: Vec<f32> =
                            (0..1920).map(|_| pcm.pop_front().unwrap_or(0.0)).collect();
                        let bytes = encoder.encode(&samples)?;
                        if let Ok(mut state) = streams.lock() {
                            state.push_packet(&session_id, EncodedAudioPacket { bytes, index });
                        }
                        index += 1;
                        next_packet += Duration::from_millis(20);
                        if now.duration_since(next_packet.min(now)) > Duration::from_millis(100) {
                            next_packet = now + Duration::from_millis(20);
                        }
                    }
                    std::thread::sleep(Duration::from_millis(2));
                }
                Ok(())
            })();
            let stopped = runtime
                .block_on(backend.stop_audio_capture(&handle))
                .map_err(|e| e.to_string());
            let restored = runtime
                .block_on(backend.restore_audio_output_setting(history))
                .map_err(|e| e.to_string());
            if restored.is_ok() {
                std::fs::remove_file(&journal_path).map_err(|e| e.to_string())?;
            }
            capture_result.and(stopped).and(restored)
        })();
        if let Err(error) = &result {
            let _ = ready_tx.try_send(Err(error.clone()));
        }
        // Signal completion before locking sessions: cleanup holds the session lock.
        let _ = done_tx.send(result.clone());
        if let Err(error) = result {
            if let Ok(mut state) = streams.lock() {
                if state.session_id == session_id {
                    state.latest_error = error;
                }
            }
            if let Ok(mut state) = sessions.lock() {
                state.mark_failed(&session_id);
            }
        }
    });
    Ok(ready)
}

pub fn set_controlled_muted(
    streams: &SharedAudioStreamState,
    session_id: &str,
    muted: bool,
) -> Result<bool, String> {
    let (result, receiver) = mpsc::sync_channel(1);
    {
        let state = streams.lock().map_err(|_| "Audio state lock poisoned")?;
        let active = state
            .active
            .as_ref()
            .filter(|a| a.session_id == session_id && !a.stop.load(Ordering::Acquire))
            .ok_or("Audio session is unavailable")?;
        active
            .commands
            .try_send(MuteCommand { muted, result })
            .map_err(|e| e.to_string())?;
    }
    receiver
        .recv_timeout(Duration::from_millis(750))
        .map_err(|e| e.to_string())?
}

#[cfg(not(windows))]
pub fn spawn_audio_stream(
    _: SharedSessionRegistry,
    _: SharedAudioStreamState,
    _: String,
    _: AudioPreference,
) -> Result<Receiver<Result<bool, String>>, String> {
    Err("System audio capture is available only on Windows".into())
}

pub fn finish_audio_session(
    streams: &SharedAudioStreamState,
    session_id: &str,
) -> Result<(), String> {
    let active = {
        let mut state = streams.lock().map_err(|_| "Audio state lock poisoned")?;
        if state
            .active
            .as_ref()
            .is_none_or(|a| a.session_id != session_id)
        {
            return Ok(());
        }
        state.packets.clear();
        let active = state.active.take().unwrap();
        active.stop.store(true, Ordering::Release);
        active
    };
    let result = match active.done.recv_timeout(Duration::from_secs(2)) {
        Ok(result) => result,
        Err(mpsc::RecvTimeoutError::Timeout) => {
            streams
                .lock()
                .map_err(|_| "Audio state lock poisoned")?
                .active = Some(active);
            return Err("Audio stop/restore timed out".into());
        }
        Err(mpsc::RecvTimeoutError::Disconnected) => {
            Err("Audio worker exited without restore confirmation".into())
        }
    };
    let mut state = streams.lock().map_err(|_| "Audio state lock poisoned")?;
    state.last_restore_result = match &result {
        Ok(()) => "Audio capture stopped and output restored".into(),
        Err(e) => e.clone(),
    };
    if let Err(e) = &result {
        state.latest_error = e.clone();
    }
    result
}

#[cfg(test)]
mod tests {
    use super::*;
    fn state() -> AudioStreamState {
        let (commands, _) = mpsc::sync_channel(1);
        let (_, done) = mpsc::sync_channel(1);
        AudioStreamState {
            session_id: "current".into(),
            active: Some(ActiveAudio {
                session_id: "current".into(),
                stop: Arc::new(AtomicBool::new(false)),
                commands,
                done,
            }),
            ..Default::default()
        }
    }
    #[test]
    fn normal_jitter_preserves_order_and_backlog_stays_bounded() {
        let mut s = state();
        for index in 0..2 {
            s.push_packet(
                "current",
                EncodedAudioPacket {
                    bytes: vec![1],
                    index,
                },
            );
        }
        assert_eq!(s.take_next_packet("current").unwrap().index, 0);
        assert_eq!(s.take_next_packet("current").unwrap().index, 1);
        for index in 2..30 {
            s.push_packet(
                "current",
                EncodedAudioPacket {
                    bytes: vec![1],
                    index,
                },
            );
        }
        assert_eq!(s.packets.len(), 10);
        assert!(s.take_next_packet("previous").is_none());
        assert_eq!(s.packets.len(), 10);
        assert_eq!(s.take_next_packet("current").unwrap().index, 27);
        s.active
            .as_ref()
            .unwrap()
            .stop
            .store(true, Ordering::Release);
        s.push_packet(
            "current",
            EncodedAudioPacket {
                bytes: vec![1],
                index: 99,
            },
        );
        assert_eq!(s.packet_count, 30);
    }
    #[test]
    fn unsupported_audio_is_rejected_before_capture() {
        let default = audio_spec(&AudioPreference::default()).unwrap();
        assert_eq!(
            (default.sample_rate, default.channels, default.bitrate_kbps),
            (48_000, 2, 64)
        );
        for p in [
            AudioPreference {
                codec: 999,
                ..Default::default()
            },
            AudioPreference {
                sample_rate: 44_100,
                ..Default::default()
            },
            AudioPreference {
                channels: 1,
                ..Default::default()
            },
            AudioPreference {
                bitrate_kbps: 129,
                ..Default::default()
            },
        ] {
            assert!(audio_spec(&p).is_err());
        }
    }
    #[test]
    fn stale_mute_and_stop_cannot_change_current_capture() {
        let streams = Arc::new(Mutex::new(state()));
        assert!(set_controlled_muted(&streams, "previous", true).is_err());
        finish_audio_session(&streams, "previous").unwrap();
        assert!(!streams
            .lock()
            .unwrap()
            .active
            .as_ref()
            .unwrap()
            .stop
            .load(Ordering::Acquire));
    }
}

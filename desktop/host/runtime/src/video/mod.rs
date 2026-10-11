use crate::generated::v1::{
    CodecImplementation, SessionIntent, SessionSnapshot, VideoCodec, VideoPreference,
    VideoSessionSpec,
};
use crate::pairing::now_ms;
use crate::session::SharedSessionRegistry;
use std::collections::VecDeque;
use std::sync::mpsc::{self, Receiver, SyncSender, TrySendError};
use std::sync::{Arc, Mutex};
use std::thread;
use std::time::Duration;

/// Detect every pixel change; idle streams encode one keepalive frame per second.
#[derive(Default)]
struct FrameCadence {
    previous: Vec<u8>,
    dimensions: (u32, u32),
    last_encoded_ms: Option<u64>,
}
impl FrameCadence {
    fn should_encode(&self, bytes: &[u8], dimensions: (u32, u32), now: u64, force: bool) -> bool {
        force || self.last_encoded_ms.is_none() || self.dimensions != dimensions || self.previous != bytes
            || now.saturating_sub(self.last_encoded_ms.unwrap_or(0)) >= 1000
    }
    fn encoded(&mut self, bytes: &[u8], dimensions: (u32, u32), now: u64) {
        if self.previous != bytes {self.previous.clear();self.previous.extend_from_slice(bytes);}
        self.dimensions = dimensions;self.last_encoded_ms = Some(now);
    }
}

#[derive(Debug, Clone, Copy, Default)]
pub struct VideoPipelineState {
    pub codec: &'static str,
    pub width: u32,
    pub height: u32,
    pub fps: u32,
    pub bitrate_kbps: u32,
}

#[derive(Debug, Clone, Copy, Default)]
pub struct VideoRuntimeSupport {
    pub has_h264_decoder: bool,
    pub has_h264_encoder: bool,
    pub has_h264_software_encoder: bool,
    pub has_h264_hardware_encoder: bool,
}

#[derive(Debug, Clone, PartialEq)]
pub struct PreparedVideoSession {
    pub spec: VideoSessionSpec,
    pub encoder_name: String,
    pub smoke_packet_len: usize,
    pub smoke_keyframe: bool,
    pub frame_source_label: String,
    pub rtp_fmtp_line: String,
}

#[derive(Debug)]
pub struct StreamingStartupResult {
    pub snapshot: SessionSnapshot,
    pub first_packet_len: usize,
    pub first_frame_source_label: String,
    pub rtp_fmtp_line: String,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct EncodedVideoPacket {
    pub session_id: String,
    pub bytes: Vec<u8>,
    pub packet_len: usize,
    pub is_keyframe: bool,
    pub frame_source_label: String,
    pub created_at_ms: i64,
}

#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct VideoStreamSnapshot {
    pub session_id: String,
    pub packet_count: u64,
    pub keyframe_count: u64,
    pub latest_packet_len: usize,
    pub latest_frame_source_label: String,
    pub latest_error: String,
    pub updated_at_ms: i64,
}

#[derive(Debug, Default)]
pub struct VideoStreamState {
    diagnostics: Option<crate::diagnostics::SharedDiagnosticBuffer>,
    active_session_id: Option<String>,
    recent_packets: VecDeque<EncodedVideoPacket>,
    subscribers: Vec<PacketSubscriber>,
    keyframe_requested: bool,
    snapshot: VideoStreamSnapshot,
}

#[derive(Debug)]
struct PacketSubscriber {
    sender: SyncSender<EncodedVideoPacket>,
    waiting_for_keyframe: bool,
}

pub type SharedVideoStreamState = Arc<Mutex<VideoStreamState>>;

const DEFAULT_H264_WIDTH: u32 = 960;
const DEFAULT_H264_HEIGHT: u32 = 540;
const DEFAULT_H264_FPS: u32 = 30;
const DEFAULT_H264_BITRATE_KBPS: u32 = 2_500;
const RECENT_PACKET_LIMIT: usize = 90;
const MAX_CONSECUTIVE_RUNTIME_FRAME_ERRORS: usize = 30;

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn unchanged_frames_keep_alive_once_per_second_and_pixel_changes_resume_immediately() {
        let mut cadence = FrameCadence::default();
        let original = vec![0; 16];
        assert!(cadence.should_encode(&original, (2, 2), 0, false));
        cadence.encoded(&original, (2, 2), 0);
        for time in [16, 33, 500, 999] {
            assert!(!cadence.should_encode(&original, (2, 2), time, false));
        }
        assert!(cadence.should_encode(&original, (2, 2), 1000, false));
        cadence.encoded(&original, (2, 2), 1000);
        let mut changed = original.clone();
        changed[7] = 1;
        assert!(cadence.should_encode(&changed, (2, 2), 1001, false));
        // A failed encode must not acknowledge the changed frame.
        assert!(cadence.should_encode(&changed, (2, 2), 1002, false));
        cadence.encoded(&changed, (2, 2), 1002);
        assert!(!cadence.should_encode(&changed, (2, 2), 1003, false));
        assert!(cadence.should_encode(&changed, (2, 2), 1003, true));
        assert!(cadence.should_encode(&changed, (1, 4), 1003, false));
    }
    #[test]
    fn slow_receiver_has_bounded_queue_and_resumes_at_a_keyframe() {
        let mut state = VideoStreamState::default();
        state.begin_session("session".into());
        let receiver = state.subscribe_packets("session").unwrap();
        let packet = |keyframe| EncodedVideoPacket {
            session_id: "session".into(),
            bytes: vec![1],
            packet_len: 1,
            is_keyframe: keyframe,
            frame_source_label: "test".into(),
            created_at_ms: 1,
        };
        for _ in 0..(RECENT_PACKET_LIMIT + 5) {
            state.push_packet(packet(false));
        }
        assert!(state.take_keyframe_request("session"));
        assert_eq!(receiver.try_iter().count(), RECENT_PACKET_LIMIT);
        state.push_packet(packet(false));
        assert!(receiver.try_recv().is_err());
        state.push_packet(packet(true));
        assert!(receiver.try_recv().unwrap().is_keyframe);
        state.push_packet(packet(false));
        assert!(!receiver.try_recv().unwrap().is_keyframe);
    }
}

#[cfg(windows)]
pub fn detect_runtime_video_support() -> VideoRuntimeSupport {
    match host_windows::ffmpeg::probe_codec_inventory() {
        Ok(inventory) => VideoRuntimeSupport {
            has_h264_decoder: inventory.h264.decoder,
            has_h264_encoder: inventory.h264_encoder.any,
            has_h264_software_encoder: inventory.h264_encoder.software,
            has_h264_hardware_encoder: inventory.h264_encoder.hardware,
        },
        Err(err) => {
            eprintln!("failed to probe local ffmpeg sdk: {err}");
            VideoRuntimeSupport::default()
        }
    }
}

#[cfg(not(windows))]
pub fn detect_runtime_video_support() -> VideoRuntimeSupport {
    VideoRuntimeSupport::default()
}

#[cfg(windows)]
pub fn prepare_h264_session(intent: &SessionIntent) -> Result<PreparedVideoSession, String> {
    let video = intent.video.clone().unwrap_or_default();
    let requested_codec = VideoCodec::try_from(video.codec).unwrap_or(VideoCodec::Unspecified);
    if requested_codec != VideoCodec::Unspecified && requested_codec != VideoCodec::H264 {
        return Err(format!(
            "V1 only accepts H.264 video requests; got {:?}",
            requested_codec
        ));
    }

    let request = host_windows::ffmpeg::H264EncoderRequest {
        width: non_zero_or(video.width, DEFAULT_H264_WIDTH),
        height: non_zero_or(video.height, DEFAULT_H264_HEIGHT),
        fps: non_zero_or(video.fps, DEFAULT_H264_FPS),
        bitrate_kbps: non_zero_or(video.bitrate_kbps, DEFAULT_H264_BITRATE_KBPS),
        allow_software_fallback: allow_software_fallback(&video),
    };
    if request.width < 2
        || request.height < 2
        || request.width > 3840
        || request.height > 2160
        || request.width % 2 != 0
        || request.height % 2 != 0
        || request.fps > 120
        || request.bitrate_kbps > 50_000
    {
        return Err(
            "Requested video dimensions, frame rate or bitrate are unsupported.".to_string(),
        );
    }
    let frame = host_windows::cap::capture_primary_display_bgra()?;
    let encoded = host_windows::ffmpeg::encode_h264_bgra_frame(
        &request,
        Some(&frame.bytes),
        frame.stride,
        frame.width,
        frame.height,
    )?;
    let frame_source_label = "primary-display".to_string();
    let host_encoder = match encoded.prepared.implementation {
        host_windows::ffmpeg::PreparedEncoderImplementation::Hardware => {
            CodecImplementation::Hardware as i32
        }
        host_windows::ffmpeg::PreparedEncoderImplementation::Software => {
            CodecImplementation::Software as i32
        }
    };

    Ok(PreparedVideoSession {
        spec: VideoSessionSpec {
            codec: VideoCodec::H264 as i32,
            width: request.width,
            height: request.height,
            fps: request.fps,
            bitrate_kbps: request.bitrate_kbps,
            host_encoder,
            controller_decoder: CodecImplementation::Software as i32,
        },
        encoder_name: encoded.prepared.encoder_name,
        smoke_packet_len: encoded.packet_len,
        smoke_keyframe: encoded.is_keyframe,
        frame_source_label,
        rtp_fmtp_line: encoded.rtp_fmtp_line,
    })
}

#[cfg(not(windows))]
pub fn prepare_h264_session(_: &SessionIntent) -> Result<PreparedVideoSession, String> {
    Err("Video session preparation is only wired for the Windows host runtime.".to_string())
}

#[cfg(windows)]
pub fn spawn_h264_streaming_loop(
    sessions: SharedSessionRegistry,
    video_streams: SharedVideoStreamState,
    session_id: String,
    spec: VideoSessionSpec,
) -> Result<Receiver<Result<StreamingStartupResult, String>>, String> {
    let mut request = request_from_spec(&spec);
    let mut frame_interval = Duration::from_millis((1_000u64 / (request.fps.max(1) as u64)).max(1));
    let (startup_tx, startup_rx) = mpsc::sync_channel(1);
    if let Ok(mut stream_state) = video_streams.lock() {
        stream_state.begin_session(session_id.clone());
    }
    thread::spawn(move || {
        let mut encoder = match host_windows::ffmpeg::start_h264_streaming_encoder(&request) {
            Ok(encoder) => encoder,
            Err(err) => {
                if let Ok(mut registry) = sessions.lock() {
                    let _ = registry.mark_failed(&session_id);
                }
                if let Ok(mut stream_state) = video_streams.lock() {
                    stream_state.record_error(&session_id, err.clone());
                    stream_state.finish_session(&session_id);
                }
                let _ = startup_tx.send(Err(err));
                return;
            }
        };
        let mut startup_sent = false;
        let mut consecutive_runtime_frame_errors = 0usize;
        let origin = std::time::Instant::now();
        let origin_ms = now_ms();
        let mut cadence = FrameCadence::default();
        loop {
            let frame_started = std::time::Instant::now();
            let mut force_frame = false;
            let keep_running = sessions
                .lock()
                .map(|registry| registry.should_keep_running(&session_id))
                .unwrap_or(false);
            if !keep_running {
                if !startup_sent {
                    let _ = startup_tx.send(Err(
                        "video session stopped before the streaming loop produced its first frame."
                            .to_string(),
                    ));
                }
                break;
            }

            let pending = sessions.lock().ok().and_then(|registry| registry.active.as_ref()
                .filter(|s| s.session_id.0 == session_id).and_then(|s| s.requested_video.clone()));
            if let Some(mut next) = pending {
                let next_request = request_from_spec(&next);
                match host_windows::ffmpeg::start_h264_streaming_encoder(&next_request) {
                    Ok(mut replacement) => {
                        replacement.request_keyframe();
                        next.host_encoder = match replacement.prepared().implementation {
                            host_windows::ffmpeg::PreparedEncoderImplementation::Hardware => CodecImplementation::Hardware as i32,
                            host_windows::ffmpeg::PreparedEncoderImplementation::Software => CodecImplementation::Software as i32,
                        };
                        encoder = replacement;
                        force_frame = true;
                        request = next_request;
                        frame_interval = Duration::from_micros(1_000_000 / request.fps.max(1) as u64);
                        if let Ok(mut registry) = sessions.lock() {
                            if let Some(active) = registry.active.as_mut().filter(|s| s.session_id.0 == session_id) {
                                active.video = Some(next);active.requested_video = None;active.video_change_error.clear();
                            }
                        }
                    }
                    Err(error) => if let Ok(mut registry) = sessions.lock() {
                        if let Some(active) = registry.active.as_mut().filter(|s| s.session_id.0 == session_id) {
                            active.requested_video = None;active.video_change_error = error;
                        }
                    }
                }
            }

            if let Ok(mut stream_state) = video_streams.lock() {
                if stream_state.take_keyframe_request(&session_id) {
                    encoder.request_keyframe();
                    force_frame = true;
                }
            }

            let frame_captured_at_ms = origin_ms + origin.elapsed().as_millis() as i64;
            match encode_runtime_frame(&mut encoder, &mut cadence, origin.elapsed().as_millis() as u64, force_frame) {
                Ok(None) => {thread::sleep(frame_interval.saturating_sub(frame_started.elapsed()));continue;}
                Ok(Some((encoded, frame_source_label))) => {
                    consecutive_runtime_frame_errors = 0;
                    if let Ok(mut stream_state) = video_streams.lock() {
                        stream_state.push_packet(EncodedVideoPacket {
                            session_id: session_id.clone(),
                            bytes: encoded.bytes.clone(),
                            packet_len: encoded.packet_len,
                            is_keyframe: encoded.is_keyframe,
                            frame_source_label: frame_source_label.clone(),
                            created_at_ms: frame_captured_at_ms,
                        });
                    }
                    if !startup_sent {
                        let snapshot = sessions
                            .lock()
                            .ok()
                            .and_then(|registry| {
                                registry.active_snapshot_for_controller(
                                    &registry.active.as_ref()?.controller_device_id,
                                )
                            })
                            .filter(|snapshot| snapshot.session_id == session_id);
                        match snapshot {
                            Some(snapshot) => {
                                let rtp_fmtp_line = encoder.rtp_fmtp_line();
                                let _ = startup_tx.send(Ok(StreamingStartupResult {
                                    snapshot,
                                    first_packet_len: encoded.packet_len,
                                    first_frame_source_label: frame_source_label,
                                    rtp_fmtp_line,
                                }));
                                startup_sent = true;
                            }
                            None => {
                                let _ = startup_tx.send(Err(
                                    "video session disappeared before streaming state could be recorded."
                                        .to_string(),
                                ));
                                break;
                            }
                        }
                    } else if let Ok(mut registry) = sessions.lock() {
                        if !registry.touch_active(&session_id) {
                            break;
                        }
                    } else {
                        break;
                    }
                }
                Err(err) => {
                    consecutive_runtime_frame_errors += 1;
                    if let Ok(mut stream_state) = video_streams.lock() {
                        stream_state.record_error(
                            &session_id,
                            format!(
                                "runtime frame capture/encode failed ({}/{}): {}",
                                consecutive_runtime_frame_errors,
                                MAX_CONSECUTIVE_RUNTIME_FRAME_ERRORS,
                                err
                            ),
                        );
                    }
                    if !startup_sent {
                        let _ = startup_tx.send(Err(err));
                        break;
                    }
                    encoder.request_keyframe();
                    if consecutive_runtime_frame_errors >= MAX_CONSECUTIVE_RUNTIME_FRAME_ERRORS {
                        let _ = sessions
                            .lock()
                            .ok()
                            .and_then(|mut registry| registry.mark_failed(&session_id));
                        break;
                    }
                    thread::sleep(frame_interval);
                    continue;
                }
            }

            thread::sleep(frame_interval.saturating_sub(frame_started.elapsed()));
        }
        if let Ok(mut stream_state) = video_streams.lock() {
            stream_state.finish_session(&session_id);
        }
    });
    Ok(startup_rx)
}

#[cfg(not(windows))]
pub fn spawn_h264_streaming_loop(
    _: SharedSessionRegistry,
    _: SharedVideoStreamState,
    _: String,
    _: VideoSessionSpec,
) -> Result<Receiver<Result<StreamingStartupResult, String>>, String> {
    Err("Video streaming loop is only wired for the Windows host runtime.".to_string())
}

fn allow_software_fallback(video: &VideoPreference) -> bool {
    if video.codec == VideoCodec::Unspecified as i32 {
        return true;
    }
    video.allow_same_codec_software_fallback
}

fn non_zero_or(value: u32, fallback: u32) -> u32 {
    if value == 0 {
        fallback
    } else {
        value
    }
}

#[cfg(windows)]
fn request_from_spec(spec: &VideoSessionSpec) -> host_windows::ffmpeg::H264EncoderRequest {
    host_windows::ffmpeg::H264EncoderRequest {
        width: non_zero_or(spec.width, DEFAULT_H264_WIDTH),
        height: non_zero_or(spec.height, DEFAULT_H264_HEIGHT),
        fps: non_zero_or(spec.fps, DEFAULT_H264_FPS),
        bitrate_kbps: non_zero_or(spec.bitrate_kbps, DEFAULT_H264_BITRATE_KBPS),
        allow_software_fallback: true,
    }
}

#[cfg(windows)]
fn encode_runtime_frame(
    encoder: &mut host_windows::ffmpeg::StreamingH264EncoderSession,
    cadence: &mut FrameCadence,
    now: u64,
    force: bool,
) -> Result<Option<(host_windows::ffmpeg::EncodedH264Packet, String)>, String> {
    let frame = host_windows::cap::capture_primary_display_bgra()
        .map_err(|err| format!("failed to capture primary display: {err}"))?;
    if !cadence.should_encode(&frame.bytes, (frame.width, frame.height), now, force) {return Ok(None);}
    let encoded = encoder
        .encode_frame(Some(&frame.bytes), frame.stride, frame.width, frame.height)
        .map_err(|err| format!("failed to encode captured BGRA frame: {err}"))?;
    cadence.encoded(&frame.bytes, (frame.width, frame.height), now);
    Ok(Some((encoded, "primary-display".to_string())))
}

impl VideoStreamState {
    pub fn set_diagnostics(&mut self, diagnostics: crate::diagnostics::SharedDiagnosticBuffer) {
        self.diagnostics = Some(diagnostics);
    }
    pub fn begin_session(&mut self, session_id: String) {
        if self.active_session_id.as_deref() == Some(session_id.as_str()) {
            self.snapshot.updated_at_ms = now_ms();
            return;
        }
        self.active_session_id = Some(session_id.clone());
        self.recent_packets.clear();
        self.subscribers.clear();
        self.keyframe_requested = false;
        self.snapshot = VideoStreamSnapshot {
            session_id,
            updated_at_ms: now_ms(),
            ..VideoStreamSnapshot::default()
        };
    }

    pub fn push_packet(&mut self, packet: EncodedVideoPacket) {
        if self.active_session_id.as_deref() != Some(packet.session_id.as_str()) {
            return;
        }
        if self.recent_packets.len() >= RECENT_PACKET_LIMIT {
            self.recent_packets.pop_front();
        }
        self.snapshot.session_id = packet.session_id.clone();
        self.snapshot.packet_count += 1;
        if packet.is_keyframe {
            self.snapshot.keyframe_count += 1;
        }
        self.snapshot.latest_packet_len = packet.packet_len;
        self.snapshot.latest_frame_source_label = packet.frame_source_label.clone();
        if !self.snapshot.latest_error.is_empty() {
            if let Some(diagnostics) = &self.diagnostics {
                if let Ok(mut d) = diagnostics.lock() {
                    d.health(
                        Some(&packet.session_id),
                        "video",
                        true,
                        "MEDIA_STREAM_RECOVERED",
                    );
                }
            }
        }
        self.snapshot.latest_error.clear();
        self.snapshot.updated_at_ms = packet.created_at_ms;
        self.recent_packets.push_back(packet.clone());
        let mut needs_keyframe = false;
        self.subscribers.retain_mut(|subscriber| {
            if subscriber.waiting_for_keyframe && !packet.is_keyframe {
                return true;
            }
            match subscriber.sender.try_send(packet.clone()) {
                Ok(()) => {
                    subscriber.waiting_for_keyframe = false;
                    true
                }
                Err(TrySendError::Full(_)) => {
                    subscriber.waiting_for_keyframe = true;
                    needs_keyframe = true;
                    true
                }
                Err(TrySendError::Disconnected(_)) => false,
            }
        });
        self.keyframe_requested |= needs_keyframe;
    }

    pub fn request_keyframe(&mut self, session_id: &str) -> bool {
        if self.active_session_id.as_deref() != Some(session_id) {
            return false;
        }
        self.keyframe_requested = true;
        self.snapshot.updated_at_ms = now_ms();
        true
    }

    pub fn take_keyframe_request(&mut self, session_id: &str) -> bool {
        if self.active_session_id.as_deref() != Some(session_id) {
            return false;
        }
        let requested = self.keyframe_requested;
        self.keyframe_requested = false;
        requested
    }

    pub fn record_error(&mut self, session_id: &str, error: String) {
        if self.active_session_id.as_deref() != Some(session_id) {
            return;
        }
        if !error.is_empty() && self.snapshot.latest_error != error {
            if let Some(diagnostics) = &self.diagnostics {
                if let Ok(mut d) = diagnostics.lock() {
                    d.health(Some(session_id), "video", false, "MEDIA_STREAM_FAILED");
                }
            }
        } else if error.is_empty() && !self.snapshot.latest_error.is_empty() {
            if let Some(diagnostics) = &self.diagnostics {
                if let Ok(mut d) = diagnostics.lock() {
                    d.health(Some(session_id), "video", true, "MEDIA_STREAM_RECOVERED");
                }
            }
        }
        self.snapshot.latest_error = error;
        self.snapshot.updated_at_ms = now_ms();
    }

    pub fn finish_session(&mut self, session_id: &str) {
        if self.active_session_id.as_deref() != Some(session_id) {
            return;
        }
        self.active_session_id = None;
        self.keyframe_requested = false;
        self.subscribers.clear();
        self.snapshot.updated_at_ms = now_ms();
    }

    pub fn snapshot(&self) -> VideoStreamSnapshot {
        self.snapshot.clone()
    }

    pub fn recent_packets(&self) -> Vec<EncodedVideoPacket> {
        self.recent_packets.iter().cloned().collect()
    }

    pub fn latest_decodable_window_packets(&self) -> Vec<EncodedVideoPacket> {
        let packets = self.recent_packets.iter().cloned().collect::<Vec<_>>();
        let Some(latest_keyframe_index) = packets.iter().rposition(|packet| packet.is_keyframe)
        else {
            return packets;
        };
        packets.into_iter().skip(latest_keyframe_index).collect()
    }

    pub fn subscribe_packets(&mut self, session_id: &str) -> Option<Receiver<EncodedVideoPacket>> {
        if self.active_session_id.as_deref() != Some(session_id) {
            return None;
        }
        let (sender, receiver) = mpsc::sync_channel(RECENT_PACKET_LIMIT);
        for packet in self.latest_decodable_window_packets() {
            if sender.send(packet.clone()).is_err() {
                return None;
            }
        }
        self.subscribers.push(PacketSubscriber {
            sender,
            waiting_for_keyframe: false,
        });
        Some(receiver)
    }
}

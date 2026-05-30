use crate::generated::v1::{
    CodecImplementation, SessionIntent, SessionSnapshot, VideoCodec, VideoPreference,
    VideoSessionSpec,
};
use crate::session::SharedSessionRegistry;
use crate::pairing::now_ms;
use std::collections::VecDeque;
use std::sync::{Arc, Mutex};
use std::sync::mpsc::{self, Receiver, Sender};
use std::thread;
use std::time::Duration;

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
    active_session_id: Option<String>,
    recent_packets: VecDeque<EncodedVideoPacket>,
    subscribers: Vec<Sender<EncodedVideoPacket>>,
    keyframe_requested: bool,
    snapshot: VideoStreamSnapshot,
}

pub type SharedVideoStreamState = Arc<Mutex<VideoStreamState>>;

const DEFAULT_H264_WIDTH: u32 = 960;
const DEFAULT_H264_HEIGHT: u32 = 540;
const DEFAULT_H264_FPS: u32 = 30;
const DEFAULT_H264_BITRATE_KBPS: u32 = 2_500;
const RECENT_PACKET_LIMIT: usize = 90;
const MAX_CONSECUTIVE_RUNTIME_FRAME_ERRORS: usize = 30;

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
    let capture = host_windows::cap::capture_primary_display_bgra();
    let (encoded, frame_source_label) = match capture {
        Ok(frame) => match host_windows::ffmpeg::encode_h264_bgra_frame(
            &request,
            Some(frame.bytes.as_slice()),
            frame.stride,
        ) {
            Ok(encoded) => (encoded, "primary-display".to_string()),
            Err(_) => (
                host_windows::ffmpeg::encode_h264_test_pattern(&request)?,
                "synthetic-test-pattern".to_string(),
            ),
        },
        Err(_) => (
            host_windows::ffmpeg::encode_h264_test_pattern(&request)?,
            "synthetic-test-pattern".to_string(),
        ),
    };
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
    let request = request_from_spec(&spec);
    let frame_interval =
        Duration::from_millis((1_000u64 / (request.fps.max(1) as u64)).max(1));
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
        loop {
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

            if let Ok(mut stream_state) = video_streams.lock() {
                if stream_state.take_keyframe_request(&session_id) {
                    encoder.request_keyframe();
                }
            }

            match encode_runtime_frame(&mut encoder, !startup_sent) {
                Ok((encoded, frame_source_label)) => {
                    consecutive_runtime_frame_errors = 0;
                    if let Ok(mut stream_state) = video_streams.lock() {
                        stream_state.push_packet(EncodedVideoPacket {
                            session_id: session_id.clone(),
                            bytes: encoded.bytes.clone(),
                            packet_len: encoded.packet_len,
                            is_keyframe: encoded.is_keyframe,
                            frame_source_label: frame_source_label.clone(),
                            created_at_ms: now_ms(),
                        });
                    }
                    if !startup_sent {
                        let snapshot = sessions
                            .lock()
                            .ok()
                            .and_then(|mut registry| registry.mark_streaming(&session_id));
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

            thread::sleep(frame_interval);
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
    allow_synthetic_fallback: bool,
) -> Result<(host_windows::ffmpeg::EncodedH264Packet, String), String> {
    let capture = host_windows::cap::capture_primary_display_bgra();
    match capture {
        Ok(frame) => match encoder.encode_frame(Some(frame.bytes.as_slice()), frame.stride) {
            Ok(encoded) => Ok((encoded, "primary-display".to_string())),
            Err(err) if allow_synthetic_fallback => {
                let encoded = encoder.encode_frame(None, 0).map_err(|fallback_err| {
                    format!(
                        "failed to encode captured BGRA frame ({err}); fallback test-pattern encode failed: {fallback_err}"
                    )
                })?;
                Ok((encoded, "synthetic-test-pattern".to_string()))
            }
            Err(err) => Err(format!("failed to encode captured BGRA frame: {err}")),
        },
        Err(err) if allow_synthetic_fallback => {
            let encoded = encoder.encode_frame(None, 0).map_err(|fallback_err| {
                format!(
                    "failed to capture primary display ({err}); fallback test-pattern encode failed: {fallback_err}"
                )
            })?;
            Ok((encoded, "synthetic-test-pattern".to_string()))
        }
        Err(err) => Err(format!("failed to capture primary display: {err}")),
    }
}

impl VideoStreamState {
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
        self.snapshot.latest_error.clear();
        self.snapshot.updated_at_ms = packet.created_at_ms;
        self.recent_packets.push_back(packet.clone());
        self.subscribers
            .retain(|subscriber| subscriber.send(packet.clone()).is_ok());
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
        let Some(latest_keyframe_index) = packets.iter().rposition(|packet| packet.is_keyframe) else {
            return packets;
        };
        packets.into_iter().skip(latest_keyframe_index).collect()
    }

    pub fn subscribe_packets(
        &mut self,
        session_id: &str,
    ) -> Option<Receiver<EncodedVideoPacket>> {
        if self.active_session_id.as_deref() != Some(session_id) {
            return None;
        }
        let (sender, receiver) = mpsc::channel();
        for packet in self.latest_decodable_window_packets() {
            if sender.send(packet.clone()).is_err() {
                return None;
            }
        }
        self.subscribers.push(sender);
        Some(receiver)
    }
}

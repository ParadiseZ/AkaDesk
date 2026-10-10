use crate::generated::v1::{IceCandidate, WebRtcOffer};
use crate::pairing::now_ms;
use crate::session::SharedSessionRegistry;
use crate::signaling::SharedSignalingState;
use crate::video::SharedVideoStreamState;
use prost::bytes::Bytes;
use rtcp::payload_feedbacks::full_intra_request::FullIntraRequest;
use rtcp::payload_feedbacks::picture_loss_indication::PictureLossIndication;
use std::collections::VecDeque;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::mpsc::{self, Receiver};
use std::sync::{Arc, Mutex};
use std::thread;
use std::time::Duration;
use std::time::SystemTime;
use tokio::runtime::Builder as TokioRuntimeBuilder;
use tokio::sync::oneshot;
use webrtc::api::interceptor_registry::register_default_interceptors;
use webrtc::api::media_engine::{MediaEngine, MIME_TYPE_H264};
use webrtc::api::APIBuilder;
use webrtc::ice_transport::ice_candidate::RTCIceCandidateInit;
use webrtc::interceptor::registry::Registry;
use webrtc::media::Sample;
use webrtc::peer_connection::configuration::RTCConfiguration;
use webrtc::peer_connection::peer_connection_state::RTCPeerConnectionState;
use webrtc::peer_connection::sdp::session_description::RTCSessionDescription;
use webrtc::rtp_transceiver::rtp_codec::{
    RTCRtpCodecCapability, RTCRtpCodecParameters, RTPCodecType,
};
use webrtc::rtp_transceiver::rtp_transceiver_direction::RTCRtpTransceiverDirection;
use webrtc::rtp_transceiver::{RTCPFeedback, RTCRtpTransceiverInit};
use webrtc::track::track_local::track_local_static_sample::TrackLocalStaticSample;
use webrtc::track::track_local::TrackLocal;

#[derive(Debug)]
pub struct TransportStartupResult {
    pub local_offer_sdp_len: usize,
    pub local_candidate_count: usize,
}

#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct TransportStatusSnapshot {
    pub session_id: String,
    pub peer_connection_state: String,
    pub remote_answer_applied: bool,
    pub applied_remote_candidate_count: u32,
    pub sent_sample_count: u64,
    pub latest_sent_packet_len: usize,
    pub latest_error: String,
    pub updated_at_ms: i64,
}

const MAX_PENDING_PRE_ANSWER_PACKETS: usize = 90;
const HOST_H264_PAYLOAD_TYPE: u8 = 102;
const MAX_DEFERRED_REMOTE_CANDIDATE_RETRIES: u8 = 8;

#[derive(Debug, Default)]
pub struct TransportStatusState {
    active_session_id: Option<String>,
    snapshot: TransportStatusSnapshot,
}

pub type SharedTransportStatusState = Arc<Mutex<TransportStatusState>>;

#[cfg(windows)]
pub fn spawn_h264_webrtc_transport_loop(
    sessions: SharedSessionRegistry,
    video_streams: SharedVideoStreamState,
    signaling: SharedSignalingState,
    transport_status: SharedTransportStatusState,
    session_id: String,
    fps: u32,
    h264_fmtp_line: String,
) -> Result<Receiver<Result<TransportStartupResult, String>>, String> {
    if let Ok(mut status) = transport_status.lock() {
        status.begin_session(session_id.clone());
    }
    let packet_rx = {
        let mut stream_state = video_streams
            .lock()
            .map_err(|_| "video stream state lock poisoned".to_string())?;
        stream_state
            .subscribe_packets(&session_id)
            .ok_or_else(|| "video packet subscription unavailable for active session".to_string())?
    };
    let frame_duration = Duration::from_millis((1_000u64 / fps.max(1) as u64).max(1));
    let (startup_tx, startup_rx) = mpsc::sync_channel(1);
    thread::spawn(move || {
        let runtime = match TokioRuntimeBuilder::new_multi_thread()
            .enable_all()
            .thread_name("akadesk-webrtc-host")
            .build()
        {
            Ok(runtime) => runtime,
            Err(err) => {
                let _ = startup_tx.send(Err(format!("failed to build tokio runtime: {err}")));
                return;
            }
        };

        let startup_result = runtime.block_on(async {
            let mut media_engine = build_h264_media_engine(&h264_fmtp_line)
                .map_err(|err| format!("build_h264_media_engine failed: {err}"))?;
            let interceptor_registry =
                register_default_interceptors(Registry::new(), &mut media_engine)
                    .map_err(|err| format!("register_default_interceptors failed: {err}"))?;
            let api = APIBuilder::new()
                .with_media_engine(media_engine)
                .with_interceptor_registry(interceptor_registry)
                .build();
            let peer_connection = Arc::new(
                api.new_peer_connection(RTCConfiguration::default())
                    .await
                    .map_err(|err| format!("create_peer_connection failed: {err}"))?,
            );
            let media_send_ready = Arc::new(AtomicBool::new(false));

            let (gather_done_tx, gather_done_rx) = oneshot::channel::<()>();
            let gather_done = Arc::new(Mutex::new(Some(gather_done_tx)));

            let signaling_for_candidates = Arc::clone(&signaling);
            let session_id_for_candidates = session_id.clone();
            let local_media_mid = Arc::new(Mutex::new(None::<String>));
            let local_media_mid_for_candidates = Arc::clone(&local_media_mid);
            peer_connection.on_ice_candidate(Box::new(move |candidate| {
                let signaling = Arc::clone(&signaling_for_candidates);
                let session_id = session_id_for_candidates.clone();
                let gather_done = Arc::clone(&gather_done);
                let local_media_mid = Arc::clone(&local_media_mid_for_candidates);
                Box::pin(async move {
                    if let Some(candidate) = candidate {
                        match candidate.to_json() {
                            Ok(mut init) => {
                                // This host has one video m-line. webrtc-rs candidates omit
                                // its MID, so attach the actual MID from our local offer.
                                init.sdp_mid =
                                    local_media_mid.lock().ok().and_then(|mid| mid.clone());
                                init.sdp_mline_index = Some(0);
                                if let Ok(mut state) = signaling.lock() {
                                    let _ = state
                                        .push_local_candidate(proto_candidate(&session_id, &init));
                                }
                            }
                            Err(err) => {
                                eprintln!("failed to convert local ICE candidate to json: {err}");
                            }
                        }
                    } else if let Ok(mut done) = gather_done.lock() {
                        if let Some(tx) = done.take() {
                            let _ = tx.send(());
                        }
                    }
                })
            }));

            let sessions_for_state = Arc::clone(&sessions);
            let session_id_for_state = session_id.clone();
            let video_streams_for_state = Arc::clone(&video_streams);
            let transport_status_for_state = Arc::clone(&transport_status);
            let media_send_ready_for_state = Arc::clone(&media_send_ready);
            peer_connection.on_peer_connection_state_change(Box::new(move |state| {
                let sessions = Arc::clone(&sessions_for_state);
                let session_id = session_id_for_state.clone();
                let video_streams = Arc::clone(&video_streams_for_state);
                let transport_status = Arc::clone(&transport_status_for_state);
                let media_send_ready = Arc::clone(&media_send_ready_for_state);
                Box::pin(async move {
                    eprintln!("webrtc peer connection state session={session_id} state={state:?}");
                    media_send_ready.store(
                        matches!(state, RTCPeerConnectionState::Connected),
                        Ordering::Relaxed,
                    );
                    if let Ok(mut status) = transport_status.lock() {
                        status.record_peer_connection_state(&session_id, format!("{state:?}"));
                    }
                    if matches!(
                        state,
                        RTCPeerConnectionState::Failed | RTCPeerConnectionState::Closed
                    ) {
                        if let Ok(mut registry) = sessions.lock() {
                            let _ = registry.mark_failed(&session_id);
                        }
                        if let Ok(mut stream_state) = video_streams.lock() {
                            stream_state.record_error(
                                &session_id,
                                format!("webrtc peer connection moved to {state:?}"),
                            );
                        }
                        if let Ok(mut status) = transport_status.lock() {
                            status.record_error(
                                &session_id,
                                format!("webrtc peer connection moved to {state:?}"),
                            );
                        }
                    } else if matches!(state, RTCPeerConnectionState::Disconnected) {
                        if let Ok(mut status) = transport_status.lock() {
                            status.record_error(
                                &session_id,
                                "webrtc peer connection temporarily disconnected".to_string(),
                            );
                        }
                    }
                })
            }));

            let video_track = Arc::new(TrackLocalStaticSample::new(
                build_h264_codec_capability(&h264_fmtp_line),
                "video".to_owned(),
                "akadesk-host".to_owned(),
            ));
            let transceiver = peer_connection
                .add_transceiver_from_track(
                    Arc::clone(&video_track) as Arc<dyn TrackLocal + Send + Sync>,
                    Some(RTCRtpTransceiverInit {
                        direction: RTCRtpTransceiverDirection::Sendonly,
                        send_encodings: vec![],
                    }),
                )
                .await
                .map_err(|err| format!("add_transceiver_from_track failed: {err}"))?;
            transceiver
                .set_codec_preferences(vec![build_h264_codec_parameters(&h264_fmtp_line)])
                .await
                .map_err(|err| format!("set_codec_preferences failed: {err}"))?;
            let rtp_sender = transceiver.sender().await;
            let video_streams_for_rtcp = Arc::clone(&video_streams);
            let session_id_for_rtcp = session_id.clone();
            tokio::spawn(async move {
                while let Ok((packets, _)) = rtp_sender.read_rtcp().await {
                    if rtcp_requests_keyframe(&packets) {
                        if let Ok(mut stream_state) = video_streams_for_rtcp.lock() {
                            let _ = stream_state.request_keyframe(&session_id_for_rtcp);
                        }
                    }
                }
            });

            let offer = peer_connection
                .create_offer(None)
                .await
                .map_err(|err| format!("create_offer failed: {err}"))?;
            let mid = offer
                .sdp
                .lines()
                .find_map(|line| line.trim().strip_prefix("a=mid:").map(str::to_owned))
                .filter(|mid| !mid.is_empty())
                .ok_or_else(|| "local offer has no video media MID".to_string())?;
            *local_media_mid
                .lock()
                .map_err(|_| "media MID lock poisoned")? = Some(mid);
            peer_connection
                .set_local_description(offer)
                .await
                .map_err(|err| format!("set_local_description failed: {err}"))?;

            let _ = tokio::time::timeout(Duration::from_millis(800), gather_done_rx).await;
            let local_description = peer_connection
                .local_description()
                .await
                .ok_or_else(|| "local_description missing after offer creation".to_string())?;
            if let Ok(mut state) = signaling.lock() {
                let _ = state.set_local_offer(WebRtcOffer {
                    session_id: session_id.clone(),
                    sdp: local_description.sdp.clone(),
                });
            }

            Ok::<_, String>((
                peer_connection,
                video_track,
                media_send_ready,
                TransportStartupResult {
                    local_offer_sdp_len: local_description.sdp.len(),
                    local_candidate_count: signaling
                        .lock()
                        .map(|state| state.local_candidate_count(&session_id))
                        .unwrap_or(0),
                },
            ))
        });

        let (peer_connection, video_track, media_send_ready) = match startup_result {
            Ok(result) => {
                let (peer_connection, video_track, media_send_ready, startup) = result;
                let _ = startup_tx.send(Ok(TransportStartupResult {
                    local_offer_sdp_len: startup.local_offer_sdp_len,
                    local_candidate_count: startup.local_candidate_count,
                }));
                (peer_connection, video_track, media_send_ready)
            }
            Err(err) => {
                let _ = startup_tx.send(Err(err));
                return;
            }
        };

        let mut remote_answer_applied = false;
        let mut processed_remote_candidate_count = 0usize;
        let mut applied_remote_candidate_count = 0usize;
        let mut packet_timestamp = 0u32;
        let timestamp_step = ((90_000u64 / fps.max(1) as u64).max(1)) as u32;
        let sample_time_origin = SystemTime::now();
        let mut sample_time_offset = Duration::from_millis(0);
        let mut pending_pre_answer_packets = VecDeque::new();
        let mut deferred_remote_candidates: VecDeque<(IceCandidate, u8)> = VecDeque::new();
        let mut media_stream_started = false;
        let negotiation_started = std::time::Instant::now();
        let mut last_connected = negotiation_started;
        let mut ever_connected = false;
        let mut keyframe_request_inflight = false;
        let mut keyframe_request_updated_at_ms = 0i64;
        let mut post_start_keyframe_pending = false;
        let mut post_start_keyframe_request_updated_at_ms = 0i64;
        loop {
            let keep_running = sessions
                .lock()
                .map(|registry| registry.should_keep_running(&session_id))
                .unwrap_or(false);
            if !keep_running {
                break;
            }
            let media_send_ready_now = media_send_ready.load(Ordering::Relaxed);
            if media_send_ready_now {
                last_connected = std::time::Instant::now();
                ever_connected = true;
            } else if (!ever_connected && negotiation_started.elapsed() > Duration::from_secs(20))
                || (ever_connected && last_connected.elapsed() > Duration::from_secs(15))
            {
                if let Ok(mut status) = transport_status.lock() {
                    status.record_error(&session_id, "WebRTC connection timed out".to_string());
                }
                if let Ok(mut registry) = sessions.lock() {
                    registry.mark_failed(&session_id);
                }
                break;
            }
            if !media_send_ready_now {
                media_stream_started = false;
                keyframe_request_inflight = false;
                keyframe_request_updated_at_ms = 0;
                post_start_keyframe_pending = false;
                post_start_keyframe_request_updated_at_ms = 0;
            }

            if !remote_answer_applied {
                let remote_answer = signaling
                    .lock()
                    .ok()
                    .and_then(|state| state.remote_answer());
                if let Some(answer) = remote_answer {
                    let result = RTCSessionDescription::answer(answer.sdp)
                        .map_err(|err| err.to_string())
                        .and_then(|description| {
                            runtime
                                .block_on(peer_connection.set_remote_description(description))
                                .map_err(|err| err.to_string())
                        });
                    match result {
                        Ok(()) => {
                            if let Ok(mut status) = transport_status.lock() {
                                status.mark_remote_answer_applied(&session_id);
                            }
                            apply_remote_candidates(
                                &runtime,
                                &peer_connection,
                                &signaling,
                                &transport_status,
                                &session_id,
                                &mut processed_remote_candidate_count,
                                &mut applied_remote_candidate_count,
                                &mut deferred_remote_candidates,
                            );
                            remote_answer_applied = true;
                        }
                        Err(err) => {
                            if let Ok(mut stream_state) = video_streams.lock() {
                                stream_state.record_error(
                                    &session_id,
                                    format!("set_remote_description failed: {err}"),
                                );
                            }
                            if let Ok(mut status) = transport_status.lock() {
                                status.record_error(
                                    &session_id,
                                    format!("set_remote_description failed: {err}"),
                                );
                            }
                            if let Ok(mut registry) = sessions.lock() {
                                let _ = registry.mark_failed(&session_id);
                            }
                            break;
                        }
                    }
                }
            }

            if remote_answer_applied {
                apply_remote_candidates(
                    &runtime,
                    &peer_connection,
                    &signaling,
                    &transport_status,
                    &session_id,
                    &mut processed_remote_candidate_count,
                    &mut applied_remote_candidate_count,
                    &mut deferred_remote_candidates,
                );
                if media_send_ready_now
                    && !media_stream_started
                    && pending_packets_start_with_keyframe(&pending_pre_answer_packets)
                {
                    if let Err(err) = flush_pending_packets(
                        &runtime,
                        &video_track,
                        &mut pending_pre_answer_packets,
                        &mut packet_timestamp,
                        timestamp_step,
                        sample_time_origin,
                        &mut sample_time_offset,
                        frame_duration,
                        &transport_status,
                        &sessions,
                        &video_streams,
                        &session_id,
                    ) {
                        if let Ok(mut stream_state) = video_streams.lock() {
                            stream_state.record_error(&session_id, err.clone());
                        }
                        if let Ok(mut status) = transport_status.lock() {
                            status.record_error(&session_id, err.clone());
                        }
                        if let Ok(mut registry) = sessions.lock() {
                            let _ = registry.mark_failed(&session_id);
                        }
                        break;
                    }
                    media_stream_started = true;
                    keyframe_request_inflight = false;
                    keyframe_request_updated_at_ms = 0;
                    if request_post_start_keyframe(
                        &video_streams,
                        &session_id,
                        post_start_keyframe_pending,
                        post_start_keyframe_request_updated_at_ms,
                    ) {
                        post_start_keyframe_pending = true;
                        post_start_keyframe_request_updated_at_ms = crate::pairing::now_ms();
                    }
                } else if media_send_ready_now
                    && !media_stream_started
                    && should_request_startup_keyframe(
                        keyframe_request_inflight,
                        keyframe_request_updated_at_ms,
                    )
                {
                    if let Ok(mut stream_state) = video_streams.lock() {
                        if stream_state.request_keyframe(&session_id) {
                            keyframe_request_inflight = true;
                            keyframe_request_updated_at_ms = crate::pairing::now_ms();
                        }
                    }
                } else if media_send_ready_now
                    && media_stream_started
                    && request_post_start_keyframe(
                        &video_streams,
                        &session_id,
                        post_start_keyframe_pending,
                        post_start_keyframe_request_updated_at_ms,
                    )
                {
                    post_start_keyframe_pending = true;
                    post_start_keyframe_request_updated_at_ms = crate::pairing::now_ms();
                }
            }

            match packet_rx.recv_timeout(Duration::from_millis(100)) {
                Ok(packet)
                    if remote_answer_applied && media_send_ready_now && media_stream_started =>
                {
                    if packet.is_keyframe {
                        post_start_keyframe_pending = false;
                        post_start_keyframe_request_updated_at_ms = 0;
                    }
                    if let Err(err) = write_video_packet(
                        &runtime,
                        &video_track,
                        packet,
                        &mut packet_timestamp,
                        timestamp_step,
                        sample_time_origin,
                        &mut sample_time_offset,
                        frame_duration,
                        &transport_status,
                    ) {
                        if let Ok(mut stream_state) = video_streams.lock() {
                            stream_state
                                .record_error(&session_id, format!("write_sample failed: {err}"));
                        }
                        if let Ok(mut status) = transport_status.lock() {
                            status.record_error(&session_id, format!("write_sample failed: {err}"));
                        }
                        if let Ok(mut registry) = sessions.lock() {
                            let _ = registry.mark_failed(&session_id);
                        }
                        break;
                    }
                }
                Ok(packet) => {
                    retain_latest_decodable_window(&mut pending_pre_answer_packets, packet);
                    if remote_answer_applied
                        && media_send_ready.load(Ordering::Relaxed)
                        && !media_stream_started
                        && pending_packets_start_with_keyframe(&pending_pre_answer_packets)
                    {
                        if let Err(err) = flush_pending_packets(
                            &runtime,
                            &video_track,
                            &mut pending_pre_answer_packets,
                            &mut packet_timestamp,
                            timestamp_step,
                            sample_time_origin,
                            &mut sample_time_offset,
                            frame_duration,
                            &transport_status,
                            &sessions,
                            &video_streams,
                            &session_id,
                        ) {
                            if let Ok(mut stream_state) = video_streams.lock() {
                                stream_state.record_error(&session_id, err.clone());
                            }
                            if let Ok(mut status) = transport_status.lock() {
                                status.record_error(&session_id, err.clone());
                            }
                            if let Ok(mut registry) = sessions.lock() {
                                let _ = registry.mark_failed(&session_id);
                            }
                            break;
                        }
                        media_stream_started = true;
                        keyframe_request_inflight = false;
                        keyframe_request_updated_at_ms = 0;
                        if request_post_start_keyframe(
                            &video_streams,
                            &session_id,
                            post_start_keyframe_pending,
                            post_start_keyframe_request_updated_at_ms,
                        ) {
                            post_start_keyframe_pending = true;
                            post_start_keyframe_request_updated_at_ms = crate::pairing::now_ms();
                        }
                    }
                }
                Err(mpsc::RecvTimeoutError::Timeout) => {}
                Err(mpsc::RecvTimeoutError::Disconnected) => break,
            }
        }

        let _ = runtime.block_on(peer_connection.close());
        if let Ok(mut status) = transport_status.lock() {
            status.record_peer_connection_state(&session_id, "Closed".to_string());
        }
        if let Ok(mut state) = signaling.lock() {
            state.finish_session(&session_id);
        }
        if let Ok(mut status) = transport_status.lock() {
            status.finish_session(&session_id);
        }
    });
    Ok(startup_rx)
}

fn retain_latest_decodable_window(
    pending_packets: &mut VecDeque<crate::video::EncodedVideoPacket>,
    packet: crate::video::EncodedVideoPacket,
) {
    if packet.is_keyframe {
        pending_packets.clear();
    }
    pending_packets.push_back(packet);
    while pending_packets.len() > MAX_PENDING_PRE_ANSWER_PACKETS {
        pending_packets.pop_front();
    }
}

fn pending_packets_start_with_keyframe(
    pending_packets: &VecDeque<crate::video::EncodedVideoPacket>,
) -> bool {
    pending_packets
        .front()
        .map(|packet| packet.is_keyframe)
        .unwrap_or(false)
}

fn should_request_startup_keyframe(
    keyframe_request_inflight: bool,
    keyframe_request_updated_at_ms: i64,
) -> bool {
    !keyframe_request_inflight
        || crate::pairing::now_ms().saturating_sub(keyframe_request_updated_at_ms) >= 500
}

fn request_post_start_keyframe(
    video_streams: &SharedVideoStreamState,
    session_id: &str,
    post_start_keyframe_pending: bool,
    post_start_keyframe_request_updated_at_ms: i64,
) -> bool {
    if post_start_keyframe_pending
        && crate::pairing::now_ms().saturating_sub(post_start_keyframe_request_updated_at_ms) < 500
    {
        return false;
    }
    if let Ok(mut stream_state) = video_streams.lock() {
        return stream_state.request_keyframe(session_id);
    }
    false
}

fn rtcp_requests_keyframe(packets: &[Box<dyn rtcp::packet::Packet + Send + Sync>]) -> bool {
    packets.iter().any(|packet| {
        packet.as_any().is::<PictureLossIndication>() || packet.as_any().is::<FullIntraRequest>()
    })
}

fn flush_pending_packets(
    runtime: &tokio::runtime::Runtime,
    video_track: &Arc<TrackLocalStaticSample>,
    pending_packets: &mut VecDeque<crate::video::EncodedVideoPacket>,
    packet_timestamp: &mut u32,
    timestamp_step: u32,
    sample_time_origin: SystemTime,
    sample_time_offset: &mut Duration,
    frame_duration: Duration,
    transport_status: &SharedTransportStatusState,
    sessions: &SharedSessionRegistry,
    video_streams: &SharedVideoStreamState,
    session_id: &str,
) -> Result<(), String> {
    while let Some(packet) = pending_packets.pop_front() {
        write_video_packet(
            runtime,
            video_track,
            packet,
            packet_timestamp,
            timestamp_step,
            sample_time_origin,
            sample_time_offset,
            frame_duration,
            transport_status,
        )
        .map_err(|err| format!("flush buffered H.264 packet failed: {err}"))?;
    }
    if let Ok(mut registry) = sessions.lock() {
        let _ = registry.mark_streaming(session_id);
    }
    if let Ok(mut stream_state) = video_streams.lock() {
        stream_state.record_error(session_id, String::new());
    }
    Ok(())
}

fn apply_remote_candidates(
    runtime: &tokio::runtime::Runtime,
    peer_connection: &Arc<webrtc::peer_connection::RTCPeerConnection>,
    signaling: &SharedSignalingState,
    transport_status: &SharedTransportStatusState,
    session_id: &str,
    processed_remote_candidate_count: &mut usize,
    applied_remote_candidate_count: &mut usize,
    deferred_remote_candidates: &mut VecDeque<(IceCandidate, u8)>,
) {
    retry_deferred_remote_candidates(
        runtime,
        peer_connection,
        transport_status,
        session_id,
        applied_remote_candidate_count,
        deferred_remote_candidates,
    );

    let remote_candidates = signaling
        .lock()
        .map(|state| state.remote_candidates())
        .unwrap_or_default();
    for candidate in remote_candidates
        .iter()
        .skip(*processed_remote_candidate_count)
        .cloned()
    {
        *processed_remote_candidate_count += 1;
        let add_result = runtime.block_on(peer_connection.add_ice_candidate(RTCIceCandidateInit {
            candidate: candidate.candidate.clone(),
            sdp_mid: (!candidate.sdp_mid.is_empty()).then_some(candidate.sdp_mid.clone()),
            sdp_mline_index: Some(candidate.sdp_mline_index as u16),
            username_fragment: None,
        }));
        match add_result {
            Ok(()) => {
                *applied_remote_candidate_count += 1;
                if let Ok(mut status) = transport_status.lock() {
                    status.set_applied_remote_candidate_count(
                        session_id,
                        *applied_remote_candidate_count as u32,
                    );
                }
            }
            Err(err) => {
                if let Ok(mut status) = transport_status.lock() {
                    status.record_error(
                        session_id,
                        format!(
                            "deferring remote ICE candidate after add_ice_candidate failed: {err}"
                        ),
                    );
                }
                deferred_remote_candidates.push_back((candidate, 1));
            }
        }
    }
}

fn retry_deferred_remote_candidates(
    runtime: &tokio::runtime::Runtime,
    peer_connection: &Arc<webrtc::peer_connection::RTCPeerConnection>,
    transport_status: &SharedTransportStatusState,
    session_id: &str,
    applied_remote_candidate_count: &mut usize,
    deferred_remote_candidates: &mut VecDeque<(IceCandidate, u8)>,
) {
    let deferred_count = deferred_remote_candidates.len();
    for _ in 0..deferred_count {
        let Some((candidate, attempts)) = deferred_remote_candidates.pop_front() else {
            break;
        };
        let add_result = runtime.block_on(peer_connection.add_ice_candidate(RTCIceCandidateInit {
            candidate: candidate.candidate.clone(),
            sdp_mid: (!candidate.sdp_mid.is_empty()).then_some(candidate.sdp_mid.clone()),
            sdp_mline_index: Some(candidate.sdp_mline_index as u16),
            username_fragment: None,
        }));
        match add_result {
            Ok(()) => {
                *applied_remote_candidate_count += 1;
                if let Ok(mut status) = transport_status.lock() {
                    status.set_applied_remote_candidate_count(
                        session_id,
                        *applied_remote_candidate_count as u32,
                    );
                }
            }
            Err(err) => {
                if attempts < MAX_DEFERRED_REMOTE_CANDIDATE_RETRIES {
                    deferred_remote_candidates.push_back((candidate, attempts + 1));
                } else if let Ok(mut status) = transport_status.lock() {
                    status.record_error(
                        session_id,
                        format!(
                            "remote ICE candidate still failed after {} retries: {}",
                            attempts, err
                        ),
                    );
                }
            }
        }
    }
}

fn write_video_packet(
    runtime: &tokio::runtime::Runtime,
    video_track: &Arc<TrackLocalStaticSample>,
    packet: crate::video::EncodedVideoPacket,
    packet_timestamp: &mut u32,
    timestamp_step: u32,
    sample_time_origin: SystemTime,
    sample_time_offset: &mut Duration,
    frame_duration: Duration,
    transport_status: &SharedTransportStatusState,
) -> Result<(), String> {
    let packet_len = packet.packet_len;
    runtime
        .block_on(video_track.write_sample(&Sample {
            data: Bytes::from(packet.bytes),
            timestamp: sample_time_origin + *sample_time_offset,
            duration: frame_duration,
            packet_timestamp: *packet_timestamp,
            prev_dropped_packets: 0,
            prev_padding_packets: 0,
            ..Sample::default()
        }))
        .map_err(|err| format!("write_sample failed: {err}"))?;
    if let Ok(mut status) = transport_status.lock() {
        status.record_sample_written(&packet.session_id, packet_len);
    }
    *packet_timestamp = packet_timestamp.wrapping_add(timestamp_step);
    *sample_time_offset = sample_time_offset
        .checked_add(frame_duration)
        .unwrap_or(*sample_time_offset);
    Ok(())
}

#[cfg(not(windows))]
pub fn spawn_h264_webrtc_transport_loop(
    _: SharedSessionRegistry,
    _: SharedVideoStreamState,
    _: SharedSignalingState,
    _: SharedTransportStatusState,
    _: String,
    _: u32,
    _: String,
) -> Result<Receiver<Result<TransportStartupResult, String>>, String> {
    Err("WebRTC transport loop is only wired for the Windows host runtime.".to_string())
}

fn proto_candidate(session_id: &str, candidate: &RTCIceCandidateInit) -> IceCandidate {
    IceCandidate {
        session_id: session_id.to_string(),
        candidate: candidate.candidate.clone(),
        sdp_mid: candidate.sdp_mid.clone().unwrap_or_default(),
        sdp_mline_index: candidate.sdp_mline_index.unwrap_or_default() as u32,
    }
}

fn build_h264_media_engine(h264_fmtp_line: &str) -> webrtc::error::Result<MediaEngine> {
    let mut media_engine = MediaEngine::default();
    media_engine.register_codec(
        build_h264_codec_parameters(h264_fmtp_line),
        RTPCodecType::Video,
    )?;
    Ok(media_engine)
}

fn build_h264_codec_capability(h264_fmtp_line: &str) -> RTCRtpCodecCapability {
    RTCRtpCodecCapability {
        mime_type: MIME_TYPE_H264.to_owned(),
        clock_rate: 90_000,
        channels: 0,
        sdp_fmtp_line: h264_fmtp_line.to_owned(),
        rtcp_feedback: vec![
            RTCPFeedback {
                typ: "goog-remb".to_owned(),
                parameter: String::new(),
            },
            RTCPFeedback {
                typ: "ccm".to_owned(),
                parameter: "fir".to_owned(),
            },
            RTCPFeedback {
                typ: "nack".to_owned(),
                parameter: String::new(),
            },
            RTCPFeedback {
                typ: "nack".to_owned(),
                parameter: "pli".to_owned(),
            },
        ],
    }
}

fn build_h264_codec_parameters(h264_fmtp_line: &str) -> RTCRtpCodecParameters {
    RTCRtpCodecParameters {
        capability: build_h264_codec_capability(h264_fmtp_line),
        payload_type: HOST_H264_PAYLOAD_TYPE,
        ..RTCRtpCodecParameters::default()
    }
}

impl TransportStatusState {
    pub fn begin_session(&mut self, session_id: String) {
        self.active_session_id = Some(session_id.clone());
        self.snapshot = TransportStatusSnapshot {
            session_id,
            peer_connection_state: "Starting".to_string(),
            updated_at_ms: now_ms(),
            ..TransportStatusSnapshot::default()
        };
    }

    pub fn record_peer_connection_state(&mut self, session_id: &str, state: String) {
        if self.active_session_id.as_deref() != Some(session_id) {
            return;
        }
        if state.eq_ignore_ascii_case("Connected") {
            self.snapshot.latest_error.clear();
        }
        self.snapshot.peer_connection_state = state;
        self.snapshot.updated_at_ms = now_ms();
    }

    pub fn mark_remote_answer_applied(&mut self, session_id: &str) {
        if self.active_session_id.as_deref() != Some(session_id) {
            return;
        }
        self.snapshot.remote_answer_applied = true;
        self.snapshot.updated_at_ms = now_ms();
    }

    pub fn set_applied_remote_candidate_count(&mut self, session_id: &str, count: u32) {
        if self.active_session_id.as_deref() != Some(session_id) {
            return;
        }
        self.snapshot.applied_remote_candidate_count = count;
        self.snapshot.latest_error.clear();
        self.snapshot.updated_at_ms = now_ms();
    }

    pub fn record_sample_written(&mut self, session_id: &str, packet_len: usize) {
        if self.active_session_id.as_deref() != Some(session_id) {
            return;
        }
        self.snapshot.sent_sample_count += 1;
        self.snapshot.latest_sent_packet_len = packet_len;
        self.snapshot.latest_error.clear();
        self.snapshot.updated_at_ms = now_ms();
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
        self.snapshot.updated_at_ms = now_ms();
    }

    pub fn snapshot(&self) -> TransportStatusSnapshot {
        self.snapshot.clone()
    }
}

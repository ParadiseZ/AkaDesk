#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub struct CodecSupport {
    pub encoder: bool,
    pub decoder: bool,
}

#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub struct H264EncoderSupport {
    pub any: bool,
    pub software: bool,
    pub hardware: bool,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum PreparedEncoderImplementation {
    Hardware,
    Software,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct H264EncoderRequest {
    pub width: u32,
    pub height: u32,
    pub fps: u32,
    pub bitrate_kbps: u32,
    pub allow_software_fallback: bool,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct PreparedH264Encoder {
    pub encoder_name: String,
    pub implementation: PreparedEncoderImplementation,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct EncodedH264SmokeFrame {
    pub prepared: PreparedH264Encoder,
    pub packet_len: usize,
    pub is_keyframe: bool,
    pub rtp_fmtp_line: String,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct EncodedH264Packet {
    pub bytes: Vec<u8>,
    pub packet_len: usize,
    pub is_keyframe: bool,
}

#[cfg(windows)]
pub struct StreamingH264EncoderSession {
    prepared: PreparedH264Encoder,
    context: CodecContextGuard,
    src_frame: FrameGuard,
    dst_frame: FrameGuard,
    packet: PacketGuard,
    sws: SwsContextGuard,
    request: H264EncoderRequest,
    next_pts: i64,
    force_keyframe_next: bool,
    cached_parameter_sets_annexb: Vec<u8>,
    source_width: u32,
    source_height: u32,
}

#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub struct FfmpegCodecInventory {
    pub h264: CodecSupport,
    pub hevc: CodecSupport,
    pub vp8: CodecSupport,
    pub vp9: CodecSupport,
    pub av1: CodecSupport,
    pub h264_encoder: H264EncoderSupport,
}

#[cfg(windows)]
pub fn probe_codec_inventory() -> Result<FfmpegCodecInventory, String> {
    use ffmpeg_sys_next::{avcodec_find_encoder, AVCodecID};

    silence_ffmpeg_logs();

    Ok(FfmpegCodecInventory {
        h264: codec_support(AVCodecID::AV_CODEC_ID_H264),
        hevc: codec_support(AVCodecID::AV_CODEC_ID_HEVC),
        vp8: codec_support(AVCodecID::AV_CODEC_ID_VP8),
        vp9: codec_support(AVCodecID::AV_CODEC_ID_VP9),
        av1: codec_support(AVCodecID::AV_CODEC_ID_AV1),
        h264_encoder: H264EncoderSupport {
            any: unsafe { !avcodec_find_encoder(AVCodecID::AV_CODEC_ID_H264).is_null() },
            software: encoder_by_name_available(&["libx264", "libopenh264"]),
            hardware: encoder_by_name_available(&["h264_nvenc", "h264_qsv", "h264_amf"]),
        },
    })
}

#[cfg(windows)]
pub fn prepare_h264_encoder(request: &H264EncoderRequest) -> Result<PreparedH264Encoder, String> {
    silence_ffmpeg_logs();
    let session = start_h264_streaming_encoder(request)?;
    Ok(session.prepared().clone())
}

#[cfg(windows)]
pub fn encode_h264_test_pattern(
    request: &H264EncoderRequest,
) -> Result<EncodedH264SmokeFrame, String> {
    encode_h264_bgra_frame(request, None, 0, request.width, request.height)
}

#[cfg(windows)]
pub fn encode_h264_bgra_frame(
    request: &H264EncoderRequest,
    bgra_bytes: Option<&[u8]>,
    bgra_stride: usize,
    source_width: u32,
    source_height: u32,
) -> Result<EncodedH264SmokeFrame, String> {
    silence_ffmpeg_logs();
    let mut session = start_h264_streaming_encoder(request)?;
    let encoded = session.encode_frame(bgra_bytes, bgra_stride, source_width, source_height)?;
    Ok(EncodedH264SmokeFrame {
        prepared: session.prepared().clone(),
        packet_len: encoded.packet_len,
        is_keyframe: encoded.is_keyframe,
        rtp_fmtp_line: session.rtp_fmtp_line(),
    })
}

#[cfg(windows)]
pub fn start_h264_streaming_encoder(
    request: &H264EncoderRequest,
) -> Result<StreamingH264EncoderSession, String> {
    silence_ffmpeg_logs();
    let hardware_candidates = ["h264_nvenc", "h264_qsv", "h264_amf"];
    let software_candidates = ["libx264", "libopenh264"];

    for encoder_name in hardware_candidates {
        if let Ok(session) = try_start_h264_streaming_encoder(
            encoder_name,
            PreparedEncoderImplementation::Hardware,
            request,
        ) {
            return Ok(session);
        }
    }

    if request.allow_software_fallback {
        for encoder_name in software_candidates {
            if let Ok(session) = try_start_h264_streaming_encoder(
                encoder_name,
                PreparedEncoderImplementation::Software,
                request,
            ) {
                return Ok(session);
            }
        }
    }

    Err("No usable H.264 encoder session could be opened from the local FFmpeg SDK.".to_string())
}

#[cfg(windows)]
fn codec_support(codec_id: ffmpeg_sys_next::AVCodecID) -> CodecSupport {
    use ffmpeg_sys_next::{avcodec_find_decoder, avcodec_find_encoder};

    CodecSupport {
        encoder: unsafe { !avcodec_find_encoder(codec_id).is_null() },
        decoder: unsafe { !avcodec_find_decoder(codec_id).is_null() },
    }
}

#[cfg(windows)]
fn encoder_by_name_available(names: &[&str]) -> bool {
    names.iter().copied().any(encoder_name_available)
}

#[cfg(windows)]
fn encoder_name_available(name: &str) -> bool {
    use ffmpeg_sys_next::avcodec_find_encoder_by_name;
    use std::ffi::CString;

    let Ok(name) = CString::new(name) else {
        return false;
    };
    unsafe { !avcodec_find_encoder_by_name(name.as_ptr()).is_null() }
}

#[cfg(windows)]
fn try_start_h264_streaming_encoder(
    name: &str,
    implementation: PreparedEncoderImplementation,
    request: &H264EncoderRequest,
) -> Result<StreamingH264EncoderSession, String> {
    let context = CodecContextGuard::from_raw(open_h264_encoder_context(name, request)?);
    let mut src_frame = FrameGuard::allocate()?;
    let mut dst_frame = FrameGuard::allocate()?;
    initialize_bgra_source_frame(src_frame.as_mut_ptr(), request)?;
    initialize_yuv420_frame(dst_frame.as_mut_ptr(), request)?;
    let sws = SwsContextGuard::new(request.width, request.height)?;
    let packet = PacketGuard::allocate()?;

    Ok(StreamingH264EncoderSession {
        prepared: PreparedH264Encoder {
            encoder_name: name.to_string(),
            implementation,
        },
        context,
        src_frame,
        dst_frame,
        packet,
        sws,
        request: request.clone(),
        next_pts: 0,
        force_keyframe_next: true,
        cached_parameter_sets_annexb: Vec::new(),
        source_width: request.width,
        source_height: request.height,
    })
}

#[cfg(windows)]
fn open_h264_encoder_context(
    name: &str,
    request: &H264EncoderRequest,
) -> Result<*mut ffmpeg_sys_next::AVCodecContext, String> {
    use ffmpeg_sys_next::{
        av_dict_free, av_dict_set, avcodec_alloc_context3, avcodec_find_encoder_by_name,
        avcodec_open2, AVDictionary, AVPixelFormat, AV_CODEC_FLAG_LOW_DELAY,
        FF_PROFILE_H264_CONSTRAINED_BASELINE,
    };
    use std::ffi::CString;
    use std::ptr;

    let encoder_name = CString::new(name).map_err(|_| format!("invalid encoder name: {name}"))?;
    let codec = unsafe { avcodec_find_encoder_by_name(encoder_name.as_ptr()) };
    if codec.is_null() {
        return Err(format!("encoder not found: {name}"));
    }

    let mut context = unsafe { avcodec_alloc_context3(codec) };
    if context.is_null() {
        return Err(format!("failed to allocate codec context for {name}"));
    }
    let keyframe_interval = (request.fps as i32 / 2).max(1);

    unsafe {
        (*context).width = request.width as i32;
        (*context).height = request.height as i32;
        (*context).bit_rate = (request.bitrate_kbps as i64) * 1000;
        (*context).time_base.num = 1;
        (*context).time_base.den = request.fps as i32;
        (*context).framerate.num = request.fps as i32;
        (*context).framerate.den = 1;
        (*context).pix_fmt = AVPixelFormat::AV_PIX_FMT_YUV420P;
        (*context).gop_size = keyframe_interval;
        (*context).max_b_frames = 0;
        (*context).profile = FF_PROFILE_H264_CONSTRAINED_BASELINE;
        (*context).flags |= AV_CODEC_FLAG_LOW_DELAY as i32;
    }

    let mut options: *mut AVDictionary = ptr::null_mut();
    let preset_key = CString::new("preset").expect("valid preset key");
    let preset_value = CString::new("ultrafast").expect("valid preset value");
    let tune_key = CString::new("tune").expect("valid tune key");
    let tune_value = CString::new("zerolatency").expect("valid tune value");
    let profile_key = CString::new("profile").expect("valid profile key");
    let profile_value = CString::new("baseline").expect("valid profile value");
    let keyint_key = CString::new("keyint").expect("valid keyint key");
    let min_keyint_key = CString::new("min-keyint").expect("valid min-keyint key");
    let scenecut_key = CString::new("scenecut").expect("valid scenecut key");
    let keyint_value = CString::new(keyframe_interval.to_string())
        .map_err(|_| "invalid keyint value".to_string())?;
    let scenecut_value = CString::new("0").expect("valid scenecut value");

    if name == "libx264" {
        unsafe {
            av_dict_set(&mut options, preset_key.as_ptr(), preset_value.as_ptr(), 0);
            av_dict_set(&mut options, tune_key.as_ptr(), tune_value.as_ptr(), 0);
            av_dict_set(
                &mut options,
                profile_key.as_ptr(),
                profile_value.as_ptr(),
                0,
            );
            av_dict_set(&mut options, keyint_key.as_ptr(), keyint_value.as_ptr(), 0);
            av_dict_set(
                &mut options,
                min_keyint_key.as_ptr(),
                keyint_value.as_ptr(),
                0,
            );
            av_dict_set(
                &mut options,
                scenecut_key.as_ptr(),
                scenecut_value.as_ptr(),
                0,
            );
        }
    }

    let status = unsafe { avcodec_open2(context, codec, &mut options) };
    unsafe {
        av_dict_free(&mut options);
    }

    if status < 0 {
        unsafe {
            ffmpeg_sys_next::avcodec_free_context(&mut context);
        }
        return Err(format!(
            "failed to open encoder {name}: {}",
            av_error_string(status)
        ));
    }

    Ok(context)
}

#[cfg(windows)]
unsafe fn fill_bgra_test_pattern(frame: *mut ffmpeg_sys_next::AVFrame, width: u32, height: u32) {
    let stride = (*frame).linesize[0] as usize;
    let base = (*frame).data[0];
    for y in 0..height as usize {
        let row = base.add(y * stride);
        for x in 0..width as usize {
            let pixel = row.add(x * 4);
            *pixel.add(0) = (x % 255) as u8;
            *pixel.add(1) = (y % 255) as u8;
            *pixel.add(2) = ((x + y) % 255) as u8;
            *pixel.add(3) = 255;
        }
    }
}

#[cfg(windows)]
unsafe fn copy_bgra_frame(
    frame: *mut ffmpeg_sys_next::AVFrame,
    source: &[u8],
    source_stride: usize,
    width: u32,
    height: u32,
) -> Result<(), String> {
    let destination_stride = (*frame).linesize[0] as usize;
    let destination_base = (*frame).data[0];
    let row_len = (width as usize) * 4;
    if source_stride < row_len {
        return Err(format!(
            "source BGRA stride {source_stride} is smaller than the expected row length {row_len}"
        ));
    }
    if source.len() < source_stride * (height as usize) {
        return Err("source BGRA frame buffer is smaller than the declared dimensions".to_string());
    }
    for y in 0..height as usize {
        let source_row = &source[(y * source_stride)..(y * source_stride + row_len)];
        let destination_row = destination_base.add(y * destination_stride);
        std::ptr::copy_nonoverlapping(source_row.as_ptr(), destination_row, row_len);
    }
    Ok(())
}

#[cfg(windows)]
fn initialize_bgra_source_frame(
    frame: *mut ffmpeg_sys_next::AVFrame,
    request: &H264EncoderRequest,
) -> Result<(), String> {
    use ffmpeg_sys_next::{av_frame_get_buffer, AVPixelFormat};

    unsafe {
        (*frame).format = AVPixelFormat::AV_PIX_FMT_BGRA as i32;
        (*frame).width = request.width as i32;
        (*frame).height = request.height as i32;
        ffmpeg_status(
            av_frame_get_buffer(frame, 32),
            "failed to allocate BGRA source frame buffer",
        )
    }
}

#[cfg(windows)]
fn initialize_yuv420_frame(
    frame: *mut ffmpeg_sys_next::AVFrame,
    request: &H264EncoderRequest,
) -> Result<(), String> {
    use ffmpeg_sys_next::{av_frame_get_buffer, AVPixelFormat};

    unsafe {
        (*frame).format = AVPixelFormat::AV_PIX_FMT_YUV420P as i32;
        (*frame).width = request.width as i32;
        (*frame).height = request.height as i32;
        ffmpeg_status(
            av_frame_get_buffer(frame, 32),
            "failed to allocate YUV420 destination frame buffer",
        )
    }
}

#[cfg(windows)]
impl StreamingH264EncoderSession {
    pub fn prepared(&self) -> &PreparedH264Encoder {
        &self.prepared
    }

    pub fn request_keyframe(&mut self) {
        self.force_keyframe_next = true;
    }

    pub fn encode_frame(
        &mut self,
        bgra_bytes: Option<&[u8]>,
        bgra_stride: usize,
        source_width: u32,
        source_height: u32,
    ) -> Result<EncodedH264Packet, String> {
        use ffmpeg_sys_next::{
            av_frame_make_writable, av_packet_unref, avcodec_receive_packet, avcodec_send_frame,
            sws_scale,
        };

        if bgra_bytes.is_some()
            && (source_width != self.source_width || source_height != self.source_height)
        {
            if source_width == 0
                || source_height == 0
                || source_width > 16384
                || source_height > 16384
            {
                return Err("invalid captured frame dimensions".to_string());
            }
            // Rebuild the source buffer/scaler when the monitor resolution changes.
            let mut frame = FrameGuard::allocate()?;
            let mut source_request = self.request.clone();
            source_request.width = source_width;
            source_request.height = source_height;
            initialize_bgra_source_frame(frame.as_mut_ptr(), &source_request)?;
            let sws = SwsContextGuard::new_scaled(
                source_width,
                source_height,
                self.request.width,
                self.request.height,
            )?;
            self.src_frame = frame;
            self.sws = sws;
            self.source_width = source_width;
            self.source_height = source_height;
        }
        unsafe {
            ffmpeg_status(
                av_frame_make_writable(self.src_frame.as_mut_ptr()),
                "failed to make BGRA source frame writable",
            )?;
            ffmpeg_status(
                av_frame_make_writable(self.dst_frame.as_mut_ptr()),
                "failed to make YUV420 destination frame writable",
            )?;

            if let Some(source) = bgra_bytes {
                copy_bgra_frame(
                    self.src_frame.as_mut_ptr(),
                    source,
                    bgra_stride,
                    self.source_width,
                    self.source_height,
                )?;
            } else {
                fill_bgra_test_pattern(
                    self.src_frame.as_mut_ptr(),
                    self.source_width,
                    self.source_height,
                );
            }

            let scaled_height = sws_scale(
                self.sws.as_mut_ptr(),
                (*self.src_frame.as_mut_ptr()).data.as_ptr() as *const *const u8,
                (*self.src_frame.as_mut_ptr()).linesize.as_ptr(),
                0,
                self.source_height as i32,
                (*self.dst_frame.as_mut_ptr()).data.as_mut_ptr(),
                (*self.dst_frame.as_mut_ptr()).linesize.as_mut_ptr(),
            );
            if scaled_height <= 0 {
                return Err("failed to scale BGRA frame into YUV420P".to_string());
            }

            if self.force_keyframe_next {
                (*self.dst_frame.as_mut_ptr()).flags |= ffmpeg_sys_next::AV_FRAME_FLAG_KEY;
                (*self.dst_frame.as_mut_ptr()).pict_type =
                    ffmpeg_sys_next::AVPictureType::AV_PICTURE_TYPE_I;
            } else {
                (*self.dst_frame.as_mut_ptr()).flags &= !ffmpeg_sys_next::AV_FRAME_FLAG_KEY;
                (*self.dst_frame.as_mut_ptr()).pict_type =
                    ffmpeg_sys_next::AVPictureType::AV_PICTURE_TYPE_NONE;
            }
            (*self.dst_frame.as_mut_ptr()).pts = self.next_pts;
            self.next_pts += 1;

            ffmpeg_status(
                avcodec_send_frame(self.context.as_mut_ptr(), self.dst_frame.as_mut_ptr()),
                "failed to send frame to H.264 encoder",
            )?;
            self.force_keyframe_next = false;
            ffmpeg_status(
                avcodec_receive_packet(self.context.as_mut_ptr(), self.packet.as_mut_ptr()),
                "failed to receive encoded H.264 packet",
            )?;

            let packet_len = (*self.packet.as_mut_ptr()).size as usize;
            let packet_bytes =
                std::slice::from_raw_parts((*self.packet.as_mut_ptr()).data, packet_len).to_vec();
            let is_keyframe =
                ((*self.packet.as_mut_ptr()).flags & ffmpeg_sys_next::AV_PKT_FLAG_KEY) != 0;
            let nal_length_size = detect_h264_avcc_nal_length_size(self.context.as_mut_ptr());
            let parameter_sets =
                extract_h264_parameter_sets(self.context.as_mut_ptr(), &packet_bytes);
            if !parameter_sets.is_empty() {
                self.cached_parameter_sets_annexb = parameter_sets;
            }
            let normalized_bytes = normalize_h264_access_unit(
                &packet_bytes,
                &self.cached_parameter_sets_annexb,
                nal_length_size,
                is_keyframe,
            )?;
            let encoded = EncodedH264Packet {
                packet_len: normalized_bytes.len(),
                bytes: normalized_bytes,
                is_keyframe,
            };
            av_packet_unref(self.packet.as_mut_ptr());
            Ok(encoded)
        }
    }

    pub fn rtp_fmtp_line(&self) -> String {
        let parameter_sets = if self.cached_parameter_sets_annexb.is_empty() {
            extract_h264_parameter_sets(self.context.0, &[])
        } else {
            self.cached_parameter_sets_annexb.clone()
        };
        h264_rtp_fmtp_line(&parameter_sets)
    }
}

#[cfg(windows)]
fn normalize_h264_access_unit(
    packet_bytes: &[u8],
    parameter_sets_annexb: &[u8],
    nal_length_size: usize,
    is_keyframe: bool,
) -> Result<Vec<u8>, String> {
    let mut annexb = if contains_annexb_start_code(packet_bytes) {
        packet_bytes.to_vec()
    } else {
        avcc_to_annexb(packet_bytes, nal_length_size)?
    };
    if is_keyframe && !parameter_sets_annexb.is_empty() && !contains_parameter_sets(&annexb) {
        let mut prefixed = Vec::with_capacity(parameter_sets_annexb.len() + annexb.len());
        prefixed.extend_from_slice(parameter_sets_annexb);
        prefixed.extend_from_slice(&annexb);
        annexb = prefixed;
    }
    Ok(annexb)
}

#[cfg(windows)]
fn h264_rtp_fmtp_line(parameter_sets_annexb: &[u8]) -> String {
    let profile_level_id = derive_h264_profile_level_id(parameter_sets_annexb)
        .map(|derived| canonical_android_h264_profile_level_id(&derived))
        .unwrap_or_else(|| "42e01f".to_string());
    let mut params = vec![
        "level-asymmetry-allowed=1".to_string(),
        "packetization-mode=1".to_string(),
        format!("profile-level-id={profile_level_id}"),
    ];
    if let Some(sprop_parameter_sets) = derive_h264_sprop_parameter_sets(parameter_sets_annexb) {
        params.push(format!("sprop-parameter-sets={sprop_parameter_sets}"));
    }
    params.join(";")
}

#[cfg(windows)]
fn derive_h264_profile_level_id(parameter_sets_annexb: &[u8]) -> Option<String> {
    for nal in split_annexb_nalus(parameter_sets_annexb) {
        let Some(&header) = nal.first() else {
            continue;
        };
        if (header & 0x1F) == 7 && nal.len() >= 4 {
            return Some(format!("{:02x}{:02x}{:02x}", nal[1], nal[2], nal[3]));
        }
    }
    None
}

#[cfg(windows)]
fn canonical_android_h264_profile_level_id(profile_level_id: &str) -> String {
    if profile_level_id.len() != 6 {
        return profile_level_id.to_string();
    }
    let normalized = profile_level_id.to_ascii_lowercase();
    if normalized.starts_with("42") {
        // Align baseline offers to the google-webrtc default constrained-baseline profile.
        return "42e01f".to_string();
    }
    if normalized.starts_with("64") {
        // Align high-profile offers to the google-webrtc default constrained-high profile.
        return "640c1f".to_string();
    }
    normalized
}

#[cfg(windows)]
fn derive_h264_sprop_parameter_sets(parameter_sets_annexb: &[u8]) -> Option<String> {
    use base64::engine::general_purpose::STANDARD as BASE64_STANDARD;
    use base64::Engine;

    let mut encoded = Vec::new();
    for nal in split_annexb_nalus(parameter_sets_annexb) {
        let Some(&header) = nal.first() else {
            continue;
        };
        let nal_type = header & 0x1F;
        if nal_type == 7 || nal_type == 8 {
            encoded.push(BASE64_STANDARD.encode(nal));
        }
    }
    if encoded.is_empty() {
        None
    } else {
        Some(encoded.join(","))
    }
}

#[cfg(windows)]
fn detect_h264_avcc_nal_length_size(context: *mut ffmpeg_sys_next::AVCodecContext) -> usize {
    unsafe {
        if context.is_null() || (*context).extradata.is_null() || (*context).extradata_size < 5 {
            return 4;
        }
        let extradata = std::slice::from_raw_parts(
            (*context).extradata as *const u8,
            (*context).extradata_size as usize,
        );
        if extradata.first().copied() == Some(1) {
            return ((extradata[4] & 0x03) + 1) as usize;
        }
    }
    4
}

#[cfg(windows)]
fn extract_h264_parameter_sets(
    context: *mut ffmpeg_sys_next::AVCodecContext,
    packet_bytes: &[u8],
) -> Vec<u8> {
    let mut parameter_sets = extract_h264_parameter_sets_from_annexb(packet_bytes);
    if !parameter_sets.is_empty() {
        return parameter_sets;
    }

    unsafe {
        if context.is_null() || (*context).extradata.is_null() || (*context).extradata_size <= 0 {
            return Vec::new();
        }
        let extradata = std::slice::from_raw_parts(
            (*context).extradata as *const u8,
            (*context).extradata_size as usize,
        );
        if extradata.first().copied() == Some(1) {
            return extract_h264_parameter_sets_from_avcc_extradata(extradata);
        }
        parameter_sets = extract_h264_parameter_sets_from_annexb(extradata);
    }
    parameter_sets
}

#[cfg(windows)]
fn extract_h264_parameter_sets_from_avcc_extradata(extradata: &[u8]) -> Vec<u8> {
    if extradata.len() < 7 {
        return Vec::new();
    }
    let mut offset = 5usize;
    let sps_count = (extradata[offset] & 0x1F) as usize;
    offset += 1;
    let mut annexb = Vec::new();
    for _ in 0..sps_count {
        let Some(nal) = read_avcc_nal(extradata, &mut offset, 2) else {
            return Vec::new();
        };
        append_annexb_nal(&mut annexb, nal);
    }
    if offset >= extradata.len() {
        return Vec::new();
    }
    let pps_count = extradata[offset] as usize;
    offset += 1;
    for _ in 0..pps_count {
        let Some(nal) = read_avcc_nal(extradata, &mut offset, 2) else {
            return Vec::new();
        };
        append_annexb_nal(&mut annexb, nal);
    }
    annexb
}

#[cfg(windows)]
fn extract_h264_parameter_sets_from_annexb(bytes: &[u8]) -> Vec<u8> {
    let mut annexb = Vec::new();
    for nal in split_annexb_nalus(bytes) {
        let Some(&header) = nal.first() else {
            continue;
        };
        let nal_type = header & 0x1F;
        if nal_type == 7 || nal_type == 8 {
            append_annexb_nal(&mut annexb, nal);
        }
    }
    annexb
}

#[cfg(windows)]
fn contains_parameter_sets(bytes: &[u8]) -> bool {
    let mut has_sps = false;
    let mut has_pps = false;
    for nal in split_annexb_nalus(bytes) {
        let Some(&header) = nal.first() else {
            continue;
        };
        match header & 0x1F {
            7 => has_sps = true,
            8 => has_pps = true,
            _ => {}
        }
        if has_sps && has_pps {
            return true;
        }
    }
    false
}

#[cfg(windows)]
fn contains_annexb_start_code(bytes: &[u8]) -> bool {
    bytes.windows(4).any(|window| window == [0, 0, 0, 1])
        || bytes.windows(3).any(|window| window == [0, 0, 1])
}

#[cfg(windows)]
fn avcc_to_annexb(bytes: &[u8], length_size: usize) -> Result<Vec<u8>, String> {
    if !(1..=4).contains(&length_size) {
        return Err(format!("unsupported AVCC NAL length size: {length_size}"));
    }
    let mut offset = 0usize;
    let mut annexb = Vec::with_capacity(bytes.len() + 16);
    while offset + length_size <= bytes.len() {
        let Some(nal) = read_avcc_nal(bytes, &mut offset, length_size) else {
            return Err("invalid AVCC H.264 packet layout".to_string());
        };
        append_annexb_nal(&mut annexb, nal);
    }
    if annexb.is_empty() {
        return Err("empty AVCC H.264 access unit".to_string());
    }
    Ok(annexb)
}

#[cfg(windows)]
fn read_avcc_nal<'a>(bytes: &'a [u8], offset: &mut usize, length_size: usize) -> Option<&'a [u8]> {
    if *offset + length_size > bytes.len() {
        return None;
    }
    let nal_len = bytes[*offset..(*offset + length_size)]
        .iter()
        .fold(0usize, |acc, value| (acc << 8) | (*value as usize));
    *offset += length_size;
    if nal_len == 0 || *offset + nal_len > bytes.len() {
        return None;
    }
    let nal = &bytes[*offset..(*offset + nal_len)];
    *offset += nal_len;
    Some(nal)
}

#[cfg(windows)]
fn split_annexb_nalus(bytes: &[u8]) -> Vec<&[u8]> {
    let mut nalus = Vec::new();
    let mut i = 0usize;
    while let Some(start) = find_annexb_start_code(bytes, i) {
        let start_code_len = annexb_start_code_len(bytes, start).unwrap_or(4);
        let nal_start = start + start_code_len;
        let next_start = find_annexb_start_code(bytes, nal_start).unwrap_or(bytes.len());
        if nal_start < next_start {
            nalus.push(&bytes[nal_start..next_start]);
        }
        i = next_start;
    }
    nalus
}

#[cfg(windows)]
fn find_annexb_start_code(bytes: &[u8], from: usize) -> Option<usize> {
    let mut i = from;
    while i + 3 <= bytes.len() {
        if i + 4 <= bytes.len() && bytes[i..(i + 4)] == [0, 0, 0, 1] {
            return Some(i);
        }
        if bytes[i..(i + 3)] == [0, 0, 1] {
            return Some(i);
        }
        i += 1;
    }
    None
}

#[cfg(windows)]
fn annexb_start_code_len(bytes: &[u8], start: usize) -> Option<usize> {
    if start + 4 <= bytes.len() && bytes[start..(start + 4)] == [0, 0, 0, 1] {
        return Some(4);
    }
    if start + 3 <= bytes.len() && bytes[start..(start + 3)] == [0, 0, 1] {
        return Some(3);
    }
    None
}

#[cfg(windows)]
fn append_annexb_nal(output: &mut Vec<u8>, nal: &[u8]) {
    if nal.is_empty() {
        return;
    }
    output.extend_from_slice(&[0, 0, 0, 1]);
    output.extend_from_slice(nal);
}

#[cfg(windows)]
fn ffmpeg_status(status: i32, context: &str) -> Result<(), String> {
    if status < 0 {
        return Err(format!("{context}: {}", av_error_string(status)));
    }
    Ok(())
}

#[cfg(windows)]
fn av_error_string(status: i32) -> String {
    use ffmpeg_sys_next::av_strerror;
    use std::ffi::CStr;

    let mut buffer = [0i8; 256];
    let err = unsafe { av_strerror(status, buffer.as_mut_ptr(), buffer.len()) };
    if err < 0 {
        return format!("ffmpeg error {status}");
    }
    unsafe { CStr::from_ptr(buffer.as_ptr()) }
        .to_string_lossy()
        .into_owned()
}

#[cfg(windows)]
fn silence_ffmpeg_logs() {
    unsafe {
        ffmpeg_sys_next::av_log_set_level(ffmpeg_sys_next::AV_LOG_QUIET);
    }
}

#[cfg(not(windows))]
pub fn probe_codec_inventory() -> Result<FfmpegCodecInventory, String> {
    Err("FFmpeg probing is only wired for the Windows host crate.".to_string())
}

#[cfg(not(windows))]
pub fn prepare_h264_encoder(_: &H264EncoderRequest) -> Result<PreparedH264Encoder, String> {
    Err("FFmpeg encoder preparation is only wired for the Windows host crate.".to_string())
}

#[cfg(not(windows))]
pub fn encode_h264_test_pattern(_: &H264EncoderRequest) -> Result<EncodedH264SmokeFrame, String> {
    Err("FFmpeg frame encoding is only wired for the Windows host crate.".to_string())
}

#[cfg(windows)]
struct FrameGuard(*mut ffmpeg_sys_next::AVFrame);

#[cfg(windows)]
impl FrameGuard {
    fn allocate() -> Result<Self, String> {
        let frame = unsafe { ffmpeg_sys_next::av_frame_alloc() };
        if frame.is_null() {
            return Err("failed to allocate AVFrame".to_string());
        }
        Ok(Self(frame))
    }

    fn as_mut_ptr(&mut self) -> *mut ffmpeg_sys_next::AVFrame {
        self.0
    }
}

#[cfg(windows)]
impl Drop for FrameGuard {
    fn drop(&mut self) {
        unsafe {
            ffmpeg_sys_next::av_frame_free(&mut self.0);
        }
    }
}

#[cfg(windows)]
struct PacketGuard(*mut ffmpeg_sys_next::AVPacket);

#[cfg(windows)]
impl PacketGuard {
    fn allocate() -> Result<Self, String> {
        let packet = unsafe { ffmpeg_sys_next::av_packet_alloc() };
        if packet.is_null() {
            return Err("failed to allocate AVPacket".to_string());
        }
        Ok(Self(packet))
    }

    fn as_mut_ptr(&mut self) -> *mut ffmpeg_sys_next::AVPacket {
        self.0
    }
}

#[cfg(windows)]
impl Drop for PacketGuard {
    fn drop(&mut self) {
        unsafe {
            ffmpeg_sys_next::av_packet_free(&mut self.0);
        }
    }
}

#[cfg(windows)]
struct CodecContextGuard(*mut ffmpeg_sys_next::AVCodecContext);

#[cfg(windows)]
impl CodecContextGuard {
    fn from_raw(context: *mut ffmpeg_sys_next::AVCodecContext) -> Self {
        Self(context)
    }

    fn as_mut_ptr(&mut self) -> *mut ffmpeg_sys_next::AVCodecContext {
        self.0
    }
}

#[cfg(windows)]
impl Drop for CodecContextGuard {
    fn drop(&mut self) {
        unsafe {
            ffmpeg_sys_next::avcodec_free_context(&mut self.0);
        }
    }
}

#[cfg(windows)]
struct SwsContextGuard(*mut ffmpeg_sys_next::SwsContext);

#[cfg(windows)]
impl SwsContextGuard {
    fn new(width: u32, height: u32) -> Result<Self, String> {
        Self::new_scaled(width, height, width, height)
    }

    fn new_scaled(
        source_width: u32,
        source_height: u32,
        width: u32,
        height: u32,
    ) -> Result<Self, String> {
        use ffmpeg_sys_next::{sws_getContext, AVPixelFormat, SwsFlags};
        use std::ptr;

        let sws = unsafe {
            sws_getContext(
                source_width as i32,
                source_height as i32,
                AVPixelFormat::AV_PIX_FMT_BGRA,
                width as i32,
                height as i32,
                AVPixelFormat::AV_PIX_FMT_YUV420P,
                SwsFlags::SWS_BILINEAR as i32,
                ptr::null_mut(),
                ptr::null_mut(),
                ptr::null(),
            )
        };
        if sws.is_null() {
            return Err("failed to allocate swscale context for BGRA -> YUV420P".to_string());
        }
        Ok(Self(sws))
    }

    fn as_mut_ptr(&mut self) -> *mut ffmpeg_sys_next::SwsContext {
        self.0
    }
}

#[cfg(windows)]
impl Drop for SwsContextGuard {
    fn drop(&mut self) {
        unsafe {
            ffmpeg_sys_next::sws_freeContext(self.0);
        }
    }
}

#[cfg(test)]
mod tests {
    use super::{
        encode_h264_test_pattern, prepare_h264_encoder, probe_codec_inventory, H264EncoderRequest,
        PreparedEncoderImplementation,
    };

    #[cfg(windows)]
    #[test]
    fn x264_software_fallback_produces_annexb_with_parameter_sets() {
        let request = H264EncoderRequest {
            width: 64,
            height: 36,
            fps: 30,
            bitrate_kbps: 1_000,
            allow_software_fallback: true,
        };
        let mut encoder = super::try_start_h264_streaming_encoder(
            "libx264",
            PreparedEncoderImplementation::Software,
            &request,
        )
        .unwrap();
        let packet = encoder.encode_frame(None, 64 * 4, 64, 36).unwrap();
        assert!(packet.is_keyframe);
        assert!(super::contains_parameter_sets(&packet.bytes));
        assert!(encoder.rtp_fmtp_line().contains("profile-level-id=42e01f"));
    }

    #[cfg(windows)]
    #[test]
    fn downscaling_preserves_bottom_right_of_source_and_handles_resolution_change() {
        let request = H264EncoderRequest {
            width: 64,
            height: 36,
            fps: 30,
            bitrate_kbps: 1_000,
            allow_software_fallback: true,
        };
        let mut encoder = super::start_h264_streaming_encoder(&request).unwrap();
        for (width, height) in [(128usize, 72usize), (192, 108)] {
            let mut pixels = vec![0u8; width * height * 4];
            for y in 0..height {
                for x in 0..width {
                    let offset = (y * width + x) * 4;
                    if x >= width / 2 && y >= height / 2 {
                        pixels[offset..offset + 3].fill(255);
                    }
                    pixels[offset + 3] = 255;
                }
            }
            encoder
                .encode_frame(Some(&pixels), width * 4, width as u32, height as u32)
                .unwrap();
            unsafe {
                let frame = encoder.dst_frame.as_mut_ptr();
                let stride = (*frame).linesize[0] as usize;
                assert!(*(*frame).data[0].add(stride * 2 + 2) < 30);
                assert!(
                    *(*frame).data[0].add(stride * 33 + 61) > 200,
                    "bottom-right source quadrant was cropped instead of scaled"
                );
            }
        }
        assert!(encoder
            .encode_frame(Some(&[0; 4]), 192 * 4, 192, 108)
            .is_err());
    }

    #[test]
    fn detects_h264_codec_support_from_local_sdk() {
        let inventory = probe_codec_inventory().expect("failed to probe local ffmpeg sdk");
        assert!(
            inventory.h264.decoder,
            "expected an H.264 decoder in the local FFmpeg SDK"
        );
        assert!(
            inventory.h264_encoder.any,
            "expected an H.264 encoder in the local FFmpeg SDK"
        );
        assert!(
            inventory.h264_encoder.software || inventory.h264_encoder.hardware,
            "expected either software or hardware H.264 encoder support"
        );
    }

    #[cfg(windows)]
    #[test]
    fn prepares_a_real_h264_encoder_from_local_sdk() {
        let prepared = prepare_h264_encoder(&H264EncoderRequest {
            width: 1280,
            height: 720,
            fps: 30,
            bitrate_kbps: 4_000,
            allow_software_fallback: true,
        })
        .expect("expected a usable H.264 encoder from local FFmpeg SDK");

        assert!(!prepared.encoder_name.is_empty());
        assert!(matches!(
            prepared.implementation,
            PreparedEncoderImplementation::Hardware | PreparedEncoderImplementation::Software
        ));
    }

    #[cfg(windows)]
    #[test]
    fn encodes_a_bgra_test_pattern_into_h264_packet() {
        let encoded = encode_h264_test_pattern(&H264EncoderRequest {
            width: 640,
            height: 360,
            fps: 30,
            bitrate_kbps: 2_000,
            allow_software_fallback: true,
        })
        .expect("expected a usable H.264 packet from BGRA test pattern");

        assert!(!encoded.prepared.encoder_name.is_empty());
        assert!(encoded.packet_len > 0, "expected encoded packet payload");
    }
}

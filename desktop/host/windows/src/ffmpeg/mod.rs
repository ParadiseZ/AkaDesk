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
    let hardware_candidates = ["h264_nvenc", "h264_qsv", "h264_amf"];
    let software_candidates = ["libx264", "libopenh264"];

    for encoder_name in hardware_candidates {
        if try_open_h264_encoder(encoder_name, request).is_ok() {
            return Ok(PreparedH264Encoder {
                encoder_name: encoder_name.to_string(),
                implementation: PreparedEncoderImplementation::Hardware,
            });
        }
    }

    if request.allow_software_fallback {
        for encoder_name in software_candidates {
            if try_open_h264_encoder(encoder_name, request).is_ok() {
                return Ok(PreparedH264Encoder {
                    encoder_name: encoder_name.to_string(),
                    implementation: PreparedEncoderImplementation::Software,
                });
            }
        }
    }

    Err("No usable H.264 encoder could be opened from the local FFmpeg SDK.".to_string())
}

#[cfg(windows)]
pub fn encode_h264_test_pattern(
    request: &H264EncoderRequest,
) -> Result<EncodedH264SmokeFrame, String> {
    silence_ffmpeg_logs();
    let hardware_candidates = ["h264_nvenc", "h264_qsv", "h264_amf"];
    let software_candidates = ["libx264", "libopenh264"];

    for encoder_name in hardware_candidates {
        if let Ok((packet_len, is_keyframe)) = try_encode_test_pattern_h264(encoder_name, request) {
            return Ok(EncodedH264SmokeFrame {
                prepared: PreparedH264Encoder {
                    encoder_name: encoder_name.to_string(),
                    implementation: PreparedEncoderImplementation::Hardware,
                },
                packet_len,
                is_keyframe,
            });
        }
    }

    if request.allow_software_fallback {
        for encoder_name in software_candidates {
            if let Ok((packet_len, is_keyframe)) =
                try_encode_test_pattern_h264(encoder_name, request)
            {
                return Ok(EncodedH264SmokeFrame {
                    prepared: PreparedH264Encoder {
                        encoder_name: encoder_name.to_string(),
                        implementation: PreparedEncoderImplementation::Software,
                    },
                    packet_len,
                    is_keyframe,
                });
            }
        }
    }

    Err(
        "No usable H.264 encoder could encode a test pattern from the local FFmpeg SDK."
            .to_string(),
    )
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
fn try_open_h264_encoder(name: &str, request: &H264EncoderRequest) -> Result<(), String> {
    let mut context = open_h264_encoder_context(name, request)?;
    unsafe {
        ffmpeg_sys_next::avcodec_free_context(&mut context);
    }
    Ok(())
}

#[cfg(windows)]
fn try_encode_test_pattern_h264(
    name: &str,
    request: &H264EncoderRequest,
) -> Result<(usize, bool), String> {
    use ffmpeg_sys_next::{
        av_frame_get_buffer, av_packet_unref, avcodec_receive_packet, avcodec_send_frame,
        sws_freeContext, sws_getContext, sws_scale, AVPixelFormat, SwsFlags,
    };
    use std::ptr;

    let mut context = open_h264_encoder_context(name, request)?;
    let mut src_frame = FrameGuard::allocate()?;
    let mut dst_frame = FrameGuard::allocate()?;
    let mut packet = PacketGuard::allocate()?;

    unsafe {
        (*src_frame.as_mut_ptr()).format = AVPixelFormat::AV_PIX_FMT_BGRA as i32;
        (*src_frame.as_mut_ptr()).width = request.width as i32;
        (*src_frame.as_mut_ptr()).height = request.height as i32;
        ffmpeg_status(
            av_frame_get_buffer(src_frame.as_mut_ptr(), 32),
            "failed to allocate BGRA source frame buffer",
        )?;
        fill_bgra_test_pattern(src_frame.as_mut_ptr(), request.width, request.height);

        (*dst_frame.as_mut_ptr()).format = AVPixelFormat::AV_PIX_FMT_YUV420P as i32;
        (*dst_frame.as_mut_ptr()).width = request.width as i32;
        (*dst_frame.as_mut_ptr()).height = request.height as i32;
        (*dst_frame.as_mut_ptr()).pts = 0;
        ffmpeg_status(
            av_frame_get_buffer(dst_frame.as_mut_ptr(), 32),
            "failed to allocate YUV420 destination frame buffer",
        )?;

        let sws = sws_getContext(
            request.width as i32,
            request.height as i32,
            AVPixelFormat::AV_PIX_FMT_BGRA,
            request.width as i32,
            request.height as i32,
            AVPixelFormat::AV_PIX_FMT_YUV420P,
            SwsFlags::SWS_BILINEAR as i32,
            ptr::null_mut(),
            ptr::null_mut(),
            ptr::null(),
        );
        if sws.is_null() {
            return Err("failed to allocate swscale context for BGRA -> YUV420P".to_string());
        }

        let scaled_height = sws_scale(
            sws,
            (*src_frame.as_mut_ptr()).data.as_ptr() as *const *const u8,
            (*src_frame.as_mut_ptr()).linesize.as_ptr(),
            0,
            request.height as i32,
            (*dst_frame.as_mut_ptr()).data.as_mut_ptr(),
            (*dst_frame.as_mut_ptr()).linesize.as_mut_ptr(),
        );
        sws_freeContext(sws);
        if scaled_height <= 0 {
            return Err("failed to scale BGRA frame into YUV420P".to_string());
        }

        ffmpeg_status(
            avcodec_send_frame(context, dst_frame.as_mut_ptr()),
            "failed to send frame to H.264 encoder",
        )?;
        let mut status = avcodec_receive_packet(context, packet.as_mut_ptr());
        if status < 0 {
            ffmpeg_status(
                avcodec_send_frame(context, ptr::null_mut()),
                "failed to flush H.264 encoder",
            )?;
            status = avcodec_receive_packet(context, packet.as_mut_ptr());
        }
        ffmpeg_status(status, "failed to receive encoded H.264 packet")?;

        let packet_len = (*packet.as_mut_ptr()).size as usize;
        let is_keyframe = ((*packet.as_mut_ptr()).flags & ffmpeg_sys_next::AV_PKT_FLAG_KEY) != 0;
        av_packet_unref(packet.as_mut_ptr());
        ffmpeg_sys_next::avcodec_free_context(&mut context);
        Ok((packet_len, is_keyframe))
    }
}

#[cfg(windows)]
fn open_h264_encoder_context(
    name: &str,
    request: &H264EncoderRequest,
) -> Result<*mut ffmpeg_sys_next::AVCodecContext, String> {
    use ffmpeg_sys_next::{
        av_dict_free, av_dict_set, avcodec_alloc_context3, avcodec_find_encoder_by_name,
        avcodec_open2, AVDictionary, AVPixelFormat,
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

    unsafe {
        (*context).width = request.width as i32;
        (*context).height = request.height as i32;
        (*context).bit_rate = (request.bitrate_kbps as i64) * 1000;
        (*context).time_base.num = 1;
        (*context).time_base.den = request.fps as i32;
        (*context).framerate.num = request.fps as i32;
        (*context).framerate.den = 1;
        (*context).pix_fmt = AVPixelFormat::AV_PIX_FMT_YUV420P;
        (*context).gop_size = request.fps as i32;
        (*context).max_b_frames = 0;
    }

    let mut options: *mut AVDictionary = ptr::null_mut();
    let preset_key = CString::new("preset").expect("valid preset key");
    let preset_value = CString::new("ultrafast").expect("valid preset value");
    let tune_key = CString::new("tune").expect("valid tune key");
    let tune_value = CString::new("zerolatency").expect("valid tune value");

    if name == "libx264" {
        unsafe {
            av_dict_set(&mut options, preset_key.as_ptr(), preset_value.as_ptr(), 0);
            av_dict_set(&mut options, tune_key.as_ptr(), tune_value.as_ptr(), 0);
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

#[cfg(test)]
mod tests {
    use super::{
        encode_h264_test_pattern, prepare_h264_encoder, probe_codec_inventory, H264EncoderRequest,
        PreparedEncoderImplementation,
    };

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

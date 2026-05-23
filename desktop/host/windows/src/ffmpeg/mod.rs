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
    use ffmpeg_sys_next::{
        av_log_set_level, avcodec_find_encoder, AVCodecID, AV_LOG_QUIET,
    };

    unsafe {
        av_log_set_level(AV_LOG_QUIET);
    }

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

#[cfg(not(windows))]
pub fn probe_codec_inventory() -> Result<FfmpegCodecInventory, String> {
    Err("FFmpeg probing is only wired for the Windows host crate.".to_string())
}

#[cfg(test)]
mod tests {
    use super::probe_codec_inventory;

    #[test]
    fn detects_h264_codec_support_from_local_sdk() {
        let inventory = probe_codec_inventory().expect("failed to probe local ffmpeg sdk");
        assert!(inventory.h264.decoder, "expected an H.264 decoder in the local FFmpeg SDK");
        assert!(inventory.h264_encoder.any, "expected an H.264 encoder in the local FFmpeg SDK");
        assert!(
            inventory.h264_encoder.software || inventory.h264_encoder.hardware,
            "expected either software or hardware H.264 encoder support"
        );
    }
}

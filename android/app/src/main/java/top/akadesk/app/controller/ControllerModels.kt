package top.akadesk.app.controller

import akadesk.v1.VideoCodec

data class ControlledHostSummary(
    val serviceName: String,
    val deviceId: String,
    val displayName: String,
    val platform: String,
    val hostname: String,
    val appVersion: String,
    val hostAddress: String,
    val controlPort: Int,
    val pairedAdvertised: Boolean,
    val reachable: Boolean,
    val lastSeenMs: Long,
    val authState: HostAuthState = HostAuthState.Unknown,
)

data class ControllerPairRecord(
    val deviceId: String,
    val displayName: String,
    val hostAddress: String,
    val platform: String,
    val savedAtMs: Long,
)

data class ActiveRemoteSession(
    val host: ControlledHostSummary,
    val sessionId: String? = null,
    val stateLabel: String = "NEGOTIATING",
    val summary: String = "正在协商远控会话...",
    val startedAtMs: Long = System.currentTimeMillis(),
)

data class ControllerDisconnectNotice(
    val title: String,
    val message: String,
    val detail: String,
)

data class RemoteVideoPathDiagnosis(
    val title: String,
    val detail: String,
    val level: RemoteVideoDiagnosisLevel,
)

enum class RemoteVideoDiagnosisLevel {
    Info,
    Warning,
    Critical,
}

enum class HostAuthState {
    Unknown,
    PairingRequired,
    PairingPending,
    Authenticated,
    Failed,
}

enum class ControllerConnectMode(
    val title: String,
    val description: String,
) {
    TrustedRemembered(
        title = "记住后免密连接",
        description = "本机已保存配对凭证，直接发起远控。",
    ),
    VerificationCode(
        title = "输入验证码连接",
        description = "输入设备验证码后发起远控，适合首次配对和凭证失效后重连。",
    ),
    AwaitRemoteApproval(
        title = "等待对方允许",
        description = "发起远控请求并等待被控端桌面本地明确允许。",
    ),
}

enum class ControllerVideoCodecPreset(
    val storageKey: String,
    val title: String,
    val detail: String,
    val wireCodec: VideoCodec?,
    val runtimeReady: Boolean,
) {
    H264(
        storageKey = "h264",
        title = "H.264",
        detail = "当前 M4 主链路已打通的实际调试选项。",
        wireCodec = VideoCodec.VIDEO_CODEC_H264,
        runtimeReady = true,
    ),
    Hevc(
        storageKey = "hevc",
        title = "H.265 / HEVC",
        detail = "预留项，等 Host 与 Android 解码链路补齐后进入真实联调。",
        wireCodec = VideoCodec.VIDEO_CODEC_HEVC,
        runtimeReady = false,
    ),
    Vp8(
        storageKey = "vp8",
        title = "VP8",
        detail = "预留项，后续用于 WebRTC 兼容路径对比。",
        wireCodec = VideoCodec.VIDEO_CODEC_VP8,
        runtimeReady = false,
    ),
    Vp9(
        storageKey = "vp9",
        title = "VP9",
        detail = "预留项，后续用于兼容路径和编码质量对比。",
        wireCodec = VideoCodec.VIDEO_CODEC_VP9,
        runtimeReady = false,
    ),
    Av1(
        storageKey = "av1",
        title = "AV1",
        detail = "预留项，后续用于高压缩率调试。",
        wireCodec = VideoCodec.VIDEO_CODEC_AV1,
        runtimeReady = false,
    ),
    ;

    companion object {
        fun fromStorageKey(storageKey: String?): ControllerVideoCodecPreset {
            return entries.firstOrNull { it.storageKey == storageKey } ?: H264
        }
    }
}

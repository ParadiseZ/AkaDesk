package top.akadesk.app.controller

import akadesk.v1.IceCandidate as ProtoIceCandidate
import akadesk.v1.WebRtcOffer
import android.content.Context
import android.os.Handler
import android.os.Looper
import android.os.SystemClock
import org.webrtc.DataChannel
import org.webrtc.DefaultVideoDecoderFactory
import org.webrtc.DefaultVideoEncoderFactory
import org.webrtc.EglBase
import org.webrtc.IceCandidate
import org.webrtc.MediaConstraints
import org.webrtc.MediaStream
import org.webrtc.MediaStreamTrack
import org.webrtc.PeerConnection
import org.webrtc.PeerConnectionFactory
import org.webrtc.RendererCommon
import org.webrtc.RtpReceiver
import org.webrtc.RtpTransceiver
import org.webrtc.SdpObserver
import org.webrtc.SessionDescription
import org.webrtc.SurfaceViewRenderer
import org.webrtc.VideoFrame
import org.webrtc.VideoSink
import org.webrtc.VideoTrack
import java.util.concurrent.atomic.AtomicBoolean
import java.util.concurrent.atomic.AtomicInteger
import java.util.concurrent.atomic.AtomicLong

class AndroidWebRtcSession(
    appContext: Context,
    private val sessionId: String,
    private val callbacks: Callbacks,
) {
    private data class PendingRemoteCandidate(
        val candidate: IceCandidate,
        val attempts: Int,
    )

    interface Callbacks {
        fun onStateChanged(sessionId: String, detail: String)
        fun onLocalAnswer(sessionId: String, sdp: String)
        fun onLocalIceCandidate(
            sessionId: String,
            candidate: String,
            sdpMid: String,
            sdpMlineIndex: Int,
        )
        fun onRemoteVideoTrack(sessionId: String)
        fun onRemoteVideoFirstFrame(sessionId: String, width: Int, height: Int)
        fun onRemoteVideoStalled(sessionId: String, stalledForMs: Long, frameCount: Int)
        fun onRemoteVideoFlowResumed(sessionId: String, width: Int, height: Int, frameCount: Int)
        fun onFailure(sessionId: String, message: String)
    }

    private val eglBase: EglBase
    private val peerConnectionFactory: PeerConnectionFactory
    private val peerConnection: PeerConnection
    private val candidateLock = Any()
    private val pendingRemoteCandidates = mutableListOf<PendingRemoteCandidate>()
    private val pendingRemoteCandidateKeys = mutableSetOf<String>()
    private val appliedRemoteCandidateKeys = mutableSetOf<String>()
    private val renderLock = Any()
    private val mainHandler = Handler(Looper.getMainLooper())

    @Volatile
    private var closed = false

    @Volatile
    private var remoteDescriptionApplied = false

    @Volatile
    private var localAnswerSent = false

    @Volatile
    private var remoteOfferSetInFlight = false

    @Volatile
    private var remoteOfferSdp: String? = null

    private val remoteFrameCount = AtomicInteger(0)
    private val remoteFirstFrameReported = AtomicBoolean(false)
    private val remoteVideoStalled = AtomicBoolean(false)
    private val lastRemoteFrameAtMs = AtomicLong(0L)
    private val remoteVideoStallCheck = Runnable {
        if (closed || !remoteFirstFrameReported.get()) {
            return@Runnable
        }
        val stalledForMs = SystemClock.elapsedRealtime() - lastRemoteFrameAtMs.get()
        val frameCount = remoteFrameCount.get()
        if (stalledForMs >= REMOTE_VIDEO_STALL_TIMEOUT_MS
            && frameCount > 0
            && remoteVideoStalled.compareAndSet(false, true)
        ) {
            callbacks.onRemoteVideoStalled(sessionId, stalledForMs, frameCount)
            callbacks.onStateChanged(
                sessionId,
                "远端视频流疑似停滞，最近 $stalledForMs ms 未收到新帧"
            )
        }
    }
    private val remoteCandidateRetry = Runnable {
        if (closed) {
            return@Runnable
        }
        drainPendingRemoteCandidates()
    }
    private val remoteVideoObserver = VideoSink { frame: VideoFrame ->
        if (closed) {
            return@VideoSink
        }
        val now = SystemClock.elapsedRealtime()
        lastRemoteFrameAtMs.set(now)
        val frameIndex = remoteFrameCount.incrementAndGet()
        if (remoteVideoStalled.compareAndSet(true, false)) {
            callbacks.onRemoteVideoFlowResumed(
                sessionId,
                frame.rotatedWidth,
                frame.rotatedHeight,
                frameIndex,
            )
            callbacks.onStateChanged(
                sessionId,
                "远端视频流已恢复 ${frame.rotatedWidth}x${frame.rotatedHeight}，继续渲染被控画面"
            )
        }
        if (remoteFirstFrameReported.compareAndSet(false, true)) {
            callbacks.onRemoteVideoFirstFrame(
                sessionId,
                frame.rotatedWidth,
                frame.rotatedHeight,
            )
            callbacks.onStateChanged(
                sessionId,
                "已收到远端首帧 ${frame.rotatedWidth}x${frame.rotatedHeight}，正在渲染被控画面"
            )
        } else if (frameIndex == 30) {
            callbacks.onStateChanged(
                sessionId,
                "远端视频帧已持续到达，已观测到 $frameIndex 帧"
            )
        }
        scheduleRemoteVideoStallCheck()
    }

    private var renderer: SurfaceViewRenderer? = null
    private var remoteVideoTrack: VideoTrack? = null

    init {
        ensurePeerConnectionFactoryInitialized(appContext.applicationContext)
        eglBase = EglBase.create()
        peerConnectionFactory = PeerConnectionFactory.builder()
            .setVideoDecoderFactory(DefaultVideoDecoderFactory(eglBase.eglBaseContext))
            .setVideoEncoderFactory(
                DefaultVideoEncoderFactory(eglBase.eglBaseContext, true, true)
            )
            .createPeerConnectionFactory()
        peerConnection = createPeerConnection()
        ensureVideoReceiveTransceiver()
        callbacks.onStateChanged(sessionId, "Android WebRTC 已创建，等待远端 Offer")
    }

    fun applyRemoteOffer(offer: WebRtcOffer) {
        if (closed) {
            return
        }
        if (remoteOfferSetInFlight) {
            if (offer.sdp == remoteOfferSdp) {
                callbacks.onStateChanged(sessionId, "检测到重复远端 Offer，当前仍在应用中，已忽略")
            } else {
                callbacks.onStateChanged(sessionId, "当前正在应用另一份远端 Offer，新的 Offer 已忽略")
            }
            return
        }
        if (remoteDescriptionApplied) {
            if (!localAnswerSent) {
                if (offer.sdp == remoteOfferSdp) {
                    callbacks.onStateChanged(sessionId, "检测到重复远端 Offer，当前正在生成本地 Answer，已忽略")
                } else {
                    callbacks.onStateChanged(sessionId, "当前远端 Offer 已应用但本地 Answer 尚未完成，新的 Offer 已忽略")
                }
                return
            }
            if (offer.sdp != remoteOfferSdp) {
                callbacks.onStateChanged(sessionId, "当前会话暂不支持重协商，新的远端 Offer 已忽略")
                return
            }
        }
        if (offer.sdp == remoteOfferSdp && remoteDescriptionApplied) {
            callbacks.onStateChanged(sessionId, "检测到重复远端 Offer，已按当前会话状态忽略")
            return
        }
        remoteOfferSdp = offer.sdp
        remoteOfferSetInFlight = true
        callbacks.onStateChanged(sessionId, "已收到远端 Offer，正在设置描述")
        peerConnection.setRemoteDescription(
            loggingSdpObserver(
                action = "set remote offer",
                onSetSuccess = {
                    remoteOfferSetInFlight = false
                    remoteDescriptionApplied = true

                    drainPendingRemoteCandidates()
                    callbacks.onStateChanged(sessionId, "远端 Offer 已应用，正在生成 Answer")
                    peerConnection.createAnswer(
                        loggingSdpObserver(
                            action = "create local answer",
                            onCreateSuccess = { description ->
                                peerConnection.setLocalDescription(
                                    loggingSdpObserver(
                                        action = "set local answer",
                                        onSetSuccess = {
                                            if (!localAnswerSent) {
                                                localAnswerSent = true

                                                drainPendingRemoteCandidates()
                                                callbacks.onLocalAnswer(sessionId, description.description)
                                                callbacks.onStateChanged(
                                                    sessionId,
                                                    "本地 Answer 已回传，等待 Host 侧数据面"
                                                )
                                            }
                                        },
                                    ),
                                    description,
                                )
                            },
                        ),
                        MediaConstraints(),
                    )
                },
            ),
            SessionDescription(SessionDescription.Type.OFFER, offer.sdp),
        )
    }

    fun addRemoteIceCandidate(candidate: ProtoIceCandidate) {
        if (closed) {
            return
        }
        val nativeCandidate = IceCandidate(
            candidate.sdpMid,
            candidate.sdpMlineIndex,
            candidate.candidate,
        )
        val candidateKey = remoteCandidateKey(nativeCandidate)
        synchronized(candidateLock) {
            if (candidateKey in appliedRemoteCandidateKeys || candidateKey in pendingRemoteCandidateKeys) {
                callbacks.onStateChanged(sessionId, "检测到重复远端 ICE candidate，已忽略")
                return
            }
            if (!remoteDescriptionApplied) {
                pendingRemoteCandidates += PendingRemoteCandidate(nativeCandidate, 0)
                pendingRemoteCandidateKeys += candidateKey
                callbacks.onStateChanged(sessionId, "已缓存远端 ICE candidate，等待 Offer 生效")
                return
            }
        }
        if (remoteDescriptionApplied) {
            applyRemoteIceCandidate(
                nativeCandidate,
                candidateKey,
                attempts = 0,
                allowDeferredRetry = true,
            )
        }
    }

    fun bindRenderer(target: SurfaceViewRenderer) {
        if (closed) {
            return
        }
        synchronized(renderLock) {
            if (renderer === target) {
                return
            }
            detachRendererLocked()
            target.clearImage()
            target.release()
            target.init(eglBase.eglBaseContext, null)
            target.setEnableHardwareScaler(true)
            target.setMirror(false)
            target.setScalingType(RendererCommon.ScalingType.SCALE_ASPECT_FIT)
            renderer = target
            remoteVideoTrack?.addSink(target)
        }

        callbacks.onStateChanged(sessionId, "Android 渲染器已绑定，等待远端视频帧")
    }

    fun releaseRenderer(target: SurfaceViewRenderer) {
        synchronized(renderLock) {
            if (renderer === target) {
                remoteVideoTrack?.removeSink(target)
                renderer = null
            } else {
                remoteVideoTrack?.removeSink(target)
            }
        }
        target.clearImage()
        target.release()
    }

    fun hasRemoteVideoTrack(): Boolean = synchronized(renderLock) { remoteVideoTrack != null }

    fun close() {
        if (closed) {
            return
        }
        closed = true
        synchronized(candidateLock) {
            pendingRemoteCandidates.clear()
            pendingRemoteCandidateKeys.clear()
            appliedRemoteCandidateKeys.clear()
        }
        mainHandler.removeCallbacks(remoteCandidateRetry)
        mainHandler.removeCallbacks(remoteVideoStallCheck)
        synchronized(renderLock) {
            detachRendererLocked()
            detachRemoteVideoObserverLocked()
            remoteVideoTrack = null
            remoteFirstFrameReported.set(false)
            remoteFrameCount.set(0)
            remoteVideoStalled.set(false)
            lastRemoteFrameAtMs.set(0L)
        }
        peerConnection.close()
        peerConnection.dispose()
        peerConnectionFactory.dispose()
        eglBase.release()
    }

    private fun createPeerConnection(): PeerConnection {
        val config = PeerConnection.RTCConfiguration(emptyList()).apply {
            sdpSemantics = PeerConnection.SdpSemantics.UNIFIED_PLAN
        }
        return requireNotNull(
            peerConnectionFactory.createPeerConnection(config, object : PeerConnection.Observer {
                override fun onSignalingChange(newState: PeerConnection.SignalingState?) = Unit

                override fun onIceConnectionChange(newState: PeerConnection.IceConnectionState?) = Unit

                override fun onStandardizedIceConnectionChange(newState: PeerConnection.IceConnectionState?) {
                    newState?.let {

                        callbacks.onStateChanged(sessionId, "ICE 状态=${it.name}")
                    }
                }

                override fun onConnectionChange(newState: PeerConnection.PeerConnectionState?) {
                    newState?.let {

                        callbacks.onStateChanged(sessionId, "PeerConnection 状态=${it.name}")
                    }
                }

                override fun onIceConnectionReceivingChange(receiving: Boolean) = Unit

                override fun onIceGatheringChange(newState: PeerConnection.IceGatheringState?) {
                    newState?.let {
                        callbacks.onStateChanged(sessionId, "ICE 收集状态=${it.name}")
                    }
                }

                override fun onIceCandidate(candidate: IceCandidate?) {
                    if (closed || candidate == null) {
                        return
                    }
                    callbacks.onLocalIceCandidate(
                        sessionId = sessionId,
                        candidate = candidate.sdp,
                        sdpMid = candidate.sdpMid.orEmpty(),
                        sdpMlineIndex = candidate.sdpMLineIndex,
                    )
                }

                override fun onIceCandidatesRemoved(candidates: Array<out IceCandidate>?) = Unit

                override fun onAddStream(stream: MediaStream?) = Unit

                override fun onRemoveStream(stream: MediaStream?) = Unit

                override fun onDataChannel(dataChannel: DataChannel?) = Unit

                override fun onRenegotiationNeeded() {
                    callbacks.onStateChanged(sessionId, "远端请求重协商，当前 Android 侧暂未扩展该路径")
                }

                override fun onAddTrack(receiver: RtpReceiver?, mediaStreams: Array<out MediaStream>?) {
                    val track = receiver?.track()
                    if (track is VideoTrack) {
                        attachRemoteVideoTrack(track, "已协商远端视频轨道")
                    }
                }

                override fun onTrack(transceiver: RtpTransceiver?) {
                    val track = transceiver?.receiver?.track()
                    if (track is VideoTrack) {
                        attachRemoteVideoTrack(track, "已收到远端视频轨道")
                    }
                }
            })
        ) {
            "Failed to create PeerConnection for session $sessionId"
        }
    }

    private fun ensureVideoReceiveTransceiver() {
        try {
            val init = RtpTransceiver.RtpTransceiverInit(
                RtpTransceiver.RtpTransceiverDirection.RECV_ONLY
            )
            val transceiver = peerConnection.addTransceiver(
                MediaStreamTrack.MediaType.MEDIA_TYPE_VIDEO,
                init,
            )
            if (transceiver == null) {
                callbacks.onFailure(sessionId, "Failed to add recvonly video transceiver")
                return
            }
            callbacks.onStateChanged(sessionId, "Android 已预置视频接收 transceiver，等待远端 Offer")
        } catch (error: Throwable) {
            callbacks.onFailure(
                sessionId,
                "Failed to prepare recvonly video transceiver: ${error.message ?: error::class.java.simpleName}"
            )
        }
    }

    private fun attachRemoteVideoTrack(track: VideoTrack, detail: String) {
        synchronized(renderLock) {
            if (remoteVideoTrack === track) {
                track.removeSink(remoteVideoObserver)
                track.addSink(remoteVideoObserver)
                renderer?.let { boundRenderer ->
                    track.removeSink(boundRenderer)
                    track.addSink(boundRenderer)
                }
                return
            }
            remoteVideoTrack?.let { previous ->
                renderer?.let(previous::removeSink)
                previous.removeSink(remoteVideoObserver)
            }
            remoteFirstFrameReported.set(false)
            remoteFrameCount.set(0)
            remoteVideoStalled.set(false)
            lastRemoteFrameAtMs.set(0L)
            remoteVideoTrack = track
            track.addSink(remoteVideoObserver)
            renderer?.let(track::addSink)
        }
        callbacks.onRemoteVideoTrack(sessionId)
        callbacks.onStateChanged(sessionId, "$detail，等待远端首帧")
    }

    private fun drainPendingRemoteCandidates() {
        val candidatesToReplay = synchronized(candidateLock) {
            if (pendingRemoteCandidates.isEmpty()) {
                return
            }
            val replay = pendingRemoteCandidates.toList()
            pendingRemoteCandidates.clear()
            pendingRemoteCandidateKeys.clear()
            replay
        }
        val pendingCount = candidatesToReplay.size
        candidatesToReplay.forEach { pending ->
            applyRemoteIceCandidate(
                pending.candidate,
                remoteCandidateKey(pending.candidate),
                attempts = pending.attempts,
                allowDeferredRetry = true,
            )
        }
        callbacks.onStateChanged(
            sessionId,
            "已补写 $pendingCount 条远端 ICE candidate"
        )
    }

    private fun applyRemoteIceCandidate(
        candidate: IceCandidate,
        candidateKey: String,
        attempts: Int,
        allowDeferredRetry: Boolean,
    ) {
        val mid = RemoteIceMedia.resolveMid(remoteOfferSdp.orEmpty(), candidate.sdpMid, candidate.sdpMLineIndex)
        if (mid == null || candidate.sdp.isBlank()) {
            callbacks.onFailure(sessionId, "远端 ICE candidate 的媒体标识无效")
            return
        }
        val accepted = peerConnection.addIceCandidate(IceCandidate(mid, candidate.sdpMLineIndex, candidate.sdp))
        if (accepted) {
            synchronized(candidateLock) {
                appliedRemoteCandidateKeys += candidateKey
                pendingRemoteCandidateKeys -= candidateKey
            }

            callbacks.onStateChanged(sessionId, "已写入远端 ICE candidate")
        } else if (allowDeferredRetry && attempts < MAX_REMOTE_CANDIDATE_RETRIES) {
            synchronized(candidateLock) {
                if (candidateKey !in pendingRemoteCandidateKeys) {
                    pendingRemoteCandidates += PendingRemoteCandidate(candidate, attempts + 1)
                    pendingRemoteCandidateKeys += candidateKey
                }
            }
            scheduleRemoteCandidateRetry()
            callbacks.onStateChanged(sessionId, "远端 ICE candidate 暂未接收，已进入延迟重试")
        } else {
            synchronized(candidateLock) {
                pendingRemoteCandidateKeys -= candidateKey
            }
            callbacks.onStateChanged(sessionId, "远端 ICE candidate 未被当前会话接收，已忽略")
        }
    }

    private fun remoteCandidateKey(candidate: IceCandidate): String {
        return buildString {
            append(candidate.sdp)
            append('|')
            append(candidate.sdpMid.orEmpty())
            append('|')
            append(candidate.sdpMLineIndex)
        }
    }

    private fun detachRendererLocked() {
        renderer?.let { target ->
            remoteVideoTrack?.removeSink(target)
            target.clearImage()
        }
        renderer = null
    }

    private fun detachRemoteVideoObserverLocked() {
        remoteVideoTrack?.removeSink(remoteVideoObserver)
    }

    private fun scheduleRemoteCandidateRetry() {
        mainHandler.removeCallbacks(remoteCandidateRetry)
        mainHandler.postDelayed(remoteCandidateRetry, REMOTE_CANDIDATE_RETRY_DELAY_MS)
    }

    private fun scheduleRemoteVideoStallCheck() {
        mainHandler.removeCallbacks(remoteVideoStallCheck)
        mainHandler.postDelayed(remoteVideoStallCheck, REMOTE_VIDEO_STALL_TIMEOUT_MS)
    }

    private fun loggingSdpObserver(
        action: String,
        onCreateSuccess: ((SessionDescription) -> Unit)? = null,
        onSetSuccess: (() -> Unit)? = null,
    ): SdpObserver {
        return object : SdpObserver {
            override fun onCreateSuccess(description: SessionDescription?) {
                if (closed) {
                    return
                }
                val safeDescription = description
                if (safeDescription == null) {
                    callbacks.onFailure(sessionId, "$action returned null SDP")
                    return
                }
                onCreateSuccess?.invoke(safeDescription)
            }

            override fun onSetSuccess() {
                if (closed) {
                    return
                }
                onSetSuccess?.invoke()
            }

            override fun onCreateFailure(error: String?) {
                if (closed) return
                if (action == "set remote offer") {
                    remoteOfferSetInFlight = false
                }
                callbacks.onFailure(sessionId, "$action failed: ${error ?: "unknown"}")
            }

            override fun onSetFailure(error: String?) {
                if (closed) return
                if (action == "set remote offer") {
                    remoteOfferSetInFlight = false
                }
                callbacks.onFailure(sessionId, "$action failed: ${error ?: "unknown"}")
            }
        }
    }

    companion object {
        private const val MAX_REMOTE_CANDIDATE_RETRIES = 6
        private const val REMOTE_CANDIDATE_RETRY_DELAY_MS = 250L
        private const val REMOTE_VIDEO_STALL_TIMEOUT_MS = 2_500L

        @Volatile
        private var peerConnectionFactoryInitialized = false

        private fun ensurePeerConnectionFactoryInitialized(context: Context) {
            if (peerConnectionFactoryInitialized) {
                return
            }
            synchronized(AndroidWebRtcSession::class.java) {
                if (peerConnectionFactoryInitialized) {
                    return
                }
                PeerConnectionFactory.initialize(
                    PeerConnectionFactory.InitializationOptions.builder(context)
                        .createInitializationOptions()
                )
                peerConnectionFactoryInitialized = true
            }
        }
    }
}

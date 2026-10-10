package top.akadesk.app.controller

import akadesk.v1.ControlEnvelope
import akadesk.v1.HostTransportStatusEvent
import akadesk.v1.HostVideoStreamEvent
import akadesk.v1.IceCandidate
import akadesk.v1.PairingChallengeType
import akadesk.v1.VideoCodec
import akadesk.v1.WebRtcOffer
import android.os.Handler
import android.os.Looper
import android.os.SystemClock
import com.google.protobuf.InvalidProtocolBufferException
import okhttp3.HttpUrl
import okhttp3.OkHttpClient
import okhttp3.Request
import okhttp3.Response
import okhttp3.WebSocket
import okhttp3.WebSocketListener
import okio.ByteString
import java.util.concurrent.atomic.AtomicLong

internal interface ControlSessionScheduler {
    fun schedule(task: Runnable, delayMs: Long)
    fun cancel(task: Runnable)
}

internal class AndroidControlSessionScheduler : ControlSessionScheduler {
    private val handler = Handler(Looper.getMainLooper())
    override fun schedule(task: Runnable, delayMs: Long) { handler.postDelayed(task, delayMs) }
    override fun cancel(task: Runnable) { handler.removeCallbacks(task) }
}

internal class HostControlSession(
    private val host: ControlledHostSummary,
    private val controllerDeviceId: String,
    private val pairToken: String?,
    private val listener: Listener,
    private val scheduler: ControlSessionScheduler = AndroidControlSessionScheduler(),
    private val elapsedRealtimeMs: () -> Long = SystemClock::elapsedRealtime,
    private val createWebSocket: (Request, WebSocketListener) -> WebSocket = sharedClient::newWebSocket,
) {
    companion object {
        private const val HEARTBEAT_INTERVAL_MS = 2_000L
        private const val HEARTBEAT_TIMEOUT_MS = 6_000L
        private val sharedClient = OkHttpClient()
    }

    @Volatile
    private var closed = false
    private val lifecycleLock = Any()
    private var started = false

    interface Listener {
        fun onAuthenticated(host: ControlledHostSummary)
        fun onPairingRequired(host: ControlledHostSummary, message: String, errorCode: String)
        fun onPairingChallenge(host: ControlledHostSummary, challengeId: String, challengeType: PairingChallengeType)
        fun onPairingSucceeded(host: ControlledHostSummary, pairToken: String)
        fun onDeviceSnapshotLoaded(host: ControlledHostSummary)
        fun onSessionAccepted(host: ControlledHostSummary, sessionId: String, stateLabel: String)
        fun onSessionChanged(host: ControlledHostSummary, sessionId: String, stateLabel: String, summary: String)
        fun onWebRtcOffer(host: ControlledHostSummary, offer: WebRtcOffer)
        fun onRemoteIceCandidate(host: ControlledHostSummary, candidate: IceCandidate)
        fun onHostVideoStream(host: ControlledHostSummary, event: HostVideoStreamEvent)
        fun onHostTransportStatus(host: ControlledHostSummary, event: HostTransportStatusEvent)
        fun onSessionClosed(host: ControlledHostSummary, sessionId: String, reason: String)
        fun onFailure(host: ControlledHostSummary, message: String)
    }

    private val requestCounter = AtomicLong(0)
    @Volatile
    private var socket: WebSocket? = null
    private var pendingChallengeId: String? = null
    private val lastHeartbeatPongAtMs = AtomicLong(0L)
    private val heartbeatRunnable = object : Runnable {
        override fun run() {
            if (closed) {
                return
            }
            val currentSocket = socket ?: return
            val now = elapsedRealtimeMs()
            if (now - lastHeartbeatPongAtMs.get() >= HEARTBEAT_TIMEOUT_MS) {
                notifyFailure("Control heartbeat timeout")
                return
            }
            send(ControlProtoCodec.buildHeartbeatPing(nextRequestId(), System.currentTimeMillis()), currentSocket)
            if (!closed) scheduler.schedule(this, HEARTBEAT_INTERVAL_MS)
        }
    }

    fun connect() {
        synchronized(lifecycleLock) {
            if (closed || started) return
            started = true
        }
        val request = try {
            Request.Builder()
                .url(HttpUrl.Builder().scheme("http").host(host.hostAddress).port(host.controlPort).build())
                .build()
        } catch (_: IllegalArgumentException) {
            notifyFailure("Invalid control endpoint")
            return
        }
        val connected = createWebSocket(request, object : WebSocketListener() {
            override fun onOpen(webSocket: WebSocket, response: Response) {
                if (closed) {
                    webSocket.cancel()
                    return
                }
                lastHeartbeatPongAtMs.set(elapsedRealtimeMs())
                send(ControlProtoCodec.buildControlHello(nextRequestId(), controllerDeviceId, pairToken), webSocket)
                if (!closed) scheduler.schedule(heartbeatRunnable, HEARTBEAT_INTERVAL_MS)
            }

            override fun onMessage(webSocket: WebSocket, bytes: ByteString) {
                if (closed) {
                    return
                }
                try {
                    handleEnvelope(ControlProtoCodec.decodeEnvelope(bytes.toByteArray()))
                } catch (_: InvalidProtocolBufferException) {
                    notifyFailure("Invalid control message")
                }
            }

            override fun onFailure(webSocket: WebSocket, t: Throwable, response: Response?) {
                if (closed) {
                    return
                }
                notifyFailure(t.message ?: "Unknown control failure")
            }

            override fun onClosed(webSocket: WebSocket, code: Int, reason: String) {
                if (closed) {
                    return
                }
                notifyFailure(if (reason.isBlank()) "Control connection closed" else reason)
            }

            override fun onClosing(webSocket: WebSocket, code: Int, reason: String) {
                webSocket.close(code, reason)
                notifyFailure(if (reason.isBlank()) "Control connection closed" else reason)
            }
        })
        synchronized(lifecycleLock) {
            if (closed) connected.cancel() else socket = connected
        }
    }

    fun requestPairing(displayName: String) {
        send(ControlProtoCodec.buildPairingRequest(nextRequestId(), controllerDeviceId, displayName))
    }

    fun confirmPairing(code: String) {
        val challengeId = pendingChallengeId ?: return
        send(ControlProtoCodec.buildPairingConfirm(nextRequestId(), challengeId, code))
    }

    fun close() {
        terminate()
    }

    fun requestRemoteSession(requestedCodec: VideoCodec?) {
        send(ControlProtoCodec.buildStartSessionRequest(nextRequestId(), controllerDeviceId, host.deviceId, requestedCodec))
    }

    fun requestStopSession(sessionId: String) {
        send(ControlProtoCodec.buildStopSessionRequest(nextRequestId(), sessionId))
    }

    fun sendWebRtcAnswer(sessionId: String, sdp: String) {
        send(ControlProtoCodec.buildWebRtcAnswer(nextRequestId(), sessionId, sdp))
    }

    fun sendIceCandidate(sessionId: String, candidate: String, sdpMid: String, sdpMlineIndex: Int) {
        send(ControlProtoCodec.buildIceCandidate(nextRequestId(), sessionId, candidate, sdpMid, sdpMlineIndex))
    }

    private fun handleEnvelope(envelope: ControlEnvelope) {
        when (envelope.type) {
            "ControlHelloAck" -> {
                val ack = ControlProtoCodec.decodeControlHelloAck(envelope)
                if (ack.accepted) {
                    listener.onAuthenticated(host)
                    send(ControlProtoCodec.buildGetDeviceSnapshot(nextRequestId()))
                } else {
                    val message = if (ack.hasError()) ack.error.message else "Pairing required"
                    val errorCode = if (ack.hasError()) ack.error.code else ""
                    listener.onPairingRequired(host, message, errorCode)
                }
            }
            "PairingChallenge" -> {
                val challenge = ControlProtoCodec.decodePairingChallenge(envelope)
                pendingChallengeId = challenge.challengeId
                listener.onPairingChallenge(
                    host,
                    challenge.challengeId,
                    challenge.challengeType,
                )
            }
            "PairingResult" -> {
                val result = ControlProtoCodec.decodePairingResult(envelope)
                if (result.accepted) {
                    listener.onPairingSucceeded(host, result.pairToken)
                    listener.onAuthenticated(host)
                    send(ControlProtoCodec.buildGetDeviceSnapshot(nextRequestId()))
                } else {
                    val message = if (result.hasError()) result.error.message else "Pairing failed"
                    notifyFailure(message)
                }
            }
            "DeviceSnapshotResponse" -> {
                val response = ControlProtoCodec.decodeDeviceSnapshotResponse(envelope)
                if (response.hasSnapshot()) {
                    listener.onDeviceSnapshotLoaded(host)
                } else {
                    val message = if (response.hasError()) response.error.message else "Snapshot unavailable"
                    notifyFailure(message)
                }
            }
            "HeartbeatPong" -> {
                ControlProtoCodec.decodeHeartbeatPong(envelope)
                lastHeartbeatPongAtMs.set(elapsedRealtimeMs())
            }
            "StartSessionAccepted" -> {
                val accepted = ControlProtoCodec.decodeStartSessionAccepted(envelope)
                val stateLabel = accepted.state.name.removePrefix("SESSION_STATE_")
                listener.onSessionAccepted(host, accepted.sessionId, stateLabel)
            }
            "StartSessionRejected" -> {
                val rejected = ControlProtoCodec.decodeStartSessionRejected(envelope)
                val message = if (rejected.hasError()) rejected.error.message else "Session rejected"
                notifyFailure(message)
            }
            "SessionChangedEvent" -> {
                val changed = ControlProtoCodec.decodeSessionChangedEvent(envelope)
                if (!changed.hasSnapshot()) {
                    notifyFailure("SessionChangedEvent missing snapshot")
                    return
                }
                val snapshot = changed.snapshot
                val stateLabel = snapshot.state.name.removePrefix("SESSION_STATE_")
                val summary = buildString {
                    append("state=")
                    append(stateLabel)
                    if (snapshot.hasVideo()) {
                        val video = snapshot.video
                        append(" • codec=")
                        append(video.codec.name.removePrefix("VIDEO_CODEC_"))
                        append(" • ")
                        append(video.width)
                        append("x")
                        append(video.height)
                        append("@")
                        append(video.fps)
                        append("fps")
                        append(" • ")
                        append(video.bitrateKbps)
                        append("kbps")
                        append(" • host=")
                        append(video.hostEncoder.name.removePrefix("CODEC_IMPLEMENTATION_"))
                        append(" • controller=")
                        append(video.controllerDecoder.name.removePrefix("CODEC_IMPLEMENTATION_"))
                    }
                }
                listener.onSessionChanged(host, snapshot.sessionId, stateLabel, summary)
            }
            "SessionClosedEvent" -> {
                val closed = ControlProtoCodec.decodeSessionClosedEvent(envelope)
                val reason = closed.reason.name.removePrefix("DISCONNECT_REASON_")
                listener.onSessionClosed(host, closed.sessionId, reason)
            }
            "WebRtcOffer" -> {
                listener.onWebRtcOffer(host, ControlProtoCodec.decodeWebRtcOffer(envelope))
            }
            "IceCandidate" -> {
                listener.onRemoteIceCandidate(host, ControlProtoCodec.decodeIceCandidate(envelope))
            }
            "HostVideoStreamEvent" -> {
                listener.onHostVideoStream(host, ControlProtoCodec.decodeHostVideoStreamEvent(envelope))
            }
            "HostTransportStatusEvent" -> {
                listener.onHostTransportStatus(host, ControlProtoCodec.decodeHostTransportStatusEvent(envelope))
            }
        }
    }

    private fun nextRequestId(): String = "android-${requestCounter.incrementAndGet()}"

    private fun notifyFailure(message: String) {
        if (terminate()) listener.onFailure(host, message)
    }

    private fun send(envelope: ControlEnvelope, target: WebSocket? = socket) {
        if (!closed && target != null && !target.send(ByteString.of(*envelope.toByteArray()))) {
            notifyFailure("Control send failed")
        }
    }

    private fun terminate(): Boolean {
        val currentSocket = synchronized(lifecycleLock) {
            if (closed) return false
            closed = true
            socket.also { socket = null }
        }
        scheduler.cancel(heartbeatRunnable)
        currentSocket?.cancel()
        return true
    }
}

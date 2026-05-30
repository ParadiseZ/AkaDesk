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
import okhttp3.OkHttpClient
import okhttp3.Request
import okhttp3.Response
import okhttp3.WebSocket
import okhttp3.WebSocketListener
import okio.ByteString
import java.util.concurrent.atomic.AtomicLong

class HostControlSession(
    private val host: ControlledHostSummary,
    private val controllerDeviceId: String,
    private val pairToken: String?,
    private val listener: Listener,
) {
    companion object {
        private const val HEARTBEAT_INTERVAL_MS = 2_000L
        private const val HEARTBEAT_TIMEOUT_MS = 6_000L
    }

    @Volatile
    private var closed = false
    @Volatile
    private var failureReported = false
    private val mainHandler = Handler(Looper.getMainLooper())

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

    private val client = OkHttpClient()
    private val requestCounter = AtomicLong(0)
    private var socket: WebSocket? = null
    private var pendingChallengeId: String? = null
    private var lastHeartbeatPongAtMs = 0L
    private val heartbeatRunnable = object : Runnable {
        override fun run() {
            if (closed) {
                return
            }
            val currentSocket = socket ?: return
            val now = System.currentTimeMillis()
            if (lastHeartbeatPongAtMs > 0L && now - lastHeartbeatPongAtMs >= HEARTBEAT_TIMEOUT_MS) {
                notifyFailure("Control heartbeat timeout")
                currentSocket.cancel()
                return
            }
            currentSocket.send(
                ByteString.of(
                    *ControlProtoCodec
                        .buildHeartbeatPing(nextRequestId(), now)
                        .toByteArray()
                )
            )
            mainHandler.postDelayed(this, HEARTBEAT_INTERVAL_MS)
        }
    }

    fun connect() {
        closed = false
        failureReported = false
        lastHeartbeatPongAtMs = System.currentTimeMillis()
        val request = Request.Builder()
            .url("ws://${host.hostAddress}:${host.controlPort}/")
            .build()
        socket = client.newWebSocket(request, object : WebSocketListener() {
            override fun onOpen(webSocket: WebSocket, response: Response) {
                if (closed) {
                    webSocket.close(1000, "close")
                    return
                }
                webSocket.send(
                    ByteString.of(
                        *ControlProtoCodec
                            .buildControlHello(nextRequestId(), controllerDeviceId, pairToken)
                            .toByteArray()
                    )
                )
                mainHandler.removeCallbacks(heartbeatRunnable)
                mainHandler.postDelayed(heartbeatRunnable, HEARTBEAT_INTERVAL_MS)
            }

            override fun onMessage(webSocket: WebSocket, bytes: ByteString) {
                if (closed) {
                    return
                }
                handleEnvelope(ControlProtoCodec.decodeEnvelope(bytes.toByteArray()))
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
        })
    }

    fun requestPairing(displayName: String) {
        socket?.send(
            ByteString.of(
                *ControlProtoCodec
                    .buildPairingRequest(nextRequestId(), controllerDeviceId, displayName)
                    .toByteArray()
            )
        )
    }

    fun confirmPairing(code: String) {
        val challengeId = pendingChallengeId ?: return
        socket?.send(
            ByteString.of(
                *ControlProtoCodec
                    .buildPairingConfirm(nextRequestId(), challengeId, code)
                    .toByteArray()
            )
        )
    }

    fun close() {
        closed = true
        mainHandler.removeCallbacks(heartbeatRunnable)
        socket?.close(1000, "close")
        socket = null
        client.dispatcher.executorService.shutdown()
    }

    fun requestRemoteSession(requestedCodec: VideoCodec?) {
        socket?.send(
            ByteString.of(
                *ControlProtoCodec
                    .buildStartSessionRequest(nextRequestId(), controllerDeviceId, host.deviceId, requestedCodec)
                    .toByteArray()
            )
        )
    }

    fun requestStopSession(sessionId: String) {
        socket?.send(
            ByteString.of(
                *ControlProtoCodec
                    .buildStopSessionRequest(nextRequestId(), sessionId)
                    .toByteArray()
            )
        )
    }

    fun sendWebRtcAnswer(sessionId: String, sdp: String) {
        socket?.send(
            ByteString.of(
                *ControlProtoCodec
                    .buildWebRtcAnswer(nextRequestId(), sessionId, sdp)
                    .toByteArray()
            )
        )
    }

    fun sendIceCandidate(sessionId: String, candidate: String, sdpMid: String, sdpMlineIndex: Int) {
        socket?.send(
            ByteString.of(
                *ControlProtoCodec
                    .buildIceCandidate(nextRequestId(), sessionId, candidate, sdpMid, sdpMlineIndex)
                    .toByteArray()
            )
        )
    }

    private fun handleEnvelope(envelope: ControlEnvelope) {
        when (envelope.type) {
            "ControlHelloAck" -> {
                val ack = ControlProtoCodec.decodeControlHelloAck(envelope)
                if (ack.accepted) {
                    listener.onAuthenticated(host)
                    socket?.send(
                        ByteString.of(
                            *ControlProtoCodec
                                .buildGetDeviceSnapshot(nextRequestId())
                                .toByteArray()
                        )
                    )
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
                    socket?.send(
                        ByteString.of(
                            *ControlProtoCodec
                                .buildGetDeviceSnapshot(nextRequestId())
                                .toByteArray()
                        )
                    )
                } else {
                    val message = if (result.hasError()) result.error.message else "Pairing failed"
                    listener.onFailure(host, message)
                }
            }
            "DeviceSnapshotResponse" -> {
                val response = ControlProtoCodec.decodeDeviceSnapshotResponse(envelope)
                if (response.hasSnapshot()) {
                    listener.onDeviceSnapshotLoaded(host)
                } else {
                    val message = if (response.hasError()) response.error.message else "Snapshot unavailable"
                    listener.onFailure(host, message)
                }
            }
            "HeartbeatPong" -> {
                ControlProtoCodec.decodeHeartbeatPong(envelope)
                lastHeartbeatPongAtMs = System.currentTimeMillis()
            }
            "StartSessionAccepted" -> {
                val accepted = ControlProtoCodec.decodeStartSessionAccepted(envelope)
                val stateLabel = accepted.state.name.removePrefix("SESSION_STATE_")
                listener.onSessionAccepted(host, accepted.sessionId, stateLabel)
            }
            "StartSessionRejected" -> {
                val rejected = ControlProtoCodec.decodeStartSessionRejected(envelope)
                val message = if (rejected.hasError()) rejected.error.message else "Session rejected"
                listener.onFailure(host, message)
            }
            "SessionChangedEvent" -> {
                val changed = ControlProtoCodec.decodeSessionChangedEvent(envelope)
                if (!changed.hasSnapshot()) {
                    listener.onFailure(host, "SessionChangedEvent missing snapshot")
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
        if (closed || failureReported) {
            return
        }
        failureReported = true
        mainHandler.removeCallbacks(heartbeatRunnable)
        socket = null
        listener.onFailure(host, message)
    }
}

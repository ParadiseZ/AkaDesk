package top.akadesk.app.controller

import akadesk.v1.ControlEnvelope
import akadesk.v1.VideoCodec
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
    interface Listener {
        fun onAuthenticated(host: ControlledHostSummary)
        fun onPairingRequired(host: ControlledHostSummary, message: String)
        fun onPairingChallenge(host: ControlledHostSummary, challengeId: String)
        fun onPairingSucceeded(host: ControlledHostSummary, pairToken: String)
        fun onDeviceSnapshotLoaded(host: ControlledHostSummary)
        fun onSessionAccepted(host: ControlledHostSummary, sessionId: String)
        fun onSessionChanged(host: ControlledHostSummary, sessionId: String, stateLabel: String, summary: String)
        fun onFailure(host: ControlledHostSummary, message: String)
    }

    private val client = OkHttpClient()
    private val requestCounter = AtomicLong(0)
    private var socket: WebSocket? = null
    private var pendingChallengeId: String? = null

    fun connect() {
        val request = Request.Builder()
            .url("ws://${host.hostAddress}:${host.controlPort}/")
            .build()
        socket = client.newWebSocket(request, object : WebSocketListener() {
            override fun onOpen(webSocket: WebSocket, response: Response) {
                webSocket.send(
                    ByteString.of(
                        *ControlProtoCodec
                            .buildControlHello(nextRequestId(), controllerDeviceId, pairToken)
                            .toByteArray()
                    )
                )
            }

            override fun onMessage(webSocket: WebSocket, bytes: ByteString) {
                handleEnvelope(ControlProtoCodec.decodeEnvelope(bytes.toByteArray()))
            }

            override fun onFailure(webSocket: WebSocket, t: Throwable, response: Response?) {
                listener.onFailure(host, t.message ?: "Unknown control failure")
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
                    listener.onPairingRequired(host, message)
                }
            }
            "PairingChallenge" -> {
                val challenge = ControlProtoCodec.decodePairingChallenge(envelope)
                pendingChallengeId = challenge.challengeId
                listener.onPairingChallenge(host, challenge.challengeId)
            }
            "PairingResult" -> {
                val result = ControlProtoCodec.decodePairingResult(envelope)
                if (result.accepted) {
                    listener.onPairingSucceeded(host, result.pairToken)
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
            "StartSessionAccepted" -> {
                val accepted = ControlProtoCodec.decodeStartSessionAccepted(envelope)
                listener.onSessionAccepted(host, accepted.sessionId)
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
        }
    }

    private fun nextRequestId(): String = "android-${requestCounter.incrementAndGet()}"
}

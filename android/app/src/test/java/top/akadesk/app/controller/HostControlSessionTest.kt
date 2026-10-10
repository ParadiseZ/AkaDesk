package top.akadesk.app.controller

import akadesk.v1.*
import com.google.protobuf.MessageLite
import okhttp3.Request
import okhttp3.Response
import okhttp3.WebSocket
import okhttp3.WebSocketListener
import okio.ByteString
import org.junit.Assert.*
import org.junit.Test

class HostControlSessionTest {
    @Test fun earlyStartWaitsForCapabilitiesAndEnablesInputExactlyOnce() {
        val fixture=Fixture();fixture.open()
        fixture.receive("ControlHelloAck",ControlHelloAck.newBuilder().setAccepted(true).build())
        fixture.session.requestRemoteSession(VideoCodec.VIDEO_CODEC_H264)
        fun starts()=fixture.socket.messages.map {ControlEnvelope.parseFrom(it.toByteArray())}.filter {it.type=="StartSessionRequest"}
        assertTrue(starts().isEmpty())
        val snapshot=DeviceSnapshotResponse.newBuilder().setSnapshot(DeviceSnapshot.newBuilder()
            .setCapabilityFlags(CapabilityFlags.newBuilder().setSupportsInput(true))).build()
        fixture.receive("DeviceSnapshotResponse",snapshot)
        fixture.receive("DeviceSnapshotResponse",snapshot)
        assertEquals(1,starts().size)
        assertTrue(StartSessionRequest.parseFrom(starts().single().payload).intent.enableInput)
    }
    @Test fun viewOnlyCapabilitiesAndCloseDoNotEnableOrReplayPendingInput() {
        val fixture=Fixture();fixture.open();fixture.session.requestRemoteSession(null)
        fixture.receive("DeviceSnapshotResponse",DeviceSnapshotResponse.newBuilder().setSnapshot(DeviceSnapshot.getDefaultInstance()).build())
        val start=fixture.socket.messages.map {ControlEnvelope.parseFrom(it.toByteArray())}.single {it.type=="StartSessionRequest"}
        assertFalse(StartSessionRequest.parseFrom(start.payload).intent.enableInput)
        val closed=Fixture();closed.open();closed.session.requestRemoteSession(null);closed.session.close()
        closed.receive("DeviceSnapshotResponse",DeviceSnapshotResponse.newBuilder().setSnapshot(DeviceSnapshot.getDefaultInstance()).build())
        assertFalse(closed.socket.messages.map {ControlEnvelope.parseFrom(it.toByteArray())}.any {it.type=="StartSessionRequest"})
    }
    @Test fun heartbeatTimeoutCancelsSocketAndIgnoresLateCallbacks() {
        val fixture = Fixture()
        fixture.open()
        fixture.scheduler.advanceTo(6_000L)
        fixture.receive("ControlHelloAck", ControlHelloAck.newBuilder().setAccepted(true).build())
        fixture.listener.onFailure(fixture.socket, IllegalStateException("late"), null)

        assertEquals(listOf("Control heartbeat timeout"), fixture.events.failures)
        assertTrue(fixture.socket.cancelled)
        assertEquals(0, fixture.events.authentications)
        assertTrue(fixture.scheduler.pending.isEmpty())
    }

    @Test fun pongExtendsHeartbeatFromReceiptTime() {
        val fixture = Fixture()
        fixture.open()
        fixture.scheduler.advanceTo(5_000L)
        fixture.receive("HeartbeatPong", HeartbeatPong.getDefaultInstance())
        fixture.scheduler.advanceTo(10_999L)
        assertTrue(fixture.events.failures.isEmpty())
        fixture.scheduler.advanceTo(12_999L)
        assertEquals(1, fixture.events.failures.size)
    }

    @Test fun explicitCloseIsSilentAndCannotRestartOrSend() {
        val fixture = Fixture()
        fixture.open()
        fixture.session.close()
        val sent = fixture.socket.messages.size
        fixture.session.requestStopSession("session")
        fixture.session.connect()
        fixture.listener.onClosed(fixture.socket, 1000, "late")
        fixture.receive("ControlHelloAck", ControlHelloAck.newBuilder().setAccepted(true).build())

        assertTrue(fixture.socket.cancelled)
        assertTrue(fixture.events.failures.isEmpty())
        assertEquals(0, fixture.events.authentications)
        assertEquals(sent, fixture.socket.messages.size)
        assertEquals(1, fixture.connectionCount)
        assertTrue(fixture.scheduler.pending.isEmpty())
    }

    @Test fun rejectedSendClosesConnectionOnce() {
        val fixture = Fixture()
        fixture.open()
        fixture.socket.acceptsMessages = false
        fixture.session.requestStopSession("session")
        fixture.session.requestStopSession("session")

        assertEquals(listOf("Control send failed"), fixture.events.failures)
        assertTrue(fixture.socket.cancelled)
        assertTrue(fixture.scheduler.pending.isEmpty())
    }

    @Test fun malformedPayloadEndsConnectionWithoutThrowing() {
        val fixture = Fixture()
        fixture.open()
        val malformed = ControlEnvelope.newBuilder().setType("HeartbeatPong")
            .setPayload(com.google.protobuf.ByteString.copyFrom(byteArrayOf(0x80.toByte()))).build()
        fixture.listener.onMessage(fixture.socket, ByteString.of(*malformed.toByteArray()))

        assertEquals(listOf("Invalid control message"), fixture.events.failures)
        assertTrue(fixture.socket.cancelled)
    }

    @Test fun pairingRejectionAlsoReleasesTheConnection() {
        val fixture = Fixture()
        fixture.open()
        fixture.receive("PairingResult", PairingResult.newBuilder().setAccepted(false).build())

        assertEquals(1, fixture.events.failures.size)
        assertTrue(fixture.socket.cancelled)
        assertTrue(fixture.scheduler.pending.isEmpty())
    }

    @Test fun remoteCloseAndLateFailureAreReportedOnce() {
        val fixture = Fixture()
        fixture.open()
        fixture.listener.onClosing(fixture.socket, 1000, "stopped")
        fixture.listener.onClosed(fixture.socket, 1000, "stopped")
        fixture.listener.onFailure(fixture.socket, IllegalStateException("late"), null)

        assertEquals(listOf("stopped"), fixture.events.failures)
        assertTrue(fixture.socket.cancelled)
        assertTrue(fixture.scheduler.pending.isEmpty())
    }

    @Test fun ipv6DiscoveryAddressProducesAValidWebSocketEndpoint() {
        val fixture = Fixture(address = "fd00::1234")
        assertEquals("fd00::1234", fixture.socket.request().url.host)
        assertEquals(46661, fixture.socket.request().url.port)
    }

    @Test fun failureDuringSocketCreationCancelsReturnedSocket() {
        val fixture = Fixture(failDuringCreation = true)
        assertEquals(listOf("connect failed"), fixture.events.failures)
        assertTrue(fixture.socket.cancelled)
    }

    @Test fun invalidDiscoveryEndpointReportsFailureWithoutStartingConnection() {
        val fixture = Fixture(port = 0)
        assertEquals(listOf("Invalid control endpoint"), fixture.events.failures)
        assertEquals(0, fixture.connectionCount)
    }

    @Test fun audioSnapshotAndMuteFailureReachTheListenerWithoutClosingVideo() {
        val fixture = Fixture()
        fixture.open()
        fixture.receive("SessionChangedEvent", SessionChangedEvent.newBuilder().setSnapshot(
            SessionSnapshot.newBuilder().setSessionId("audio-session").setState(SessionState.SESSION_STATE_STREAMING)
                .setAudio(AudioSessionSpec.newBuilder().setCodec(AudioCodec.AUDIO_CODEC_OPUS))
                .setMuteControlledLocalPlayback(true)).build())
        assertEquals(listOf("audio-session:true:true"), fixture.events.audioStates)
        fixture.session.setComputerMuted("audio-session", false)
        val envelope = ControlEnvelope.parseFrom(fixture.socket.messages.last().toByteArray())
        assertEquals("SetAudioMuteRequest", envelope.type)
        assertFalse(SetAudioMuteRequest.parseFrom(envelope.payload).muted)
        fixture.receive("SetAudioMuteResponse", SetAudioMuteResponse.newBuilder().setSessionId("audio-session")
            .setMuted(true).setError(ErrorInfo.newBuilder().setMessage("endpoint unavailable")).build())
        assertEquals(listOf("audio-session:true:endpoint unavailable"), fixture.events.audioResults)
        assertTrue(fixture.events.failures.isEmpty())
        assertFalse(fixture.socket.cancelled)
    }

    private class Fixture(address: String = "127.0.0.1", failDuringCreation: Boolean = false, port: Int = 46661) {
        val scheduler = FakeScheduler()
        val socket = FakeWebSocket()
        val events = RecordingListener()
        lateinit var listener: WebSocketListener
        var connectionCount = 0
        private val host = ControlledHostSummary("test", "123456789", "test", "windows", "test",
            "test", address, port, true, true, 0L)
        val session = HostControlSession(host, "987654321", "token", events, scheduler,
            elapsedRealtimeMs = { scheduler.now },
            createWebSocket = { request, callback ->
                connectionCount++
                socket.endpoint = request
                listener = callback
                if (failDuringCreation) callback.onFailure(socket, IllegalStateException("connect failed"), null)
                socket
            })

        init { session.connect() }
        fun open() {
            listener.onOpen(socket, Response.Builder().request(socket.request())
                .protocol(okhttp3.Protocol.HTTP_1_1).code(101).message("Switching Protocols").build())
        }
        fun receive(type: String, payload: MessageLite) {
            val envelope = ControlEnvelope.newBuilder().setType(type).setPayload(payload.toByteString()).build()
            listener.onMessage(socket, ByteString.of(*envelope.toByteArray()))
        }
    }

    private class FakeScheduler : ControlSessionScheduler {
        var now = 0L
        val pending = mutableMapOf<Runnable, Long>()
        override fun schedule(task: Runnable, delayMs: Long) { pending[task] = now + delayMs }
        override fun cancel(task: Runnable) { pending.remove(task) }
        fun advanceTo(time: Long) {
            while (true) {
                val entry = pending.entries.minByOrNull { it.value } ?: break
                if (entry.value > time) break
                now = entry.value
                pending.remove(entry.key)
                entry.key.run()
            }
            now = time
        }
    }

    private class FakeWebSocket : WebSocket {
        lateinit var endpoint: Request
        var cancelled = false
        var acceptsMessages = true
        val messages = mutableListOf<ByteString>()
        override fun request() = endpoint
        override fun queueSize() = 0L
        override fun send(text: String) = acceptsMessages
        override fun send(bytes: ByteString): Boolean {
            if (!acceptsMessages) return false
            messages += bytes
            return true
        }
        override fun close(code: Int, reason: String?) = true
        override fun cancel() { cancelled = true }
    }

    private class RecordingListener : HostControlSession.Listener {
        var authentications = 0
        val audioStates = mutableListOf<String>()
        val audioResults = mutableListOf<String>()
        override fun onAudioState(host: ControlledHostSummary, sessionId: String, available: Boolean, muted: Boolean) {
            audioStates += "$sessionId:$available:$muted"
        }
        override fun onAudioMuteResult(host: ControlledHostSummary, sessionId: String, muted: Boolean, error: String?) {
            audioResults += "$sessionId:$muted:$error"
        }
        val failures = mutableListOf<String>()
        override fun onAuthenticated(host: ControlledHostSummary) { authentications++ }
        override fun onFailure(host: ControlledHostSummary, message: String) { failures += message }
        override fun onPairingRequired(host: ControlledHostSummary, message: String, errorCode: String) = Unit
        override fun onPairingChallenge(host: ControlledHostSummary, challengeId: String, challengeType: PairingChallengeType) = Unit
        override fun onPairingSucceeded(host: ControlledHostSummary, pairToken: String) = Unit
        override fun onDeviceSnapshotLoaded(host: ControlledHostSummary) = Unit
        override fun onSessionAccepted(host: ControlledHostSummary, sessionId: String, stateLabel: String) = Unit
        override fun onSessionChanged(host: ControlledHostSummary, sessionId: String, stateLabel: String, summary: String) = Unit
        override fun onWebRtcOffer(host: ControlledHostSummary, offer: WebRtcOffer) = Unit
        override fun onRemoteIceCandidate(host: ControlledHostSummary, candidate: IceCandidate) = Unit
        override fun onHostVideoStream(host: ControlledHostSummary, event: HostVideoStreamEvent) = Unit
        override fun onHostTransportStatus(host: ControlledHostSummary, event: HostTransportStatusEvent) = Unit
        override fun onSessionClosed(host: ControlledHostSummary, sessionId: String, reason: String) = Unit
    }
}

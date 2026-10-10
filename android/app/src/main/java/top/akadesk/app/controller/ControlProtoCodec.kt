package top.akadesk.app.controller

import akadesk.v1.ControlEnvelope
import akadesk.v1.ControlHello
import akadesk.v1.ControlHelloAck
import akadesk.v1.ControlMessageKind
import akadesk.v1.DeviceSnapshotResponse
import akadesk.v1.GetDeviceSnapshot
import akadesk.v1.HeartbeatPong
import akadesk.v1.HeartbeatPing
import akadesk.v1.HostTransportStatusEvent
import akadesk.v1.HostVideoStreamEvent
import akadesk.v1.IceCandidate
import akadesk.v1.PairingChallenge
import akadesk.v1.PairingConfirm
import akadesk.v1.PairingRequest
import akadesk.v1.PairingResult
import akadesk.v1.PlatformKind
import akadesk.v1.SessionChangedEvent
import akadesk.v1.SessionClosedEvent
import akadesk.v1.SessionIntent
import akadesk.v1.StopSessionRequest
import akadesk.v1.StartSessionAccepted
import akadesk.v1.StartSessionRejected
import akadesk.v1.StartSessionRequest
import akadesk.v1.VideoCodec
import akadesk.v1.VideoPreference
import akadesk.v1.VersionInfo
import akadesk.v1.WebRtcAnswer
import akadesk.v1.WebRtcOffer
import com.google.protobuf.ByteString
import com.google.protobuf.MessageLite

object ControlProtoCodec {
    fun buildControlHello(
        requestId: String,
        controllerDeviceId: String,
        pairToken: String?,
    ): ControlEnvelope {
        val payload = ControlHello.newBuilder()
            .setRequestId(requestId)
            .setControllerDeviceId(controllerDeviceId)
            .setPairToken(pairToken.orEmpty())
            .setVersion(defaultVersion())
            .addSupportedFeatures("pairing")
            .build()
        return buildEnvelope(requestId, ControlMessageKind.CONTROL_MESSAGE_KIND_REQUEST, "ControlHello", payload)
    }

    fun buildPairingRequest(
        requestId: String,
        controllerDeviceId: String,
        controllerDisplayName: String,
    ): ControlEnvelope {
        val payload = PairingRequest.newBuilder()
            .setRequestId(requestId)
            .setControllerDeviceId(controllerDeviceId)
            .setControllerDisplayName(controllerDisplayName)
            .setVersion(defaultVersion())
            .build()
        return buildEnvelope(requestId, ControlMessageKind.CONTROL_MESSAGE_KIND_REQUEST, "PairingRequest", payload)
    }

    fun buildPairingConfirm(
        requestId: String,
        challengeId: String,
        verificationCode: String,
    ): ControlEnvelope {
        val payload = PairingConfirm.newBuilder()
            .setRequestId(requestId)
            .setChallengeId(challengeId)
            .setVerificationCode(verificationCode.trim().uppercase())
            .build()
        return buildEnvelope(requestId, ControlMessageKind.CONTROL_MESSAGE_KIND_REQUEST, "PairingConfirm", payload)
    }

    fun buildGetDeviceSnapshot(requestId: String): ControlEnvelope {
        val payload = GetDeviceSnapshot.newBuilder()
            .setRequestId(requestId)
            .build()
        return buildEnvelope(requestId, ControlMessageKind.CONTROL_MESSAGE_KIND_REQUEST, "GetDeviceSnapshot", payload)
    }

    fun buildHeartbeatPing(requestId: String, timestampMs: Long): ControlEnvelope {
        val payload = HeartbeatPing.newBuilder()
            .setRequestId(requestId)
            .setTimestampMs(timestampMs)
            .build()
        return buildEnvelope(requestId, ControlMessageKind.CONTROL_MESSAGE_KIND_EVENT, "HeartbeatPing", payload)
    }

    fun buildStopSessionRequest(requestId: String, sessionId: String): ControlEnvelope {
        val payload = StopSessionRequest.newBuilder()
            .setSessionId(sessionId)
            .build()
        return buildEnvelope(requestId, ControlMessageKind.CONTROL_MESSAGE_KIND_REQUEST, "StopSessionRequest", payload)
    }

    fun buildStartSessionRequest(
        requestId: String,
        controllerDeviceId: String,
        controlledDeviceId: String,
        requestedCodec: VideoCodec?,
    ): ControlEnvelope {
        val videoPreference = VideoPreference.newBuilder()
            .setWidth(960)
            .setHeight(540)
            .setFps(30)
            .setBitrateKbps(2500)
            .setAllowSameCodecSoftwareFallback(true)
        requestedCodec?.let(videoPreference::setCodec)
        val payload = StartSessionRequest.newBuilder()
            .setIntent(
                SessionIntent.newBuilder()
                    .setControllerDeviceId(controllerDeviceId)
                    .setControlledDeviceId(controlledDeviceId)
                    .setEnableInput(false)
                    .setVideo(videoPreference)
            )
            .build()
        return buildEnvelope(requestId, ControlMessageKind.CONTROL_MESSAGE_KIND_REQUEST, "StartSessionRequest", payload)
    }

    fun buildWebRtcAnswer(
        requestId: String,
        sessionId: String,
        sdp: String,
    ): ControlEnvelope {
        val payload = WebRtcAnswer.newBuilder()
            .setSessionId(sessionId)
            .setSdp(sdp)
            .build()
        return buildEnvelope(requestId, ControlMessageKind.CONTROL_MESSAGE_KIND_EVENT, "WebRtcAnswer", payload)
    }

    fun buildIceCandidate(
        requestId: String,
        sessionId: String,
        candidate: String,
        sdpMid: String,
        sdpMlineIndex: Int,
    ): ControlEnvelope {
        val payload = IceCandidate.newBuilder()
            .setSessionId(sessionId)
            .setCandidate(candidate)
            .setSdpMid(sdpMid)
            .setSdpMlineIndex(sdpMlineIndex)
            .build()
        return buildEnvelope(requestId, ControlMessageKind.CONTROL_MESSAGE_KIND_EVENT, "IceCandidate", payload)
    }

    fun decodeEnvelope(bytes: ByteArray): ControlEnvelope = ControlEnvelope.parseFrom(bytes)

    fun decodeControlHelloAck(envelope: ControlEnvelope): ControlHelloAck =
        ControlHelloAck.parseFrom(envelope.payload)

    fun decodePairingChallenge(envelope: ControlEnvelope): PairingChallenge =
        PairingChallenge.parseFrom(envelope.payload)

    fun decodePairingResult(envelope: ControlEnvelope): PairingResult =
        PairingResult.parseFrom(envelope.payload)

    fun decodeDeviceSnapshotResponse(envelope: ControlEnvelope): DeviceSnapshotResponse =
        DeviceSnapshotResponse.parseFrom(envelope.payload)

    fun decodeHeartbeatPong(envelope: ControlEnvelope): HeartbeatPong =
        HeartbeatPong.parseFrom(envelope.payload)

    fun decodeStartSessionAccepted(envelope: ControlEnvelope): StartSessionAccepted =
        StartSessionAccepted.parseFrom(envelope.payload)

    fun decodeStartSessionRejected(envelope: ControlEnvelope): StartSessionRejected =
        StartSessionRejected.parseFrom(envelope.payload)

    fun decodeSessionChangedEvent(envelope: ControlEnvelope): SessionChangedEvent =
        SessionChangedEvent.parseFrom(envelope.payload)

    fun decodeSessionClosedEvent(envelope: ControlEnvelope): SessionClosedEvent =
        SessionClosedEvent.parseFrom(envelope.payload)

    fun decodeWebRtcOffer(envelope: ControlEnvelope): WebRtcOffer =
        WebRtcOffer.parseFrom(envelope.payload)

    fun decodeIceCandidate(envelope: ControlEnvelope): IceCandidate =
        IceCandidate.parseFrom(envelope.payload)

    fun decodeHostVideoStreamEvent(envelope: ControlEnvelope): HostVideoStreamEvent =
        HostVideoStreamEvent.parseFrom(envelope.payload)

    fun decodeHostTransportStatusEvent(envelope: ControlEnvelope): HostTransportStatusEvent =
        HostTransportStatusEvent.parseFrom(envelope.payload)

    private fun defaultVersion(): VersionInfo =
        VersionInfo.newBuilder()
            .setSchemaVersion(1)
            .setAppVersion("1.0")
            .setBuildChannel("debug")
            .setPlatform(PlatformKind.PLATFORM_KIND_ANDROID)
            .build()

    private fun buildEnvelope(
        requestId: String,
        kind: ControlMessageKind,
        type: String,
        payload: MessageLite,
    ): ControlEnvelope {
        return ControlEnvelope.newBuilder()
            .setRequestId(requestId)
            .setKind(kind)
            .setType(type)
            .setPayload(ByteString.copyFrom(payload.toByteArray()))
            .setTimestampMs(System.currentTimeMillis())
            .build()
    }
}

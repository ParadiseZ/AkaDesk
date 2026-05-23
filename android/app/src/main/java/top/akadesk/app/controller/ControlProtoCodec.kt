package top.akadesk.app.controller

import akadesk.v1.ControlEnvelope
import akadesk.v1.ControlHello
import akadesk.v1.ControlHelloAck
import akadesk.v1.ControlMessageKind
import akadesk.v1.DeviceSnapshotResponse
import akadesk.v1.GetDeviceSnapshot
import akadesk.v1.PairingChallenge
import akadesk.v1.PairingConfirm
import akadesk.v1.PairingRequest
import akadesk.v1.PairingResult
import akadesk.v1.PlatformKind
import akadesk.v1.VersionInfo
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

    fun decodeEnvelope(bytes: ByteArray): ControlEnvelope = ControlEnvelope.parseFrom(bytes)

    fun decodeControlHelloAck(envelope: ControlEnvelope): ControlHelloAck =
        ControlHelloAck.parseFrom(envelope.payload)

    fun decodePairingChallenge(envelope: ControlEnvelope): PairingChallenge =
        PairingChallenge.parseFrom(envelope.payload)

    fun decodePairingResult(envelope: ControlEnvelope): PairingResult =
        PairingResult.parseFrom(envelope.payload)

    fun decodeDeviceSnapshotResponse(envelope: ControlEnvelope): DeviceSnapshotResponse =
        DeviceSnapshotResponse.parseFrom(envelope.payload)

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

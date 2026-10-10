package top.akadesk.app.controller

import akadesk.v1.ControlEnvelope
import akadesk.v1.StartSessionRequest
import akadesk.v1.StopSessionRequest
import akadesk.v1.VideoCodec
import org.junit.Assert.*
import org.junit.Test

class ControlProtoCodecTest {
    @Test
    fun sessionRequestUsesFullHostIdentityAndViewOnlyH264() {
        val hostId = "AbCdEf012345GhIjKl678901"
        val wire = ControlProtoCodec.buildStartSessionRequest("start-1", "phone", hostId, VideoCodec.VIDEO_CODEC_H264)
        val decoded = ControlEnvelope.parseFrom(wire.toByteArray())
        val intent = StartSessionRequest.parseFrom(decoded.payload).intent
        assertEquals(hostId, intent.controlledDeviceId)
        assertEquals("phone", intent.controllerDeviceId)
        assertFalse(intent.enableInput)
        assertFalse(intent.hasAudio())
        assertEquals(VideoCodec.VIDEO_CODEC_H264, intent.video.codec)
        assertEquals(960, intent.video.width)
        assertEquals(540, intent.video.height)
    }

    @Test
    fun stopRequestCarriesExactSessionIdentity() {
        val wire = ControlProtoCodec.buildStopSessionRequest("stop-1", "session-current")
        assertEquals("stop-1", wire.requestId)
        assertEquals("StopSessionRequest", wire.type)
        assertEquals("session-current", StopSessionRequest.parseFrom(wire.payload).sessionId)
    }
}

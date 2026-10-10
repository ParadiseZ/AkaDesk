package top.akadesk.app.controller

import akadesk.v1.ControlEnvelope
import akadesk.v1.StartSessionRequest
import akadesk.v1.StopSessionRequest
import akadesk.v1.VideoCodec
import org.junit.Assert.*
import org.junit.Test

class ControlProtoCodecTest {
    @Test
    fun sessionRequestUsesFullHostIdentityAndH264WithInput() {
        val hostId = "AbCdEf012345GhIjKl678901"
        val wire = ControlProtoCodec.buildStartSessionRequest("start-1", "phone", hostId, VideoCodec.VIDEO_CODEC_H264)
        val decoded = ControlEnvelope.parseFrom(wire.toByteArray())
        val intent = StartSessionRequest.parseFrom(decoded.payload).intent
        assertEquals(hostId, intent.controlledDeviceId)
        assertEquals("phone", intent.controllerDeviceId)
        assertTrue(intent.enableInput)
        assertTrue(intent.hasAudio())
        assertEquals(akadesk.v1.AudioCodec.AUDIO_CODEC_OPUS, intent.audio.codec)
        assertEquals(48_000, intent.audio.sampleRate)
        assertEquals(2, intent.audio.channels)
        assertEquals(64, intent.audio.bitrateKbps)
        assertFalse(intent.audio.muteControlledLocalPlayback)
        assertEquals(VideoCodec.VIDEO_CODEC_H264, intent.video.codec)
        assertEquals(960, intent.video.width)
        assertEquals(540, intent.video.height)
    }
    @Test fun viewOnlyHostCanStillBeRequestedWithoutInput() {
        val wire=ControlProtoCodec.buildStartSessionRequest("start","phone","host",VideoCodec.VIDEO_CODEC_H264,false)
        assertFalse(StartSessionRequest.parseFrom(wire.payload).intent.enableInput)
    }

    @Test
    fun stopRequestCarriesExactSessionIdentity() {
        val wire = ControlProtoCodec.buildStopSessionRequest("stop-1", "session-current")
        assertEquals("stop-1", wire.requestId)
        assertEquals("StopSessionRequest", wire.type)
        assertEquals("session-current", StopSessionRequest.parseFrom(wire.payload).sessionId)
    }

    @Test
    fun muteRequestChangesOnlyTheNamedSession() {
        val wire = ControlProtoCodec.buildSetAudioMuteRequest("mute-1", "session-current", true)
        assertEquals("SetAudioMuteRequest", wire.type)
        val request = akadesk.v1.SetAudioMuteRequest.parseFrom(wire.payload)
        assertEquals("session-current", request.sessionId)
        assertTrue(request.muted)
    }
}

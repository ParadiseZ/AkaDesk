package top.akadesk.app.controller

import org.junit.Assert.*
import org.junit.Test

class RemoteIceMediaTest {
    private val sdp = "v=0\r\nm=audio 9 UDP/TLS/RTP/SAVPF 111\r\na=mid:sound\r\nm=video 9 UDP/TLS/RTP/SAVPF 96\r\na=mid:screen\r\n"

    @Test fun missingMidResolvesFromActualMediaSection() {
        assertEquals("screen", RemoteIceMedia.resolveMid(sdp, "", 1))
        assertEquals("sound", RemoteIceMedia.resolveMid(sdp, null, 0))
        assertEquals("screen", RemoteIceMedia.resolveMid(sdp, "screen", 1))
    }

    @Test fun invalidMediaMappingIsRejectedBeforeNativeCall() {
        assertNull(RemoteIceMedia.resolveMid(sdp, "sound", 1))
        assertNull(RemoteIceMedia.resolveMid(sdp, "", -1))
        assertNull(RemoteIceMedia.resolveMid(sdp, "", 2))
        assertNull(RemoteIceMedia.resolveMid("v=0\r\nm=video 9 RTP/AVP 96", "", 0))
    }
}

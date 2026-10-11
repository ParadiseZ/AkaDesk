package top.akadesk.app.controller

import org.junit.Assert.*
import org.junit.Test
import org.webrtc.RTCStats
import org.webrtc.RTCStatsReport

class RemoteNetworkStatsTest {
    private fun report(time:Long,bytes:Long,frames:Long,decode:Double):RTCStatsReport {
        fun stat(id:String,type:String,values:Map<String,Any>)=RTCStats(time,type,id,values)
        return RTCStatsReport(time,mapOf(
            "video" to stat("video","inbound-rtp",mapOf("kind" to "video","bytesReceived" to bytes,"framesDecoded" to frames,"totalDecodeTime" to decode,"packetsReceived" to 990,"packetsLost" to 10,"codecId" to "codec","frameWidth" to 1280,"frameHeight" to 720)),
            "codec" to stat("codec","codec",mapOf("mimeType" to "video/H264")),
            "transport" to stat("transport","transport",mapOf("selectedCandidatePairId" to "pair")),
            "pair" to stat("pair","candidate-pair",mapOf("currentRoundTripTime" to .02,"localCandidateId" to "local","remoteCandidateId" to "remote")),
            "local" to stat("local","local-candidate",mapOf("address" to "192.168.1.2","protocol" to "udp","candidateType" to "host")),
            "remote" to stat("remote","remote-candidate",mapOf("address" to "192.168.1.3","candidateType" to "host")),
        ))
    }
    @Test fun nativeCountersProduceRatesAndResolveTheSelectedCandidatePair() {
        val sampler=RemoteStatsSampler()
        val first=sampler.sample(report(1_000_000,100_000,10,.05))
        assertNull(first.mbps);assertNull(first.decodeMs)
        val second=sampler.sample(report(3_000_000,600_000,110,.55))
        assertEquals(2.0,second.mbps!!,.001);assertEquals(50.0,second.fps!!,.001)
        assertEquals(5.0,second.decodeMs!!,.001);assertEquals(20.0,second.rttMs!!,.001)
        assertEquals(1.0,second.lossPercent!!,.001);assertEquals("video/H264",second.codec)
        assertEquals("1280 × 720",second.resolution);assertEquals("192.168.1.3",second.remoteAddress)
        assertTrue(second.transport.contains("直连"))
    }
    @Test fun missingVideoAndResetCountersDoNotReportNegativeOrInventedRates() {
        val sampler=RemoteStatsSampler()
        sampler.sample(report(1_000_000,100_000,100,1.0))
        val reset=sampler.sample(report(2_000_000,10,1,.01))
        assertNull(reset.mbps);assertNull(reset.fps);assertNull(reset.decodeMs)
        val empty=sampler.sample(RTCStatsReport(3_000_000,emptyMap()))
        assertNull(empty.mbps);assertNull(empty.fps);assertEquals("—",empty.transport)
        assertNull(sampler.sample(report(4_000_000,100_000,100,1.0)).mbps)
    }
}

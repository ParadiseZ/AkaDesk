package top.akadesk.app.controller

import org.junit.Assert.*
import org.junit.Test
import org.webrtc.RTCStats
import org.webrtc.RTCStatsReport

class RemoteNetworkStatsTest {
    @Test fun legacyPlayoutTargetIsIdentifiedAndNeverPresentedAsNetworkRoundTrip() {
        val source=report(1_000_000,100_000,10,.1);val all=source.statsMap.toMutableMap()
        val video=all.getValue("video");all["video"]=RTCStats(1_000_000,"inbound-rtp","video",video.members+mapOf("receiverTargetDelayMs" to 50.0))
        val sampled=RemoteStatsSampler().sample(RTCStatsReport(1_000_000,all))
        assertEquals(50.0,sampled.frameProcessingMs!!,.001);assertEquals(20.0,sampled.rttMs!!,.001)
        assertTrue(sampled.processingTarget);assertFalse(sampled.processingEstimated)
    }
    @Test fun processingDelayUsesNativeCountersOrClearlyMarkedBufferPlusDecodeEstimate() {
        fun enriched(time:Long,processing:Double?):RTCStatsReport {
            val source=report(time,100_000+time/10,time/100_000,time/20_000_000.0)
            val all=source.statsMap.toMutableMap()
            val members=all.getValue("video").members.toMutableMap()
            members["jitterBufferDelay"]=time/10_000_000.0;members["jitterBufferEmittedCount"]=time/100_000
            processing?.let {members["totalProcessingDelay"]=it}
            all["video"]=RTCStats(time,"inbound-rtp","video",members)
            val pair=all.getValue("pair").members.toMutableMap();pair["bytesReceived"]=time/2
            all["pair"]=RTCStats(time,"candidate-pair","pair",pair)
            return RTCStatsReport(time,all)
        }
        val native=RemoteStatsSampler();native.sample(enriched(1_000_000,.2))
        val n=native.sample(enriched(3_000_000,.8))
        assertEquals(30.0,n.frameProcessingMs!!,.001);assertFalse(n.processingEstimated)
        assertEquals(4.0,n.mbps!!,.001) // transport bytes rather than video bytes
        val fallback=RemoteStatsSampler();fallback.sample(enriched(1_000_000,null))
        val f=fallback.sample(enriched(3_000_000,null))
        assertEquals(15.0,f.frameProcessingMs!!,.001);assertTrue(f.processingEstimated)
        assertNull(fallback.sample(report(4_000_000,500_000,50,.5)).mbps) // source changed
    }
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

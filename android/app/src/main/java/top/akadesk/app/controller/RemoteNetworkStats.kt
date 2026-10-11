package top.akadesk.app.controller

data class RemoteNetworkStats(
    val rttMs: Double? = null,
    val frameProcessingMs: Double? = null,
    val processingEstimated:Boolean = false,
    val processingTarget:Boolean = false,
    val decodeMs: Double? = null,
    val fps: Double? = null,
    val mbps: Double? = null,
    val lossPercent: Double? = null,
    val localAddress: String = "—",
    val remoteAddress: String = "—",
    val transport: String = "—",
    val codec: String = "—",
    val decoder: String = "—",
    val resolution: String = "—",
)

class RemoteStatsSampler {
    private var previousTime = 0.0
    private var previousBytes = 0.0
    private var previousFrames = 0.0
    private var previousDecode = 0.0
    private var previousProcessing = 0.0
    private var previousBuffered = 0.0
    private var previousEmitted = 0.0
    private var previousCounterSource:String? = null
    fun sample(report: org.webrtc.RTCStatsReport): RemoteNetworkStats {
        val all = report.statsMap
        val video = all.values.firstOrNull { it.type == "inbound-rtp" && (it.members["kind"] == "video" || it.members["mediaType"] == "video") }
        fun number(map: Map<String, Any>?, key: String): Double? = (map?.get(key) as? Number)?.toDouble()
        val selectedPair=all.values.firstOrNull {it.type=="transport"}?.members?.get("selectedCandidatePairId")?.let {all[it.toString()]}
        val pair = selectedPair ?: all.values.firstOrNull { it.type == "candidate-pair" && it.members["state"] == "succeeded" && it.members["nominated"] == true }
        val local = pair?.members?.get("localCandidateId")?.let { all[it.toString()] }
        val remote = pair?.members?.get("remoteCandidateId")?.let { all[it.toString()] }
        val codec = video?.members?.get("codecId")?.let { all[it.toString()] }
        val counterSource=if(number(pair?.members,"bytesReceived")!=null) pair!!.id else "media"
        val bytes = number(pair?.members,"bytesReceived") ?: all.values.filter {it.type=="inbound-rtp"}.sumOf {number(it.members,"bytesReceived") ?: 0.0}
        val frames = number(video?.members,"framesDecoded") ?: 0.0
        val decoded = number(video?.members,"totalDecodeTime") ?: 0.0
        val processing = number(video?.members,"totalProcessingDelay")
        val targetDelay=number(video?.members,"receiverTargetDelayMs")
        val buffered=number(video?.members,"jitterBufferDelay")
        val emitted=number(video?.members,"jitterBufferEmittedCount")
        val now = report.timestampUs
        val seconds = (now - previousTime) / 1_000_000.0
        val deltaFrames = frames - previousFrames
        val decodeMs=if(video!=null && previousTime>0 && deltaFrames>0 && decoded>=previousDecode) (decoded-previousDecode)*1000/deltaFrames else null
        val bufferMs=if(buffered!=null && emitted!=null && previousTime>0 && emitted>previousEmitted && buffered>=previousBuffered) (buffered-previousBuffered)*1000/(emitted-previousEmitted) else null
        val received = number(video?.members,"packetsReceived") ?: 0.0
        val lost = (number(video?.members,"packetsLost") ?: 0.0).coerceAtLeast(0.0)
        val result = RemoteNetworkStats(
            rttMs=number(pair?.members,"currentRoundTripTime")?.times(1000),
            frameProcessingMs=if(processing!=null && previousTime>0 && deltaFrames>0 && processing>=previousProcessing) (processing-previousProcessing)*1000/deltaFrames else if(processing==null && bufferMs!=null && decodeMs!=null) bufferMs+decodeMs else if(processing==null) targetDelay else null,
            processingEstimated=processing==null && bufferMs!=null && decodeMs!=null,
            processingTarget=processing==null && (bufferMs==null || decodeMs==null) && targetDelay!=null,
            decodeMs=decodeMs,
            fps=number(video?.members,"framesPerSecond")?.takeIf {it>=0} ?: if(video!=null && previousTime>0 && seconds>0 && deltaFrames>=0) deltaFrames/seconds else null,
            mbps=if(previousTime>0 && seconds>0 && counterSource==previousCounterSource && bytes>=previousBytes) (bytes-previousBytes)*8/seconds/1_000_000 else null,
            lossPercent=if(received+lost>0) lost*100/(received+lost) else null,
            localAddress=(local?.members?.get("address") ?: local?.members?.get("ip") ?: "—").toString(),
            remoteAddress=(remote?.members?.get("address") ?: remote?.members?.get("ip") ?: "—").toString(),
            transport=if(pair==null) "—" else "${local?.members?.get("protocol") ?: "UDP"} · ${if(local?.members?.get("candidateType")=="relay" || remote?.members?.get("candidateType")=="relay") "中转" else "直连"} · DTLS/SRTP",
            codec=(codec?.members?.get("mimeType") ?: "—").toString(),
            decoder=(video?.members?.get("decoderImplementation") ?: "—").toString(),
            resolution=if(video==null) "—" else "${video.members["frameWidth"] ?: "—"} × ${video.members["frameHeight"] ?: "—"}",
        )
        previousTime=if(video==null) 0.0 else now; previousBytes=bytes; previousFrames=frames; previousDecode=decoded;previousProcessing=processing ?: 0.0
        previousBuffered=buffered ?: 0.0;previousEmitted=emitted ?: 0.0;previousCounterSource=counterSource
        return result
    }
}

package top.akadesk.app.controller

data class RemoteNetworkStats(
    val rttMs: Double? = null,
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
    fun sample(report: org.webrtc.RTCStatsReport): RemoteNetworkStats {
        val all = report.statsMap
        val video = all.values.firstOrNull { it.type == "inbound-rtp" && (it.members["kind"] == "video" || it.members["mediaType"] == "video") }
        fun number(map: Map<String, Any>?, key: String): Double? = (map?.get(key) as? Number)?.toDouble()
        val selectedPair=all.values.firstOrNull {it.type=="transport"}?.members?.get("selectedCandidatePairId")?.let {all[it.toString()]}
        val pair = selectedPair ?: all.values.firstOrNull { it.type == "candidate-pair" && it.members["state"] == "succeeded" && it.members["nominated"] == true }
        val local = pair?.members?.get("localCandidateId")?.let { all[it.toString()] }
        val remote = pair?.members?.get("remoteCandidateId")?.let { all[it.toString()] }
        val codec = video?.members?.get("codecId")?.let { all[it.toString()] }
        val bytes = number(video?.members,"bytesReceived") ?: 0.0
        val frames = number(video?.members,"framesDecoded") ?: 0.0
        val decoded = number(video?.members,"totalDecodeTime") ?: 0.0
        val now = report.timestampUs
        val seconds = (now - previousTime) / 1_000_000.0
        val deltaFrames = frames - previousFrames
        val received = number(video?.members,"packetsReceived") ?: 0.0
        val lost = (number(video?.members,"packetsLost") ?: 0.0).coerceAtLeast(0.0)
        val result = RemoteNetworkStats(
            rttMs=number(pair?.members,"currentRoundTripTime")?.times(1000),
            decodeMs=if(video!=null && previousTime>0 && deltaFrames>0 && decoded>=previousDecode) (decoded-previousDecode)*1000/deltaFrames else null,
            fps=number(video?.members,"framesPerSecond")?.takeIf {it>=0} ?: if(video!=null && previousTime>0 && seconds>0 && deltaFrames>=0) deltaFrames/seconds else null,
            mbps=if(previousTime>0 && seconds>0 && bytes>=previousBytes) (bytes-previousBytes)*8/seconds/1_000_000 else null,
            lossPercent=if(received+lost>0) lost*100/(received+lost) else null,
            localAddress=(local?.members?.get("address") ?: local?.members?.get("ip") ?: "—").toString(),
            remoteAddress=(remote?.members?.get("address") ?: remote?.members?.get("ip") ?: "—").toString(),
            transport=if(pair==null) "—" else "${local?.members?.get("protocol") ?: "UDP"} · ${if(local?.members?.get("candidateType")=="relay" || remote?.members?.get("candidateType")=="relay") "中转" else "直连"} · DTLS/SRTP",
            codec=(codec?.members?.get("mimeType") ?: "—").toString(),
            decoder=(video?.members?.get("decoderImplementation") ?: "—").toString(),
            resolution=if(video==null) "—" else "${video.members["frameWidth"] ?: "—"} × ${video.members["frameHeight"] ?: "—"}",
        )
        previousTime=if(video==null) 0.0 else now; previousBytes=bytes; previousFrames=frames; previousDecode=decoded
        return result
    }
}

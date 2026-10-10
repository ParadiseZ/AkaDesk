package top.akadesk.app.controller

/** Resolve the media section before passing a candidate to native WebRTC. */
internal object RemoteIceMedia {
    fun resolveMid(sdp: String, suppliedMid: String?, mediaIndex: Int): String? {
        if (mediaIndex < 0) return null
        val mediaMids = mutableListOf<String?>()
        sdp.lineSequence().forEach { raw ->
            val line = raw.trim()
            if (line.startsWith("m=")) mediaMids.add(null)
            if (line.startsWith("a=mid:") && mediaMids.isNotEmpty()) {
                mediaMids[mediaMids.lastIndex] = line.removePrefix("a=mid:").trim().ifBlank { null }
            }
        }
        val negotiatedMid = mediaMids.getOrNull(mediaIndex) ?: return null
        return negotiatedMid.takeIf { suppliedMid.isNullOrBlank() || suppliedMid == it }
    }
}

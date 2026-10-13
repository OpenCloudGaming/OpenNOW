package com.opencloudgaming.opennow

/** Maps the media timeline to SurfaceView's monotonic clock without accumulating latency. */
internal class SurfacePresentationClock(fps: Int) {
    private val frameNs = 1_000_000_000L / fps.coerceAtLeast(1)
    private var mediaAnchorNs = 0L
    private var localAnchorNs = 0L
    private var lastMediaNs: Long? = null

    fun next(mediaNs: Long, nowNs: Long): Long {
        val previous = lastMediaNs
        val target = localAnchorNs + (mediaNs - mediaAnchorNs)
        // One frame of headroom absorbs callback jitter. A pause, a backward timestamp,
        // or a burst outside two frames starts a fresh timeline rather than building a queue.
        val reset = previous == null || mediaNs <= previous || target < nowNs ||
            target - nowNs > 2 * frameNs
        lastMediaNs = mediaNs
        if (reset) {
            mediaAnchorNs = mediaNs
            localAnchorNs = nowNs + frameNs
            return localAnchorNs
        }
        return target
    }

    fun reset() { lastMediaNs = null }
}

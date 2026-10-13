package com.opencloudgaming.opennow

/** Codec priming and WebRTC catch-up arrive in bursts, not at the negotiated frame interval. */
internal class SurfaceDecoderQueuePolicy(fps: Int) {
    private val maximumFrames = maxOf(32, (fps.coerceAtLeast(1) + 1) / 2)

    fun failure(inFlightFrames: Int, queuedBytes: Long, oldestAgeMs: Long, priming: Boolean): String? = when {
        queuedBytes > MAX_QUEUED_BYTES -> "surface queue memory limit bytes=$queuedBytes"
        inFlightFrames >= maximumFrames -> "surface queue frame limit frames=$inFlightFrames limit=$maximumFrames"
        inFlightFrames > 0 && oldestAgeMs > if (priming) STARTUP_TIMEOUT_MS else ACTIVE_TIMEOUT_MS ->
            "surface queue stalled ageMs=$oldestAgeMs priming=$priming frames=$inFlightFrames"
        else -> null
    }

    private companion object {
        const val MAX_QUEUED_BYTES = 16L * 1024 * 1024
        const val STARTUP_TIMEOUT_MS = 1000L
        const val ACTIVE_TIMEOUT_MS = 250L
    }
}

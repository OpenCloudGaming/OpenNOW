package com.opencloudgaming.opennow

/** Shared server policy for WebRTC and native NVST on every Android device. */
internal object StreamNetworkAdaptation {
    // Keep the encoded profile fixed. Enabling NVIDIA's dynamic mode caused post-1.4.9
    // resolution changes and unstable frame delivery even when the client requested 60 FPS.
    const val DYNAMIC_STREAMING_MODE = 0
    const val DYNAMIC_RESOLUTION_CONTROL = 0

    fun bitrateRange(maxBitrateMbps: Double): StreamBitrateRange {
        val maximum = StreamBitrate.maximumKbps(maxBitrateMbps)
        // Recommended profiles may adapt down to 5 Mbps without remaining visibly
        // over-compressed. Explicit manual profiles below 5 Mbps still keep their selected cap
        // instead of being raised above it.
        val minimum = minOf(5_000, maximum)
        val initial = maxOf(minimum, maximum / 4)
        return StreamBitrateRange(minimum, initial, maximum)
    }
}

internal data class StreamBitrateRange(
    val minimumKbps: Int,
    val initialKbps: Int,
    val maximumKbps: Int,
)

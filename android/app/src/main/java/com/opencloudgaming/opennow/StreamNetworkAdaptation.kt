package com.opencloudgaming.opennow

/** Shared server policy for WebRTC and native NVST on every Android device. */
internal object StreamNetworkAdaptation {
    // Keep the encoded profile fixed. Enabling NVIDIA's dynamic mode caused post-1.4.9
    // resolution changes and unstable frame delivery even when the client requested 60 FPS.
    const val DYNAMIC_STREAMING_MODE = 0
    const val DYNAMIC_RESOLUTION_CONTROL = 0

    fun bitrateRange(maxBitrateMbps: Int): StreamBitrateRange {
        val maximum = maxBitrateMbps.coerceIn(1, 200) * 1000
        // Preserve the normal NVIDIA 4 Mbps floor so a standard profile does not start at
        // one quarter of the user's limit and remain visibly over-compressed. Explicit 1-3
        // Mbps profiles still keep their selected cap instead of being raised above it.
        val minimum = minOf(4_000, maximum)
        val initial = maxOf(minimum, maximum / 4)
        return StreamBitrateRange(minimum, initial, maximum)
    }
}

internal data class StreamBitrateRange(
    val minimumKbps: Int,
    val initialKbps: Int,
    val maximumKbps: Int,
)

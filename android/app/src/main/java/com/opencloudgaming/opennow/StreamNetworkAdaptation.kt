package com.opencloudgaming.opennow

/** Shared server policy for WebRTC and native NVST on every Android device. */
internal object StreamNetworkAdaptation {
    // Keep the encoded profile fixed. Enabling NVIDIA's dynamic mode caused post-1.4.9
    // resolution changes and unstable frame delivery even when the client requested 60 FPS.
    const val DYNAMIC_STREAMING_MODE = 0
    const val DYNAMIC_RESOLUTION_CONTROL = 0

    fun bitrateRange(maxBitrateMbps: Int): StreamBitrateRange {
        val maximum = maxBitrateMbps.coerceIn(1, 200) * 1000
        // A maximum is a ceiling, not a target or a minimum. Even the 1-3 Mbps profiles
        // need room to back off when packets queue up. Start conservatively and let BWE
        // increase quality up to the user's ceiling as the path permits.
        val minimum = minOf(1000, maximum / 4)
        return StreamBitrateRange(minimum, maximum / 4, maximum)
    }
}

internal data class StreamBitrateRange(
    val minimumKbps: Int,
    val initialKbps: Int,
    val maximumKbps: Int,
)

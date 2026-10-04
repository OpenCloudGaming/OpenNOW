package com.opencloudgaming.opennow

/** Shared server policy for WebRTC and native NVST on every Android device. */
internal object StreamNetworkAdaptation {
    // Dynamic resolution previously caused unstable frame delivery. Keep it opt-in.
    fun dynamicStreamingMode(settings: StreamSettings): Int =
        if (settings.experimentalDynamicNetworkAdjustment) 1 else 0

    fun dynamicResolutionControl(settings: StreamSettings): Int = dynamicStreamingMode(settings)

    fun bitrateRange(settings: StreamSettings): StreamBitrateRange {
        val standard = bitrateRange(settings.maxBitrateMbps)
        if (!settings.experimentalDynamicNetworkAdjustment) return standard
        val minimum = settings.experimentalDynamicMinimumBitrateMbps
            .coerceIn(1, standard.maximumKbps / 1000) * 1000
        return standard.copy(
            minimumKbps = minimum,
            initialKbps = maxOf(minimum, standard.initialKbps),
        )
    }

    fun bitrateRange(maxBitrateMbps: Int): StreamBitrateRange {
        val maximum = maxBitrateMbps.coerceIn(1, 200) * 1000
        // Recommended profiles may adapt down to 5 Mbps without remaining visibly
        // over-compressed. Explicit manual 1-4 Mbps profiles still keep their selected cap
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

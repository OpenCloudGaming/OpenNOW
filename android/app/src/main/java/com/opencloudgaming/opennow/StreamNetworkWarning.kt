package com.opencloudgaming.opennow

import java.util.Locale

internal data class StreamNetworkWarning(
    val key: String,
    val message: String,
)

/**
 * Turns only direct network measurements into an in-stream warning. Decoder FPS is deliberately
 * excluded as a trigger because a slow device is not evidence of a bad connection.
 */
internal fun streamNetworkWarning(
    stats: StreamRuntimeStats,
): StreamNetworkWarning? {
    val reasons = buildList {
        stats.pingMs
            ?.takeIf { it >= 0 && StreamQuality.latency(it) == StreamQualityLevel.Poor }
            ?.let { add("latency" to "$it ms latency") }

        val packetDeltaIsUsable = stats.packetsLostDelta != null &&
            stats.packetsReceivedDelta != null &&
            stats.packetsReceivedDelta >= 0L &&
            stats.packetsLostDelta + stats.packetsReceivedDelta > 0L
        stats.packetLossPct
            ?.takeIf {
                packetDeltaIsUsable &&
                    it >= 0.0 &&
                    StreamQuality.packetLoss(it) == StreamQualityLevel.Poor
            }
            ?.let { add("loss" to "${"%.2f".format(Locale.US, it)}% packet loss") }

        stats.jitterMs
            ?.takeIf { it >= 0.0 && StreamQuality.jitter(it) == StreamQualityLevel.Poor }
            ?.let { add("jitter" to "${"%.1f".format(Locale.US, it)} ms jitter") }
    }
    if (reasons.isEmpty()) return null

    val measured = reasons.map { it.second }
    return StreamNetworkWarning(
        key = reasons.map { it.first }.sorted().joinToString("+"),
        message = buildString {
            append(measured.joinToString(" · "))
            append(". You may experience lag due to your internet connection.")
        },
    )
}

/** Requires a sustained problem and shows at most one connection banner per stream session. */
internal class StreamNetworkWarningGate(
    private val minimumConsecutiveSamples: Int = 3,
) {
    private var candidateKey: String? = null
    private var consecutiveSamples = 0
    private var warningShown = false

    init {
        require(minimumConsecutiveSamples > 0)
    }

    fun update(candidate: StreamNetworkWarning?): StreamNetworkWarning? {
        if (candidate == null) {
            candidateKey = null
            consecutiveSamples = 0
            return null
        }

        if (candidate.key == candidateKey) {
            consecutiveSamples += 1
        } else {
            candidateKey = candidate.key
            consecutiveSamples = 1
        }
        if (consecutiveSamples < minimumConsecutiveSamples) return null
        if (warningShown) return null
        warningShown = true
        return candidate
    }
}

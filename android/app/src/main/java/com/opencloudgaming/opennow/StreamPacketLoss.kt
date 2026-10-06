package com.opencloudgaming.opennow

import java.util.ArrayDeque

internal data class StreamPacketDelta(
    val lost: Long,
    val received: Long,
)

/** Drops delayed WebRTC stats callbacks before they can rewind cumulative counters or windows. */
internal fun isNewerStreamStatsSample(currentTimestampMs: Double, previousTimestampMs: Double?): Boolean =
    currentTimestampMs.isFinite() &&
        (previousTimestampMs == null || currentTimestampMs > previousTimestampMs)

/**
 * Received packets must increase within the same RTP stream (identity is checked by the caller).
 * RFC3550 cumulative loss is signed: late/duplicate packets can reduce it without a stream reset.
 * Preserve that correction so the rolling loss window reports net loss, rather than resetting.
 */
internal fun streamPacketDelta(
    currentLost: Long,
    currentReceived: Long,
    previousLost: Long,
    previousReceived: Long,
): StreamPacketDelta? {
    if (currentReceived < previousReceived) return null
    return StreamPacketDelta(
        lost = currentLost - previousLost,
        received = currentReceived - previousReceived,
    )
}

/**
 * Smooths the one-second WebRTC packet counters into a short rolling measurement. A sparse delta
 * can otherwise make the overlay jump from 0% to 50% for one sample; three samples are enough to
 * avoid that misleading flash while five seconds remains responsive to a real network problem.
 */
internal class StreamPacketLossWindow(
    private val maximumSamples: Int = 5,
    private val minimumSamples: Int = 3,
) {
    private val samples = ArrayDeque<StreamPacketDelta>()

    init {
        require(maximumSamples > 0)
        require(minimumSamples in 1..maximumSamples)
    }

    fun add(delta: StreamPacketDelta): Double? {
        samples.addLast(delta)
        while (samples.size > maximumSamples) samples.removeFirst()
        if (samples.size < minimumSamples) return null

        val lost = samples.sumOf(StreamPacketDelta::lost)
        val received = samples.sumOf(StreamPacketDelta::received)
        val total = lost + received
        return if (total > 0L) {
            (lost.toDouble() / total.toDouble() * 100.0).coerceIn(0.0, 100.0)
        } else {
            0.0
        }
    }

    fun reset() {
        samples.clear()
    }
}

/**
 * Requests one clean frame after a sustained raw-loss burst ends.
 *
 * This deliberately does not change stream settings or restart transport. Waiting for a healthy
 * sample avoids adding a large keyframe while the lossy path is still congested.
 */
internal class StreamPacketLossRecoveryGate(
    private val badSamplesBeforeArmed: Int = 2,
    private val lossThresholdPct: Double = 5.0,
    private val minimumPacketSample: Long = 100L,
    private val cooldownSamples: Int = 15,
) {
    private var consecutiveBadSamples = 0
    private var recoveryArmed = false
    private var remainingCooldownSamples = 0

    init {
        require(badSamplesBeforeArmed > 0)
        require(lossThresholdPct in 0.0..100.0)
        require(minimumPacketSample > 0L)
        require(cooldownSamples >= 0)
    }

    fun reset() {
        consecutiveBadSamples = 0
        recoveryArmed = false
        remainingCooldownSamples = 0
    }

    fun observe(stats: StreamRuntimeStats, recoveryEligible: Boolean): Boolean {
        if (!recoveryEligible) {
            consecutiveBadSamples = 0
            recoveryArmed = false
            return false
        }

        // A negative signed-loss delta corrects earlier missing packets; it is a healthy sample,
        // not an invalid sample or a new burst. The displayed window keeps the signed correction.
        val lost = stats.packetsLostDelta?.coerceAtLeast(0L) ?: return false
        val received = stats.packetsReceivedDelta?.takeIf { it >= 0L } ?: return false
        val total = lost + received
        if (total < minimumPacketSample) return false
        if (remainingCooldownSamples > 0) remainingCooldownSamples -= 1

        val rawLossPct = lost.toDouble() / total.toDouble() * 100.0
        if (rawLossPct >= lossThresholdPct) {
            consecutiveBadSamples += 1
            if (consecutiveBadSamples >= badSamplesBeforeArmed) recoveryArmed = true
            return false
        }

        consecutiveBadSamples = 0
        if (!recoveryArmed) return false
        recoveryArmed = false
        if (remainingCooldownSamples > 0) return false
        remainingCooldownSamples = cooldownSamples
        return true
    }
}

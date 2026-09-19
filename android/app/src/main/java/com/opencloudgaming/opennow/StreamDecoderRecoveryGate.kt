package com.opencloudgaming.opennow

/**
 * Detects a sustained local decoder bottleneck without confusing it with a slow cloud game or a
 * network drop. The transport must keep delivering near the requested frame rate while decoder
 * output falls materially behind and consumes more than one frame budget. A positive result is a
 * one-shot diagnostic finding; callers must not tear down healthy media merely to retry the same
 * selected codec and profile.
 */
internal class StreamDecoderRecoveryGate(
    private val badSamplesBeforeRecovery: Int = DEFAULT_BAD_SAMPLES_BEFORE_RECOVERY,
    private val minimumReceivedRatio: Double = DEFAULT_MINIMUM_RECEIVED_RATIO,
    private val maximumDecodedRatio: Double = DEFAULT_MAXIMUM_DECODED_RATIO,
    private val minimumDecodeBudgetRatio: Double = DEFAULT_MINIMUM_DECODE_BUDGET_RATIO,
) {
    private var badSamples = 0
    private var recoveryIssued = false

    init {
        require(badSamplesBeforeRecovery > 0)
        require(minimumReceivedRatio in 0.0..1.0)
        require(maximumDecodedRatio in 0.0..1.0)
        require(minimumDecodeBudgetRatio > 0.0)
    }

    fun reset() {
        badSamples = 0
        recoveryIssued = false
    }

    fun observe(
        stats: StreamRuntimeStats,
        requestedFps: Int,
        advancedCodecActive: Boolean,
        recoveryEligible: Boolean,
    ): Boolean {
        if (!advancedCodecActive || !recoveryEligible || recoveryIssued) {
            badSamples = 0
            return false
        }

        val overloaded = isDecoderOverloadSample(
            stats = stats,
            requestedFps = requestedFps,
            minimumReceivedRatio = minimumReceivedRatio,
            maximumDecodedRatio = maximumDecodedRatio,
            minimumDecodeBudgetRatio = minimumDecodeBudgetRatio,
        )

        badSamples = if (overloaded) badSamples + 1 else 0
        if (badSamples < badSamplesBeforeRecovery) return false

        badSamples = 0
        recoveryIssued = true
        return true
    }

    private companion object {
        const val DEFAULT_BAD_SAMPLES_BEFORE_RECOVERY = 5
        const val DEFAULT_MINIMUM_RECEIVED_RATIO = 0.85
        const val DEFAULT_MAXIMUM_DECODED_RATIO = 0.80
        const val DEFAULT_MINIMUM_DECODE_BUDGET_RATIO = 1.10
    }
}

internal fun isDecoderOverloadSample(
    stats: StreamRuntimeStats,
    requestedFps: Int,
    minimumReceivedRatio: Double = 0.85,
    maximumDecodedRatio: Double = 0.80,
    minimumDecodeBudgetRatio: Double = 1.10,
): Boolean {
    val receivedFps = stats.receivedFps ?: return false
    val decodedFps = stats.decodedFps ?: return false
    val decodeMs = stats.decodeMs ?: return false
    val frameBudgetMs = 1_000.0 / requestedFps.coerceAtLeast(1)
    return receivedFps >= requestedFps * minimumReceivedRatio &&
        decodedFps <= receivedFps * maximumDecodedRatio &&
        decodeMs >= frameBudgetMs * minimumDecodeBudgetRatio
}

package com.opencloudgaming.opennow

import kotlinx.serialization.Serializable

internal const val DIAGNOSTIC_STREAM_SAMPLE_LIMIT = 600

@Serializable
internal data class DiagnosticStreamSample(val capturedAtEpochMs: Long, val stats: StreamRuntimeStats)

internal data class DiagnosticStreamSnapshot(val samples: List<DiagnosticStreamSample>, val totalSamples: Long, val capacity: Int) {
    val evictedSamples: Long get() = totalSamples - samples.size
}

/** Keep the original media samples; no additional sampling, services or string formatting. */
internal class DiagnosticStreamHistory(private val capacity: Int = DIAGNOSTIC_STREAM_SAMPLE_LIMIT) {
    private val samples = ArrayDeque<DiagnosticStreamSample>()
    private var totalSamples = 0L
    init { require(capacity > 0) }

    @Synchronized
    fun record(timestampMs: Long, stats: StreamRuntimeStats) {
        samples.addLast(DiagnosticStreamSample(timestampMs, stats.finiteDiagnosticValues()))
        totalSamples++
        if (samples.size > capacity) samples.removeFirst()
    }

    @Synchronized
    fun clear() { samples.clear(); totalSamples = 0 }

    @Synchronized
    fun capture() = DiagnosticStreamSnapshot(samples.toList(), totalSamples, capacity)
}

internal fun StreamRuntimeStats.finiteDiagnosticValues(): StreamRuntimeStats = copy(
    decodeMs = decodeMs?.takeIf(Double::isFinite),
    jitterMs = jitterMs?.takeIf(Double::isFinite),
    packetLossPct = packetLossPct?.takeIf(Double::isFinite),
    processCpuPercent = processCpuPercent?.takeIf(Double::isFinite),
    deviceCpuCapacityPercent = deviceCpuCapacityPercent?.takeIf(Double::isFinite),
)

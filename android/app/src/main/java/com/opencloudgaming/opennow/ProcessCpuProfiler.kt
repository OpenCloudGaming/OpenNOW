package com.opencloudgaming.opennow

import android.os.Process
import android.os.SystemClock
import java.util.Locale
import kotlinx.serialization.Serializable

private const val PROCESS_CPU_PROFILE_MAX_SAMPLES = 180

@Serializable
internal data class ProcessCpuUsageSample(
    val capturedAtElapsedRealtimeMs: Long,
    val windowMs: Long,
    val processCpuPercent: Double,
    val deviceCpuCapacityPercent: Double,
    val logicalCoreCount: Int,
)

/**
 * Samples CPU time consumed by the whole app process, including native/WebRTC threads.
 * [processCpuPercent] is expressed in logical-core equivalents and may exceed 100%;
 * [deviceCpuCapacityPercent] normalizes the same value across the device's logical cores.
 */
internal class ProcessCpuSampler(
    private val processCpuTimeMs: () -> Long = Process::getElapsedCpuTime,
    private val elapsedRealtimeMs: () -> Long = SystemClock::elapsedRealtime,
    private val logicalCoreCount: Int = Runtime.getRuntime().availableProcessors().coerceAtLeast(1),
) {
    private var previousProcessCpuTimeMs: Long? = null
    private var previousElapsedRealtimeMs: Long? = null

    @Synchronized
    fun reset() {
        previousProcessCpuTimeMs = null
        previousElapsedRealtimeMs = null
    }

    @Synchronized
    fun sample(): ProcessCpuUsageSample? {
        val currentProcessCpuTimeMs = processCpuTimeMs()
        val currentElapsedRealtimeMs = elapsedRealtimeMs()
        val previousCpu = previousProcessCpuTimeMs
        val previousElapsed = previousElapsedRealtimeMs
        previousProcessCpuTimeMs = currentProcessCpuTimeMs
        previousElapsedRealtimeMs = currentElapsedRealtimeMs

        if (previousCpu == null || previousElapsed == null) return null
        val cpuDeltaMs = currentProcessCpuTimeMs - previousCpu
        val elapsedDeltaMs = currentElapsedRealtimeMs - previousElapsed
        if (cpuDeltaMs < 0L || elapsedDeltaMs <= 0L) return null

        val maximumProcessPercent = logicalCoreCount * 100.0
        val processPercent = (cpuDeltaMs * 100.0 / elapsedDeltaMs)
            .coerceIn(0.0, maximumProcessPercent)
        return ProcessCpuUsageSample(
            capturedAtElapsedRealtimeMs = currentElapsedRealtimeMs,
            windowMs = elapsedDeltaMs,
            processCpuPercent = processPercent,
            deviceCpuCapacityPercent = (processPercent / logicalCoreCount).coerceIn(0.0, 100.0),
            logicalCoreCount = logicalCoreCount,
        )
    }
}

internal class ProcessCpuProfileBuffer(
    private val maxSamples: Int = PROCESS_CPU_PROFILE_MAX_SAMPLES,
) {
    init {
        require(maxSamples > 0)
    }

    private val samples = ArrayDeque<ProcessCpuUsageSample>()

    @Synchronized
    fun reset() {
        samples.clear()
    }

    @Synchronized
    fun record(sample: ProcessCpuUsageSample) {
        samples.addLast(sample)
        while (samples.size > maxSamples) {
            samples.removeFirst()
        }
    }

    @Synchronized
    fun capture(): List<ProcessCpuUsageSample> = samples.toList()

    fun snapshot(anchor: DiagnosticTimeAnchor? = null): String = formatCpuProfile(capture(), anchor)
}

internal fun formatCpuProfile(current: List<ProcessCpuUsageSample>, anchor: DiagnosticTimeAnchor? = null): String {
    if (current.isEmpty()) return "cpu.profile=empty"
    val averageProcessPercent = current.map { it.processCpuPercent }.average()
    val peakProcessPercent = current.maxOf { it.processCpuPercent }
    val averageDevicePercent = current.map { it.deviceCpuCapacityPercent }.average()
    val peakDevicePercent = current.maxOf { it.deviceCpuCapacityPercent }
    return buildString {
        appendLine(
            "cpu.profile samples=${current.size} cores=${current.last().logicalCoreCount} " +
                "processAvgPct=${averageProcessPercent.cpuPercent()} processPeakPct=${peakProcessPercent.cpuPercent()} " +
                "deviceCapacityAvgPct=${averageDevicePercent.cpuPercent()} deviceCapacityPeakPct=${peakDevicePercent.cpuPercent()}",
        )
        appendLine("cpu.profile.basis=process CPU time divided by wall time; processPct may exceed 100; deviceCapacityPct is normalized by logical cores")
        cpuDiagnosticBuckets(current).forEachIndexed { index, bucket ->
            val timestamp = anchor?.formatElapsed(bucket.endElapsedMs) ?: "uptimeMs=${bucket.endElapsedMs}"
            appendLine(
                "cpu.${index + 1} $timestamp samples=${bucket.samples} " +
                    "processAvgPct=${bucket.processAvgPct.cpuPercent()} processPeakPct=${bucket.processPeakPct.cpuPercent()} " +
                    "deviceCapacityAvgPct=${bucket.deviceCapacityAvgPct.cpuPercent()} deviceCapacityPeakPct=${bucket.deviceCapacityPeakPct.cpuPercent()}",
            )
        }
    }.trimEnd()
}

@Serializable
internal data class CpuDiagnosticBucket(
    val startElapsedMs: Long,
    val endElapsedMs: Long,
    val samples: Int,
    val processAvgPct: Double,
    val processPeakPct: Double,
    val deviceCapacityAvgPct: Double,
    val deviceCapacityPeakPct: Double,
)

internal fun cpuDiagnosticBuckets(samples: List<ProcessCpuUsageSample>): List<CpuDiagnosticBucket> =
    samples.groupBy { it.capturedAtElapsedRealtimeMs / 10_000L }.values.map { window ->
        CpuDiagnosticBucket(
            startElapsedMs = window.first().capturedAtElapsedRealtimeMs,
            endElapsedMs = window.last().capturedAtElapsedRealtimeMs,
            samples = window.size,
            processAvgPct = window.sumOf { it.processCpuPercent * it.windowMs } / window.sumOf { it.windowMs }.coerceAtLeast(1L),
            processPeakPct = window.maxOf { it.processCpuPercent },
            deviceCapacityAvgPct = window.sumOf { it.deviceCpuCapacityPercent * it.windowMs } / window.sumOf { it.windowMs }.coerceAtLeast(1L),
            deviceCapacityPeakPct = window.maxOf { it.deviceCpuCapacityPercent },
        )
    }

internal object ProcessCpuDiagnostics {
    private val profile = ProcessCpuProfileBuffer()

    fun beginStream() {
        profile.reset()
    }

    fun record(sample: ProcessCpuUsageSample) {
        profile.record(sample)
    }

    fun capture(): List<ProcessCpuUsageSample> = profile.capture()

    fun snapshot(): String = profile.snapshot(DiagnosticTimeAnchor(System.currentTimeMillis(), SystemClock.elapsedRealtime()))
}

private fun Double.cpuPercent(): String = "%.1f".format(Locale.US, this)

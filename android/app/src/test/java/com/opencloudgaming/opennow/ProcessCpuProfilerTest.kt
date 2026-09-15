package com.opencloudgaming.opennow

import org.junit.Assert.assertEquals
import org.junit.Assert.assertNull
import org.junit.Assert.assertTrue
import org.junit.Test

class ProcessCpuProfilerTest {
    @Test
    fun reportsProcessAndDeviceNormalizedCpuFromDeltas() {
        val cpuTimes = ArrayDeque(listOf(500L, 750L))
        val elapsedTimes = ArrayDeque(listOf(1_000L, 2_000L))
        val sampler = ProcessCpuSampler(
            processCpuTimeMs = { cpuTimes.removeFirst() },
            elapsedRealtimeMs = { elapsedTimes.removeFirst() },
            logicalCoreCount = 8,
        )

        assertNull(sampler.sample())
        val sample = requireNotNull(sampler.sample())

        assertEquals(1_000L, sample.windowMs)
        assertEquals(25.0, sample.processCpuPercent, 0.0001)
        assertEquals(3.125, sample.deviceCpuCapacityPercent, 0.0001)
        assertEquals(8, sample.logicalCoreCount)
    }

    @Test
    fun rejectsInvalidClockDeltasAndUsesNextSampleAsNewBaseline() {
        val cpuTimes = ArrayDeque(listOf(500L, 400L, 500L))
        val elapsedTimes = ArrayDeque(listOf(1_000L, 2_000L, 3_000L))
        val sampler = ProcessCpuSampler(
            processCpuTimeMs = { cpuTimes.removeFirst() },
            elapsedRealtimeMs = { elapsedTimes.removeFirst() },
            logicalCoreCount = 4,
        )

        assertNull(sampler.sample())
        assertNull(sampler.sample())
        assertEquals(10.0, requireNotNull(sampler.sample()).processCpuPercent, 0.0001)
    }

    @Test
    fun profileBufferKeepsBoundedSamplesAndSummarizesThem() {
        val profile = ProcessCpuProfileBuffer(maxSamples = 2)
        profile.record(ProcessCpuUsageSample(1_000L, 1_000L, 20.0, 5.0, 4))
        profile.record(ProcessCpuUsageSample(2_000L, 1_000L, 40.0, 10.0, 4))
        profile.record(ProcessCpuUsageSample(3_000L, 1_000L, 60.0, 15.0, 4))

        val snapshot = profile.snapshot()

        assertTrue(snapshot.contains("samples=2"))
        assertTrue(snapshot.contains("processAvgPct=50.0"))
        assertTrue(snapshot.contains("processPeakPct=60.0"))
        assertTrue(snapshot.contains("cpu.1 uptimeMs=3000 samples=2"))
        assertEquals(1, cpuDiagnosticBuckets(profile.capture()).size)
    }

    @Test
    fun tenSecondBucketsKeepBriefCpuPeaks() {
        val samples = (1L..180L).map { second ->
            ProcessCpuUsageSample(second * 1_000L, 1_000L, if (second == 45L) 600.0 else 80.0, if (second == 45L) 75.0 else 10.0, 8)
        }
        val buckets = cpuDiagnosticBuckets(samples)
        assertTrue(buckets.size <= 19)
        assertEquals(180, buckets.sumOf { it.samples })
        assertEquals(600.0, buckets.maxOf { it.processPeakPct }, 0.001)
        assertEquals(75.0, buckets.maxOf { it.deviceCapacityPeakPct }, 0.001)
    }
}

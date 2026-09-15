package com.opencloudgaming.opennow

import org.junit.Assert.*
import org.junit.Test

class DiagnosticStreamHistoryTest {
    @Test
    fun originalSampleValuesAndChangesSurviveUntilExplicitEviction() {
        val history = DiagnosticStreamHistory(2)
        val first = StreamRuntimeStats(pingMs = 30, fps = 60, receivedFps = 60, decodedFps = 60,
            packetLossPct = 0.2, decodeMs = 8.234, processCpuPercent = 121.5)
        history.record(1000, first)
        val captured = history.capture()
        history.record(2000, first.copy(fps = 25, receivedFps = 30))
        history.record(3000, first.copy(pingMs = 100))
        assertEquals(first, captured.samples.single().stats)
        val current = history.capture()
        assertEquals(3L, current.totalSamples)
        assertEquals(1L, current.evictedSamples)
        assertEquals(listOf(2000L, 3000L), current.samples.map { it.capturedAtEpochMs })
        assertEquals(25, current.samples.first().stats.fps)
        history.clear()
        assertEquals(0L, history.capture().totalSamples)
        assertTrue(history.capture().samples.isEmpty())
    }
}

package com.opencloudgaming.opennow

import org.junit.Assert.*
import org.junit.Test

class HdrFrameFlowTest {
    private val start = 1_000_000_000L
    @Test fun tracesAnInputGapWithoutBlamingFastDecoder() {
        val logs = mutableListOf<String>()
        val flow = HdrFrameFlow(120) { _, message -> logs += message }
        val a = flow.input(start, 10_000_000)
        flow.submitted(a, start + 100_000)
        flow.output(a, start + 1_000_000)
        a.sink(start + 1_100_000)
        a.released(start + 1_100_000, start + 1_200_000, start + 20_000_000, true)
        val b = flow.input(start + 80_000_000, 18_333_333)
        flow.submitted(b, start + 80_100_000)
        flow.output(b, start + 81_000_000)
        b.sink(start + 81_100_000)
        b.released(start + 81_100_000, start + 81_200_000, start + 95_000_000, true)
        assertTrue(logs.any { it.contains("stage=input") && it.contains("gapMs=80.000") })
        assertTrue(logs.any { it.contains("stage=sink") && it.contains("codecMs=0.900") && it.contains("postDecodeMs=0.100") })
        assertTrue(logs.any { it.contains("stage=recovery") && it.contains("targetNs=1095000000") })
    }
    @Test fun distinguishesPostDecodeWaitAndBlockedRelease() {
        val logs = mutableListOf<String>()
        val flow = HdrFrameFlow(120) { _, message -> logs += message }
        val a = flow.input(start, 1)
        flow.submitted(a, start + 100_000)
        flow.output(a, start + 1_000_000)
        a.sink(start + 81_000_000)
        a.released(start + 81_000_000, start + 111_000_000, start + 90_000_000, true)
        assertTrue(logs.any { it.contains("postDecodeMs=80.000") })
        assertTrue(logs.any { it.contains("stage=release") && it.contains("gapMs=30.000") })
    }
    @Test fun oldBuffersCannotPolluteNewSurfaceEpoch() {
        val logs = mutableListOf<String>()
        val flow = HdrFrameFlow(120) { _, message -> logs += message }
        val a = flow.input(start, 1)
        flow.reset()
        flow.submitted(a, start + 10_000_000)
        flow.output(a, start + 11_000_000)
        a.sink(start + 100_000_000)
        a.released(start, start + 100_000_000, null, true)
        flow.input(start + 1_000_000_000, 1)
        flow.input(start + 2_000_000_000, 1)
        val report = logs.first { it.contains("stage=summary") }
        assertTrue(report.contains("outputs=0 sinks=0 presented=0 dropped=0"))
    }
    @Test fun boundsLogsAndUnknownSourceTime() {
        val logs = mutableListOf<String>()
        val flow = HdrFrameFlow(120) { _, message -> logs += message }
        repeat(10) { n ->
            val frame = flow.input(start + n * 30_000_000, 0)
            flow.submitted(frame, start + n * 30_000_000)
            flow.output(frame, start + n * 30_000_000 + 1_000_000)
            frame.sink(start + n * 30_000_000 + 1_100_000)
            frame.released(start + n * 30_000_000 + 1_100_000,
                start + n * 30_000_000 + 1_200_000, null, n % 2 == 0)
        }
        assertEquals(2, logs.count { it.contains("stage=input") })
        assertEquals(2, logs.count { it.contains("stage=recovery") })
        assertTrue(logs.all { it.contains("sourceStepMs=-") })
    }
    @Test fun reportsMaximaThenClearsOnlyTheMeasurementWindow() {
        val logs = mutableListOf<String>()
        val flow = HdrFrameFlow(120) { _, message -> logs += message }
        val a = flow.input(start, 1)
        flow.submitted(a, start)
        flow.output(a, start + 1_000_000)
        a.sink(start + 2_000_000)
        a.released(start + 2_000_000, start + 2_100_000, null, false)
        flow.input(start + 1_000_000_000, 2)
        flow.input(start + 2_000_000_000, 3)
        val reports = logs.filter { it.contains("stage=summary") }
        assertTrue(reports[0].contains("dropped=1"))
        assertTrue(reports[0].contains("postDecodeMaxMs=1.000 releaseCallMaxMs=0.100"))
        assertTrue(reports[1].contains("dropped=0"))
    }
}

package com.opencloudgaming.opennow

import org.junit.Assert.*
import org.junit.Test

class HdrPresentationClockTest {
    private val tick = 8_333_333L
    private val origin = 1_000_000_000L

    @Test fun noDisplayClockUsesImmediatePath() {
        assertNull(HdrPresentationClock(120).nextPresentationTimeNs(origin))
    }

    @Test fun arrivalJitterKeepsARegularCadence() {
        val clock = HdrPresentationClock(120)
        clock.observeVsync(origin, tick)
        val first = requireNotNull(clock.nextPresentationTimeNs(origin + 1_000_000))
        val second = requireNotNull(clock.nextPresentationTimeNs(origin + tick + 2_000_000))
        val third = requireNotNull(clock.nextPresentationTimeNs(origin + tick * 2 + 500_000))
        assertEquals(tick, second - first)
        assertEquals(tick, third - second)
    }

    @Test fun repairBurstHasABoundedFutureHorizon() {
        val clock = HdrPresentationClock(120)
        clock.observeVsync(origin, tick)
        val now = origin + 1_000_000
        val targets = List(100) { requireNotNull(clock.nextPresentationTimeNs(now)) }
        assertTrue(targets.all { it > now && it - now <= tick * 3 })
        assertEquals(1, targets.toSet().size)
    }

    @Test fun longNetworkGapRebasesWithoutReplayingOldTargets() {
        val clock = HdrPresentationClock(120)
        clock.observeVsync(origin, tick)
        clock.nextPresentationTimeNs(origin + 1_000_000)
        val now = origin + 200_000_000
        clock.observeVsync(now, tick)
        val target = requireNotNull(clock.nextPresentationTimeNs(now + 1_000_000))
        assertEquals(now + tick * 2, target)
    }

    @Test fun gapRecoveryRestoresHeadroomOnTheNextNormalFrame() {
        val clock = HdrPresentationClock(120)
        clock.observeVsync(origin, tick)
        clock.nextPresentationTimeNs(origin + 1_000_000)
        val recovered = origin + tick * 20
        clock.observeVsync(recovered, tick)
        val first = requireNotNull(clock.nextPresentationTimeNs(recovered + 1_000_000))
        clock.observeVsync(recovered + tick, tick)
        val second = requireNotNull(clock.nextPresentationTimeNs(recovered + tick + 1_000_000))
        assertEquals(recovered + tick * 2, first)
        assertEquals(recovered + tick * 4, second)
        assertEquals(tick * 2, second - first)
        assertEquals(1, clock.gapRebases)
    }

    @Test fun recoveryBurstIsMonotonicAndDoesNotExtendTheBufferHorizon() {
        val clock = HdrPresentationClock(120)
        clock.observeVsync(origin, tick)
        clock.nextPresentationTimeNs(origin + 1_000_000)
        val recovered = origin + tick * 20
        clock.observeVsync(recovered, tick)
        val now = recovered + 1_000_000
        val targets = List(100) { requireNotNull(clock.nextPresentationTimeNs(now)) }
        assertEquals(recovered + tick * 2, targets.first())
        assertTrue(targets.zipWithNext().all { (a, b) -> b >= a })
        assertTrue(targets.all { it > now && it - now <= tick * 3 })
    }

    @Test fun staleOrFutureClockFallsBackAndCanRecover() {
        val clock = HdrPresentationClock(120)
        clock.observeVsync(origin, tick)
        assertNull(clock.nextPresentationTimeNs(origin - 1))
        assertNull(clock.nextPresentationTimeNs(origin + 250_000_001))
        clock.observeVsync(origin + 300_000_000, tick)
        assertNotNull(clock.nextPresentationTimeNs(origin + 301_000_000))
    }

    @Test fun sourceSlowerThanDisplayKeepsItsOwnCadence() {
        val clock = HdrPresentationClock(60)
        clock.observeVsync(origin, tick)
        val a = requireNotNull(clock.nextPresentationTimeNs(origin + 1_000_000))
        val b = requireNotNull(clock.nextPresentationTimeNs(origin + tick * 2 + 1_000_000))
        assertEquals(tick * 2, b - a)
    }

    @Test fun sourceFasterThanDisplayCannotQueueUnboundedExtraFrames() {
        val clock = HdrPresentationClock(120)
        val displayTick = tick * 2
        clock.observeVsync(origin, displayTick)
        for (n in 0..40) {
            val now = origin + n * tick + 1_000_000
            clock.observeVsync(origin + (n / 2) * displayTick, displayTick)
            val target = requireNotNull(clock.nextPresentationTimeNs(now))
            assertTrue(target > now && target - now <= displayTick * 3)
        }
    }

    @Test fun longRunningStreamDoesNotAccumulateBufferedLatency() {
        val clock = HdrPresentationClock(120)
        var previous = 0L
        repeat(10_000) { n ->
            val vsync = origin + tick * n
            clock.observeVsync(vsync, tick)
            val now = vsync + 1_000_000
            val target = requireNotNull(clock.nextPresentationTimeNs(now))
            assertTrue(target > previous && target - now <= tick * 3)
            previous = target
        }
    }

    @Test fun displayModeChangeAndSurfaceResetDiscardTheOldCadence() {
        val clock = HdrPresentationClock(120)
        clock.observeVsync(origin, tick)
        clock.nextPresentationTimeNs(origin + 1_000_000)
        clock.observeVsync(origin + tick, tick * 2)
        assertEquals(origin + tick * 7, clock.nextPresentationTimeNs(origin + tick + 1_000_000))
        clock.reset()
        assertNull(clock.nextPresentationTimeNs(origin + tick * 2))
    }
}

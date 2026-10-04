package com.opencloudgaming.opennow

import org.junit.Assert.assertEquals
import org.junit.Assert.assertNull
import org.junit.Test

class GamepadStateBurstLimiterTest {
    @Test
    fun quickHatTapPreservesPressAndReleaseInsideOneStickInterval() {
        val limiter = GamepadStateBurstLimiter(minimumIntervalMs = 16)
        assertEquals(0, limiter.offer(0, 100))
        assertNull(limiter.offer(0, 102)) // Pending stick motion must not swallow the tap.
        assertEquals(0, limiter.offer(0, 104, hatButtons = GamepadButtonMapping.DPAD_UP))
        assertEquals(0, limiter.offer(0, 108, hatButtons = 0))
        assertNull(limiter.flush(116))
    }

    @Test
    fun rapidTriggerTapsPreserveBothTriggersAndPartialPressure() {
        val limiter = GamepadStateBurstLimiter(minimumIntervalMs = 16)
        assertEquals(0, limiter.offer(0, 100))
        assertEquals(0, limiter.offer(0, 102, leftTrigger = 80))
        assertEquals(0, limiter.offer(0, 104, leftTrigger = 255))
        assertEquals(0, limiter.offer(0, 106, rightTrigger = 255))
        assertEquals(0, limiter.offer(0, 108))
        assertEquals(0, limiter.offer(0, 110, rightTrigger = 90))
        assertEquals(0, limiter.offer(0, 112))
    }

    @Test
    fun heldMotionButtonsStillAllowStickCoalescing() {
        val limiter = GamepadStateBurstLimiter(minimumIntervalMs = 16)
        assertEquals(0, limiter.offer(0, 100, hatButtons = 1, leftTrigger = 255))
        assertNull(limiter.offer(0, 104, hatButtons = 1, leftTrigger = 255))
        assertEquals(12L, limiter.delayUntilFlushMs(104))
        assertEquals(0, limiter.flush(116))
    }

    @Test
    fun tracksMotionButtonChangesPerControllerAndClearsThemOnReset() {
        val limiter = GamepadStateBurstLimiter(minimumIntervalMs = 16)
        assertEquals(0, limiter.offer(0, 100, leftTrigger = 255))
        assertEquals(1, limiter.offer(1, 102, leftTrigger = 255))
        assertEquals(0, limiter.offer(0, 104))
        assertEquals(1, limiter.offer(1, 106))
        limiter.reset()
        assertEquals(0, limiter.offer(0, 108))
        assertNull(limiter.offer(1, 110))
    }

    @Test
    fun sendsLeadingSnapshotAndCoalescesBurstToLatestController() {
        val limiter = GamepadStateBurstLimiter(minimumIntervalMs = 16)

        assertEquals(0, limiter.offer(controllerId = 0, nowMs = 100))
        assertNull(limiter.offer(controllerId = 0, nowMs = 104))
        assertNull(limiter.offer(controllerId = 1, nowMs = 108))
        assertEquals(8L, limiter.delayUntilFlushMs(nowMs = 108))
        assertEquals(1, limiter.flush(nowMs = 116))
        assertNull(limiter.flush(nowMs = 116))
    }

    @Test
    fun idleSnapshotSendsImmediately() {
        val limiter = GamepadStateBurstLimiter(minimumIntervalMs = 16)

        assertEquals(0, limiter.offer(controllerId = 0, nowMs = 100))
        assertEquals(0, limiter.offer(controllerId = 0, nowMs = 116))
        assertNull(limiter.delayUntilFlushMs(nowMs = 116))
    }

    @Test
    fun resetClearsPendingSnapshotAndTiming() {
        val limiter = GamepadStateBurstLimiter(minimumIntervalMs = 16)

        limiter.offer(controllerId = 0, nowMs = 100)
        limiter.offer(controllerId = 1, nowMs = 104)
        limiter.reset()

        assertNull(limiter.flush(nowMs = 105))
        assertEquals(1, limiter.offer(controllerId = 1, nowMs = 105))
    }

    @Test
    fun physicalStickReturnToCenterBypassesPendingMotion() {
        val limiter = GamepadStateBurstLimiter(minimumIntervalMs = 16)

        assertEquals(0, limiter.offer(0, 100, neutralSticks = 3))
        assertNull(limiter.offer(0, 102, neutralSticks = 2)) // Left stick moves.
        assertEquals(0, limiter.offer(0, 104, neutralSticks = 3)) // Left stick released.
        assertNull(limiter.flush(116)) // The superseded movement must not reappear.
    }

    @Test
    fun oneStickReleaseIsImmediateWhileOtherStickRemainsActive() {
        val limiter = GamepadStateBurstLimiter(minimumIntervalMs = 16)

        assertEquals(0, limiter.offer(0, 100, neutralSticks = 0))
        assertEquals(0, limiter.offer(0, 104, neutralSticks = 1))
        assertNull(limiter.offer(0, 106, neutralSticks = 1))
    }
}

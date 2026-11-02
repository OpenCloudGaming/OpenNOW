package com.opencloudgaming.opennow

import org.junit.Assert.*
import org.junit.Test

class DoubleTapDirectClickGestureTest {
    private fun DoubleTapDirectClickGesture.tap(time: Long, x: Float = 100f, y: Float = 100f): Boolean {
        begin(x, y, time)
        return finish(x, y, time + 40)
    }

    @Test fun requiresTwoNearbyCompletedTapsAndConsumesThePair() {
        val gesture = DoubleTapDirectClickGesture()
        assertFalse(gesture.tap(1000))
        gesture.begin(110f, 105f, 1100)
        assertFalse(gesture.isSliding)
        assertTrue(gesture.finish(112f, 106f, 1140))
        assertFalse(gesture.tap(1200))
        assertTrue(gesture.tap(1300))
    }

    @Test fun slowDistantOrHeldTapsCannotClick() {
        val gesture = DoubleTapDirectClickGesture()
        assertFalse(gesture.tap(1000))
        assertFalse(gesture.tap(1400))
        assertFalse(gesture.tap(1500, x = 200f))
        gesture.begin(200f, 100f, 1600)
        assertFalse(gesture.finish(200f, 100f, 2100))
        assertFalse(gesture.tap(2150, x = 200f))
    }

    @Test fun slideReturningToOriginAndTapThenSlideNeverClickOrArmAnotherTap() {
        val gesture = DoubleTapDirectClickGesture()
        assertFalse(gesture.tap(1000))
        gesture.begin(100f, 100f, 1100)
        gesture.move(140f, 100f)
        gesture.move(100f, 100f)
        assertTrue(gesture.isSliding)
        assertFalse(gesture.finish(100f, 100f, 1150))
        assertFalse(gesture.tap(1200))
    }

    @Test fun cancellationAndMultitouchInvalidateThePair() {
        val gesture = DoubleTapDirectClickGesture()
        assertFalse(gesture.tap(1000))
        gesture.reset()
        assertFalse(gesture.tap(1100))
        gesture.begin(100f, 100f, 1200)
        gesture.invalidateTap()
        assertFalse(gesture.finish(100f, 100f, 1240))
        assertFalse(gesture.tap(1300))
    }
}

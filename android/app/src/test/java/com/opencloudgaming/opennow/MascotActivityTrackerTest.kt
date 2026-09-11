package com.opencloudgaming.opennow

import org.junit.Assert.assertSame
import org.junit.Test

class MascotActivityTrackerTest {
    @Test
    fun onlyOneAttachedOverlayOwnsTheMascot() {
        val tracker = MascotActivityTracker()
        val first = Any()
        val second = Any()

        tracker.attachOverlay(first)
        tracker.attachOverlay(second)

        assertSame(first, tracker.overlayOwner)
    }

    @Test
    fun ownershipPassesToTheNextAttachedOverlay() {
        val tracker = MascotActivityTracker()
        val first = Any()
        val second = Any()
        tracker.attachOverlay(first)
        tracker.attachOverlay(second)

        tracker.detachOverlay(first)

        assertSame(second, tracker.overlayOwner)
    }
}

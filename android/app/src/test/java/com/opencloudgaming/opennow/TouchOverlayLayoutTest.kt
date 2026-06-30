package com.opencloudgaming.opennow

import org.junit.Assert.assertEquals
import org.junit.Assert.assertTrue
import org.junit.Test

class TouchOverlayLayoutTest {
    @Test
    fun keepsLandscapeTopControlsBelowPhoneTopInformationBand() {
        val clearance = landscapeTouchTopControlClearanceDp(viewportHeightDp = 390f, controlScale = 1f)

        assertTrue(clearance >= 40f)
    }

    @Test
    fun scalesLandscapeTopClearanceForLargerControlsWithoutRunningAway() {
        val normal = landscapeTouchTopControlClearanceDp(viewportHeightDp = 430f, controlScale = 1f)
        val large = landscapeTouchTopControlClearanceDp(viewportHeightDp = 800f, controlScale = 1.5f)

        assertTrue(large > normal)
        assertEquals(76f, large, 0.001f)
    }

    @Test
    fun keepsTinyLandscapeScreensUsable() {
        val clearance = landscapeTouchTopControlClearanceDp(viewportHeightDp = 300f, controlScale = 0.6f)

        assertEquals(30f, clearance, 0.001f)
    }
}

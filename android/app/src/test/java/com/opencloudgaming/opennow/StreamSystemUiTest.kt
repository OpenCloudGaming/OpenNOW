package com.opencloudgaming.opennow

import org.junit.Assert.assertFalse
import org.junit.Assert.assertTrue
import org.junit.Test

class StreamSystemUiTest {
    @Test
    fun leavesTransientThreeButtonNavigationVisibleLongEnoughToUse() {
        assertFalse(
            shouldPeriodicallyEnforceStreamSystemUi(
                streamActive = true,
                navigationBarsVisible = true,
                pointerLockEnabled = false,
            ),
        )
    }

    @Test
    fun mouseLockForcesImmersiveModeWhenSystemNavigationAppears() {
        assertTrue(
            shouldPeriodicallyEnforceStreamSystemUi(
                streamActive = true,
                navigationBarsVisible = true,
                pointerLockEnabled = true,
            ),
        )
    }

    @Test
    fun keepsFullscreenEnforcementForHiddenNavigationBars() {
        assertTrue(
            shouldPeriodicallyEnforceStreamSystemUi(
                streamActive = true,
                navigationBarsVisible = false,
                pointerLockEnabled = false,
            ),
        )
        assertFalse(
            shouldPeriodicallyEnforceStreamSystemUi(
                streamActive = false,
                navigationBarsVisible = false,
                pointerLockEnabled = true,
            ),
        )
    }

    @Test
    fun expandedControlsKeepAndroidBarsVisibleEvenWithMouseLock() {
        assertFalse(
            shouldPeriodicallyEnforceStreamSystemUi(
                streamActive = true,
                panelExpanded = true,
                navigationBarsVisible = true,
                pointerLockEnabled = true,
            ),
        )
    }
}

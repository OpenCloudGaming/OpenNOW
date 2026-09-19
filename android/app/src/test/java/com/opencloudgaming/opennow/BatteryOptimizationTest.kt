package com.opencloudgaming.opennow

import org.junit.Assert.assertFalse
import org.junit.Assert.assertTrue
import org.junit.Test

class BatteryOptimizationTest {
    @Test
    fun promptsOnlyForBatteryDevicesThatRemainOptimized() {
        assertTrue(
            shouldPromptForBatteryOptimization(
                deviceHasBattery = true,
                ignoringBatteryOptimizations = false,
                promptDismissed = false,
            ),
        )
        assertFalse(shouldPromptForBatteryOptimization(true, true, false))
        assertFalse(shouldPromptForBatteryOptimization(false, false, false))
        assertFalse(shouldPromptForBatteryOptimization(true, false, true))
    }
}

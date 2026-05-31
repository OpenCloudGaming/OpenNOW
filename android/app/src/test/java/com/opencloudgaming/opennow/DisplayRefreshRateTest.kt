package com.opencloudgaming.opennow

import org.junit.Assert.assertEquals
import org.junit.Test

class DisplayRefreshRateTest {
    @Test
    fun choosesSmallestModeAtOrAboveRequestedFps() {
        val selected = selectStreamDisplayMode(
            supportedModes = listOf(
                mode(id = 1, refreshRate = 72f),
                mode(id = 2, refreshRate = 90f),
                mode(id = 3, refreshRate = 120f),
            ),
            currentMode = mode(id = 1, refreshRate = 72f),
            requestedFps = 90,
        )

        assertEquals(2, selected?.id)
    }

    @Test
    fun usesHighestModeWhenRequestedFpsExceedsDisplaySupport() {
        val selected = selectStreamDisplayMode(
            supportedModes = listOf(
                mode(id = 1, refreshRate = 60f),
                mode(id = 2, refreshRate = 90f),
            ),
            currentMode = mode(id = 1, refreshRate = 60f),
            requestedFps = 120,
        )

        assertEquals(2, selected?.id)
    }

    @Test
    fun prefersCurrentPhysicalResolutionWhenModesIncludeMultipleSizes() {
        val selected = selectStreamDisplayMode(
            supportedModes = listOf(
                mode(id = 1, refreshRate = 90f, physicalWidth = 1920, physicalHeight = 1080),
                mode(id = 2, refreshRate = 90f, physicalWidth = 2560, physicalHeight = 1440),
                mode(id = 3, refreshRate = 120f, physicalWidth = 2560, physicalHeight = 1440),
            ),
            currentMode = mode(id = 4, refreshRate = 60f, physicalWidth = 2560, physicalHeight = 1440),
            requestedFps = 90,
        )

        assertEquals(2, selected?.id)
    }

    @Test
    fun toleratesFractionalDisplayRatesNearRequestedFps() {
        val selected = selectStreamDisplayMode(
            supportedModes = listOf(
                mode(id = 1, refreshRate = 72f),
                mode(id = 2, refreshRate = 89.98f),
                mode(id = 3, refreshRate = 119.88f),
            ),
            currentMode = mode(id = 1, refreshRate = 72f),
            requestedFps = 90,
        )

        assertEquals(2, selected?.id)
    }

    private fun mode(
        id: Int,
        refreshRate: Float,
        physicalWidth: Int = 1920,
        physicalHeight: Int = 1080,
    ): DisplayRefreshMode =
        DisplayRefreshMode(
            id = id,
            refreshRate = refreshRate,
            physicalWidth = physicalWidth,
            physicalHeight = physicalHeight,
        )
}

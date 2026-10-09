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
    fun selectsOneRefreshPerFrameInsteadOfRetainingCurrentDoubleRate() {
        val selected = selectStreamDisplayMode(
            supportedModes = listOf(
                mode(id = 1, refreshRate = 60f),
                mode(id = 2, refreshRate = 120f),
            ),
            currentMode = mode(id = 2, refreshRate = 120f),
            requestedFps = 60,
        )

        assertEquals(1, selected?.id)
    }

    @Test
    fun keepsHighRefreshRateForHighFpsStream() {
        val selected = selectStreamDisplayMode(
            supportedModes = listOf(
                mode(id = 1, refreshRate = 60f),
                mode(id = 2, refreshRate = 120f),
            ),
            currentMode = mode(id = 2, refreshRate = 120f),
            requestedFps = 120,
        )

        assertEquals(2, selected?.id)
    }

    @Test
    fun rejectsCadenceIncompatibleCurrentHighRefreshMode() {
        val selected = selectStreamDisplayMode(
            supportedModes = listOf(
                mode(id = 1, refreshRate = 60f),
                mode(id = 2, refreshRate = 90f),
                mode(id = 3, refreshRate = 120f),
            ),
            currentMode = mode(id = 2, refreshRate = 90f),
            requestedFps = 60,
        )

        assertEquals(1, selected?.id)
    }

    @Test
    fun prefersCadenceCompatibleModeOverCloserIncompatibleMode() {
        val selected = selectStreamDisplayMode(
            supportedModes = listOf(
                mode(id = 1, refreshRate = 90f),
                mode(id = 2, refreshRate = 120f),
                mode(id = 3, refreshRate = 144f),
            ),
            currentMode = mode(id = 3, refreshRate = 144f),
            requestedFps = 60,
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
    fun selects360HzModeForUltimateStream() {
        val selected = selectStreamDisplayMode(
            supportedModes = listOf(
                mode(id = 1, refreshRate = 120f),
                mode(id = 2, refreshRate = 240f),
                mode(id = 3, refreshRate = 360f),
            ),
            currentMode = mode(id = 1, refreshRate = 120f),
            requestedFps = 360,
        )

        assertEquals(3, selected?.id)
        assertEquals(360f, normalizedStreamDisplayFps(360))
    }

    @Test
    fun matchesHighFpsStreamOnDisplayWithHigherRefreshModes() {
        val modes = listOf(mode(1, 60f), mode(2, 120f), mode(3, 240f), mode(4, 360f))
        assertEquals(2, selectStreamDisplayMode(modes, modes.last(), 120)?.id)
        assertEquals(3, selectStreamDisplayMode(modes, modes.last(), 240)?.id)
        assertEquals(4, selectStreamDisplayMode(modes, modes.first(), 360)?.id)
    }

    @Test
    fun usesIntegerMultipleWhenExactHighFpsRefreshIsUnavailable() {
        val modes = listOf(mode(1, 60f), mode(2, 144f), mode(3, 240f), mode(4, 360f))
        assertEquals(3, selectStreamDisplayMode(modes, modes.last(), 120)?.id)
    }

    @Test
    fun usesDisplayMaximumWithoutReducingRequestedHighStreamFps() {
        val modes = listOf(mode(1, 60f), mode(2, 120f))
        assertEquals(2, selectStreamDisplayMode(modes, modes.first(), 360)?.id)
        assertEquals(360f, normalizedStreamDisplayFps(360))
    }

    @Test
    fun toleratesFractionalHighRefreshModes() {
        val modes = listOf(mode(1, 119.88f), mode(2, 239.76f), mode(3, 359.64f))
        assertEquals(1, selectStreamDisplayMode(modes, modes.last(), 120)?.id)
        assertEquals(2, selectStreamDisplayMode(modes, modes.last(), 240)?.id)
        assertEquals(3, selectStreamDisplayMode(modes, modes.first(), 360)?.id)
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

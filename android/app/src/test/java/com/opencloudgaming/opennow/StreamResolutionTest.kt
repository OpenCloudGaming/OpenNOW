package com.opencloudgaming.opennow

import org.junit.Assert.assertEquals
import org.junit.Test

class StreamResolutionTest {
    @Test
    fun streamResolutionPixelsKeepsSelected1080pFor16By9() {
        val settings = StreamSettings(resolution = "1920x1080", aspectRatio = "16:9")

        assertEquals(1920 to 1080, streamResolutionPixels(settings))
    }

    @Test
    fun streamResolutionPixelsMaps1080pTierToUltrawideMode() {
        val settings = StreamSettings(resolution = "1920x1080", aspectRatio = "21:9")

        assertEquals(2560 to 1080, streamResolutionPixels(settings))
    }

    @Test
    fun streamResolutionPixelsMaps1440pTierToUltrawideMode() {
        val settings = StreamSettings(resolution = "2560x1440", aspectRatio = "21:9")

        assertEquals(3440 to 1440, streamResolutionPixels(settings))
    }

    @Test
    fun streamResolutionPixelsKeepsSelected4kFor16By9() {
        val settings = StreamSettings(resolution = "3840x2160", aspectRatio = "16:9")

        assertEquals(3840 to 2160, streamResolutionPixels(settings))
    }

    @Test
    fun streamResolutionPixelsMapsSelectedTierForTallerAspectRatio() {
        val settings = StreamSettings(resolution = "1920x1080", aspectRatio = "16:10")

        assertEquals(1920 to 1200, streamResolutionPixels(settings))
    }

    @Test
    fun streamResolutionPixelsKeepsExactStoredUltrawideMode() {
        val settings = StreamSettings(resolution = "3440x1440", aspectRatio = "21:9")

        assertEquals(3440 to 1440, streamResolutionPixels(settings))
    }
}

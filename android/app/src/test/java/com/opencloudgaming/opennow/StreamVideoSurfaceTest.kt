package com.opencloudgaming.opennow

import org.junit.Assert.assertEquals
import org.junit.Test

class StreamVideoSurfaceTest {
    @Test
    fun decodedFrameWithDifferentAspectFitsCompletelyInsideViewport() {
        assertEquals(
            2322 to 995,
            aspectFitStreamSurfaceSize(
                frameWidth = 1376,
                frameHeight = 590,
                containerWidth = 2322,
                containerHeight = 1080,
            ),
        )
    }

    @Test
    fun matchingAspectUsesCompleteViewport() {
        assertEquals(
            1920 to 1080,
            aspectFitStreamSurfaceSize(
                frameWidth = 1280,
                frameHeight = 720,
                containerWidth = 1920,
                containerHeight = 1080,
            ),
        )
    }

    @Test
    fun invalidFrameFallsBackToContainer() {
        assertEquals(2322 to 1080, aspectFitStreamSurfaceSize(0, 0, 2322, 1080))
    }
}

package com.opencloudgaming.opennow

import org.junit.Assert.*
import org.junit.Test

class SurfaceDecoderQueuePolicyTest {
    @Test fun observedEightFrameStartupBurstIsNotDecoderFailure() {
        assertNull(SurfaceDecoderQueuePolicy(60).failure(8, 800_000, 28, priming = true))
    }

    @Test fun activeCatchUpBurstIsAllowedWhileAgeRemainsShort() {
        assertNull(SurfaceDecoderQueuePolicy(60).failure(16, 2_000_000, 100, priming = false))
    }

    @Test fun startupAndActiveStallsHaveBoundedGrace() {
        val policy = SurfaceDecoderQueuePolicy(60)
        assertNull(policy.failure(8, 1000, 500, priming = true))
        assertNotNull(policy.failure(8, 1000, 1001, priming = true))
        assertNotNull(policy.failure(8, 1000, 251, priming = false))
        assertNull(policy.failure(0, 0, 2000, priming = false))
    }

    @Test fun frameCountRemainsBoundedAndScalesWithHighFps() {
        assertNotNull(SurfaceDecoderQueuePolicy(60).failure(32, 1000, 28, true))
        for (fps in listOf(120, 240, 360)) {
            val policy = SurfaceDecoderQueuePolicy(fps)
            assertNull(policy.failure(fps / 2 - 1, 1000, 28, true))
            assertNotNull(policy.failure(fps / 2, 1000, 28, true))
        }
    }

    @Test fun encodedMemoryCannotGrowWithoutLimitAtHighFrameRates() {
        assertNotNull(SurfaceDecoderQueuePolicy(360).failure(10, 16L * 1024 * 1024 + 1, 28, true))
    }
}

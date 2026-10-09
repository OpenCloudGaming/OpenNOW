package com.opencloudgaming.opennow

import org.junit.Assert.*
import org.junit.Test
import java.util.concurrent.CountDownLatch
import java.util.concurrent.Executors
import java.util.concurrent.TimeUnit
import java.util.concurrent.atomic.AtomicInteger

class MediaCodecSurfaceBufferTest {
    @Test fun presentationRacingWithFrameDropReturnsOneCodecSlot() {
        val releases = AtomicInteger()
        val buffer = MediaCodecSurfaceBuffer(1920, 1080) { releases.incrementAndGet(); it }
        val ready = CountDownLatch(2)
        val start = CountDownLatch(1)
        val executor = Executors.newFixedThreadPool(2)
        try {
            val presentation = executor.submit {
                ready.countDown()
                check(start.await(5, TimeUnit.SECONDS))
                buffer.present()
            }
            val drop = executor.submit {
                ready.countDown()
                check(start.await(5, TimeUnit.SECONDS))
                buffer.release()
            }
            assertTrue(ready.await(5, TimeUnit.SECONDS))
            start.countDown()
            presentation.get(5, TimeUnit.SECONDS)
            drop.get(5, TimeUnit.SECONDS)
            assertEquals(1, releases.get())
            assertFalse(buffer.present())
        } finally {
            start.countDown()
            executor.shutdownNow()
        }
    }

    @Test fun completeFrameCanBeRetainedWithoutPixelReadback() {
        val releases = mutableListOf<Boolean>()
        val buffer = MediaCodecSurfaceBuffer(1920, 1080) { releases += it; it }
        assertSame(buffer, buffer.cropAndScale(0, 0, 1920, 1080, 1920, 1080))
        assertNull(buffer.toI420())
        buffer.release()
        assertTrue(releases.isEmpty())
        buffer.release()
        assertEquals(listOf(false), releases)
    }

    @Test(expected = IllegalArgumentException::class)
    fun opaqueFramesCannotSilentlyRequestUnsupportedPixelScaling() {
        MediaCodecSurfaceBuffer(1920, 1080) { it }.cropAndScale(0, 0, 1920, 1080, 1280, 720)
    }
}

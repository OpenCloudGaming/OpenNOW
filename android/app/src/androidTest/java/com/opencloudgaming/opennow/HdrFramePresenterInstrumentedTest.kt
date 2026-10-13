package com.opencloudgaming.opennow

import org.junit.Assert.*
import org.junit.Test
import java.util.concurrent.atomic.AtomicInteger

class HdrFramePresenterInstrumentedTest {
    @Test fun surfaceClockCanStopAndRestartWithoutLeakingItsThread() {
        val presenter = HdrFramePresenter(120)
        val timed = AtomicInteger()
        val immediate = AtomicInteger()
        fun submit(): Boolean {
            val before = System.nanoTime()
            val buffer = HdrSurfaceBuffer(1280, 720, timedFinish = {
                assertTrue(it >= before && it - before < 100_000_000L)
                timed.incrementAndGet()
                true
            }) { immediate.incrementAndGet(); it }
            return try { presenter.present(buffer) } finally { buffer.release() }
        }
        try {
            assertTrue(submit())
            assertEquals(1, immediate.get())
            repeat(3) {
                presenter.start(120f)
                val expected = timed.get()
                val deadline = System.nanoTime() + 2_000_000_000L
                while (timed.get() == expected && System.nanoTime() < deadline) {
                    Thread.sleep(10)
                    assertTrue(submit())
                }
                assertTrue("Display clock never became available", timed.get() > expected)
                presenter.stop()
                val afterStop = timed.get()
                assertTrue(submit())
                assertEquals(afterStop, timed.get())
            }
        } finally { presenter.stop() }
        val deadline = System.nanoTime() + 2_000_000_000L
        while (Thread.getAllStackTraces().keys.any { it.isAlive && it.name == "OpenNOW-HDR-vsync" }
            && System.nanoTime() < deadline) Thread.sleep(10)
        assertFalse(Thread.getAllStackTraces().keys.any { it.isAlive && it.name == "OpenNOW-HDR-vsync" })
    }
}

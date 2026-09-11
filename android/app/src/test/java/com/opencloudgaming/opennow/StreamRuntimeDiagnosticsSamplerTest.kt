package com.opencloudgaming.opennow

import java.util.concurrent.CountDownLatch
import java.util.concurrent.TimeUnit
import kotlinx.coroutines.Dispatchers
import kotlinx.coroutines.runBlocking
import kotlinx.coroutines.withContext
import kotlinx.coroutines.yield
import org.junit.Assert.assertEquals
import org.junit.Assert.assertFalse
import org.junit.Assert.assertNull
import org.junit.Assert.assertSame
import org.junit.Assert.assertTrue
import org.junit.Test

class StreamRuntimeDiagnosticsSamplerTest {
    @Test
    fun samplesOnWorkerAndReturnsToOwnerWithOriginalCadence() = runBlocking {
        val ownerThread = Thread.currentThread()
        var now = 0L
        val delivered = mutableListOf<Boolean>()
        val expected = AndroidRuntimeDiagnosticsSnapshot(batteryPercent = 75)
        val sampler = StreamRuntimeDiagnosticsSampler(
            scope = this,
            elapsedRealtimeMs = { now },
            readSnapshot = {
                assertFalse("Android service query ran on the owner thread", Thread.currentThread() === ownerThread)
                expected
            },
        )
        val onSample: (AndroidRuntimeDiagnosticsSnapshot, Boolean) -> Unit = { snapshot, includeDevice ->
            assertSame(ownerThread, Thread.currentThread())
            assertSame(expected, snapshot)
            delivered += includeDevice
        }
        sampler.sampleIfDue(onSample)!!.join()
        now = 4_999L
        assertNull(sampler.sampleIfDue(onSample))
        now = 5_000L
        sampler.sampleIfDue(onSample)!!.join()
        now = 25_000L
        sampler.sampleIfDue(onSample)!!.join()
        now = 30_000L
        sampler.sampleIfDue(onSample)!!.join()
        assertEquals(listOf(true, false, false, true), delivered)
        sampler.reset()
        sampler.sampleIfDue(onSample)!!.join()
        assertEquals(listOf(true, false, false, true, true), delivered)
    }

    @Test
    fun slowServiceDoesNotQueueMoreSamplesOrDeliverAfterSessionReset() = runBlocking {
        val started = CountDownLatch(1)
        val release = CountDownLatch(1)
        var now = 0L
        var delivered = 0
        val sampler = StreamRuntimeDiagnosticsSampler(
            scope = this,
            elapsedRealtimeMs = { now },
            readSnapshot = {
                started.countDown()
                check(release.await(5, TimeUnit.SECONDS)) { "Test did not release the service query" }
                AndroidRuntimeDiagnosticsSnapshot()
            },
        )
        val first = sampler.sampleIfDue { _, _ -> delivered++ }!!
        try {
            yield()
            assertTrue(withContext(Dispatchers.IO) { started.await(5, TimeUnit.SECONDS) })
            now = 60_000L
            assertNull(sampler.sampleIfDue { _, _ -> delivered++ })
            sampler.reset()
            assertNull("Cancelled Binder query is still running", sampler.sampleIfDue { _, _ -> delivered++ })
        } finally {
            release.countDown()
        }
        first.join()
        assertEquals(0, delivered)
        sampler.sampleIfDue { _, includeDevice ->
            assertTrue(includeDevice)
            delivered++
        }!!.join()
        assertEquals(1, delivered)
    }

    @Test
    fun failedOptionalServiceIsRetriedWithoutFailingTheOwner() = runBlocking {
        var now = 0L
        var attempts = 0
        var delivered = 0
        val sampler = StreamRuntimeDiagnosticsSampler(
            scope = this,
            elapsedRealtimeMs = { now },
            readSnapshot = {
                attempts++
                if (attempts == 1) throw SecurityException("Service unavailable")
                AndroidRuntimeDiagnosticsSnapshot()
            },
        )
        sampler.sampleIfDue { _, _ -> delivered++ }!!.join()
        assertEquals(0, delivered)
        assertTrue(coroutineContext[kotlinx.coroutines.Job]!!.isActive)
        now = 5_000L
        sampler.sampleIfDue { _, _ -> delivered++ }!!.join()
        assertEquals(1, delivered)
        assertEquals(2, attempts)
    }
}

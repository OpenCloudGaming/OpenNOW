package com.opencloudgaming.opennow

import kotlinx.coroutines.CompletableDeferred
import kotlinx.coroutines.CoroutineStart
import kotlinx.coroutines.cancelAndJoin
import kotlinx.coroutines.launch
import kotlinx.coroutines.runBlocking
import org.junit.Assert.assertEquals
import org.junit.Assert.assertFalse
import org.junit.Assert.assertTrue
import org.junit.Test

class StreamKeyboardKeyStrokeTest {
    @Test
    fun pressRemainsVisibleAcrossA60HzInputPoll() = runBlocking {
        val events = mutableListOf<Pair<Boolean, Long>>()
        assertTrue(sendStreamKeyboardKeyStroke { pressed ->
            events += pressed to System.nanoTime()
            true
        })
        assertEquals(listOf(true, false), events.map { it.first })
        assertTrue(events[1].second - events[0].second >= STREAM_KEY_PRESS_DURATION_MS * 1_000_000L)
    }

    @Test
    fun rejectedPressDoesNotReleaseAnUnrelatedHeldKey() = runBlocking {
        val events = mutableListOf<Boolean>()
        assertFalse(sendStreamKeyboardKeyStroke { pressed -> events += pressed; false })
        assertEquals(listOf(true), events)
    }

    @Test
    fun cancellationDuringHoldStillReleasesTheKey() = runBlocking {
        val events = mutableListOf<Boolean>()
        val releaseCompleted = CompletableDeferred<Unit>()
        val job = launch(start = CoroutineStart.UNDISPATCHED) {
            sendStreamKeyboardKeyStroke { pressed ->
                events += pressed
                if (!pressed) {
                    // A release may suspend while retrying a congested input channel.
                    kotlinx.coroutines.delay(1)
                    releaseCompleted.complete(Unit)
                }
                true
            }
        }
        assertEquals(listOf(true), events)
        job.cancelAndJoin()
        assertEquals(listOf(true, false), events)
        assertTrue(releaseCompleted.isCompleted)
    }

    @Test
    fun failedReleaseIsReportedToStopTheRemainingEdit() = runBlocking {
        assertFalse(sendStreamKeyboardKeyStroke { pressed -> pressed })
    }

    @Test
    fun repeatedBackspaceHasAReleasedIntervalBetweenPresses() = runBlocking {
        val events = mutableListOf<Pair<Boolean, Long>>()
        repeat(2) {
            assertTrue(sendStreamKeyboardKeyStroke { pressed ->
                events += pressed to System.nanoTime()
                true
            })
        }
        assertEquals(listOf(true, false, true, false), events.map { it.first })
        assertTrue(events[2].second - events[1].second >= STREAM_KEY_PRESS_DURATION_MS * 1_000_000L)
    }
}

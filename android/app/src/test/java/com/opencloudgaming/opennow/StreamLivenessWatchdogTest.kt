package com.opencloudgaming.opennow

import org.junit.Assert.assertEquals
import org.junit.Assert.assertTrue
import org.junit.Test

class StreamLivenessWatchdogTest {
    @Test
    fun requestsKeyframesBeforeRestartingStalledMedia() {
        val watchdog = StreamLivenessWatchdog(
            keyframeAfterMs = 1_000L,
            keyframeIntervalMs = 500L,
            restartAfterMs = 3_000L,
        )

        watchdog.markConnected(0L)

        assertEquals(StreamLivenessAction.None, watchdog.observe(0L, bytesReceived = 10L, framesDecoded = 1L, connected = true))

        val first = watchdog.observe(1_000L, bytesReceived = 10L, framesDecoded = 1L, connected = true)
        assertTrue(first is StreamLivenessAction.RequestKeyframe)
        assertEquals(1, (first as StreamLivenessAction.RequestKeyframe).attempt)

        assertEquals(StreamLivenessAction.None, watchdog.observe(1_200L, bytesReceived = 10L, framesDecoded = 1L, connected = true))

        val second = watchdog.observe(1_500L, bytesReceived = 10L, framesDecoded = 1L, connected = true)
        assertTrue(second is StreamLivenessAction.RequestKeyframe)
        assertEquals(2, (second as StreamLivenessAction.RequestKeyframe).attempt)

        val restart = watchdog.observe(3_000L, bytesReceived = 10L, framesDecoded = 1L, connected = true)
        assertTrue(restart is StreamLivenessAction.RestartTransport)
    }

    @Test
    fun progressClearsPendingStallRecovery() {
        val watchdog = StreamLivenessWatchdog(
            keyframeAfterMs = 1_000L,
            keyframeIntervalMs = 500L,
            restartAfterMs = 3_000L,
        )

        watchdog.markConnected(0L)
        assertEquals(StreamLivenessAction.None, watchdog.observe(0L, bytesReceived = 10L, framesDecoded = 1L, connected = true))
        assertTrue(watchdog.observe(1_000L, bytesReceived = 10L, framesDecoded = 1L, connected = true) is StreamLivenessAction.RequestKeyframe)
        assertEquals(StreamLivenessAction.None, watchdog.observe(1_200L, bytesReceived = 11L, framesDecoded = 2L, connected = true))
        assertEquals(StreamLivenessAction.None, watchdog.observe(1_900L, bytesReceived = 11L, framesDecoded = 2L, connected = true))
    }

    @Test
    fun incomingBytesDoNotHideDecoderFrameStall() {
        val watchdog = StreamLivenessWatchdog(
            keyframeAfterMs = 1_000L,
            keyframeIntervalMs = 500L,
            restartAfterMs = 3_000L,
        )

        watchdog.markConnected(0L)
        assertEquals(StreamLivenessAction.None, watchdog.observe(100L, bytesReceived = 10L, framesDecoded = 0L, connected = true))
        assertEquals(StreamLivenessAction.None, watchdog.observe(900L, bytesReceived = 100L, framesDecoded = 0L, connected = true))

        val first = watchdog.observe(1_000L, bytesReceived = 200L, framesDecoded = 0L, connected = true)
        assertTrue(first is StreamLivenessAction.RequestKeyframe)
    }

    @Test
    fun fallsBackToBytesWhenFrameCounterIsMissing() {
        val watchdog = StreamLivenessWatchdog(
            keyframeAfterMs = 1_000L,
            keyframeIntervalMs = 500L,
            restartAfterMs = 3_000L,
        )

        watchdog.markConnected(0L)
        assertEquals(StreamLivenessAction.None, watchdog.observe(900L, bytesReceived = 10L, framesDecoded = null, connected = true))
        assertEquals(StreamLivenessAction.None, watchdog.observe(1_700L, bytesReceived = 20L, framesDecoded = null, connected = true))
        assertEquals(StreamLivenessAction.None, watchdog.observe(2_500L, bytesReceived = 30L, framesDecoded = null, connected = true))
    }
}

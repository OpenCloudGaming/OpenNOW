package com.opencloudgaming.opennow

import org.junit.Assert.assertEquals
import org.junit.Assert.assertNull
import org.junit.Test

class StreamPacketLossTest {
    @Test
    fun delayedStatsCallbacksAreRejected() {
        assertEquals(true, isNewerStreamStatsSample(currentTimestampMs = 2_000.0, previousTimestampMs = 1_000.0))
        assertEquals(false, isNewerStreamStatsSample(currentTimestampMs = 1_000.0, previousTimestampMs = 1_000.0))
        assertEquals(false, isNewerStreamStatsSample(currentTimestampMs = 999.0, previousTimestampMs = 1_000.0))
        assertEquals(false, isNewerStreamStatsSample(currentTimestampMs = Double.NaN, previousTimestampMs = 1_000.0))
    }

    @Test
    fun counterResetDoesNotBecomeAFalseLossSample() {
        assertNull(
            streamPacketDelta(
                currentLost = 0,
                currentReceived = 20,
                previousLost = 4,
                previousReceived = 1_000,
            ),
        )
    }

    @Test
    fun latePacketsCorrectLossWithoutResettingWindow() {
        assertEquals(StreamPacketDelta(lost = -3, received = 100), streamPacketDelta(1, 200, 4, 100))
        val window = StreamPacketLossWindow(maximumSamples = 3, minimumSamples = 3)
        window.add(StreamPacketDelta(lost = 4, received = 100))
        window.add(StreamPacketDelta(lost = -3, received = 100))
        assertEquals(100.0 / 301.0, window.add(StreamPacketDelta(lost = 0, received = 100))!!, 0.0001)
    }

    @Test
    fun negativeCumulativeLossDoesNotCreateNegativeDisplayedLoss() {
        assertEquals(StreamPacketDelta(lost = -3, received = 100), streamPacketDelta(-2, 200, 1, 100))
        val window = StreamPacketLossWindow(maximumSamples = 1, minimumSamples = 1)
        assertEquals(0.0, window.add(StreamPacketDelta(lost = -3, received = 100))!!, 0.0)
    }

    @Test
    fun rollingWindowDoesNotPublishAOneSampleFiftyPercentSpike() {
        val window = StreamPacketLossWindow(maximumSamples = 5, minimumSamples = 3)

        assertNull(window.add(StreamPacketDelta(lost = 1, received = 1)))
        assertNull(window.add(StreamPacketDelta(lost = 0, received = 600)))
        assertEquals(
            1.0 / 1_202.0 * 100.0,
            window.add(StreamPacketDelta(lost = 0, received = 600)) ?: -1.0,
            0.0001,
        )
    }

    @Test
    fun rollingWindowKeepsOnlyRecentSamples() {
        val window = StreamPacketLossWindow(maximumSamples = 3, minimumSamples = 1)

        window.add(StreamPacketDelta(lost = 3, received = 0))
        window.add(StreamPacketDelta(lost = 0, received = 100))
        window.add(StreamPacketDelta(lost = 0, received = 100))
        assertEquals(0.0, window.add(StreamPacketDelta(lost = 0, received = 100)) ?: -1.0, 0.0001)
    }
}

package com.opencloudgaming.opennow

import org.junit.Assert.assertEquals
import org.junit.Assert.assertNull
import org.junit.Test

class StreamNetworkWarningTest {
    @Test
    fun warnsFromMeasuredPacketDeltasWithConciseInternetMessage() {
        val warning = streamNetworkWarning(
            stats = StreamRuntimeStats(
                receivedFps = 70,
                packetLossPct = 3.25,
                packetsLostDelta = 13,
                packetsReceivedDelta = 387,
            ),
        )

        requireNotNull(warning)
        assertEquals("loss", warning.key)
        assertEquals(
            "3.25% packet loss. You may experience lag due to your internet connection.",
            warning.message,
        )
    }

    @Test
    fun signedCorrectionDoesNotInvalidateMeasuredRollingLoss() {
        val warning = streamNetworkWarning(StreamRuntimeStats(
            packetLossPct = 3.25, packetsLostDelta = -2, packetsReceivedDelta = 100,
        ))
        assertEquals("loss", warning?.key)
        assertNull(streamNetworkWarning(StreamRuntimeStats(
            packetLossPct = 0.0, packetsLostDelta = -2, packetsReceivedDelta = 100,
        )))
    }

    @Test
    fun ignoresLossPercentageWithoutUsableCounterDelta() {
        assertNull(
            streamNetworkWarning(
                stats = StreamRuntimeStats(packetLossPct = 9.0),
            ),
        )
    }

    @Test
    fun latencyUsesTheSameConciseInternetMessage() {
        val warning = streamNetworkWarning(
            stats = StreamRuntimeStats(pingMs = 150),
        )

        requireNotNull(warning)
        assertEquals(
            "150 ms latency. You may experience lag due to your internet connection.",
            warning.message,
        )
    }

    @Test
    fun decoderOnlySlowdownDoesNotBlameTheConnection() {
        assertNull(
            streamNetworkWarning(
                stats = StreamRuntimeStats(receivedFps = 120, decodedFps = 30, decodeMs = 35.0),
            ),
        )
    }

    @Test
    fun sustainedPoorMeasurementsShowOnlyOncePerStreamSession() {
        val gate = StreamNetworkWarningGate(minimumConsecutiveSamples = 3)
        val warning = StreamNetworkWarning("latency", "150 ms latency")

        assertNull(gate.update(warning))
        assertNull(gate.update(warning))
        assertEquals(warning, gate.update(warning))
        assertNull(gate.update(warning))
        assertNull(gate.update(null))
        assertNull(gate.update(warning))
        assertNull(gate.update(warning))
        assertNull(gate.update(warning))
    }

    @Test
    fun healthySampleResetsConsecutiveWarningCount() {
        val gate = StreamNetworkWarningGate(minimumConsecutiveSamples = 2)
        val warning = StreamNetworkWarning("jitter", "40 ms jitter")

        assertNull(gate.update(warning))
        assertNull(gate.update(null))
        assertNull(gate.update(warning))
        assertEquals(warning, gate.update(warning))
    }
}

package com.opencloudgaming.opennow
import org.junit.Assert.*
import org.junit.Test
class WebRtcRecoveryWindowTest {
    private fun sample(received: Long, lost: Long = 0, extras: Map<String, Any> = emptyMap()): Map<String, Any> =
        mapOf("packetsReceived" to received, "packetsLost" to lost) + extras
    @Test fun measuresRepairAndBufferIntervals() {
        val window = WebRtcRecoveryWindow()
        window.observe("v", 1000.0, sample(100, 2, mapOf("nackCount" to 3, "retransmittedPacketsReceived" to 10,
            "jitterBufferDelay" to 2.0, "jitterBufferEmittedCount" to 100)))
        val result = window.observe("v", 2000.0, sample(1100, 6, mapOf("nackCount" to 5,
            "retransmittedPacketsReceived" to 14, "jitterBufferDelay" to 3.2, "jitterBufferEmittedCount" to 220)))!!
        assertEquals(4.0, result.deltas["retransmittedPacketsReceived"]!!, 0.0)
        assertEquals(10.0, result.meanMs("jitterBufferDelay", "jitterBufferEmittedCount")!!, 0.0001)
        assertTrue(result.diagnostic().contains("nack=2 rtx=4"))
    }
    @Test fun allowsSignedLossCorrection() {
        val window = WebRtcRecoveryWindow()
        window.observe("v", 1000.0, sample(100, 2))
        val result = window.observe("v", 2000.0, sample(200, -1))!!
        assertEquals(-3.0, result.deltas["packetsLost"]!!, 0.0)
        assertEquals(100.0, result.deltas["packetsReceived"]!!, 0.0)
    }
    @Test fun invalidAndMissingFieldsRemainUnknownWithoutPrivateData() {
        val window = WebRtcRecoveryWindow()
        val fields = mapOf<String, Any>("jitterBufferDelay" to Double.NaN, "fecPacketsReceived" to -1,
            "authToken" to "private-token", "remoteAddress" to "private-address")
        window.observe("private-id", 1000.0, sample(1, extras = fields))
        val result = window.observe("private-id", 2000.0, sample(2, extras = fields))!!
        assertNull(result.deltas["jitterBufferDelay"])
        assertNull(result.deltas["fecPacketsReceived"])
        assertTrue(result.diagnostic().contains("rtx=- fec=-"))
        assertFalse(result.diagnostic().contains("private"))
    }
    @Test fun rejectsDelayedCallbacksWithoutRewindingBaseline() {
        val window = WebRtcRecoveryWindow()
        window.observe("v", 1000.0, sample(100))
        assertNull(window.observe("v", 999.0, sample(0)))
        assertNull(window.observe("v", Double.NaN, sample(0)))
        assertEquals(100.0, window.observe("v", 2000.0, sample(200))!!.deltas["packetsReceived"]!!, 0.0)
    }
    @Test fun identityAndCounterResetsRebase() {
        val window = WebRtcRecoveryWindow()
        window.observe("v", 1000.0, sample(100, extras = mapOf("ssrc" to 1)))
        assertNull(window.observe("v", 2000.0, sample(200, extras = mapOf("ssrc" to 2))))
        assertNull(window.observe("v2", 3000.0, sample(300, extras = mapOf("ssrc" to 2))))
        assertNull(window.observe("v2", 4000.0, sample(0, extras = mapOf("ssrc" to 2))))
        assertEquals(20.0, window.observe("v2", 5000.0, sample(20, extras = mapOf("ssrc" to 2)))!!.deltas["packetsReceived"]!!, 0.0)
        window.reset()
        assertNull(window.observe("v2", 6000.0, sample(30)))
    }
    @Test fun optionalCounterResetIsNotNegativeRepair() {
        val window = WebRtcRecoveryWindow()
        window.observe("v", 1000.0, sample(100, extras = mapOf("nackCount" to 10)))
        val result = window.observe("v", 2000.0, sample(200, extras = mapOf("nackCount" to 0)))!!
        assertNull(result.deltas["nackCount"])
        assertNull(result.meanMs("jitterBufferDelay", "jitterBufferEmittedCount"))
    }
}

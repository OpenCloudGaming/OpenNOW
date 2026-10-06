package com.opencloudgaming.opennow

import org.junit.Assert.assertFalse
import org.junit.Assert.assertTrue
import org.junit.Test

class StreamPacketLossRecoveryGateTest {
    @Test
    fun misleadingDisplayedSpikeDoesNotArmRecovery() {
        val gate = StreamPacketLossRecoveryGate(badSamplesBeforeArmed = 2)
        val misleading = stats(lost = 2, received = 141, displayedLoss = 11.221449851042701)

        repeat(4) { assertFalse(gate.observe(misleading, recoveryEligible = true)) }
        assertFalse(gate.observe(stats(lost = 0, received = 500), recoveryEligible = true))
    }

    @Test
    fun transientLossDoesNotRequestKeyframe() {
        val gate = StreamPacketLossRecoveryGate(badSamplesBeforeArmed = 2)

        assertFalse(gate.observe(stats(lost = 60, received = 940), recoveryEligible = true))
        assertFalse(gate.observe(stats(lost = 0, received = 1_000), recoveryEligible = true))
    }

    @Test
    fun sustainedLossRequestsOneKeyframeAfterPathRecovers() {
        val gate = StreamPacketLossRecoveryGate(badSamplesBeforeArmed = 2, cooldownSamples = 3)
        val bad = stats(lost = 60, received = 940)
        val good = stats(lost = 0, received = 1_000)

        assertFalse(gate.observe(bad, recoveryEligible = true))
        assertFalse(gate.observe(bad, recoveryEligible = true))
        assertTrue(gate.observe(good, recoveryEligible = true))
        assertFalse(gate.observe(good, recoveryEligible = true))
    }

    @Test
    fun signedLossCorrectionAllowsOneRecoveryAfterSustainedBurst() {
        val gate = StreamPacketLossRecoveryGate(badSamplesBeforeArmed = 2, cooldownSamples = 3)
        val bad = stats(lost = 60, received = 940)
        assertFalse(gate.observe(bad, recoveryEligible = true))
        assertFalse(gate.observe(bad, recoveryEligible = true))
        assertTrue(gate.observe(stats(lost = -10, received = 1010), recoveryEligible = true))
        assertFalse(gate.observe(stats(lost = -2, received = 1002), recoveryEligible = true))
    }

    @Test
    fun ineligibleTransportClearsArmedRecovery() {
        val gate = StreamPacketLossRecoveryGate(badSamplesBeforeArmed = 2)
        val bad = stats(lost = 60, received = 940)

        assertFalse(gate.observe(bad, recoveryEligible = true))
        assertFalse(gate.observe(bad, recoveryEligible = true))
        assertFalse(gate.observe(bad, recoveryEligible = false))
        assertFalse(gate.observe(stats(lost = 0, received = 1_000), recoveryEligible = true))
    }

    private fun stats(lost: Long, received: Long, displayedLoss: Double = 0.0): StreamRuntimeStats =
        StreamRuntimeStats(
            packetLossPct = displayedLoss,
            packetsLostDelta = lost,
            packetsReceivedDelta = received,
        )
}

package com.opencloudgaming.opennow

import org.junit.Assert.*
import org.junit.Test

class DecoderLowLatencyPolicyTest {
    @Test fun rejectedFenceProfileFallsBackWithoutLeakingItsKeysOrMaxRate() {
        val attempts = mutableListOf<Map<String, Int>>()
        val failures = mutableListOf<DecoderLatencyProfile>()
        val accepted = tryDecoderLatencyProfiles(decoderLatencyProfiles(true), { p, _ -> failures += p }) { p ->
            val options = decoderLatencyOptions("c2.qti.hevc.decoder", 144, p, true, "SM8850P")
            attempts += options
            if (p == DecoderLatencyProfile.FULL) throw IllegalArgumentException("fences unsupported")
            options
        }
        assertEquals(2, attempts.size)
        assertEquals(listOf(DecoderLatencyProfile.FULL), failures)
        assertEquals(32767, attempts.first()["operating-rate"])
        assertEquals(144, accepted["operating-rate"])
        assertNull(accepted["vendor.qti-ext-output-fence.enable"])
        assertNull(accepted["vendor.qti-ext-dec-picture-order.enable"])
        assertEquals(1, accepted["vendor.qti-ext-dec-low-latency.enable"])
    }
    @Test fun disablingTuningOrRejectingAllVendorKeysRetainsRealtimeOnly() {
        val options = tryDecoderLatencyProfiles(decoderLatencyProfiles(true), { _, _ -> }) { p ->
            if (p != DecoderLatencyProfile.REALTIME) throw IllegalStateException("unsupported")
            decoderLatencyOptions("c2.qti.av1.decoder", 120, p, true, "SM8850P")
        }
        assertEquals(mapOf("priority" to 0, "operating-rate" to 120), options)
        assertEquals(listOf(DecoderLatencyProfile.REALTIME), decoderLatencyProfiles(false))
    }
    @Test fun unusableBaselinePropagatesFailureRatherThanClaimingAWorkingDecoder() {
        val error = IllegalStateException("cannot configure HDR surface")
        try {
            tryDecoderLatencyProfiles<Unit>(decoderLatencyProfiles(true), { _, _ -> }) { throw error }
            fail("failure must propagate")
        } catch (actual: Exception) { assertSame(error, actual) }
    }
    @Test fun unknownAndAdreno620SocNeverRequestMaximumRate() {
        for (soc in listOf("SM7250", "SM7250-AB", "", "unknown")) {
            val options = decoderLatencyOptions("c2.qti.hevc.decoder", 90, DecoderLatencyProfile.FULL, true, soc)
            assertEquals(90, options["operating-rate"])
        }
    }
    @Test fun vendorTuningAndRateCanBeMeasuredIndependentlyWithoutTouchingOtherVendors() {
        val qti = decoderLatencyOptions("c2.qti.av1.decoder", 120, DecoderLatencyProfile.FULL, false, "SM8850P")
        assertEquals(120, qti["operating-rate"])
        assertEquals(1, qti["vendor.qti-ext-dec-instant-decode.enable"])
        val other = decoderLatencyOptions("c2.exynos.hevc.decoder", 120, DecoderLatencyProfile.FULL, false, "")
        assertTrue(other.keys.none { it.startsWith("vendor.qti") })
    }
}

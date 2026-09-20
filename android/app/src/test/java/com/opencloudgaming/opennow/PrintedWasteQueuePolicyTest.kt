package com.opencloudgaming.opennow

import org.junit.Assert.assertFalse
import org.junit.Assert.assertTrue
import org.junit.Test

class PrintedWasteQueuePolicyTest {
    @Test
    fun nvidiaFreeAccountsSeeTheQueueSelector() {
        assertTrue(
            shouldUsePrintedWasteQueue(
                hideServerSelector = false,
                providerCode = "NVIDIA",
                providerStreamingServiceUrl = "https://prod.cloudmatchbeta.nvidiagrid.net/",
                effectiveStreamingBaseUrl = "https://np-lax-03.cloudmatchbeta.nvidiagrid.net/",
                membershipTier = "FREE",
            ),
        )
    }

    @Test
    fun allianceAccountsNeverSeeTheQueueSelector() {
        assertFalse(
            shouldUsePrintedWasteQueue(
                hideServerSelector = false,
                providerCode = "YES",
                providerStreamingServiceUrl = "https://yes.geforcenow.nvidiagrid.net/",
                effectiveStreamingBaseUrl = "https://yes.geforcenow.nvidiagrid.net/",
                membershipTier = "FREE",
            ),
        )
    }

    @Test
    fun allianceServerOverrideIsExcludedForNvidiaAccount() {
        assertFalse(
            shouldUsePrintedWasteQueue(
                hideServerSelector = false,
                providerCode = "NVIDIA",
                providerStreamingServiceUrl = "https://prod.cloudmatchbeta.nvidiagrid.net/",
                effectiveStreamingBaseUrl = "https://cloudmatch.alliance.example/",
                membershipTier = "FREE",
            ),
        )
    }
}

package com.opencloudgaming.opennow

import org.junit.Assert.assertFalse
import org.junit.Assert.assertTrue
import org.junit.Test

class LoginProviderTest {
    @Test
    fun deviceCodeLoginIsAvailableForNvidiaOnly() {
        val nvidia = LoginProvider(
            idpId = "idp-nvidia",
            code = "NVIDIA",
            displayName = "NVIDIA",
            streamingServiceUrl = "https://prod.cloudmatchbeta.nvidiagrid.net/",
        )
        val alliance = LoginProvider(
            idpId = "idp-alliance",
            code = "YES",
            displayName = "YES Malaysia",
            streamingServiceUrl = "https://yes.geforcenow.nvidiagrid.net/",
        )

        assertTrue(nvidia.supportsDeviceCodeLogin)
        assertFalse(alliance.supportsDeviceCodeLogin)
    }
}

package com.opencloudgaming.opennow

import org.junit.Assert.assertEquals
import org.junit.Assert.assertNull
import org.junit.Test

class StreamStatusWifiNameTest {
    @Test
    fun redactedAndMissingNamesAreUnavailable() {
        listOf(null, "", "\"\"", "   ", "<unknown ssid>", "\"<unknown ssid>\"").forEach {
            assertNull(normalizeStreamWifiName(it))
        }
    }

    @Test
    fun androidQuotesAreRemovedWhileNetworkNameContentIsPreserved() {
        assertEquals("Home Wi-Fi", normalizeStreamWifiName("\"Home Wi-Fi\""))
        assertEquals("Café 網路", normalizeStreamWifiName("Café 網路"))
        assertEquals(" Guest ", normalizeStreamWifiName("\" Guest \""))
        assertEquals("Home \"5G\"", normalizeStreamWifiName("\"Home \"5G\"\""))
    }
}

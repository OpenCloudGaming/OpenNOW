package com.opencloudgaming.opennow

import org.junit.Assert.assertEquals
import org.junit.Test

class SdpToolsTest {
    @Test
    fun partiallyReliableGamepadMaskDefaultsToAllControllerSlots() {
        assertEquals(0x0f, SdpTools.parsePartiallyReliableGamepadMask("v=0\n"))
    }

    @Test
    fun partiallyReliableGamepadMaskParsesDecimalAndHexAttributes() {
        assertEquals(
            0x03,
            SdpTools.parsePartiallyReliableGamepadMask("a=ri.enablePartiallyReliableTransferGamepad:3\n"),
        )
        assertEquals(
            0x0f,
            SdpTools.parsePartiallyReliableGamepadMask("a=ri.enablePartiallyReliableTransferGamepad:0x0f\n"),
        )
    }
}

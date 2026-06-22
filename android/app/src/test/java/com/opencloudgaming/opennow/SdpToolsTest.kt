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

    @Test
    fun prefersEightBitH265ProfileForNonHdrAndroidStream() {
        val munged = SdpTools.preferCodec(h265Offer(), StreamSettings(codec = VideoCodec.H265, colorQuality = ColorQuality.EightBit420))

        assertEquals("m=video 9 UDP/TLS/RTP/SAVPF 97 96", munged.lineSequence().first())
    }

    @Test
    fun prefersTenBitH265ProfileForHdrAndroidStream() {
        val munged = SdpTools.preferCodec(
            h265Offer(),
            StreamSettings(codec = VideoCodec.H265, colorQuality = ColorQuality.TenBit420, hdrEnabled = true),
        )

        assertEquals("m=video 9 UDP/TLS/RTP/SAVPF 96 97", munged.lineSequence().first())
    }

    private fun h265Offer(): String =
        """
        m=video 9 UDP/TLS/RTP/SAVPF 96 97 98
        a=rtpmap:96 H265/90000
        a=fmtp:96 profile-id=2
        a=rtpmap:97 H265/90000
        a=fmtp:97 profile-id=1
        a=rtpmap:98 H264/90000
        """.trimIndent()
}

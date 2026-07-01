package com.opencloudgaming.opennow

import org.junit.Assert.assertEquals
import org.junit.Assert.assertFalse
import org.junit.Assert.assertTrue
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

    @Test
    fun rewritesH265TierFlagAndClampsLevelByProfile() {
        val offer = """
            m=video 9 UDP/TLS/RTP/SAVPF 96 97
            a=rtpmap:96 H265/90000
            a=fmtp:96 profile-id=1;tier-flag=1;level-id=186
            a=rtpmap:97 H265/90000
            a=fmtp:97 profile-id=2;tier-flag=1;level-id=255
        """.trimIndent()

        val tier = SdpTools.rewriteH265TierFlag(offer, 0)
        val level = SdpTools.rewriteH265LevelIdByProfile(tier.sdp, mapOf(1 to 153, 2 to 186))

        assertEquals(2, tier.replacements)
        assertEquals(2, level.replacements)
        assertTrue(level.sdp.contains("a=fmtp:96 profile-id=1;tier-flag=0;level-id=153"))
        assertTrue(level.sdp.contains("a=fmtp:97 profile-id=2;tier-flag=0;level-id=186"))
    }

    @Test
    fun detectsNegotiatedVideoCodecInLocalAnswer() {
        val answer = """
            m=audio 9 UDP/TLS/RTP/SAVPF 111
            a=rtpmap:111 opus/48000/2
            m=video 9 UDP/TLS/RTP/SAVPF 96
            a=rtpmap:96 HEVC/90000
        """.trimIndent()

        assertTrue(SdpTools.negotiatesCodec(answer, VideoCodec.H265))
        assertFalse(SdpTools.negotiatesCodec(answer, VideoCodec.AV1))
    }

    @Test
    fun nvstSdpUsesConfiguredResolutionViewport() {
        val nvst = SdpTools.buildNvstSdp(
            offerSdp = "a=ri.partialReliableThresholdMs:42",
            settings = StreamSettings(resolution = "1680x720", aspectRatio = "21:9", codec = VideoCodec.H265),
            localAnswer = """
                a=ice-ufrag:testUfrag
                a=ice-pwd:testPassword
                a=fingerprint:sha-256 11:22:33
            """.trimIndent(),
        )

        assertTrue(nvst.contains("a=video.clientViewportWd:1680"))
        assertTrue(nvst.contains("a=video.clientViewportHt:720"))
        assertTrue(nvst.contains("a=vqos.resControl.cpmRtc.minResolutionPercent:100"))
        assertTrue(nvst.contains("a=vqos.resControl.cpmRtc.resolutionChangeHoldonMs:999999"))
        assertFalse(nvst.contains("a=video.clientViewportWd:1920"))
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

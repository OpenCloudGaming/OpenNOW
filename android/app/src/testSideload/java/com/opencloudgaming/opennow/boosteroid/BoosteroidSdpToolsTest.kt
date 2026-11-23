// Adapted from OpenStroid Android. See the bundled OpenStroid notices.
package com.opencloudgaming.opennow.boosteroid

import com.opencloudgaming.opennow.StreamSettings
import com.opencloudgaming.opennow.VideoCodec

import org.junit.Assert.assertEquals
import org.junit.Assert.assertFalse
import org.junit.Assert.assertTrue
import org.junit.Test

class BoosteroidSdpToolsTest {
    @Test
    fun highResolutionQueryEnablesTvCompatibilityProfile() {
        val query = buildBoosteroidStreamRuntimeQuery(
            StreamSettings(resolution = "3840x2160", fps = 60),
        )

        assertTrue(query.contains("x=3840"))
        assertTrue(query.contains("y=2160"))
        assertTrue(query.contains("refreshRate=60"))
        assertTrue(query.contains("tv=1"))
    }

    @Test
    fun highFrameRateQueryEnablesTvCompatibilityProfile() {
        val query = buildBoosteroidStreamRuntimeQuery(
            StreamSettings(resolution = "1920x1080", fps = 120, codec = VideoCodec.AV1),
        )

        assertTrue(query.contains("refreshRate=120"))
        assertTrue(query.contains("tv=1"))
        assertTrue(query.contains("codec=av1"))
    }

    @Test
    fun standardProfileDoesNotImpersonateTvClient() {
        val query = buildBoosteroidStreamRuntimeQuery(StreamSettings())

        assertFalse(query.contains("tv=1"))
    }

    @Test
    fun runtimeQueryReplacesUpstreamResolutionInsteadOfAppendingDuplicates() {
        val query = mergeBoosteroidStreamRuntimeQuery(
            sessionQuery = "token=signed-value&x=1920&y=1080&refreshRate=60&tv=0",
            settings = StreamSettings(resolution = "3840x2160", fps = 120),
        )

        assertTrue(query.startsWith("token=signed-value&"))
        assertTrue(query.contains("x=3840"))
        assertTrue(query.contains("y=2160"))
        assertTrue(query.contains("refreshRate=120"))
        assertTrue(query.contains("tv=1"))
        assertFalse(query.contains("x=1920"))
        assertFalse(query.contains("y=1080"))
        assertEquals(1, Regex("(?:^|&)x=").findAll(query).count())
        assertEquals(1, Regex("(?:^|&)y=").findAll(query).count())
    }

    @Test
    fun h264SelectionRemovesUnsupportedVideoCodecs() {
        val result = BoosteroidSdpTools.prepareOffer(sampleOffer(), VideoCodec.H264, "H264")

        assertTrue(result.contains("H264/90000"))
        assertTrue(result.contains("opus/48000/2"))
        assertFalse(result.contains("AV1/90000"))
        assertFalse(result.contains("VP8/90000"))
    }

    @Test
    fun av1SelectionFallsBackWhenGatewayRequiresH264() {
        val result = BoosteroidSdpTools.prepareOffer(sampleOffer(), VideoCodec.AV1, "H264")

        assertTrue(result.contains("H264/90000"))
        assertFalse(result.contains("AV1/90000"))
    }

    @Test
    fun av1SelectionKeepsProfileZeroWhenGatewayAllowsIt() {
        val result = BoosteroidSdpTools.prepareOffer(sampleOffer(), VideoCodec.AV1, "AV1")

        assertTrue(result.contains("AV1/90000"))
        assertFalse(result.contains("VP8/90000"))
    }

    @Test
    fun offerKeepsGatewayOpusParametersUnmodified() {
        val result = BoosteroidSdpTools.prepareOffer(sampleOffer(), VideoCodec.H264, "H264")

        assertTrue(result.contains("a=fmtp:111 useinbandfec=1"))
        assertFalse(result.contains("stereo=1"))
        assertFalse(result.contains("maxaveragebitrate="))
    }

    @Test
    fun gatewayHostPreservesPortAndIpv6() {
        assertEquals("stream.example:8443", normalizeBoosteroidGatewayHost("wss://stream.example:8443/?signed=value"))
        assertEquals("[2001:db8::1]:8443", normalizeBoosteroidGatewayHost("https://[2001:db8::1]:8443/"))
    }

    @Test
    fun gatewayHostRejectsInvalidAuthorities() {
        assertEquals("", normalizeBoosteroidGatewayHost("wss://user@stream.example/"))
        assertEquals("", normalizeBoosteroidGatewayHost("wss://stream.example:bad/"))
        assertEquals("", normalizeBoosteroidGatewayHost("file:///tmp/gateway"))
    }

    private fun sampleOffer(): String =
        """
        v=0
        m=video 9 UDP/TLS/RTP/SAVPF 96 97 98 99
        a=rtpmap:96 H264/90000
        a=fmtp:96 profile-level-id=42e01f;packetization-mode=1
        a=rtpmap:97 AV1/90000
        a=fmtp:97 profile=0
        a=rtpmap:98 VP8/90000
        a=rtpmap:99 rtx/90000
        a=fmtp:99 apt=96
        m=audio 9 UDP/TLS/RTP/SAVPF 111
        a=rtpmap:111 opus/48000/2
        a=fmtp:111 useinbandfec=1
        """.trimIndent()
}

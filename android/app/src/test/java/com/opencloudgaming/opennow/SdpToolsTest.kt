package com.opencloudgaming.opennow

import org.junit.Assert.assertEquals
import org.junit.Assert.assertFalse
import org.junit.Assert.assertTrue
import org.junit.Test

class SdpToolsTest {
    @Test
    fun codecSelectionPreservesOfferedFlexfecAndMatchingRetransmissionForEveryCodec() {
        for ((codec, payload) in listOf(VideoCodec.H264 to 99, VideoCodec.H265 to 103, VideoCodec.AV1 to 96)) {
            for (lineEnding in listOf("\n", "\r\n")) {
                val audio = "m=audio 47998 UDP/TLS/RTP/SAVPF 111${lineEnding}a=rtpmap:111 opus/48000/2"
                val application = "m=application 47998 UDP/DTLS/SCTP webrtc-datachannel"
                val offer = listOf(
                    audio,
                    "m=video 47998 UDP/TLS/RTP/SAVPF 99 100 103 104 96 97 98",
                    "a=rtpmap:99 H264/90000",
                    "a=rtpmap:100 rtx/90000",
                    "a=fmtp:100 apt=99",
                    "a=rtpmap:103 H265/90000",
                    "a=fmtp:103 profile-id=1;level-id=153;tier-flag=0",
                    "a=rtpmap:104 rtx/90000",
                    "a=fmtp:104 apt=103",
                    "a=rtpmap:96 AV1/90000",
                    "a=rtpmap:97 rtx/90000",
                    "a=fmtp:97 apt=96",
                    "a=rtpmap:98 flexfec-03/90000",
                    "a=fmtp:98 repair-window=10000000",
                    "a=rtcp-fb:98 transport-cc",
                    "a=rtcp-fb:* nack",
                    "a=ssrc-group:FEC-FR 1234 5678",
                    "a=ssrc:1234 cname:video",
                    "a=ssrc:5678 cname:video",
                    application,
                ).joinToString(lineEnding)

                val filtered = SdpTools.preferCodec(offer, codec)
                val lines = filtered.split(lineEnding)
                assertTrue(lines.contains("m=video 47998 UDP/TLS/RTP/SAVPF $payload ${payload + 1} 98"))
                assertTrue(lines.contains("a=fmtp:${payload + 1} apt=$payload"))
                assertTrue(lines.contains("a=rtpmap:98 flexfec-03/90000"))
                assertTrue(lines.contains("a=fmtp:98 repair-window=10000000"))
                assertTrue(lines.contains("a=rtcp-fb:98 transport-cc"))
                assertTrue(lines.contains("a=rtcp-fb:* nack"))
                assertTrue(lines.contains("a=ssrc-group:FEC-FR 1234 5678"))
                assertTrue(lines.contains("a=ssrc:5678 cname:video"))
                assertTrue(filtered.startsWith(audio + lineEnding))
                assertTrue(filtered.endsWith(application))
                for (other in listOf(99, 103, 96).filter { it != payload }) {
                    assertFalse(filtered.contains("a=rtpmap:$other "))
                    assertFalse(filtered.contains("a=rtpmap:${other + 1} "))
                    assertFalse(filtered.contains("a=fmtp:${other + 1} "))
                }
                assertEquals(filtered, SdpTools.preferCodec(filtered, codec))
            }
        }
    }

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
    fun fixesPlaceholderCandidatesWithSignalingEndpointWhenMediaEndpointIsMissing() {
        val offer = """
            v=0
            c=IN IP4 0.0.0.0
            m=video 47998 UDP/TLS/RTP/SAVPF 96
            a=candidate:1 1 udp 2122260223 0.0.0.0 47998 typ host generation 0
            a=rtpmap:96 H264/90000
        """.trimIndent()

        val fixed = SdpTools.fixServerIp(
            offer,
            serverIp = "66-22-131-132.cloudmatchbeta.nvidiagrid.net",
        )

        assertTrue(fixed.contains("c=IN IP4 66.22.131.132"))
        assertTrue(fixed.contains("a=candidate:1 1 udp 2122260223 66.22.131.132 47998 typ host generation 0"))
    }

    @Test
    fun fixesPlaceholderCandidatesWithCloudMatchMediaEndpoint() {
        val offer = """
            v=0
            c=IN IP4 0.0.0.0
            m=video 47998 UDP/TLS/RTP/SAVPF 96
            a=candidate:1 1 udp 2122260223 0.0.0.0 47998 typ host generation 0
            a=rtpmap:96 H264/90000
        """.trimIndent()

        val fixed = SdpTools.fixServerEndpoint(
            offer,
            serverIp = "183-78-14-231.yes.geforcenow.nvidiagrid.net",
            mediaConnectionInfo = MediaConnectionInfo("183-78-14-231.yes.geforcenow.nvidiagrid.net", 19353),
        )

        assertTrue(fixed.contains("c=IN IP4 183.78.14.231"))
        assertTrue(fixed.contains("a=candidate:1 1 udp 2122260223 183.78.14.231 19353 typ host generation 0"))
    }

    @Test
    fun leavesPrivateCandidatesWithoutCloudMatchMediaEndpoint() {
        val offer = """
            v=0
            c=IN IP4 10.0.175.0
            m=video 47998 UDP/TLS/RTP/SAVPF 96
            a=candidate:1 1 udp 2122260223 10.0.175.0 47998 typ host generation 0
            a=rtpmap:96 H264/90000
        """.trimIndent()

        val fixed = SdpTools.fixServerIp(
            offer,
            serverIp = "183-78-14-231.yes.geforcenow.nvidiagrid.net",
        )

        assertEquals(offer, fixed)
    }

    @Test
    fun fixesPrivateCandidatesWithCloudMatchMediaEndpoint() {
        val offer = """
            v=0
            c=IN IP4 10.0.175.0
            m=video 47998 UDP/TLS/RTP/SAVPF 96
            a=candidate:1 1 udp 2122260223 10.0.175.0 47998 typ host generation 0
            a=rtpmap:96 H264/90000
        """.trimIndent()

        val fixed = SdpTools.fixServerEndpoint(
            offer,
            serverIp = "183-78-14-231.yes.geforcenow.nvidiagrid.net",
            mediaConnectionInfo = MediaConnectionInfo("183.78.14.231", 14317),
        )

        assertTrue(fixed.contains("c=IN IP4 183.78.14.231"))
        assertTrue(fixed.contains("a=candidate:1 1 udp 2122260223 183.78.14.231 14317 typ host generation 0"))
    }

    @Test
    fun leavesResolvedCandidatesOnTheirAdvertisedEndpoint() {
        val offer = """
            v=0
            c=IN IP4 203.0.113.10
            m=video 47998 UDP/TLS/RTP/SAVPF 96
            a=candidate:1 1 udp 2122260223 203.0.113.10 47998 typ host generation 0
            a=rtpmap:96 H264/90000
        """.trimIndent()

        val fixed = SdpTools.fixServerEndpoint(
            offer,
            serverIp = "183-78-14-231.yes.geforcenow.nvidiagrid.net",
            mediaConnectionInfo = MediaConnectionInfo("183-78-14-231.yes.geforcenow.nvidiagrid.net", 19353),
        )

        assertEquals(offer, fixed)
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
        assertTrue(nvst.contains("a=vqos.dynamicStreamingMode:0"))
        assertTrue(nvst.contains("a=vqos.drc.enable:0"))
        assertTrue(nvst.contains("a=vqos.dfc.adjustResAndFps:0"))
        assertTrue(nvst.contains("a=vqos.adjustStreamingFpsDuringOutOfFocus:0"))
        assertFalse(nvst.contains("a=vqos.adjustStreamingFpsDuringOutOfFocus:1"))
        assertTrue(nvst.contains("a=vqos.resControl.cpmRtc.enable:0"))
        assertTrue(nvst.contains("a=vqos.resControl.cpmRtc.minResolutionPercent:100"))
        assertTrue(nvst.contains("a=vqos.resControl.cpmRtc.resolutionChangeHoldonMs:999999"))
        assertTrue(nvst.contains("a=vqos.grc.enable:0"))
        assertTrue(nvst.contains("a=video.scalingFeature1:0"))
        assertFalse(nvst.contains("a=video.clientViewportWd:1920"))
    }

    @Test
    fun nvstSdpHonorsConfiguredBitrateBelowTheNormalFourMbpsFloor() {
        val nvst = buildNvstSdp(StreamSettings(maxBitrateMbps = 1))

        assertTrue(nvst.contains("a=video.initialBitrateKbps:1000"))
        assertTrue(nvst.contains("a=video.initialPeakBitrateKbps:1000"))
        assertTrue(nvst.contains("a=vqos.bw.maximumBitrateKbps:1000"))
        assertTrue(nvst.contains("a=vqos.bw.minimumBitrateKbps:1000"))
        assertTrue(nvst.contains("a=vqos.bw.peakBitrateKbps:1000"))
        assertTrue(nvst.contains("a=vqos.bw.serverPeakBitrateKbps:1000"))
    }

    @Test
    fun nvstSdpKeepsTheNormalFourMbpsMinimumForHigherBitrateProfiles() {
        val nvst = buildNvstSdp(StreamSettings(maxBitrateMbps = 18))

        assertTrue(nvst.contains("a=vqos.bw.maximumBitrateKbps:18000"))
        assertTrue(nvst.contains("a=vqos.bw.minimumBitrateKbps:4000"))
    }

    @Test
    fun everyResolutionCodecAndSupportedFpsProducesFixedGeometrySdp() {
        val modes = STREAM_RESOLUTION_OPTIONS.map { option ->
            Triple(option.value, option.aspectRatio, parseResolutionPixels(option.value))
        }

        for ((resolution, aspectRatio, pixels) in modes) {
            for (codec in VideoCodec.entries) {
                for (fps in listOf(60, 120, 240, 360)) {
                    val settings = StreamSettings(
                        resolution = resolution,
                        aspectRatio = aspectRatio,
                        fps = fps,
                        codec = codec,
                        colorQuality = if (codec == VideoCodec.H264) ColorQuality.EightBit420 else ColorQuality.TenBit420,
                    )
                    val preferred = SdpTools.preferCodec(allCodecOffer(), settings)
                    val nvst = SdpTools.buildNvstSdp(
                        offerSdp = preferred,
                        settings = settings,
                        localAnswer = """
                            a=ice-ufrag:testUfrag
                            a=ice-pwd:testPassword
                            a=fingerprint:sha-256 11:22:33
                        """.trimIndent(),
                    )

                    val case = "$resolution $codec ${fps}fps"
                    assertTrue("$case was not preferred", SdpTools.negotiatesCodec(preferred, codec))
                    assertTrue("$case width missing", nvst.contains("a=video.clientViewportWd:${pixels.first}"))
                    assertTrue("$case height missing", nvst.contains("a=video.clientViewportHt:${pixels.second}"))
                    assertTrue("$case fps missing", nvst.contains("a=video.maxFPS:$fps"))
                    if (fps > 60) {
                        assertTrue("$case FPS estimate missing", nvst.contains("a=vqos.maxStreamFpsEstimate:$fps"))
                    }
                    assertTrue("$case scaling must remain disabled", nvst.contains("a=video.scalingFeature1:0"))
                    assertFalse("$case must not enable scaling", nvst.contains("a=video.scalingFeature1:1"))
                }
            }
        }
    }

    @Test
    fun nvstSdpDisablesHdrForSdrStream() {
        val nvst = buildNvstSdp(StreamSettings(hdrEnabled = false))

        assertTrue(nvst.contains("a=video.dx9EnableHdr:0"))
        assertFalse(nvst.contains("a=video.dx9EnableHdr:1"))
    }

    @Test
    fun nvstSdpEnablesHdrOnlyForHdrStream() {
        val nvst = buildNvstSdp(StreamSettings(codec = VideoCodec.H265, hdrEnabled = true))

        assertTrue(nvst.contains("a=video.dx9EnableHdr:1"))
        assertFalse(nvst.contains("a=video.dx9EnableHdr:0"))
    }

    @Test
    fun nvstSdpCarriesRequested360FpsEstimate() {
        val nvst = SdpTools.buildNvstSdp(
            offerSdp = "a=ri.partialReliableThresholdMs:42",
            settings = StreamSettings(resolution = "1920x1080", aspectRatio = "16:9", fps = 360, codec = VideoCodec.AV1),
            localAnswer = """
                a=ice-ufrag:testUfrag
                a=ice-pwd:testPassword
                a=fingerprint:sha-256 11:22:33
            """.trimIndent(),
        )

        assertTrue(nvst.contains("a=video.maxFPS:360"))
        assertTrue(nvst.contains("a=vqos.maxStreamFpsEstimate:360"))
        assertTrue(nvst.contains("a=video.framePacing.mode:2"))
        assertTrue(nvst.contains("a=video.framePacing.pid.minTargetFrameTimeUs:2638"))
        assertTrue(nvst.contains("a=packetPacing.version:3"))
        assertTrue(nvst.contains("a=packetPacing.enableAccurateSleep:1"))
        assertTrue(nvst.contains("a=video.videoSplitEncodeStripsPerFrame:3"))
    }

    @Test
    fun nvstSdpCarriesRequested120FpsEstimate() {
        val nvst = buildNvstSdp(StreamSettings(fps = 120, codec = VideoCodec.H265))

        assertTrue(nvst.contains("a=video.maxFPS:120"))
        assertTrue(nvst.contains("a=vqos.maxStreamFpsEstimate:120"))
    }

    @Test
    fun nvstSdpUsesWideSplitEncodeFor1440p240Av1Only() {
        val av1 = buildNvstSdp(
            StreamSettings(resolution = "2560x1440", aspectRatio = "16:9", fps = 240, codec = VideoCodec.AV1),
        )
        val h265 = buildNvstSdp(
            StreamSettings(resolution = "2560x1440", aspectRatio = "16:9", fps = 240, codec = VideoCodec.H265),
        )

        assertTrue(av1.contains("a=video.videoSplitEncodeStripsPerFrame:63"))
        assertTrue(h265.contains("a=video.videoSplitEncodeStripsPerFrame:3"))
        assertTrue(av1.contains("a=vqos.bllFec.enable:0"))
        assertTrue(h265.contains("a=vqos.bllFec.enable:0"))
    }

    @Test
    fun partiallyReliableAbsoluteMouseRequiresBothNegotiatedHidMasks() {
        val absoluteMouseMask = 1 shl InputEncoder.INPUT_MOUSE_ABS

        assertTrue(
            SdpTools.supportsPartiallyReliableHidInput(
                hidDeviceMask = absoluteMouseMask,
                partiallyReliableHidMask = absoluteMouseMask,
                inputType = InputEncoder.INPUT_MOUSE_ABS,
            ),
        )
        assertFalse(
            SdpTools.supportsPartiallyReliableHidInput(
                hidDeviceMask = absoluteMouseMask,
                partiallyReliableHidMask = 0,
                inputType = InputEncoder.INPUT_MOUSE_ABS,
            ),
        )
    }

    private fun buildNvstSdp(settings: StreamSettings): String =
        SdpTools.buildNvstSdp(
            offerSdp = "a=ri.partialReliableThresholdMs:42",
            settings = settings,
            localAnswer = """
                a=ice-ufrag:testUfrag
                a=ice-pwd:testPassword
                a=fingerprint:sha-256 11:22:33
            """.trimIndent(),
        )

    private fun h265Offer(): String =
        """
        m=video 9 UDP/TLS/RTP/SAVPF 96 97 98
        a=rtpmap:96 H265/90000
        a=fmtp:96 profile-id=2
        a=rtpmap:97 H265/90000
        a=fmtp:97 profile-id=1
        a=rtpmap:98 H264/90000
        """.trimIndent()

    private fun allCodecOffer(): String =
        """
        m=video 9 UDP/TLS/RTP/SAVPF 96 97 98 99 100 101
        a=rtpmap:96 H264/90000
        a=rtpmap:97 rtx/90000
        a=fmtp:97 apt=96
        a=rtpmap:98 H265/90000
        a=fmtp:98 profile-id=2;tier-flag=0;level-id=153
        a=rtpmap:99 rtx/90000
        a=fmtp:99 apt=98
        a=rtpmap:100 AV1/90000
        a=rtpmap:101 rtx/90000
        a=fmtp:101 apt=100
        """.trimIndent()
}

package com.opencloudgaming.opennow

import org.junit.Assert.assertEquals
import org.junit.Assert.assertFalse
import org.junit.Assert.assertTrue
import org.junit.Test

class AndroidRecommendedProfileTest {
    @Test
    fun constrainedDeviceUsesSafe720pThirtyProfile() {
        val recommendation = recommendedAndroidStreamProfile(
            displayWidth = 2560,
            displayHeight = 1440,
            processorCount = 4,
            totalMemoryMiB = 2_048,
            androidTvProfile = false,
            report = codecReport(
                lowPower = true,
                constrained = true,
                h264Max = 3840 to 2160,
                h265Max = 3840 to 2160,
            ),
        )

        assertEquals("1280x720", recommendation.stream.resolution)
        assertEquals(30, recommendation.stream.fps)
        assertEquals(12, recommendation.stream.maxBitrateMbps)
        assertEquals(VideoCodec.H264, recommendation.stream.codec)
    }

    @Test
    fun highEndDeviceUsesVerifiedH265For1440p() {
        val recommendation = recommendedAndroidStreamProfile(
            displayWidth = 2560,
            displayHeight = 1440,
            processorCount = 8,
            totalMemoryMiB = 8_192,
            androidTvProfile = false,
            report = codecReport(
                h264Max = 1920 to 1080,
                h265Max = 2560 to 1440,
            ),
        )

        assertEquals("2560x1440", recommendation.stream.resolution)
        assertEquals(60, recommendation.stream.fps)
        assertEquals(45, recommendation.stream.maxBitrateMbps)
        assertEquals(VideoCodec.H265, recommendation.stream.codec)
    }

    @Test
    fun recommendationDropsResolutionWhenNoVerifiedDecoderSupportsDisplayMaximum() {
        val recommendation = recommendedAndroidStreamProfile(
            displayWidth = 2560,
            displayHeight = 1440,
            processorCount = 8,
            totalMemoryMiB = 8_192,
            androidTvProfile = false,
            report = codecReport(
                h264Max = 1920 to 1080,
                h265Max = 1920 to 1080,
            ),
        )

        assertEquals("1920x1080", recommendation.stream.resolution)
        assertEquals(VideoCodec.H264, recommendation.stream.codec)
    }

    @Test
    fun verifiedHardwareH265BeatsSoftwareH264At1080p() {
        val report = RuntimeCodecReport(
            capabilities = listOf(
                hardwareCapability(VideoCodec.H264, 1920 to 1080, hardware = false),
                hardwareCapability(VideoCodec.H265, 1920 to 1080),
            ),
            nativeRuntimeSummary = "test",
            androidTvProfile = false,
            lowPowerGpuProfile = false,
        )

        val recommendation = recommendedAndroidStreamProfile(
            displayWidth = 1920,
            displayHeight = 1080,
            processorCount = 8,
            totalMemoryMiB = 6_144,
            androidTvProfile = false,
            report = report,
        )

        assertEquals("1920x1080", recommendation.stream.resolution)
        assertEquals(VideoCodec.H265, recommendation.stream.codec)
    }

    @Test
    fun shieldCanUseVerifiedFourKDecoderPath() {
        val recommendation = recommendedAndroidStreamProfile(
            displayWidth = 3840,
            displayHeight = 2160,
            processorCount = 8,
            totalMemoryMiB = 3_072,
            androidTvProfile = true,
            nvidiaShieldTv = true,
            report = codecReport(
                androidTv = true,
                h264Max = 3840 to 2160,
                h265Max = 3840 to 2160,
            ),
        )

        assertEquals("3840x2160", recommendation.stream.resolution)
        assertEquals(VideoCodec.H265, recommendation.stream.codec)
        assertEquals(75, recommendation.stream.maxBitrateMbps)
    }

    @Test
    fun customProfileListsOnlyPerformanceChoicesAboveRecommendation() {
        val recommended = StreamSettings(
            resolution = "1920x1080",
            aspectRatio = "16:9",
            fps = 60,
            maxBitrateMbps = 35,
            codec = VideoCodec.H264,
            colorQuality = ColorQuality.EightBit420,
        )
        val selected = recommended.copy(
            resolution = "2560x1440",
            fps = 120,
            maxBitrateMbps = 75,
            hdrEnabled = true,
            colorQuality = ColorQuality.TenBit420,
            streamSharpeningEnabled = true,
        )

        val overrides = selected.performanceOverridesComparedTo(recommended, report = null)

        assertTrue(overrides.any { it.startsWith("2560x1440 resolution") })
        assertTrue(overrides.any { it.startsWith("120 FPS") })
        assertTrue(overrides.any { it.startsWith("75 Mbps bitrate") })
        assertTrue(overrides.contains("HDR"))
        assertFalse(overrides.contains("10-bit color"))
        assertTrue(overrides.contains("stream sharpening"))
        assertTrue(recommended.performanceOverridesComparedTo(recommended, report = null).isEmpty())
    }

    private fun codecReport(
        androidTv: Boolean = false,
        lowPower: Boolean = false,
        constrained: Boolean = false,
        h264Max: Pair<Int, Int>,
        h265Max: Pair<Int, Int>,
    ): RuntimeCodecReport = RuntimeCodecReport(
        capabilities = listOf(
            hardwareCapability(VideoCodec.H264, h264Max),
            hardwareCapability(VideoCodec.H265, h265Max),
        ),
        nativeRuntimeSummary = "test",
        androidTvProfile = androidTv,
        lowPowerGpuProfile = lowPower,
        constrainedRuntimeProfile = constrained,
    )

    private fun hardwareCapability(
        codec: VideoCodec,
        maximum: Pair<Int, Int>,
        hardware: Boolean = true,
    ): CodecCapability = CodecCapability(
        codec = codec,
        decoderAvailable = true,
        encoderAvailable = false,
        hardwareDecoder = hardware,
        hardwareEncoder = false,
        nativeDecoderAvailable = hardware,
        webRtcDecoderAvailable = true,
        webRtcHardwareDecoderAvailable = hardware,
        maxSupportedWidth = maximum.first,
        maxSupportedHeight = maximum.second,
    )
}

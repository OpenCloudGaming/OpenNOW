package com.opencloudgaming.opennow

import org.junit.Assert.assertEquals
import org.junit.Assert.assertFalse
import org.junit.Assert.assertTrue
import org.junit.Test

class StreamSettingsDeviceAdjustmentTest {
    @Test
    fun preservesSelectedH265WhenWebRtcHardwareDecoderExists() {
        val adjusted = StreamSettings(codec = VideoCodec.H265, colorQuality = ColorQuality.TenBit420)
            .adjustedForDevice(
                codecReport(
                    VideoCodec.H265,
                    hardwareDecoder = true,
                    realtimeSafe = true,
                    webRtcDecoderAvailable = true,
                    webRtcHardwareDecoderAvailable = true,
                ),
            )

        assertEquals(VideoCodec.H265, adjusted.codec)
        assertEquals(ColorQuality.EightBit420, adjusted.colorQuality)
    }

    @Test
    fun preservesSelectedAv1WhenWebRtcHardwareDecoderExists() {
        val adjusted = StreamSettings(codec = VideoCodec.AV1, colorQuality = ColorQuality.TenBit420)
            .adjustedForDevice(
                codecReport(
                    VideoCodec.AV1,
                    hardwareDecoder = true,
                    realtimeSafe = true,
                    webRtcDecoderAvailable = true,
                    webRtcHardwareDecoderAvailable = true,
                ),
            )

        assertEquals(VideoCodec.AV1, adjusted.codec)
        assertEquals(ColorQuality.EightBit420, adjusted.colorQuality)
    }

    @Test
    fun knownAmlogicAv1DecoderUsesH265For1440p() {
        val av1 = codecReport(
            VideoCodec.AV1,
            hardwareDecoder = true,
            realtimeSafe = true,
            webRtcDecoderAvailable = true,
            webRtcHardwareDecoderAvailable = true,
            webRtcDecoderName = "OMX.amlogic.av1.decoder.awesome",
            maxSupportedWidth = 3840,
            maxSupportedHeight = 2160,
        ).capabilities.single()
        val h265 = codecReport(
            VideoCodec.H265,
            hardwareDecoder = true,
            realtimeSafe = true,
            webRtcDecoderAvailable = true,
            webRtcHardwareDecoderAvailable = true,
            maxSupportedWidth = 3840,
            maxSupportedHeight = 2160,
        ).capabilities.single()
        val report = RuntimeCodecReport(
            capabilities = listOf(av1, h265),
            nativeRuntimeSummary = "{}",
            androidTvProfile = true,
            lowPowerGpuProfile = false,
        )

        val adjusted = StreamSettings(
            resolution = "2560x1440",
            aspectRatio = "16:9",
            codec = VideoCodec.AV1,
        ).adjustedForDevice(report)

        assertEquals("2560x1440", adjusted.resolution)
        assertEquals(VideoCodec.H265, adjusted.codec)
    }

    @Test
    fun knownAmlogicAv1DecoderRemainsAvailableAt1080p() {
        val adjusted = StreamSettings(
            resolution = "1920x1080",
            aspectRatio = "16:9",
            codec = VideoCodec.AV1,
        ).adjustedForDevice(
            codecReport(
                VideoCodec.AV1,
                hardwareDecoder = true,
                realtimeSafe = true,
                webRtcDecoderAvailable = true,
                webRtcHardwareDecoderAvailable = true,
                webRtcDecoderName = "OMX.amlogic.av1.decoder.awesome",
                maxSupportedWidth = 3840,
                maxSupportedHeight = 2160,
            ),
        )

        assertEquals(VideoCodec.AV1, adjusted.codec)
    }

    @Test
    fun av1DropsChroma444BeforeLaunch() {
        val adjusted = StreamSettings(codec = VideoCodec.AV1, colorQuality = ColorQuality.EightBit444)
            .adjustedForDevice(
                codecReport(
                    VideoCodec.AV1,
                    hardwareDecoder = true,
                    realtimeSafe = true,
                    webRtcDecoderAvailable = true,
                    webRtcHardwareDecoderAvailable = true,
                ),
            )

        assertEquals(VideoCodec.AV1, adjusted.codec)
        assertEquals(ColorQuality.EightBit420, adjusted.colorQuality)
    }

    @Test
    fun av1DowngradesTenBitAndDisablesHdrBeforeLaunch() {
        val adjusted = StreamSettings(codec = VideoCodec.AV1, colorQuality = ColorQuality.TenBit444, hdrEnabled = true)
            .adjustedForDevice(
                codecReport(
                    VideoCodec.AV1,
                    hardwareDecoder = true,
                    realtimeSafe = true,
                    webRtcDecoderAvailable = true,
                    webRtcHardwareDecoderAvailable = true,
                ),
            )

        assertEquals(VideoCodec.AV1, adjusted.codec)
        assertEquals(ColorQuality.EightBit420, adjusted.colorQuality)
        assertFalse(adjusted.hdrEnabled)
    }

    @Test
    fun androidSettingsAvailabilityAllowsCodecsAndWithholdsChroma444() {
        assertTrue(VideoCodec.AV1.availableForAndroidSettings())
        assertTrue(VideoCodec.H264.availableForAndroidSettings())
        assertTrue(VideoCodec.H265.availableForAndroidSettings())
        assertFalse(ColorQuality.EightBit444.availableForCodec(VideoCodec.H265))
        assertFalse(ColorQuality.TenBit444.availableForCodec(VideoCodec.H265))
        assertTrue(ColorQuality.EightBit420.availableForCodec(VideoCodec.H265))
        assertTrue(ColorQuality.TenBit420.availableForCodec(VideoCodec.H265))
        assertTrue(ColorQuality.EightBit420.availableForCodec(VideoCodec.AV1))
        assertFalse(ColorQuality.TenBit420.availableForCodec(VideoCodec.AV1))
    }

    @Test
    fun chroma444SettingsNormalizeTo420ForAndroid() {
        val adjusted = StreamSettings(codec = VideoCodec.H265, colorQuality = ColorQuality.TenBit444)
            .withCodecColorCompatibility()

        assertEquals(VideoCodec.H265, adjusted.codec)
        assertEquals(ColorQuality.TenBit420, adjusted.colorQuality)
    }

    @Test
    fun av1SettingsNormalizePersistedTenBitHdrToEightBitSdr() {
        val adjusted = StreamSettings(
            codec = VideoCodec.AV1,
            colorQuality = ColorQuality.TenBit420,
            hdrEnabled = true,
        ).withCodecColorCompatibility()

        assertEquals(VideoCodec.AV1, adjusted.codec)
        assertEquals(ColorQuality.EightBit420, adjusted.colorQuality)
        assertFalse(adjusted.hdrEnabled)
        assertFalse(adjusted.usesTenBitStreamProfile())
    }

    @Test
    fun streamPresetsApplyExpectedAndroidProfiles() {
        val base = StreamSettings(
            aspectRatio = "21:9",
            resolution = "1680x720",
            codec = VideoCodec.AV1,
            colorQuality = ColorQuality.EightBit444,
            enableL4S = true,
        )

        val custom = base.applyingStreamPreset(StreamPreset.Custom)
        assertTrue(custom.enableL4S)

        val low = base.applyingStreamPreset(StreamPreset.LowDataSaver)
        assertEquals("1680x720", low.resolution)
        assertEquals("21:9", low.aspectRatio)
        assertEquals(30, low.fps)
        assertEquals(12, low.maxBitrateMbps)
        assertEquals(VideoCodec.AV1, low.codec)
        assertEquals(ColorQuality.EightBit420, low.colorQuality)
        assertFalse(low.enableL4S)

        val medium = base.applyingStreamPreset(StreamPreset.Medium)
        assertEquals("2560x1080", medium.resolution)
        assertEquals(60, medium.fps)
        assertEquals(35, medium.maxBitrateMbps)
        assertFalse(medium.enableL4S)

        val high = base.applyingStreamPreset(StreamPreset.High)
        assertEquals("3440x1440", high.resolution)
        assertEquals(360, high.fps)
        assertEquals(75, high.maxBitrateMbps)
        assertFalse(high.enableL4S)

        val recommended = base.applyingStreamPreset(StreamPreset.Recommended)
        assertFalse(recommended.enableL4S)
    }

    @Test
    fun preservesUltimate360FpsAt5kForStableAndroidProfile() {
        val adjusted = StreamSettings(resolution = "5120x2880", aspectRatio = "16:9", fps = 360, codec = VideoCodec.AV1)
            .adjustedForDevice(
                codecReport(
                    VideoCodec.AV1,
                    hardwareDecoder = true,
                    realtimeSafe = true,
                    webRtcDecoderAvailable = true,
                    webRtcHardwareDecoderAvailable = true,
                ),
            )

        assertEquals("5120x2880", adjusted.resolution)
        assertEquals(360, adjusted.fps)
    }

    @Test
    fun forcesEightBitWhenHdrIsEnabled() {
        val adjusted = StreamSettings(codec = VideoCodec.H265, colorQuality = ColorQuality.TenBit420, hdrEnabled = true)
            .adjustedForDevice(
                codecReport(
                    VideoCodec.H265,
                    hardwareDecoder = true,
                    realtimeSafe = true,
                    webRtcDecoderAvailable = true,
                    webRtcHardwareDecoderAvailable = true,
                ),
            )

        assertEquals(VideoCodec.H265, adjusted.codec)
        assertEquals(ColorQuality.EightBit420, adjusted.colorQuality)
        assertFalse(adjusted.usesTenBitStreamProfile())
    }

    @Test
    fun fallsBackToH264WhenSelectedDecoderHasNoHardwarePath() {
        val adjusted = StreamSettings(codec = VideoCodec.AV1, colorQuality = ColorQuality.TenBit420, maxBitrateMbps = 90)
            .adjustedForDevice(codecReport(VideoCodec.AV1, hardwareDecoder = false, realtimeSafe = false))

        assertEquals(VideoCodec.H264, adjusted.codec)
        assertEquals(ColorQuality.EightBit420, adjusted.colorQuality)
        assertEquals(90, adjusted.maxBitrateMbps)
    }

    @Test
    fun fallsBackToH264WhenH265WebRtcDecoderIsSoftwareOnly() {
        val adjusted = StreamSettings(codec = VideoCodec.H265, colorQuality = ColorQuality.TenBit420, maxBitrateMbps = 90)
            .adjustedForDevice(
                codecReport(
                    VideoCodec.H265,
                    hardwareDecoder = true,
                    realtimeSafe = true,
                    webRtcDecoderAvailable = true,
                    webRtcHardwareDecoderAvailable = false,
                ),
            )

        assertEquals(VideoCodec.H264, adjusted.codec)
        assertEquals(ColorQuality.EightBit420, adjusted.colorQuality)
        assertEquals(90, adjusted.maxBitrateMbps)
    }

    @Test
    fun preservesSelectedResolutionWhenFallingBackToH264() {
        val adjusted = StreamSettings(
            resolution = "1680x720",
            aspectRatio = "21:9",
            codec = VideoCodec.H265,
            colorQuality = ColorQuality.TenBit420,
            maxBitrateMbps = 90,
        )
            .adjustedForDevice(
                codecReport(
                    VideoCodec.H265,
                    hardwareDecoder = true,
                    realtimeSafe = true,
                    webRtcDecoderAvailable = true,
                    webRtcHardwareDecoderAvailable = false,
                ),
            )

        assertEquals(VideoCodec.H264, adjusted.codec)
        assertEquals(ColorQuality.EightBit420, adjusted.colorQuality)
        assertEquals("1680x720", adjusted.resolution)
        assertEquals("21:9", adjusted.aspectRatio)
        assertEquals(90, adjusted.maxBitrateMbps)
    }

    @Test
    fun fallsBackToH264WhenH265OnlyHasPlatformHardwareDecoder() {
        val adjusted = StreamSettings(codec = VideoCodec.H265, colorQuality = ColorQuality.TenBit420, maxBitrateMbps = 90)
            .adjustedForDevice(codecReport(VideoCodec.H265, hardwareDecoder = true, realtimeSafe = true))

        assertEquals(VideoCodec.H264, adjusted.codec)
        assertEquals(ColorQuality.EightBit420, adjusted.colorQuality)
        assertEquals(90, adjusted.maxBitrateMbps)
    }

    @Test
    fun preservesH265WhenWebRtcHardwareDecoderWorksButNativeProbeFails() {
        val adjusted = StreamSettings(codec = VideoCodec.H265, colorQuality = ColorQuality.TenBit420, maxBitrateMbps = 90)
            .adjustedForDevice(
                codecReport(
                    VideoCodec.H265,
                    hardwareDecoder = true,
                    realtimeSafe = true,
                    nativeDecoderAvailable = false,
                    webRtcDecoderAvailable = true,
                    webRtcHardwareDecoderAvailable = true,
                ),
            )

        assertEquals(VideoCodec.H265, adjusted.codec)
        assertEquals(ColorQuality.EightBit420, adjusted.colorQuality)
        assertEquals(90, adjusted.maxBitrateMbps)
    }

    @Test
    fun changingResolutionDoesNotForceAWebRtcHardwareCodecBackToH264() {
        val report = codecReport(
            VideoCodec.H265,
            hardwareDecoder = true,
            realtimeSafe = true,
            nativeDecoderAvailable = false,
            webRtcDecoderAvailable = true,
            webRtcHardwareDecoderAvailable = true,
        )

        listOf("1280x720", "1920x1080", "2560x1440", "3840x2160").forEach { resolution ->
            val adjusted = StreamSettings(
                resolution = resolution,
                aspectRatio = "16:9",
                codec = VideoCodec.H265,
            ).adjustedForDevice(report)

            assertEquals("$resolution should retain the selected codec", VideoCodec.H265, adjusted.codec)
        }
    }

    @Test
    fun selectsAv1WhenNativeAndWebRtcHardwarePathsExist() {
        val adjusted = StreamSettings(codec = VideoCodec.AV1, colorQuality = ColorQuality.TenBit420)
            .adjustedForDevice(
                codecReport(
                    VideoCodec.AV1,
                    hardwareDecoder = true,
                    realtimeSafe = true,
                    nativeDecoderAvailable = true,
                    webRtcDecoderAvailable = true,
                    webRtcHardwareDecoderAvailable = true,
                ),
            )

        assertEquals(VideoCodec.AV1, adjusted.codec)
        assertEquals(ColorQuality.EightBit420, adjusted.colorQuality)
    }

    @Test
    fun keepsSelectedCodecOnLowPowerDevicesWhenWebRtcHardwareDecoderExists() {
        val adjusted = StreamSettings(codec = VideoCodec.H265, colorQuality = ColorQuality.TenBit420, maxBitrateMbps = 90)
            .adjustedForDevice(
                codecReport(
                    VideoCodec.H265,
                    hardwareDecoder = true,
                    realtimeSafe = true,
                    lowPower = true,
                    webRtcDecoderAvailable = true,
                    webRtcHardwareDecoderAvailable = true,
                ),
            )

        assertEquals(VideoCodec.H265, adjusted.codec)
        assertEquals(ColorQuality.EightBit420, adjusted.colorQuality)
        assertEquals(90, adjusted.maxBitrateMbps)
    }

    @Test
    fun preservesHighRefreshRateForSupportedAndroidStreams() {
        val adjusted = StreamSettings(codec = VideoCodec.AV1, fps = 120, maxBitrateMbps = 90)
            .adjustedForDevice(
                codecReport(
                    VideoCodec.AV1,
                    hardwareDecoder = true,
                    realtimeSafe = true,
                    webRtcDecoderAvailable = true,
                    webRtcHardwareDecoderAvailable = true,
                ),
            )

        assertEquals(VideoCodec.AV1, adjusted.codec)
        assertEquals(120, adjusted.fps)
        assertEquals(90, adjusted.maxBitrateMbps)
    }

    @Test
    fun preservesSelectedH264BitrateCeiling() {
        val adjusted = StreamSettings(codec = VideoCodec.H264, maxBitrateMbps = 150)
            .adjustedForDevice(codecReport(VideoCodec.H264, hardwareDecoder = true, realtimeSafe = true))

        assertEquals(VideoCodec.H264, adjusted.codec)
        assertEquals(150, adjusted.maxBitrateMbps)
    }

    @Test
    fun stabilizesExtremeH264CloudMatchProfileBeforeLaunch() {
        val adjusted = StreamSettings(
            resolution = "5120x1440",
            aspectRatio = "32:9",
            fps = 240,
            maxBitrateMbps = 150,
            codec = VideoCodec.H264,
            colorQuality = ColorQuality.TenBit444,
            hdrEnabled = true,
            enableL4S = true,
            streamSharpeningEnabled = true,
        ).adjustedForDevice(codecReport(VideoCodec.H264, hardwareDecoder = true, realtimeSafe = true))

        assertEquals("5120x1440", adjusted.resolution)
        assertEquals("32:9", adjusted.aspectRatio)
        assertEquals(240, adjusted.fps)
        assertEquals(150, adjusted.maxBitrateMbps)
        assertEquals(VideoCodec.H264, adjusted.codec)
        assertEquals(ColorQuality.EightBit420, adjusted.colorQuality)
        assertEquals(false, adjusted.hdrEnabled)
        assertEquals(true, adjusted.enableL4S)
    }

    @Test
    fun stabilizesExtremeH265CloudMatchProfileWithoutDroppingAdvancedFeatures() {
        val adjusted = StreamSettings(
            resolution = "5120x2160",
            aspectRatio = "21:9",
            fps = 240,
            maxBitrateMbps = 150,
            codec = VideoCodec.H265,
            colorQuality = ColorQuality.TenBit420,
            hdrEnabled = true,
            enableL4S = true,
            streamSharpeningEnabled = true,
        ).adjustedForDevice(
            codecReport(
                VideoCodec.H265,
                hardwareDecoder = true,
                realtimeSafe = true,
                nativeDecoderAvailable = true,
                webRtcDecoderAvailable = true,
                webRtcHardwareDecoderAvailable = true,
            ),
        )

        assertEquals("5120x2160", adjusted.resolution)
        assertEquals("21:9", adjusted.aspectRatio)
        assertEquals(240, adjusted.fps)
        assertEquals(150, adjusted.maxBitrateMbps)
        assertEquals(VideoCodec.H265, adjusted.codec)
        assertEquals(ColorQuality.EightBit420, adjusted.colorQuality)
        assertEquals(false, adjusted.hdrEnabled)
        assertEquals(true, adjusted.enableL4S)
    }

    @Test
    fun safeVideoFallbackChangesCodecWithoutChangingRequested4kGeometry() {
        val fallback = StreamSettings(
            resolution = "3840x2160",
            aspectRatio = "16:9",
            fps = 120,
            maxBitrateMbps = 150,
            codec = VideoCodec.H265,
            colorQuality = ColorQuality.TenBit420,
            hdrEnabled = true,
            streamSharpeningEnabled = true,
        ).androidSafeVideoFallback()

        assertEquals("3840x2160", fallback.resolution)
        assertEquals("16:9", fallback.aspectRatio)
        assertEquals(60, fallback.fps)
        assertEquals(150, fallback.maxBitrateMbps)
        assertEquals(VideoCodec.H264, fallback.codec)
        assertEquals(ColorQuality.EightBit420, fallback.colorQuality)
        assertEquals(false, fallback.hdrEnabled)
        assertEquals(false, fallback.streamSharpeningEnabled)
    }

    @Test
    fun safeVideoFallbackPreservesLaunchResolutionInsideH264Bounds() {
        val fallback = StreamSettings(
            resolution = "1680x720",
            aspectRatio = "21:9",
            fps = 60,
            maxBitrateMbps = 75,
            codec = VideoCodec.H265,
            colorQuality = ColorQuality.TenBit420,
            hdrEnabled = true,
            streamSharpeningEnabled = true,
        ).androidSafeVideoFallback()

        assertEquals("1680x720", fallback.resolution)
        assertEquals("21:9", fallback.aspectRatio)
        assertEquals(60, fallback.fps)
        assertEquals(75, fallback.maxBitrateMbps)
        assertEquals(VideoCodec.H264, fallback.codec)
        assertEquals(ColorQuality.EightBit420, fallback.colorQuality)
    }

    @Test
    fun safeVideoFallbackPreservesEveryKnownResolutionAndAspect() {
        STREAM_RESOLUTION_OPTIONS.forEach { option ->
            val fallback = StreamSettings(
                resolution = option.value,
                aspectRatio = option.aspectRatio,
                fps = 240,
                maxBitrateMbps = 150,
                codec = VideoCodec.AV1,
                colorQuality = ColorQuality.TenBit420,
            ).androidSafeVideoFallback()

            assertEquals(option.value, fallback.resolution)
            assertEquals(option.aspectRatio, fallback.aspectRatio)
            assertEquals(VideoCodec.H264, fallback.codec)
        }
    }

    @Test
    fun constrainedTvFirstFrameRecoveryChangesCodecOnceWithoutReducing1440p() {
        val launch = StreamSettings(
            resolution = "2560x1440",
            aspectRatio = "16:9",
            fps = 60,
            maxBitrateMbps = 75,
            codec = VideoCodec.H265,
            colorQuality = ColorQuality.TenBit420,
        ).adjustedForDevice(
            codecReport(
                VideoCodec.H265,
                hardwareDecoder = true,
                realtimeSafe = true,
                lowPower = true,
                tv = true,
                constrainedRuntime = true,
                webRtcDecoderAvailable = true,
                webRtcHardwareDecoderAvailable = true,
            ),
        )
        val recovery = launch.androidSafeVideoFallback()

        assertEquals("2560x1440", launch.resolution)
        assertEquals(VideoCodec.H265, launch.codec)
        assertEquals("2560x1440", recovery.resolution)
        assertEquals("16:9", recovery.aspectRatio)
        assertEquals(VideoCodec.H264, recovery.codec)
        assertEquals(recovery, recovery.androidSafeVideoFallback())
    }

    @Test
    fun usesSafeH264ProfileForLowPowerAndroidTv() {
        val adjusted = StreamSettings(
            resolution = "3840x2160",
            aspectRatio = "16:9",
            fps = 120,
            maxBitrateMbps = 90,
            codec = VideoCodec.H265,
            colorQuality = ColorQuality.TenBit420,
            streamSharpeningEnabled = true,
        ).adjustedForDevice(codecReport(VideoCodec.H265, hardwareDecoder = true, realtimeSafe = true, lowPower = true, tv = true))

        assertEquals(VideoCodec.H264, adjusted.codec)
        assertEquals(ColorQuality.EightBit420, adjusted.colorQuality)
        assertEquals("3840x2160", adjusted.resolution)
        assertEquals("16:9", adjusted.aspectRatio)
        assertEquals(60, adjusted.fps)
        assertEquals(90, adjusted.maxBitrateMbps)
        assertEquals(false, adjusted.streamSharpeningEnabled)
    }

    @Test
    fun preservesHighCustomProfileOn32BitPhone() {
        val adjusted = StreamSettings(
            resolution = "3840x2160",
            aspectRatio = "16:9",
            fps = 120,
            maxBitrateMbps = 75,
            codec = VideoCodec.H265,
            colorQuality = ColorQuality.TenBit420,
            hdrEnabled = true,
            streamSharpeningEnabled = true,
        ).adjustedForDevice(
            codecReport(
                VideoCodec.H265,
                hardwareDecoder = true,
                realtimeSafe = true,
                lowPower = true,
                constrainedRuntime = true,
                webRtcDecoderAvailable = true,
                webRtcHardwareDecoderAvailable = true,
                maxSupportedWidth = 3840,
                maxSupportedHeight = 2160,
            ),
        )

        assertEquals("3840x2160", adjusted.resolution)
        assertEquals("16:9", adjusted.aspectRatio)
        assertEquals(120, adjusted.fps)
        assertEquals(75, adjusted.maxBitrateMbps)
        assertEquals(VideoCodec.H265, adjusted.codec)
        assertEquals(ColorQuality.EightBit420, adjusted.colorQuality)
        assertEquals(false, adjusted.hdrEnabled)
        assertEquals(true, adjusted.streamSharpeningEnabled)
    }

    @Test
    fun preservesHighCustomProfileOnMemoryConstrainedTv() {
        val adjusted = StreamSettings(
            resolution = "2560x1440",
            aspectRatio = "16:9",
            fps = 60,
            maxBitrateMbps = 75,
            codec = VideoCodec.H265,
            colorQuality = ColorQuality.TenBit420,
            hdrEnabled = true,
            streamSharpeningEnabled = true,
        ).adjustedForDevice(
            codecReport(
                VideoCodec.H265,
                hardwareDecoder = true,
                realtimeSafe = true,
                lowPower = true,
                tv = true,
                constrainedRuntime = true,
                webRtcDecoderAvailable = true,
                webRtcHardwareDecoderAvailable = true,
                maxSupportedWidth = 3840,
                maxSupportedHeight = 2160,
            ),
        )

        assertEquals("2560x1440", adjusted.resolution)
        assertEquals(60, adjusted.fps)
        assertEquals(75, adjusted.maxBitrateMbps)
        assertEquals(VideoCodec.H265, adjusted.codec)
        assertEquals(ColorQuality.EightBit420, adjusted.colorQuality)
        assertEquals(false, adjusted.hdrEnabled)
        assertEquals(true, adjusted.streamSharpeningEnabled)
    }

    @Test
    fun warnsAboutDemandingSettingsWithoutChangingThem() {
        val settings = StreamSettings(
            resolution = "1920x1080",
            aspectRatio = "16:9",
            fps = 60,
            maxBitrateMbps = 35,
            hdrEnabled = true,
            streamSharpeningEnabled = true,
        )
        val report = codecReport(
            VideoCodec.H264,
            hardwareDecoder = true,
            realtimeSafe = true,
            lowPower = true,
            constrainedRuntime = true,
        )

        assertEquals(
            listOf(
                "1920x1080 resolution",
                "60 FPS",
                "35 Mbps bitrate",
                "HDR",
                "stream sharpening",
            ),
            settings.lowPowerPerformanceWarningReasons(report),
        )
    }

    @Test
    fun recommendedLowPowerProfileDoesNotWarn() {
        val settings = StreamSettings(
            resolution = "1280x720",
            aspectRatio = "16:9",
            fps = 30,
            maxBitrateMbps = 12,
        )
        val report = codecReport(
            VideoCodec.H264,
            hardwareDecoder = true,
            realtimeSafe = true,
            lowPower = true,
            constrainedRuntime = true,
        )

        assertTrue(settings.lowPowerPerformanceWarningReasons(report).isEmpty())
    }

    @Test
    fun preservesHardwareH265ForLowPowerAndroidTvInsideSafeLimits() {
        val adjusted = StreamSettings(
            resolution = "3840x2160",
            aspectRatio = "16:9",
            fps = 120,
            maxBitrateMbps = 90,
            codec = VideoCodec.H265,
            colorQuality = ColorQuality.TenBit420,
            hdrEnabled = true,
            streamSharpeningEnabled = true,
        ).adjustedForDevice(
            codecReport(
                VideoCodec.H265,
                hardwareDecoder = true,
                realtimeSafe = true,
                lowPower = true,
                tv = true,
                nativeDecoderAvailable = true,
                webRtcDecoderAvailable = true,
                webRtcHardwareDecoderAvailable = true,
            ),
        )

        assertEquals(VideoCodec.H265, adjusted.codec)
        assertEquals(ColorQuality.EightBit420, adjusted.colorQuality)
        assertEquals("3840x2160", adjusted.resolution)
        assertEquals(60, adjusted.fps)
        assertEquals(90, adjusted.maxBitrateMbps)
        assertEquals(false, adjusted.hdrEnabled)
        assertEquals(false, adjusted.streamSharpeningEnabled)
    }

    @Test
    fun lowPowerAndroidTvKeeps1440pWhenHardwareDecoderExplicitlySupportsIt() {
        val adjusted = StreamSettings(
            resolution = "2560x1440",
            aspectRatio = "16:9",
            codec = VideoCodec.H265,
            colorQuality = ColorQuality.TenBit420,
            maxBitrateMbps = 75,
            fps = 120,
            streamSharpeningEnabled = true,
        ).adjustedForDevice(
            codecReport(
                VideoCodec.H265,
                hardwareDecoder = true,
                realtimeSafe = true,
                lowPower = true,
                tv = true,
                nativeDecoderAvailable = true,
                webRtcDecoderAvailable = true,
                webRtcHardwareDecoderAvailable = true,
                maxSupportedWidth = 3840,
                maxSupportedHeight = 2160,
            ),
        )

        assertEquals("2560x1440", adjusted.resolution)
        assertEquals("16:9", adjusted.aspectRatio)
        assertEquals(VideoCodec.H265, adjusted.codec)
        assertEquals(60, adjusted.fps)
        assertEquals(75, adjusted.maxBitrateMbps)
        assertEquals(false, adjusted.streamSharpeningEnabled)
    }

    @Test
    fun lowPowerAndroidTvKeeps1440pWhenHardwareDecoderLimitsAreMissing() {
        val adjusted = StreamSettings(
            resolution = "2560x1440",
            aspectRatio = "16:9",
            codec = VideoCodec.H265,
            colorQuality = ColorQuality.TenBit420,
            maxBitrateMbps = 75,
            fps = 60,
        ).adjustedForDevice(
            codecReport(
                VideoCodec.H265,
                hardwareDecoder = true,
                realtimeSafe = true,
                lowPower = true,
                tv = true,
                webRtcDecoderAvailable = true,
                webRtcHardwareDecoderAvailable = true,
            ),
        )

        assertEquals("2560x1440", adjusted.resolution)
        assertEquals("16:9", adjusted.aspectRatio)
        assertEquals(VideoCodec.H265, adjusted.codec)
        assertEquals(60, adjusted.fps)
        assertEquals(75, adjusted.maxBitrateMbps)
    }

    @Test
    fun lowPowerAndroidTvKeeps1440pWhenHardwareDecoderProbeUnderreportsLimits() {
        val adjusted = StreamSettings(
            resolution = "2560x1440",
            aspectRatio = "16:9",
            codec = VideoCodec.H265,
            colorQuality = ColorQuality.TenBit420,
            maxBitrateMbps = 75,
            fps = 60,
        ).adjustedForDevice(
            codecReport(
                VideoCodec.H265,
                hardwareDecoder = true,
                realtimeSafe = true,
                lowPower = true,
                tv = true,
                webRtcDecoderAvailable = true,
                webRtcHardwareDecoderAvailable = true,
                maxSupportedWidth = 1920,
                maxSupportedHeight = 1080,
            ),
        )

        assertEquals("2560x1440", adjusted.resolution)
        assertEquals("16:9", adjusted.aspectRatio)
        assertEquals(VideoCodec.H265, adjusted.codec)
    }

    @Test
    fun keepsSelectedLowPowerAndroidTvUltrawideGeometry() {
        val adjusted = StreamSettings(
            resolution = "3440x1440",
            aspectRatio = "21:9",
            codec = VideoCodec.H265,
            colorQuality = ColorQuality.TenBit420,
        ).adjustedForDevice(codecReport(VideoCodec.H265, hardwareDecoder = true, realtimeSafe = true, lowPower = true, tv = true))

        assertEquals(VideoCodec.H264, adjusted.codec)
        assertEquals(ColorQuality.EightBit420, adjusted.colorQuality)
        assertEquals("3440x1440", adjusted.resolution)
        assertEquals("21:9", adjusted.aspectRatio)
    }

    @Test
    fun disablesRendererSharpeningForAndroidTvLaunchProfiles() {
        val adjusted = StreamSettings(
            codec = VideoCodec.AV1,
            maxBitrateMbps = 75,
            streamSharpeningEnabled = true,
        ).adjustedForDevice(
            codecReport(
                VideoCodec.AV1,
                hardwareDecoder = true,
                realtimeSafe = true,
                tv = true,
                webRtcDecoderAvailable = true,
                webRtcHardwareDecoderAvailable = true,
            ),
        )

        assertEquals(VideoCodec.AV1, adjusted.codec)
        assertEquals(75, adjusted.maxBitrateMbps)
        assertEquals(false, adjusted.streamSharpeningEnabled)
    }

    @Test
    fun preservesAv1WhenWebRtcHardwarePathExistsAndPlatformProbeMissesIt() {
        val adjusted = StreamSettings(codec = VideoCodec.AV1, colorQuality = ColorQuality.TenBit420)
            .adjustedForDevice(
                codecReport(
                    VideoCodec.AV1,
                    decoderAvailable = false,
                    hardwareDecoder = false,
                    realtimeSafe = false,
                    webRtcDecoderAvailable = true,
                    webRtcHardwareDecoderAvailable = true,
                ),
        )

        assertEquals(VideoCodec.AV1, adjusted.codec)
        assertEquals(ColorQuality.EightBit420, adjusted.colorQuality)
    }

    @Test
    fun usesAv1HardwareFallbackWhenH264ProbeFails() {
        val report = RuntimeCodecReport(
            capabilities = listOf(
                CodecCapability(
                    codec = VideoCodec.H264,
                    decoderAvailable = true,
                    encoderAvailable = false,
                    hardwareDecoder = true,
                    hardwareEncoder = false,
                    realtimeSafe = true,
                    webRtcDecoderAvailable = false,
                ),
                CodecCapability(
                    codec = VideoCodec.AV1,
                    decoderAvailable = false,
                    encoderAvailable = false,
                    hardwareDecoder = false,
                    hardwareEncoder = false,
                    realtimeSafe = false,
                    webRtcDecoderAvailable = true,
                    webRtcHardwareDecoderAvailable = true,
                ),
            ),
            nativeRuntimeSummary = "{}",
            androidTvProfile = false,
            lowPowerGpuProfile = false,
        )

        val adjusted = StreamSettings(codec = VideoCodec.H264, colorQuality = ColorQuality.EightBit420)
            .adjustedForDevice(report)

        assertEquals(VideoCodec.AV1, adjusted.codec)
    }

    @Test
    fun preservesResolutionWhenDecoderCapabilitiesAreConservative() {
        val report = RuntimeCodecReport(
            capabilities = listOf(
                CodecCapability(
                    codec = VideoCodec.H264,
                    decoderAvailable = true,
                    encoderAvailable = false,
                    hardwareDecoder = true,
                    hardwareEncoder = false,
                    realtimeSafe = true,
                    maxSupportedWidth = 1920,
                    maxSupportedHeight = 1080,
                )
            ),
            nativeRuntimeSummary = "{}",
            androidTvProfile = false,
            lowPowerGpuProfile = false,
        )

        val settings = StreamSettings(
            resolution = "3440x1440",
            aspectRatio = "21:9",
            codec = VideoCodec.H264,
        )

        val adjusted = settings.adjustedForDevice(report)
        assertEquals("3440x1440", adjusted.resolution)
    }

    @Test
    fun portalGeometryPreservesSelectedResolutionAndFps() {
        val adjusted = StreamSettings(
            resolution = "1376x640",
            aspectRatio = "19.5:9",
            fps = 120,
            codec = VideoCodec.H265,
        ).adjustedForDevice(report = null)

        assertEquals("1376x640", adjusted.resolution)
        assertEquals("19.5:9", adjusted.aspectRatio)
        assertEquals(120, adjusted.fps)
        assertEquals(VideoCodec.H265, adjusted.codec)
    }

    @Test
    fun phoneFullHdGeometryPreservesHardwareH265() {
        val report = codecReport(
            codec = VideoCodec.H265,
            hardwareDecoder = true,
            realtimeSafe = true,
            webRtcDecoderAvailable = true,
            webRtcHardwareDecoderAvailable = true,
            maxSupportedWidth = 1920,
            maxSupportedHeight = 1080,
        )
        val adjusted = StreamSettings(
            resolution = "2340x1080",
            aspectRatio = "19.5:9",
            fps = 60,
            codec = VideoCodec.H265,
        ).adjustedForDevice(report)

        assertEquals("2340x1080", adjusted.resolution)
        assertEquals("19.5:9", adjusted.aspectRatio)
        assertEquals(VideoCodec.H265, adjusted.codec)
    }

    @Test
    fun preserves1440pByUsingAnotherHardwareCodecBeforeReducingResolution() {
        val report = RuntimeCodecReport(
            capabilities = listOf(
                CodecCapability(
                    codec = VideoCodec.H264,
                    decoderAvailable = true,
                    encoderAvailable = false,
                    hardwareDecoder = true,
                    hardwareEncoder = false,
                    realtimeSafe = true,
                    webRtcDecoderAvailable = true,
                    webRtcHardwareDecoderAvailable = true,
                    maxSupportedWidth = 1920,
                    maxSupportedHeight = 1080,
                ),
                CodecCapability(
                    codec = VideoCodec.H265,
                    decoderAvailable = true,
                    encoderAvailable = false,
                    hardwareDecoder = true,
                    hardwareEncoder = false,
                    realtimeSafe = true,
                    webRtcDecoderAvailable = true,
                    webRtcHardwareDecoderAvailable = true,
                    maxSupportedWidth = 3840,
                    maxSupportedHeight = 2160,
                ),
            ),
            nativeRuntimeSummary = "{}",
            androidTvProfile = false,
            lowPowerGpuProfile = false,
        )

        val adjusted = StreamSettings(
            resolution = "2560x1440",
            aspectRatio = "16:9",
            codec = VideoCodec.H264,
            colorQuality = ColorQuality.EightBit420,
        ).adjustedForDevice(report)

        assertEquals("2560x1440", adjusted.resolution)
        assertEquals("16:9", adjusted.aspectRatio)
        assertEquals(VideoCodec.H265, adjusted.codec)
    }

    @Test
    fun testAdjustedForDevicePreservesRequestedFpsAtEverySupportedResolution() {
        val report = codecReport(
            codec = VideoCodec.H265,
            hardwareDecoder = true,
            realtimeSafe = true,
            webRtcDecoderAvailable = true,
            webRtcHardwareDecoderAvailable = true,
            maxSupportedWidth = 3840,
            maxSupportedHeight = 2160,
        )

        listOf("1920x1080", "2560x1440", "3840x2160").forEach { resolution ->
            val settings = StreamSettings(
                resolution = resolution,
                aspectRatio = "16:9",
                fps = 120,
                codec = VideoCodec.H265,
            )

            val adjusted = settings.adjustedForDevice(report)
            assertEquals(resolution, adjusted.resolution)
            assertEquals("$resolution should preserve the selected FPS", 120, adjusted.fps)
        }
    }

    private fun codecReport(
        codec: VideoCodec,
        decoderAvailable: Boolean = true,
        hardwareDecoder: Boolean,
        realtimeSafe: Boolean,
        lowPower: Boolean = false,
        tv: Boolean = false,
        constrainedRuntime: Boolean = false,
        nativeDecoderAvailable: Boolean? = null,
        webRtcDecoderAvailable: Boolean? = null,
        webRtcHardwareDecoderAvailable: Boolean? = null,
        webRtcDecoderName: String? = null,
        maxSupportedWidth: Int? = null,
        maxSupportedHeight: Int? = null,
    ): RuntimeCodecReport =
        RuntimeCodecReport(
            capabilities = listOf(
                CodecCapability(
                    codec = codec,
                    decoderAvailable = decoderAvailable,
                    encoderAvailable = false,
                    hardwareDecoder = hardwareDecoder,
                    hardwareEncoder = false,
                    realtimeSafe = realtimeSafe,
                    nativeDecoderAvailable = nativeDecoderAvailable,
                    webRtcDecoderAvailable = webRtcDecoderAvailable,
                    webRtcHardwareDecoderAvailable = webRtcHardwareDecoderAvailable,
                    webRtcDecoderName = webRtcDecoderName,
                    maxSupportedWidth = maxSupportedWidth,
                    maxSupportedHeight = maxSupportedHeight,
                ),
            ),
            nativeRuntimeSummary = "{}",
            androidTvProfile = tv,
            lowPowerGpuProfile = lowPower,
            constrainedRuntimeProfile = constrainedRuntime,
        )
}

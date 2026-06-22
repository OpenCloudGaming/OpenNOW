package com.opencloudgaming.opennow

import org.junit.Assert.assertEquals
import org.junit.Test

class StreamSettingsDeviceAdjustmentTest {
    @Test
    fun preservesSelectedH265WhenHardwareDecoderExists() {
        val adjusted = StreamSettings(codec = VideoCodec.H265, colorQuality = ColorQuality.TenBit420)
            .adjustedForDevice(codecReport(VideoCodec.H265, hardwareDecoder = true, realtimeSafe = false))

        assertEquals(VideoCodec.H265, adjusted.codec)
        assertEquals(ColorQuality.EightBit420, adjusted.colorQuality)
    }

    @Test
    fun preservesSelectedAv1WhenHardwareDecoderExists() {
        val adjusted = StreamSettings(codec = VideoCodec.AV1, colorQuality = ColorQuality.TenBit420)
            .adjustedForDevice(codecReport(VideoCodec.AV1, hardwareDecoder = true, realtimeSafe = false))

        assertEquals(VideoCodec.AV1, adjusted.codec)
        assertEquals(ColorQuality.EightBit420, adjusted.colorQuality)
    }

    @Test
    fun preservesTenBitWhenHdrIsEnabled() {
        val adjusted = StreamSettings(codec = VideoCodec.H265, colorQuality = ColorQuality.TenBit420, hdrEnabled = true)
            .adjustedForDevice(codecReport(VideoCodec.H265, hardwareDecoder = true, realtimeSafe = true))

        assertEquals(VideoCodec.H265, adjusted.codec)
        assertEquals(ColorQuality.TenBit420, adjusted.colorQuality)
    }

    @Test
    fun fallsBackToH264WhenSelectedDecoderHasNoHardwarePath() {
        val adjusted = StreamSettings(codec = VideoCodec.AV1, colorQuality = ColorQuality.TenBit420, maxBitrateMbps = 90)
            .adjustedForDevice(codecReport(VideoCodec.AV1, hardwareDecoder = false, realtimeSafe = false))

        assertEquals(VideoCodec.H264, adjusted.codec)
        assertEquals(ColorQuality.EightBit420, adjusted.colorQuality)
        assertEquals(35, adjusted.maxBitrateMbps)
    }

    @Test
    fun keepsSelectedCodecOnLowPowerDevicesWhenHardwareDecoderExists() {
        val adjusted = StreamSettings(codec = VideoCodec.H265, colorQuality = ColorQuality.TenBit420, maxBitrateMbps = 90)
            .adjustedForDevice(codecReport(VideoCodec.H265, hardwareDecoder = true, realtimeSafe = true, lowPower = true))

        assertEquals(VideoCodec.H265, adjusted.codec)
        assertEquals(ColorQuality.EightBit420, adjusted.colorQuality)
        assertEquals(25, adjusted.maxBitrateMbps)
    }

    @Test
    fun preservesHighRefreshRateForSupportedAndroidStreams() {
        val adjusted = StreamSettings(codec = VideoCodec.AV1, fps = 120, maxBitrateMbps = 90)
            .adjustedForDevice(codecReport(VideoCodec.AV1, hardwareDecoder = true, realtimeSafe = true))

        assertEquals(VideoCodec.AV1, adjusted.codec)
        assertEquals(120, adjusted.fps)
        assertEquals(75, adjusted.maxBitrateMbps)
    }

    @Test
    fun capsH264AndroidBandwidthAtUserSafeCeiling() {
        val adjusted = StreamSettings(codec = VideoCodec.H264, maxBitrateMbps = 150)
            .adjustedForDevice(codecReport(VideoCodec.H264, hardwareDecoder = true, realtimeSafe = true))

        assertEquals(VideoCodec.H264, adjusted.codec)
        assertEquals(75, adjusted.maxBitrateMbps)
    }

    @Test
    fun safeVideoFallbackUsesBasicWorkingAndroidProfile() {
        val fallback = StreamSettings(
            resolution = "3840x2160",
            aspectRatio = "16:9",
            fps = 120,
            maxBitrateMbps = 150,
            codec = VideoCodec.H265,
            colorQuality = ColorQuality.TenBit420,
            hdrEnabled = true,
            enableCloudGsync = true,
        ).androidSafeVideoFallback()

        assertEquals("1920x1080", fallback.resolution)
        assertEquals("16:9", fallback.aspectRatio)
        assertEquals(60, fallback.fps)
        assertEquals(75, fallback.maxBitrateMbps)
        assertEquals(VideoCodec.H264, fallback.codec)
        assertEquals(ColorQuality.EightBit420, fallback.colorQuality)
        assertEquals(false, fallback.hdrEnabled)
        assertEquals(false, fallback.enableCloudGsync)
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
        ).adjustedForDevice(codecReport(VideoCodec.H265, hardwareDecoder = true, realtimeSafe = true, lowPower = true, tv = true))

        assertEquals(VideoCodec.H264, adjusted.codec)
        assertEquals(ColorQuality.EightBit420, adjusted.colorQuality)
        assertEquals("1920x1080", adjusted.resolution)
        assertEquals("16:9", adjusted.aspectRatio)
        assertEquals(60, adjusted.fps)
        assertEquals(25, adjusted.maxBitrateMbps)
    }

    @Test
    fun keepsLowPowerAndroidTvUltrawideWithinDecoderBounds() {
        val adjusted = StreamSettings(
            resolution = "3440x1440",
            aspectRatio = "21:9",
            codec = VideoCodec.H265,
            colorQuality = ColorQuality.TenBit420,
        ).adjustedForDevice(codecReport(VideoCodec.H265, hardwareDecoder = true, realtimeSafe = true, lowPower = true, tv = true))

        assertEquals(VideoCodec.H264, adjusted.codec)
        assertEquals(ColorQuality.EightBit420, adjusted.colorQuality)
        assertEquals("1680x720", adjusted.resolution)
        assertEquals("21:9", adjusted.aspectRatio)
    }

    @Test
    fun preservesSelectedAv1WhenWebRtcDecoderExistsEvenIfPlatformProbeMissesIt() {
        val adjusted = StreamSettings(codec = VideoCodec.AV1, colorQuality = ColorQuality.TenBit420)
            .adjustedForDevice(
                codecReport(
                    VideoCodec.AV1,
                    decoderAvailable = false,
                    hardwareDecoder = false,
                    realtimeSafe = false,
                    webRtcDecoderAvailable = true,
                ),
        )

        assertEquals(VideoCodec.AV1, adjusted.codec)
        assertEquals(ColorQuality.EightBit420, adjusted.colorQuality)
    }

    @Test
    fun avoidsH264WhenPlatformAdvertisesItButWebRtcCannotCreateDecoder() {
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

    private fun codecReport(
        codec: VideoCodec,
        decoderAvailable: Boolean = true,
        hardwareDecoder: Boolean,
        realtimeSafe: Boolean,
        lowPower: Boolean = false,
        tv: Boolean = false,
        webRtcDecoderAvailable: Boolean? = null,
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
                    webRtcDecoderAvailable = webRtcDecoderAvailable,
                ),
            ),
            nativeRuntimeSummary = "{}",
            androidTvProfile = tv,
            lowPowerGpuProfile = lowPower,
        )
}

package com.opencloudgaming.opennow

import org.junit.Assert.assertEquals
import org.junit.Test

class StreamSettingsDeviceAdjustmentTest {
    @Test
    fun preservesSelectedH265WhenHardwareDecoderExists() {
        val adjusted = StreamSettings(codec = VideoCodec.H265, colorQuality = ColorQuality.TenBit420)
            .adjustedForDevice(codecReport(VideoCodec.H265, hardwareDecoder = true, realtimeSafe = false))

        assertEquals(VideoCodec.H265, adjusted.codec)
        assertEquals(ColorQuality.TenBit420, adjusted.colorQuality)
    }

    @Test
    fun preservesSelectedAv1WhenHardwareDecoderExists() {
        val adjusted = StreamSettings(codec = VideoCodec.AV1, colorQuality = ColorQuality.TenBit420)
            .adjustedForDevice(codecReport(VideoCodec.AV1, hardwareDecoder = true, realtimeSafe = false))

        assertEquals(VideoCodec.AV1, adjusted.codec)
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
        assertEquals(ColorQuality.TenBit420, adjusted.colorQuality)
        assertEquals(25, adjusted.maxBitrateMbps)
    }

    private fun codecReport(
        codec: VideoCodec,
        hardwareDecoder: Boolean,
        realtimeSafe: Boolean,
        lowPower: Boolean = false,
        tv: Boolean = false,
    ): RuntimeCodecReport =
        RuntimeCodecReport(
            capabilities = listOf(
                CodecCapability(
                    codec = codec,
                    decoderAvailable = true,
                    encoderAvailable = false,
                    hardwareDecoder = hardwareDecoder,
                    hardwareEncoder = false,
                    realtimeSafe = realtimeSafe,
                ),
            ),
            nativeRuntimeSummary = "{}",
            androidTvProfile = tv,
            lowPowerGpuProfile = lowPower,
        )
}

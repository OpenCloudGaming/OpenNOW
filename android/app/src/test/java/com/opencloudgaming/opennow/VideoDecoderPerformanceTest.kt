package com.opencloudgaming.opennow

import org.junit.Assert.assertEquals
import org.junit.Assert.assertFalse
import org.junit.Assert.assertNull
import org.junit.Assert.assertTrue
import org.junit.Test
import org.webrtc.EncodedImage
import org.webrtc.VideoCodecStatus
import org.webrtc.VideoDecoder

class VideoDecoderPerformanceTest {
    @Test
    fun qualcommLowLatencyOperatingRateDoesNotUnderRequestHighFps() {
        assertEquals(120, vendorLowLatencyOperatingRate("c2.qti.avc.decoder", 60))
        assertEquals(120, vendorLowLatencyOperatingRate("OMX.qcom.video.decoder.avc", 120))
        assertEquals(240, vendorLowLatencyOperatingRate("c2.qti.hevc.decoder", 240))
        assertEquals(360, vendorLowLatencyOperatingRate("c2.qti.av1.decoder", 360))
    }

    @Test
    fun highFpsDecoderTuningPreservesExactUserSelection() {
        assertEquals(120, mediaCodecPerformanceTargetFps(120))
        assertEquals(240, mediaCodecPerformanceTargetFps(240))
        assertEquals(340, mediaCodecPerformanceTargetFps(340))
        assertEquals(360, mediaCodecPerformanceTargetFps(360))
    }

    @Test
    fun sixtyFpsUsesRealtimeDecoderScheduling() {
        assertEquals(60, mediaCodecPerformanceTargetFps(60))
    }

    @Test
    fun lowFpsDoesNotRequireDecoderPerformanceOverride() {
        assertNull(mediaCodecPerformanceTargetFps(30))
    }

    @Test
    fun mediaTekStandardLowLatencyRequiresAdvertisedSupportAtSixtyFps() {
        assertTrue(
            shouldEnableMediaTekStandardLowLatency(
                decoderImplementationName = "c2.mtk.avc.decoder",
                requestedFps = 60,
                featureAdvertised = true,
            ),
        )
        assertFalse(
            shouldEnableMediaTekStandardLowLatency(
                decoderImplementationName = "c2.mtk.avc.decoder",
                requestedFps = 30,
                featureAdvertised = true,
            ),
        )
        assertFalse(
            shouldEnableMediaTekStandardLowLatency(
                decoderImplementationName = "c2.mtk.hevc.decoder",
                requestedFps = 60,
                featureAdvertised = false,
            ),
        )
        assertFalse(
            shouldEnableMediaTekStandardLowLatency(
                decoderImplementationName = "c2.qti.avc.decoder",
                requestedFps = 60,
                featureAdvertised = true,
            ),
        )
    }

    @Test
    fun approvedHardwareDecoderGetsMediaCodecTuning() {
        val hardwareDecoder = fakeDecoder()

        assertTrue(
            shouldUseMediaCodecDecoderTuning(
                selectedDecoder = hardwareDecoder,
                approvedHardwareDecoder = hardwareDecoder,
                requestedFps = 60,
                lowLatencyEnabled = false,
            ),
        )
    }

    @Test
    fun qualcommH264SixtyFpsBypassesPerformanceWrapper() {
        val hardwareDecoder = fakeDecoder()

        assertFalse(
            shouldUseMediaCodecDecoderTuning(
                selectedDecoder = hardwareDecoder,
                approvedHardwareDecoder = hardwareDecoder,
                requestedFps = 60,
                lowLatencyEnabled = false,
                codec = VideoCodec.H264,
                decoderImplementationName = "c2.qti.avc.decoder",
            ),
        )
    }

    @Test
    fun qualcommGuardDoesNotDisableOtherPerformanceTuning() {
        val hardwareDecoder = fakeDecoder()

        assertTrue(
            shouldUseMediaCodecDecoderTuning(
                selectedDecoder = hardwareDecoder,
                approvedHardwareDecoder = hardwareDecoder,
                requestedFps = 120,
                lowLatencyEnabled = false,
                codec = VideoCodec.H264,
                decoderImplementationName = "c2.qti.avc.decoder",
            ),
        )
        assertTrue(
            shouldUseMediaCodecDecoderTuning(
                selectedDecoder = hardwareDecoder,
                approvedHardwareDecoder = hardwareDecoder,
                requestedFps = 60,
                lowLatencyEnabled = false,
                codec = VideoCodec.H265,
                decoderImplementationName = "c2.qti.hevc.decoder",
            ),
        )
        assertTrue(
            shouldUseMediaCodecDecoderTuning(
                selectedDecoder = hardwareDecoder,
                approvedHardwareDecoder = hardwareDecoder,
                requestedFps = 60,
                lowLatencyEnabled = false,
                codec = VideoCodec.H264,
                decoderImplementationName = "c2.mtk.avc.decoder",
            ),
        )
    }

    @Test
    fun explicitLowLatencyStillUsesQualcommWrapper() {
        val hardwareDecoder = fakeDecoder()

        assertTrue(
            shouldUseMediaCodecDecoderTuning(
                selectedDecoder = hardwareDecoder,
                approvedHardwareDecoder = hardwareDecoder,
                requestedFps = 60,
                lowLatencyEnabled = true,
                codec = VideoCodec.H264,
                decoderImplementationName = "OMX.qcom.video.decoder.avc",
            ),
        )
    }

    @Test
    fun nativeFallbackIsNotWrappedForPerformanceTuning() {
        assertFalse(
            shouldUseMediaCodecDecoderTuning(
                selectedDecoder = fakeDecoder(),
                approvedHardwareDecoder = null,
                requestedFps = 60,
                lowLatencyEnabled = false,
            ),
        )
    }

    @Test
    fun nativeFallbackIsNotWrappedForLowLatencyTuning() {
        assertFalse(
            shouldUseMediaCodecDecoderTuning(
                selectedDecoder = fakeDecoder(),
                approvedHardwareDecoder = null,
                requestedFps = 30,
                lowLatencyEnabled = true,
            ),
        )
    }

    private fun fakeDecoder(): VideoDecoder =
        object : VideoDecoder {
            override fun initDecode(
                settings: VideoDecoder.Settings?,
                decodeCallback: VideoDecoder.Callback?,
            ): VideoCodecStatus = error("unused")

            override fun release(): VideoCodecStatus = error("unused")

            override fun decode(
                frame: EncodedImage?,
                info: VideoDecoder.DecodeInfo?,
            ): VideoCodecStatus = error("unused")

            override fun getImplementationName(): String = "test"
        }
}

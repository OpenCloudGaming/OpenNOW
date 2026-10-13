package com.opencloudgaming.opennow

import org.junit.Assert.*
import org.junit.Test

class StreamSurfaceDecoderPolicyTest {
    private val sdr = StreamSettings(colorQuality = ColorQuality.EightBit420,
        videoOutput = StreamVideoOutput.MediaCodecSurface)

    @Test fun absentChoiceFollowsDistributionAndExplicitChoiceOverridesIt() {
        val legacy = sdr.copy(videoOutput = StreamVideoOutput.Default)
        assertTrue(shouldPreferDirectSdrSurface(legacy, playStoreRelease = false))
        assertFalse(shouldPreferDirectSdrSurface(legacy, playStoreRelease = true))
        assertTrue(shouldPreferDirectSdrSurface(sdr, playStoreRelease = true))
        assertFalse(shouldPreferDirectSdrSurface(sdr.copy(videoOutput = StreamVideoOutput.WebRtcTexture),
            playStoreRelease = false))
    }

    @Test fun directSurfaceSupportsSdrAtRequestedHighFrameRates() {
        for (fps in listOf(30, 60, 120, 240, 360)) {
            for (codec in VideoCodec.values()) {
                assertTrue("$codec at $fps FPS", shouldPreferDirectSdrSurface(sdr.copy(codec = codec, fps = fps)))
            }
        }
    }

    @Test fun sharpeningKeepsTextureFrames() {
        assertFalse(shouldPreferDirectSdrSurface(sdr.copy(streamSharpeningEnabled = true)))
    }

    @Test fun tenBitSdrKeepsEstablishedColorPath() {
        assertFalse(shouldPreferDirectSdrSurface(sdr.copy(colorQuality = ColorQuality.TenBit420)))
        assertFalse(shouldPreferDirectSdrSurface(sdr.copy(colorQuality = ColorQuality.TenBit444)))
    }

    @Test fun hdrMustUseDedicatedHdrConfiguration() {
        assertFalse(shouldPreferDirectSdrSurface(sdr.copy(codec = VideoCodec.H265, hdrEnabled = true)))
    }
}

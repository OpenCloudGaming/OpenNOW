package com.opencloudgaming.opennow

import org.junit.Assert.*
import org.junit.Test

class StreamHdrTest {
    @Test fun luminanceComesFromTheDisplayWithoutAWhitePointMultiplier() {
        assertEquals(HdrDisplayProfile(650f, 0.005f, 280f), hdrDisplayProfile(650f, 0.005f, 280f))
    }

    @Test fun incompleteOrInvalidDisplayLuminanceCannotEnableHdr() {
        assertNull(hdrDisplayProfile(-1f, 0f, 100f))
        assertNull(hdrDisplayProfile(1000f, -1f, 100f))
        assertNull(hdrDisplayProfile(1000f, 0f, -1f))
        assertNull(hdrDisplayProfile(Float.NaN, 0f, 100f))
        assertNull(hdrDisplayProfile(1000f, 0f, Float.POSITIVE_INFINITY))
        assertNull(hdrDisplayProfile(300f, 0f, 500f))
        assertNull(hdrDisplayProfile(300f, 300f, 100f))
    }

    @Test fun presentedFramesReturnTheirCodecSlotExactlyOnce() {
        val releases = mutableListOf<Boolean>()
        val buffer = MediaCodecSurfaceBuffer(1920, 1080) { releases += it; it }
        buffer.retain()
        assertTrue(buffer.present())
        assertFalse(buffer.present())
        buffer.release()
        buffer.release()
        assertEquals(listOf(true), releases)
        assertNull(buffer.toI420())
    }

    @Test fun droppedFramesReturnTheirCodecSlotWithoutDisplayingStalePixels() {
        val releases = mutableListOf<Boolean>()
        val buffer = MediaCodecSurfaceBuffer(3840, 2160) { releases += it; it }
        buffer.retain()
        buffer.release()
        assertTrue(releases.isEmpty())
        buffer.release()
        assertEquals(listOf(false), releases)
        assertFalse(buffer.present())
    }

    @Test fun obsoleteSurfaceFramesCannotBePresented() {
        var attempted = 0
        val buffer = MediaCodecSurfaceBuffer(1920, 1080) { attempted++; false }
        assertFalse(buffer.present())
        buffer.release()
        assertEquals(1, attempted)
    }
    @Test fun decoderCannotSilentlyConvertHdrIntoSdr() {
        assertTrue(hdrOutputColorSupported(6, 6)) // BT.2020 / ST2084
        assertTrue(hdrOutputColorSupported(null, null)) // Preserve configured format
        assertFalse(hdrOutputColorSupported(1, 6)) // BT.709
        assertFalse(hdrOutputColorSupported(6, 3)) // SDR transfer
        assertFalse(hdrOutputColorSupported(6, 7)) // HLG is not PQ
    }
}

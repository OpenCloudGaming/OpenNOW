package com.opencloudgaming.opennow

import android.media.MediaCodecInfo
import org.junit.Assert.*
import org.junit.Test
import java.nio.ByteBuffer

class StreamHdrTest {
    @Test fun deferredDecodeOwnsPayloadWhenNativeBufferIsReused() {
        val nativeBuffer = ByteBuffer.allocateDirect(8)
        nativeBuffer.put(byteArrayOf(9, 1, 2, 3, 9)).flip()
        nativeBuffer.position(1)
        nativeBuffer.limit(4)
        val pending = copyHdrEncodedPayload(nativeBuffer)
        assertEquals(1, nativeBuffer.position())
        nativeBuffer.put(1, 7)
        nativeBuffer.put(2, 8)
        assertArrayEquals(byteArrayOf(1, 2, 3), pending)
    }
    @Test fun absentAndDuplicateCaptureTimestampsKeepRealFrameIntervals() {
        val first = hdrPresentationTimeUs(0, 0, 120)
        val second = hdrPresentationTimeUs(first, 0, 120)
        assertEquals(8333L, first)
        assertEquals(16666L, second)
        assertEquals(100000L, hdrPresentationTimeUs(second, 100000000L, 120))
        assertEquals(108333L, hdrPresentationTimeUs(100000L, 100000000L, 120))
    }
    @Test fun inputsWithoutOutputDoNotPreventSubsequentFrameMatching() {
        val metadata = HdrFrameMetadata<String>(128)
        // A long sequence of hidden/dropped inputs must remain bounded, while recent
        // outputs still match their own metadata even when returned out of order.
        repeat(1000) { metadata.put(it.toLong(), "frame-$it") }
        assertEquals(128, metadata.size)
        assertEquals("frame-999", metadata.remove(999))
        assertEquals("frame-998", metadata.remove(998))
        assertNull(metadata.remove(0))
        metadata.clear()
        assertEquals(0, metadata.size)
    }
    @Test fun hdrRequiresTheCorrectCodecTenBitProfile() {
        assertEquals(MediaCodecInfo.CodecProfileLevel.HEVCProfileMain10, hdrCodecProfile(VideoCodec.H265))
        assertEquals(MediaCodecInfo.CodecProfileLevel.AV1ProfileMain10, hdrCodecProfile(VideoCodec.AV1))
        assertNull(hdrCodecProfile(VideoCodec.H264))
        assertTrue(hdrDecoderProfileSupported(VideoCodec.H265, MediaCodecInfo.CodecProfileLevel.HEVCProfileMain10HDR10))
        assertTrue(hdrDecoderProfileSupported(VideoCodec.AV1, MediaCodecInfo.CodecProfileLevel.AV1ProfileMain10HDR10))
        assertFalse(hdrDecoderProfileSupported(VideoCodec.AV1, MediaCodecInfo.CodecProfileLevel.AV1ProfileMain8))
        assertFalse(hdrDecoderProfileSupported(VideoCodec.H265, MediaCodecInfo.CodecProfileLevel.HEVCProfileMain))
        assertFalse(hdrDecoderProfileSupported(VideoCodec.H264, MediaCodecInfo.CodecProfileLevel.AVCProfileHigh10))
    }

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
        val buffer = HdrSurfaceBuffer(1920, 1080) { releases += it; it }
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
        val buffer = HdrSurfaceBuffer(3840, 2160) { releases += it; it }
        buffer.retain()
        buffer.release()
        assertTrue(releases.isEmpty())
        buffer.release()
        assertEquals(listOf(false), releases)
        assertFalse(buffer.present())
    }

    @Test fun obsoleteSurfaceFramesCannotBePresented() {
        var attempted = 0
        val buffer = HdrSurfaceBuffer(1920, 1080) { attempted++; false }
        assertFalse(buffer.present())
        buffer.release()
        assertEquals(1, attempted)
    }

    @Test fun timedPresentationReturnsTheSlotExactlyOnce() {
        val times = mutableListOf<Long>()
        val drops = mutableListOf<Boolean>()
        val buffer = HdrSurfaceBuffer(1920, 1080, timedFinish = { times += it; true }) { drops += it; it }
        buffer.retain()
        assertTrue(buffer.presentAt(1_000_000_000L))
        assertFalse(buffer.present())
        assertFalse(buffer.presentAt(2_000_000_000L))
        buffer.release()
        buffer.release()
        assertEquals(listOf(1_000_000_000L), times)
        assertTrue(drops.isEmpty())
    }

    @Test fun failedTimedPresentationCannotReleaseANewCodecSlot() {
        var timed = 0
        var dropped = 0
        val buffer = HdrSurfaceBuffer(1920, 1080, timedFinish = { timed++; false }) { dropped++; false }
        assertFalse(buffer.presentAt(1_000_000_000L))
        buffer.release()
        assertEquals(1, timed)
        assertEquals(0, dropped)
    }
    @Test fun decoderCannotSilentlyConvertHdrIntoSdr() {
        assertTrue(hdrOutputColorSupported(6, 6)) // BT.2020 / ST2084
        assertTrue(hdrOutputColorSupported(null, null)) // Preserve configured format
        assertFalse(hdrOutputColorSupported(1, 6)) // BT.709
        assertFalse(hdrOutputColorSupported(6, 3)) // SDR transfer
        assertFalse(hdrOutputColorSupported(6, 7)) // HLG is not PQ
    }
}

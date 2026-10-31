package com.opencloudgaming.opennow

import org.webrtc.EncodedImage
import org.webrtc.OpenNowMediaCodecTuning
import org.webrtc.VideoDecoder
import java.nio.ByteBuffer
import java.util.concurrent.ArrayBlockingQueue

/** Unknown decoders retain the existing owned-copy contract. Never infer borrowing from a codec name. */
internal fun VideoDecoder.supportsBorrowedNvstInput(): Boolean = when (this) {
    is MediaCodecSurfaceVideoDecoder -> true // Copies any asynchronously queued input itself.
    is LowLatencyVideoDecoder -> this.canBorrowEncodedInput
    is DecoderInitializationFallback -> this.canBorrowEncodedInput
    is SurfaceOrTextureVideoDecoder -> this.canBorrowEncodedInput
    else -> OpenNowMediaCodecTuning.consumesEncodedInputSynchronously(this)
}

/** Rust keeps the frame alive until onNativeMedia returns. Only audited decoders can borrow it. */
internal class NvstDecoderInput(private val borrowInput: Boolean) {
    private val copiedBuffers = ArrayBlockingQueue<ByteBuffer>(8)

    fun frame(source: ByteBuffer, timestampNs: Long, keyframe: Boolean): EncodedImage {
        if (borrowInput) {
            // A slice prevents decoder reads from mutating the caller's position/limit.
            return nvstEncodedImage(source.slice(), timestampNs, keyframe) {}
        }
        val reusable = copiedBuffers.poll()
        val buffer = if (reusable != null && reusable.capacity() >= source.remaining()) reusable
            else ByteBuffer.allocateDirect(source.remaining())
        buffer.clear()
        buffer.put(source.duplicate())
        buffer.flip()
        return nvstEncodedImage(buffer, timestampNs, keyframe) { copiedBuffers.offer(buffer) }
    }
}

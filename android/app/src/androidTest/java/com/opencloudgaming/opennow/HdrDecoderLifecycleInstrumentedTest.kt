package com.opencloudgaming.opennow

import android.graphics.SurfaceTexture
import android.os.Build
import android.view.Surface
import androidx.test.platform.app.InstrumentationRegistry
import org.junit.Assert.*
import org.junit.Assume.assumeTrue
import org.junit.Test
import org.webrtc.EncodedImage
import org.webrtc.VideoCodecStatus
import org.webrtc.VideoDecoder
import org.webrtc.VideoFrame
import java.nio.ByteBuffer
import java.util.concurrent.LinkedBlockingQueue
import java.util.concurrent.TimeUnit

/** Exercises real callback codecs, native-buffer reuse and stale surface buffers on a device. */
class HdrDecoderLifecycleInstrumentedTest {
    @Test fun hevcCallbackLifecycle() = exercise(VideoCodec.H265, "hdr-decoder-keyframe.hevc")
    @Test fun av1CallbackLifecycle() = exercise(VideoCodec.AV1, "hdr-decoder-keyframe.obu")

    private fun exercise(codec: VideoCodec, asset: String) {
        assumeTrue(Build.VERSION.SDK_INT >= 30 && StreamHdr.decoderName(1280, 720, 120, codec) != null)
        val payload = InstrumentationRegistry.getInstrumentation().context.assets.open(asset).use { it.readBytes() }
        val textures = listOf(SurfaceTexture(false), SurfaceTexture(false))
        val surfaces = textures.map { Surface(it) }
        val targets = surfaces.map { HdrSurfaceTarget(it) }
        var target: HdrSurfaceTarget? = targets[0]
        val delivered = LinkedBlockingQueue<VideoFrame>()
        val held = mutableListOf<VideoFrame>()
        val decoder = HdrSurfaceVideoDecoder(codec, 120, { target }, true)
        val callback = VideoDecoder.Callback { frame, _, _ -> frame.retain(); delivered.add(frame) }
        fun initialize() = assertEquals(VideoCodecStatus.OK,
            decoder.initDecode(VideoDecoder.Settings(2, 1280, 720), callback))
        fun submit(timestamp: Long, key: Boolean = true): VideoCodecStatus {
            val borrowed = ByteBuffer.allocateDirect(payload.size).apply { put(payload); flip() }
            val image = EncodedImage.builder().setBuffer(borrowed, null).setEncodedWidth(1280)
                .setEncodedHeight(720).setCaptureTimeNs(timestamp).setFrameType(
                    if (key) EncodedImage.FrameType.VideoFrameKey else EncodedImage.FrameType.VideoFrameDelta)
                .createEncodedImage()
            try {
                val status = decoder.decode(image, null)
                // This buffer can immediately be returned to the native RTP pool after decode().
                repeat(borrowed.capacity()) { borrowed.put(it, 0) }
                return status
            } finally { image.release() }
        }
        fun receive(timestamp: Long): VideoFrame {
            val frame = delivered.poll(5, TimeUnit.SECONDS)
            assertNotNull("No asynchronous $codec output", frame)
            return requireNotNull(frame).also {
                held += it
                assertEquals(timestamp, it.timestampNs)
                assertEquals(1280, it.buffer.width)
                assertEquals(720, it.buffer.height)
            }
        }
        try {
            initialize()
            assertEquals(VideoCodecStatus.OK, submit(1_000_000_000L))
            val oldSurfaceFrame = receive(1_000_000_000L)
            target = targets[1]
            assertEquals(VideoCodecStatus.OK, submit(2_000_000_000L))
            val oldCodecFrame = receive(2_000_000_000L)
            assertFalse((oldSurfaceFrame.buffer as HdrSurfaceBuffer).presentAt(System.nanoTime() + 8_333_333L))
            assertEquals(VideoCodecStatus.OK, decoder.release())
            assertFalse((oldCodecFrame.buffer as HdrSurfaceBuffer).presentAt(System.nanoTime() + 8_333_333L))
            initialize()
            target = null
            assertEquals(VideoCodecStatus.NO_OUTPUT, submit(3_000_000_000L))
            target = targets[0]
            assertEquals(VideoCodecStatus.ERROR, submit(4_000_000_000L, key = false))
            assertEquals(VideoCodecStatus.OK, submit(5_000_000_000L))
            val current = receive(5_000_000_000L)
            assertTrue((current.buffer as HdrSurfaceBuffer).presentAt(System.nanoTime() + 8_333_333L))
        } finally {
            decoder.release()
            held.forEach { it.release() }
            while (true) (delivered.poll() ?: break).release()
            surfaces.forEach { it.release() }
            textures.forEach { it.release() }
        }
    }
}

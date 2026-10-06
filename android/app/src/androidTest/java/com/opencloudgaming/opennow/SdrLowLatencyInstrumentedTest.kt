package com.opencloudgaming.opennow

import android.os.Build
import androidx.test.platform.app.InstrumentationRegistry
import org.junit.Assert.*
import org.junit.Assume.assumeTrue
import org.junit.Test
import org.webrtc.*
import java.nio.ByteBuffer
import java.util.concurrent.LinkedBlockingQueue
import java.util.concurrent.TimeUnit

class SdrLowLatencyInstrumentedTest {
    @Test fun sharedQualcommProfileWorksThroughTheWebRtcCodecWrapper() {
        assumeTrue(Build.VERSION.SDK_INT >= 30)
        val context = InstrumentationRegistry.getInstrumentation().targetContext
        PeerConnectionFactory.initialize(PeerConnectionFactory.InitializationOptions.builder(context).createInitializationOptions())
        val egl = EglBase.create()
        val factory = OpenNowVideoDecoderFactory(egl.eglBaseContext, true, { 120 }, directJavaDecode = true)
        val decoder = factory.createDecoder(VideoCodecInfo("H265", emptyMap(), emptyList()))
        assumeTrue(decoder != null && isQualcommMediaCodecDecoder(decoder.implementationName))
        val output = LinkedBlockingQueue<VideoFrame>()
        val payload = InstrumentationRegistry.getInstrumentation().context.assets.open("sdr-decoder-keyframe.hevc").use { it.readBytes() }
        var frame: VideoFrame? = null
        try {
            assertEquals(VideoCodecStatus.OK, requireNotNull(decoder).initDecode(VideoDecoder.Settings(2, 1280, 720),
                VideoDecoder.Callback { decoded, _, _ -> decoded.retain(); output.add(decoded) }))
            val image = EncodedImage.builder().setBuffer(ByteBuffer.allocateDirect(payload.size).apply { put(payload); flip() }, null)
                .setEncodedWidth(1280).setEncodedHeight(720).setCaptureTimeNs(1_000_000_000L)
                .setFrameType(EncodedImage.FrameType.VideoFrameKey).createEncodedImage()
            try { assertEquals(VideoCodecStatus.OK, decoder.decode(image, null)) }
            finally { image.release() }
            frame = output.poll(5, TimeUnit.SECONDS)
            assertNotNull("SDR output lost after low-latency configuration", frame)
            assertEquals(1280, requireNotNull(frame).buffer.width)
            assertEquals(720, frame.buffer.height)
        } finally {
            frame?.release()
            decoder?.release()
            egl.release()
        }
    }
}

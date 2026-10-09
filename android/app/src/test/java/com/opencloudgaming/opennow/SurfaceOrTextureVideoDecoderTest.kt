package com.opencloudgaming.opennow

import org.junit.Assert.*
import org.junit.Test
import org.webrtc.EncodedImage
import org.webrtc.VideoCodecStatus
import org.webrtc.VideoDecoder

class SurfaceOrTextureVideoDecoderTest {
    private class Decoder(private val name: String, private val calls: MutableList<String>) : VideoDecoder {
        var initialization = VideoCodecStatus.OK
        var decoding = VideoCodecStatus.OK
        var releasing = VideoCodecStatus.OK
        var afterDecode: () -> Unit = {}
        var lastSettings: VideoDecoder.Settings? = null
        var lastCallback: VideoDecoder.Callback? = null
        override fun initDecode(settings: VideoDecoder.Settings?, callback: VideoDecoder.Callback?): VideoCodecStatus {
            calls += "$name.init"
            lastSettings = settings
            lastCallback = callback
            return initialization
        }
        override fun decode(frame: EncodedImage?, info: VideoDecoder.DecodeInfo?): VideoCodecStatus {
            calls += "$name.decode"
            afterDecode()
            return decoding
        }
        override fun release(): VideoCodecStatus { calls += "$name.release"; return releasing }
        override fun getImplementationName() = name
    }

    private class Fixture {
        val calls = mutableListOf<String>()
        val surface = Decoder("surface", calls)
        val texture = Decoder("texture", calls)
        var preferred = true
        var failure: String? = null
        val settings = VideoDecoder.Settings(4, 1920, 1080)
        val callback = VideoDecoder.Callback { _, _, _ -> }
        val decoder = SurfaceOrTextureVideoDecoder(surface, texture, { preferred }, { failure }, { calls += "request:$it" })
        fun init() = decoder.initDecode(settings, callback)
        fun decode() = decoder.decode(null, null)
    }

    @Test fun healthySurfaceDoesNotStartTextureDecoder() {
        val f = Fixture()
        assertEquals(VideoCodecStatus.OK, f.init())
        assertEquals(VideoCodecStatus.OK, f.decode())
        assertEquals("surface", f.decoder.implementationName)
        assertEquals(VideoCodecStatus.OK, f.decoder.release())
        assertEquals(listOf("surface.init", "surface.decode", "surface.release"), f.calls)
    }

    @Test fun featureRequestReleasesSurfaceBeforeStartingTextureAndNeverFlipsBack() {
        val f = Fixture()
        f.init()
        f.preferred = false
        assertEquals(VideoCodecStatus.OK, f.decode())
        f.preferred = true
        assertEquals(VideoCodecStatus.OK, f.decode())
        f.decoder.release()
        assertEquals(listOf("surface.init", "request:texture frames requested", "surface.release",
            "texture.init", "texture.decode", "texture.decode", "texture.release"), f.calls)
        assertSame(f.settings, f.texture.lastSettings)
        assertSame(f.callback, f.texture.lastCallback)
    }

    @Test fun initializationFailureFallsBackWithinSameDecoder() {
        val f = Fixture()
        f.surface.initialization = VideoCodecStatus.ERROR
        assertEquals(VideoCodecStatus.OK, f.init())
        assertEquals(listOf("surface.init", "request:surface initialization failed", "surface.release", "texture.init"), f.calls)
    }

    @Test fun asynchronousCodecFailureFallsBackBeforeNextFrame() {
        val f = Fixture()
        f.init()
        f.failure = "codec callback"
        assertEquals(VideoCodecStatus.OK, f.decode())
        assertEquals(listOf("surface.init", "request:codec callback", "surface.release", "texture.init", "texture.decode"), f.calls)
    }

    @Test fun configurationFailureRetriesCurrentFrameOnTextureDecoder() {
        val f = Fixture()
        f.init()
        f.surface.decoding = VideoCodecStatus.ERROR
        f.surface.afterDecode = { f.failure = "unsupported size/rate/profile" }
        assertEquals(VideoCodecStatus.OK, f.decode())
        assertEquals(listOf("surface.init", "surface.decode", "request:unsupported size/rate/profile",
            "surface.release", "texture.init", "texture.decode"), f.calls)
    }

    @Test fun keyframeRecoveryOrMissingSurfaceDoesNotPermanentlyDisableDirectOutput() {
        val f = Fixture()
        f.init()
        f.surface.decoding = VideoCodecStatus.ERROR
        assertEquals(VideoCodecStatus.ERROR, f.decode())
        f.surface.decoding = VideoCodecStatus.NO_OUTPUT
        assertEquals(VideoCodecStatus.NO_OUTPUT, f.decode())
        assertFalse(f.calls.any { it.startsWith("texture") || it.startsWith("request") })
    }

    @Test fun releaseTimeoutCannotStartAnotherProducer() {
        val f = Fixture()
        f.init()
        f.surface.releasing = VideoCodecStatus.TIMEOUT
        f.preferred = false
        assertEquals(VideoCodecStatus.TIMEOUT, f.decode())
        assertEquals(VideoCodecStatus.UNINITIALIZED, f.decode())
        assertFalse(f.calls.any { it.startsWith("texture") })
    }

    @Test fun failedTextureInitializationIsReleasedAndCannotDecode() {
        val f = Fixture()
        f.init()
        f.texture.initialization = VideoCodecStatus.ERROR
        f.preferred = false
        assertEquals(VideoCodecStatus.ERROR, f.decode())
        assertEquals(VideoCodecStatus.UNINITIALIZED, f.decode())
        assertEquals("texture.release", f.calls.last())
    }

    @Test fun textureOnlyModeDoesNotInitializeSurface() {
        val f = Fixture()
        f.preferred = false
        assertEquals(VideoCodecStatus.OK, f.init())
        assertEquals(VideoCodecStatus.OK, f.decode())
        f.decoder.release()
        assertEquals(listOf("texture.init", "texture.decode", "texture.release"), f.calls)
    }

    @Test fun rejectsInvalidOrRepeatedInitializationAndReleaseIsIdempotent() {
        val f = Fixture()
        assertEquals(VideoCodecStatus.ERR_PARAMETER, f.decoder.initDecode(null, f.callback))
        assertEquals(VideoCodecStatus.UNINITIALIZED, f.decode())
        f.init()
        assertEquals(VideoCodecStatus.ERROR, f.init())
        f.decoder.release()
        f.decoder.release()
        assertEquals(listOf("surface.init", "surface.release"), f.calls)
    }
}

package com.opencloudgaming.opennow

import org.junit.Assert.*
import org.junit.Test
import org.webrtc.EncodedImage
import org.webrtc.VideoCodecStatus
import org.webrtc.VideoDecoder

class DecoderInitializationFallbackTest {
    private class Decoder(val name: String, val calls: MutableList<String>) : VideoDecoder {
        var initialization = VideoCodecStatus.OK
        var releasing = VideoCodecStatus.OK
        var decoding = VideoCodecStatus.OK
        var failure: RuntimeException? = null
        var releaseFailure: RuntimeException? = null
        var settings: VideoDecoder.Settings? = null
        var callback: VideoDecoder.Callback? = null
        override fun initDecode(settings: VideoDecoder.Settings?, callback: VideoDecoder.Callback?): VideoCodecStatus {
            calls += "$name.init"
            this.settings = settings
            this.callback = callback
            failure?.let { throw it }
            return initialization
        }
        override fun decode(frame: EncodedImage?, info: VideoDecoder.DecodeInfo?): VideoCodecStatus {
            calls += "$name.decode"
            return decoding
        }
        override fun release(): VideoCodecStatus {
            calls += "$name.release"
            releaseFailure?.let { throw it }
            return releasing
        }
        override fun getImplementationName() = name
    }

    private class Fixture {
        val calls = mutableListOf<String>()
        val reports = mutableListOf<String>()
        val tuned = Decoder("tuned", calls)
        val plain = Decoder("plain", calls)
        val settings = VideoDecoder.Settings(2, 1680, 720)
        val callback = VideoDecoder.Callback { _, _, _ -> }
        val decoder = DecoderInitializationFallback(tuned, { calls += "createPlain"; plain }, reports::add)
        fun init() = decoder.initDecode(settings, callback)
    }

    @Test fun healthyDecoderKeepsTuningAndDoesNotCreateAnotherCodec() {
        val f = Fixture()
        assertEquals(VideoCodecStatus.OK, f.init())
        assertEquals(VideoCodecStatus.OK, f.decoder.decode(null, null))
        f.decoder.release()
        f.decoder.release()
        assertEquals(listOf("tuned.init", "tuned.decode", "tuned.release"), f.calls)
        assertTrue(f.reports.isEmpty())
    }

    @Test fun rejectedTuningReleasesCodecThenRetriesSameProfileAndCallback() {
        val f = Fixture()
        f.tuned.initialization = VideoCodecStatus.FALLBACK_SOFTWARE
        assertEquals(VideoCodecStatus.OK, f.init())
        assertSame(f.settings, f.plain.settings)
        assertSame(f.callback, f.plain.callback)
        assertEquals("plain", f.decoder.implementationName)
        assertEquals(VideoCodecStatus.OK, f.decoder.decode(null, null))
        f.decoder.release()
        assertEquals(listOf("tuned.init", "tuned.release", "createPlain", "plain.init", "plain.decode", "plain.release"), f.calls)
    }

    @Test fun thrownInitializationFailureIsRetainedAndRecoversWithinDecoder() {
        val f = Fixture()
        f.tuned.failure = IllegalStateException("configure rejected", IllegalArgumentException("vendor key"))
        assertEquals(VideoCodecStatus.OK, f.init())
        assertEquals(1, f.reports.size)
        assertTrue(f.reports.single().contains("IllegalStateException: configure rejected"))
        assertTrue(f.reports.single().contains("IllegalArgumentException: vendor key"))
        assertEquals(listOf("tuned.init", "tuned.release", "createPlain", "plain.init"), f.calls)
    }

    @Test fun releaseTimeoutCannotStartAnotherDecoder() {
        val f = Fixture()
        f.tuned.initialization = VideoCodecStatus.ERROR
        f.tuned.releasing = VideoCodecStatus.TIMEOUT
        assertEquals(VideoCodecStatus.TIMEOUT, f.init())
        assertEquals(VideoCodecStatus.UNINITIALIZED, f.decoder.decode(null, null))
        f.decoder.release()
        assertEquals(listOf("tuned.init", "tuned.release"), f.calls)
    }

    @Test fun failedPlainDecoderDoesNotRetryAgainOrAcceptFrames() {
        val f = Fixture()
        f.tuned.initialization = VideoCodecStatus.ERROR
        f.plain.initialization = VideoCodecStatus.ERROR
        assertEquals(VideoCodecStatus.ERROR, f.init())
        assertEquals(VideoCodecStatus.ERROR, f.init())
        assertEquals(VideoCodecStatus.UNINITIALIZED, f.decoder.decode(null, null))
        f.decoder.release()
        assertEquals(listOf("tuned.init", "tuned.release", "createPlain", "plain.init", "plain.release"), f.calls)
    }

    @Test fun thrownPlainFailureRemainsVisibleAndCanBeReleased() {
        val f = Fixture()
        f.tuned.initialization = VideoCodecStatus.ERROR
        val failure = IllegalStateException("plain configure failed")
        f.plain.failure = failure
        assertEquals(VideoCodecStatus.ERROR, f.init())
        assertTrue(f.reports.last().contains("plain configure failed"))
        f.decoder.release()
        assertEquals("plain.release", f.calls.last())
    }

    @Test fun frameErrorsDoNotReinitializeOrReconnectHealthyCodec() {
        val f = Fixture()
        f.init()
        f.tuned.decoding = VideoCodecStatus.ERROR
        assertEquals(VideoCodecStatus.ERROR, f.decoder.decode(null, null))
        assertEquals(listOf("tuned.init", "tuned.decode"), f.calls)
        assertFalse(f.decoder.supportsBorrowedNvstInput()) // Unknown implementations still copy.
    }

    @Test fun invalidOrRepeatedInitializationDoesNotAllocateCodecs() {
        val f = Fixture()
        assertEquals(VideoCodecStatus.ERR_PARAMETER, f.decoder.initDecode(null, f.callback))
        assertEquals(VideoCodecStatus.UNINITIALIZED, f.decoder.decode(null, null))
        f.init()
        assertEquals(VideoCodecStatus.ERROR, f.init())
        assertEquals(listOf("tuned.init"), f.calls)
    }

    @Test fun webRtcCanReleaseAndReinitializeHealthyDecoderWithNewSettings() {
        val f = Fixture()
        f.init()
        f.decoder.release()
        f.decoder.release()
        assertEquals(VideoCodecStatus.UNINITIALIZED, f.decoder.decode(null, null))
        val nextSettings = VideoDecoder.Settings(2, 1920, 1080)
        val nextCallback = VideoDecoder.Callback { _, _, _ -> }
        assertEquals(VideoCodecStatus.OK, f.decoder.initDecode(nextSettings, nextCallback))
        assertSame(nextSettings, f.tuned.settings)
        assertSame(nextCallback, f.tuned.callback)
        assertEquals(VideoCodecStatus.OK, f.decoder.decode(null, null))
        f.decoder.release()
        assertEquals(listOf("tuned.init", "tuned.release", "tuned.init", "tuned.decode", "tuned.release"), f.calls)
    }

    @Test fun webRtcReinitializationStaysUntunedAfterStartupFallback() {
        val f = Fixture()
        f.tuned.initialization = VideoCodecStatus.FALLBACK_SOFTWARE
        assertEquals(VideoCodecStatus.OK, f.init())
        f.decoder.release()
        assertEquals(VideoCodecStatus.OK, f.init())
        assertEquals(VideoCodecStatus.OK, f.decoder.decode(null, null))
        f.decoder.release()
        assertEquals(listOf("tuned.init", "tuned.release", "createPlain", "plain.init", "plain.release",
            "plain.init", "plain.decode", "plain.release"), f.calls)
        assertEquals(1, f.reports.size)
    }

    @Test fun laterInitializationFailureCanFallbackAfterHealthyFirstRun() {
        val f = Fixture()
        f.init()
        f.decoder.release()
        f.tuned.initialization = VideoCodecStatus.ERROR
        assertEquals(VideoCodecStatus.OK, f.init())
        assertEquals(listOf("tuned.init", "tuned.release", "tuned.init", "tuned.release", "createPlain", "plain.init"), f.calls)
    }

    @Test fun failedUntunedReinitializationCannotRetryTuningOrCreateAnotherFallback() {
        val f = Fixture()
        f.tuned.initialization = VideoCodecStatus.ERROR
        f.init()
        f.decoder.release()
        f.plain.initialization = VideoCodecStatus.ERROR
        assertEquals(VideoCodecStatus.ERROR, f.init())
        assertEquals(VideoCodecStatus.UNINITIALIZED, f.decoder.decode(null, null))
        f.decoder.release()
        assertEquals(1, f.calls.count { it == "tuned.init" })
        assertEquals(1, f.calls.count { it == "createPlain" })
        assertEquals(2, f.calls.count { it == "plain.init" })
    }

    @Test fun releaseExceptionCannotEscapeJniOrStartAnotherCodec() {
        val f = Fixture()
        f.tuned.initialization = VideoCodecStatus.ERROR
        f.tuned.releaseFailure = IllegalStateException("release failed")
        assertEquals(VideoCodecStatus.ERROR, f.init())
        assertTrue(f.reports.last().contains("release failed"))
        f.decoder.release()
        assertEquals(VideoCodecStatus.UNINITIALIZED, f.init())
        assertFalse(f.calls.contains("createPlain"))
    }

    @Test fun failedLifecycleReleaseCannotReinitializeUncertainPlatformResources() {
        val f = Fixture()
        f.init()
        f.tuned.releaseFailure = IllegalStateException("codec release failed")
        assertEquals(VideoCodecStatus.ERROR, f.decoder.release())
        assertEquals(VideoCodecStatus.OK, f.decoder.release())
        assertEquals(VideoCodecStatus.UNINITIALIZED, f.init())
        assertEquals(listOf("tuned.init", "tuned.release"), f.calls)
    }

    @Test fun diagnosticsFailureDoesNotEscapeNativeInitializationBoundary() {
        val calls = mutableListOf<String>()
        val preferred = Decoder("tuned", calls).apply { initialization = VideoCodecStatus.ERROR }
        val fallback = Decoder("plain", calls)
        val decoder = DecoderInitializationFallback(preferred, { fallback }, { throw IllegalStateException("observer") })
        assertEquals(VideoCodecStatus.OK, decoder.initDecode(VideoDecoder.Settings(2, 1680, 720),
            VideoDecoder.Callback { _, _, _ -> }))
        assertEquals(listOf("tuned.init", "tuned.release", "plain.init"), calls)
    }
}

package com.opencloudgaming.opennow

import org.webrtc.EncodedImage
import org.webrtc.VideoCodecStatus
import org.webrtc.VideoDecoder

/** Retry rejected codec tuning once, on a fresh decoder in the current media attachment. */
internal class DecoderInitializationFallback(
    preferred: VideoDecoder,
    private val createFallback: () -> VideoDecoder?,
    private val reportFailure: (String) -> Unit,
) : VideoDecoder {
    private var active: VideoDecoder? = preferred
    private var initialized = false
    private var ready = false

    internal val canBorrowEncodedInput: Boolean
        get() = active?.supportsBorrowedNvstInput() == true

    override fun initDecode(settings: VideoDecoder.Settings?, callback: VideoDecoder.Callback?): VideoCodecStatus {
        if (settings == null || callback == null) return VideoCodecStatus.ERR_PARAMETER
        if (initialized) return VideoCodecStatus.ERROR
        val preferred = active ?: return VideoCodecStatus.UNINITIALIZED
        initialized = true
        var failure: RuntimeException? = null
        val status = try {
            preferred.initDecode(settings, callback)
        } catch (error: RuntimeException) {
            failure = error
            VideoCodecStatus.ERROR
        }
        if (status == VideoCodecStatus.OK) {
            ready = true
            return status
        }
        reportFailure("tuned decoder initialization failed; retrying without tuning: " +
            (failure?.let(::boundedJavaCrashTrace) ?: status.toString()))
        // Never start a second codec while the first still owns platform resources.
        active = null
        val released = preferred.release()
        if (released != VideoCodecStatus.OK) return released
        val fallback = createFallback() ?: return VideoCodecStatus.ERROR
        active = fallback
        return fallback.initDecode(settings, callback).also { ready = it == VideoCodecStatus.OK }
    }

    override fun decode(frame: EncodedImage?, info: VideoDecoder.DecodeInfo?): VideoCodecStatus =
        if (ready) active?.decode(frame, info) ?: VideoCodecStatus.UNINITIALIZED
        else VideoCodecStatus.UNINITIALIZED

    override fun release(): VideoCodecStatus {
        val previous = active
        active = null
        ready = false
        return previous?.release() ?: VideoCodecStatus.OK
    }

    override fun getImplementationName(): String = active?.implementationName ?: "decoder-released"
}

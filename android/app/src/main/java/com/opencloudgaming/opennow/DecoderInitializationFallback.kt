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
    private var released = false
    private var fallbackAttempted = false

    internal val canBorrowEncodedInput: Boolean
        get() = active?.supportsBorrowedNvstInput() == true

    override fun initDecode(settings: VideoDecoder.Settings?, callback: VideoDecoder.Callback?): VideoCodecStatus {
        if (settings == null || callback == null) return VideoCodecStatus.ERR_PARAMETER
        if (initialized) return VideoCodecStatus.ERROR
        val preferred = active ?: return VideoCodecStatus.UNINITIALIZED
        initialized = true
        released = false
        val result = initialize(preferred, settings, callback)
        val status = result.status
        if (status == VideoCodecStatus.OK) {
            ready = true
            return status
        }
        if (fallbackAttempted) {
            retainFailure("untuned decoder initialization failed: ${result.detail()}")
            return status
        }
        fallbackAttempted = true
        retainFailure("tuned decoder initialization failed; retrying without tuning: ${result.detail()}")
        // Never start a second codec while the first still owns platform resources.
        active = null
        val released = releaseDecoder(preferred)
        if (released != VideoCodecStatus.OK) return released
        val fallback = try {
            createFallback()
        } catch (error: RuntimeException) {
            retainFailure("untuned decoder creation failed: ${boundedJavaCrashTrace(error)}")
            null
        } ?: return VideoCodecStatus.ERROR
        active = fallback
        val retried = initialize(fallback, settings, callback)
        ready = retried.status == VideoCodecStatus.OK
        if (!ready) retainFailure("untuned decoder initialization failed: ${retried.detail()}")
        return retried.status
    }

    override fun decode(frame: EncodedImage?, info: VideoDecoder.DecodeInfo?): VideoCodecStatus =
        if (ready) active?.decode(frame, info) ?: VideoCodecStatus.UNINITIALIZED
        else VideoCodecStatus.UNINITIALIZED

    override fun release(): VideoCodecStatus {
        if (released) return VideoCodecStatus.OK
        released = true
        val previous = active
        ready = false
        initialized = false
        // WebRTC may reuse this Java decoder after Release/InitDecode. Keep the current
        // implementation, including its one-way untuned choice, only after a clean release.
        return (previous?.let(::releaseDecoder) ?: VideoCodecStatus.OK).also {
            if (it != VideoCodecStatus.OK) active = null
        }
    }

    private data class Initialization(val status: VideoCodecStatus, val error: RuntimeException? = null) {
        fun detail(): String = error?.let(::boundedJavaCrashTrace) ?: status.toString()
    }

    // Java decoder lifecycle methods are also called from WebRTC JNI: failures must return
    // status codes rather than leave a pending Java exception at that native boundary.
    private fun initialize(decoder: VideoDecoder, settings: VideoDecoder.Settings,
        callback: VideoDecoder.Callback): Initialization = try {
        Initialization(decoder.initDecode(settings, callback))
    } catch (error: RuntimeException) {
        Initialization(VideoCodecStatus.ERROR, error)
    }

    private fun releaseDecoder(decoder: VideoDecoder): VideoCodecStatus = try {
        decoder.release()
    } catch (error: RuntimeException) {
        retainFailure("decoder release failed: ${boundedJavaCrashTrace(error)}")
        VideoCodecStatus.ERROR
    }

    private fun retainFailure(detail: String) {
        try { reportFailure(detail) } catch (_: RuntimeException) { /* Diagnostics cannot abort JNI. */ }
    }

    override fun getImplementationName(): String = active?.implementationName ?: "decoder-released"
}

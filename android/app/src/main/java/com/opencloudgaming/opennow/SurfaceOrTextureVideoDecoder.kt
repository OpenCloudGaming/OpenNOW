package com.opencloudgaming.opennow

import org.webrtc.EncodedImage
import org.webrtc.VideoCodecStatus
import org.webrtc.VideoDecoder

/** One-way fallback within the existing transport; never repeatedly retries a failed surface. */
internal class SurfaceOrTextureVideoDecoder(
    private val surfaceDecoder: VideoDecoder,
    private val textureDecoder: VideoDecoder,
    private val useSurface: () -> Boolean,
    private val surfaceFailure: () -> String?,
    private val requestTexture: (String) -> Unit,
) : VideoDecoder {
    private var settings: VideoDecoder.Settings? = null
    private var callback: VideoDecoder.Callback? = null
    private var active: VideoDecoder? = null
    private var usingTexture = false

    override fun initDecode(settings: VideoDecoder.Settings?, decodeCallback: VideoDecoder.Callback?): VideoCodecStatus {
        if (settings == null || decodeCallback == null) return VideoCodecStatus.ERR_PARAMETER
        if (active != null) return VideoCodecStatus.ERROR
        this.settings = settings
        callback = decodeCallback
        usingTexture = !useSurface()
        active = if (usingTexture) textureDecoder else surfaceDecoder
        val result = active!!.initDecode(settings, decodeCallback)
        if (!usingTexture && result != VideoCodecStatus.OK) return switchToTexture("surface initialization failed")
        if (result != VideoCodecStatus.OK) {
            active?.release()
            active = null
        }
        return result
    }

    override fun decode(frame: EncodedImage?, info: VideoDecoder.DecodeInfo?): VideoCodecStatus {
        if (active == null) return VideoCodecStatus.UNINITIALIZED
        val initialFailure = surfaceFailure()
        if (!usingTexture && (!useSurface() || initialFailure != null)) {
            val initialized = switchToTexture(initialFailure ?: "texture frames requested")
            if (initialized != VideoCodecStatus.OK) return initialized
        }
        val result = active!!.decode(frame, info)
        val decodeFailure = surfaceFailure()
        if (!usingTexture && decodeFailure != null) {
            val initialized = switchToTexture(decodeFailure)
            return if (initialized == VideoCodecStatus.OK) textureDecoder.decode(frame, info) else initialized
        }
        return result
    }

    private fun switchToTexture(reason: String): VideoCodecStatus {
        usingTexture = true
        requestTexture(reason)
        val released = active?.release()
        if (released != null && released != VideoCodecStatus.OK) {
            active = null
            return released
        }
        active = textureDecoder
        // The ordinary hardware texture decoder requests a keyframe after init. Its recovery
        // stays inside the current WebRTC/NVST session rather than reconnecting signaling.
        val result = textureDecoder.initDecode(settings, callback)
        if (result != VideoCodecStatus.OK) {
            textureDecoder.release()
            active = null
        }
        return result
    }

    override fun release(): VideoCodecStatus {
        val previous = active
        active = null
        settings = null
        callback = null
        return previous?.release() ?: VideoCodecStatus.OK
    }

    override fun getImplementationName(): String =
        active?.implementationName ?: "${surfaceDecoder.implementationName}+texture-fallback"
}

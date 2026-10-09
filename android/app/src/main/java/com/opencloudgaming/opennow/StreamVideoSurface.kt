package com.opencloudgaming.opennow

import android.content.Context
import android.view.Gravity
import android.view.SurfaceHolder
import android.view.SurfaceView
import android.widget.FrameLayout
import org.webrtc.EglBase
import org.webrtc.RendererCommon
import org.webrtc.SurfaceViewRenderer
import org.webrtc.VideoFrame
import org.webrtc.VideoSink

internal fun aspectFitStreamSurfaceSize(
    frameWidth: Int,
    frameHeight: Int,
    containerWidth: Int,
    containerHeight: Int,
): Pair<Int, Int> {
    if (frameWidth <= 0 || frameHeight <= 0 || containerWidth <= 0 || containerHeight <= 0) {
        return containerWidth.coerceAtLeast(0) to containerHeight.coerceAtLeast(0)
    }
    val scale = minOf(
        containerWidth.toFloat() / frameWidth,
        containerHeight.toFloat() / frameHeight,
    )
    return (frameWidth * scale).toInt().coerceIn(1, containerWidth) to
        (frameHeight * scale).toInt().coerceIn(1, containerHeight)
}

/** Owns separate surface producers so MediaCodec and EGL never connect to the same Surface. */
class StreamVideoSurface(context: Context, private val hdr: Boolean, preferDirectSdr: Boolean = false) : FrameLayout(context), VideoSink {
    private val sdr = if (hdr) null else SurfaceViewRenderer(context)
    private val direct = if (hdr || preferDirectSdr) SurfaceView(context) else null
    private val surfaces = listOfNotNull(direct, sdr)
    @Volatile private var textureRequired = !preferDirectSdr
    @Volatile private var released = false
    internal val prefersDirectSdr: Boolean get() = !hdr && direct != null && !textureRequired && !released
    private val surfaceView: SurfaceView get() = if (hdr || prefersDirectSdr) direct!! else sdr!!
    val holder: SurfaceHolder get() = surfaceView.holder
    @Volatile internal var decoderTarget: DecoderSurfaceTarget? = null
        private set
    private var events: RendererCommon.RendererEvents? = null
    @Volatile private var frameWidth = 0
    @Volatile private var frameHeight = 0
    private var firstFrame = true
    @Volatile private var recordingSink: VideoSink? = null

    // SDR can switch to texture frames before recording; HDR must keep opaque ten-bit output.
    internal val supportsDirectRecording: Boolean get() = sdr != null
    internal fun currentDecodedSize(): Pair<Int, Int>? =
        if (frameWidth > 0 && frameHeight > 0) frameWidth to frameHeight else null

    internal fun setRecordingSink(sink: VideoSink?) {
        if (sink != null) requestTextureOutput("recording needs texture frames")
        recordingSink = sink
    }

    internal fun addSurfaceCallback(callback: SurfaceHolder.Callback) = surfaces.forEach { it.holder.addCallback(callback) }
    internal fun removeSurfaceCallback(callback: SurfaceHolder.Callback) = surfaces.forEach { it.holder.removeCallback(callback) }

    internal fun requestTextureOutput(reason: String) {
        if (hdr || released || textureRequired) return
        // Decoder workers observe the flag immediately. View mutations remain on the UI thread.
        textureRequired = true
        decoderTarget = null
        NativeInputDiagnostics.addRetained("surface-fallback", "video output switched to EGL textures reason=$reason")
        post {
            if (!released) {
                direct?.visibility = GONE
                sdr?.visibility = VISIBLE
                requestLayout()
            }
        }
    }

    init {
        clipChildren = false
        clipToPadding = false
        surfaces.forEach { child ->
            child.visibility = if (child === surfaceView) VISIBLE else GONE
            addView(child, LayoutParams(LayoutParams.MATCH_PARENT, LayoutParams.MATCH_PARENT, Gravity.CENTER))
        }
        direct?.holder?.addCallback(object : SurfaceHolder.Callback {
            override fun surfaceCreated(holder: SurfaceHolder) {
                decoderTarget = when {
                    released || (!hdr && !prefersDirectSdr) -> null
                    hdr && StreamHdr.displayProfile(context) == null -> null
                    else -> DecoderSurfaceTarget(holder.surface)
                }
                if (hdr && decoderTarget == null) NativeInputDiagnostics.add("HDR surface unavailable: display no longer supports HDR10")
            }
            override fun surfaceDestroyed(holder: SurfaceHolder) { decoderTarget = null }
            override fun surfaceChanged(holder: SurfaceHolder, format: Int, width: Int, height: Int) = Unit
        })
    }

    fun init(context: EglBase.Context, events: RendererCommon.RendererEvents, config: IntArray,
        drawer: RendererCommon.GlDrawer) {
        this.events = events
        sdr?.init(context, object : RendererCommon.RendererEvents {
            override fun onFirstFrameRendered() = events.onFirstFrameRendered()
            override fun onFrameResolutionChanged(width: Int, height: Int, rotation: Int) {
                val quarterTurns = ((rotation % 360) + 360) % 360
                publishSize(if (quarterTurns == 90 || quarterTurns == 270) height else width,
                    if (quarterTurns == 90 || quarterTurns == 270) width else height)
                events.onFrameResolutionChanged(width, height, rotation)
            }
        }, config, drawer)
    }

    private fun publishSize(width: Int, height: Int): Boolean {
        if (width <= 0 || height <= 0 || (frameWidth == width && frameHeight == height)) return false
        frameWidth = width
        frameHeight = height
        post { requestLayout() }
        return true
    }

    override fun onFrame(frame: VideoFrame) {
        if (released) return
        val opaque = frame.buffer as? MediaCodecSurfaceBuffer
        if (opaque != null) {
            if ((!hdr && !prefersDirectSdr) || !opaque.present()) return
            if (publishSize(frame.rotatedWidth, frame.rotatedHeight)) {
                events?.onFrameResolutionChanged(frame.rotatedWidth, frame.rotatedHeight, 0)
            }
            if (firstFrame) {
                firstFrame = false
                events?.onFirstFrameRendered()
            }
        } else if (sdr != null) {
            // A renderer may be recreated while an existing decoder has already fallen back.
            // Follow the frame type rather than leaving that new direct surface black.
            if (prefersDirectSdr) requestTextureOutput("decoder produced texture frames")
            publishSize(frame.rotatedWidth, frame.rotatedHeight)
            recordingSink?.onFrame(frame)
            sdr.onFrame(frame)
        }
    }

    override fun onLayout(changed: Boolean, left: Int, top: Int, right: Int, bottom: Int) {
        if (frameWidth <= 0 || frameHeight <= 0) {
            super.onLayout(changed, left, top, right, bottom)
            return
        }
        val (videoWidth, videoHeight) = aspectFitStreamSurfaceSize(frameWidth, frameHeight, width, height)
        val x = (width - videoWidth) / 2
        val y = (height - videoHeight) / 2
        surfaces.forEach { it.layout(x, y, x + videoWidth, y + videoHeight) }
    }

    fun setEnableHardwareScaler(enabled: Boolean) { sdr?.setEnableHardwareScaler(enabled) }
    fun setMirror(mirror: Boolean) { sdr?.setMirror(mirror) }
    fun setScalingType(type: RendererCommon.ScalingType) { sdr?.setScalingType(type) }

    /** Apply transforms to both native layers so a texture fallback preserves stretch-to-fit. */
    fun setPresentationScale(scaleX: Float, scaleY: Float) {
        surfaces.forEach {
            it.scaleX = scaleX
            it.scaleY = scaleY
        }
    }

    fun release() {
        if (released) return
        released = true
        recordingSink = null
        decoderTarget = null
        sdr?.release()
    }
}

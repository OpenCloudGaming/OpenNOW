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

/** Owns one surface producer: WebRTC GL for SDR, or the hardware decoder for HDR. */
class StreamVideoSurface(context: Context, private val hdr: Boolean) : FrameLayout(context), VideoSink {
    private val sdr = if (hdr) null else SurfaceViewRenderer(context)
    private val surfaceView = sdr ?: SurfaceView(context)
    val holder: SurfaceHolder get() = surfaceView.holder
    @Volatile internal var hdrTarget: HdrSurfaceTarget? = null
        private set
    private var events: RendererCommon.RendererEvents? = null
    @Volatile private var frameWidth = 0
    @Volatile private var frameHeight = 0
    private var firstFrame = true
    @Volatile private var released = false

    init {
        // Stretch-to-fit deliberately lets the native video surface extend past this wrapper's
        // aspect-fit bounds. The Compose viewport remains the final clip boundary.
        clipChildren = false
        clipToPadding = false
        addView(surfaceView, LayoutParams(LayoutParams.MATCH_PARENT, LayoutParams.MATCH_PARENT, Gravity.CENTER))
        if (hdr) holder.addCallback(object : SurfaceHolder.Callback {
            override fun surfaceCreated(holder: SurfaceHolder) {
                hdrTarget = if (StreamHdr.displayProfile(context) != null) HdrSurfaceTarget(holder.surface) else null
                if (hdrTarget == null) NativeInputDiagnostics.add("HDR surface unavailable: display no longer supports HDR10")
            }
            override fun surfaceDestroyed(holder: SurfaceHolder) { hdrTarget = null }
            override fun surfaceChanged(holder: SurfaceHolder, format: Int, width: Int, height: Int) = Unit
        })
    }

    fun init(context: EglBase.Context, events: RendererCommon.RendererEvents, config: IntArray,
        drawer: RendererCommon.GlDrawer) {
        this.events = events
        if (sdr != null) sdr.init(context, events, config, drawer)
    }

    override fun onFrame(frame: VideoFrame) {
        if (released) return
        if (sdr != null) {
            sdr.onFrame(frame)
            return
        }
        val buffer = frame.buffer as? HdrSurfaceBuffer ?: return
        if (!buffer.present()) return
        val width = frame.rotatedWidth
        val height = frame.rotatedHeight
        if (frameWidth != width || frameHeight != height) {
            frameWidth = width
            frameHeight = height
            events?.onFrameResolutionChanged(width, height, 0)
            post { requestLayout() }
        }
        if (firstFrame) {
            firstFrame = false
            events?.onFirstFrameRendered()
        }
    }

    override fun onLayout(changed: Boolean, left: Int, top: Int, right: Int, bottom: Int) {
        if (!hdr || frameWidth <= 0 || frameHeight <= 0) {
            super.onLayout(changed, left, top, right, bottom)
            return
        }
        val scale = minOf(width.toFloat() / frameWidth, height.toFloat() / frameHeight)
        val videoWidth = (frameWidth * scale).toInt()
        val videoHeight = (frameHeight * scale).toInt()
        val x = (width - videoWidth) / 2
        val y = (height - videoHeight) / 2
        surfaceView.layout(x, y, x + videoWidth, y + videoHeight)
    }

    fun setEnableHardwareScaler(enabled: Boolean) { sdr?.setEnableHardwareScaler(enabled) }
    fun setMirror(mirror: Boolean) { sdr?.setMirror(mirror) }
    fun setScalingType(type: RendererCommon.ScalingType) { sdr?.setScalingType(type) }

    /**
     * Apply presentation transforms to the SurfaceView itself, not this wrapper.
     *
     * SurfaceView buffers are composed in a separate native layer. Some Android 9/OEM
     * compositors do not reliably carry a parent View transform to that layer, which can leave a
     * black strip at one edge while stretch-to-fit is enabled. Scaling the actual surface also
     * preserves the behavior from before this HDR-capable wrapper was introduced.
     */
    fun setPresentationScale(scaleX: Float, scaleY: Float) {
        surfaceView.scaleX = scaleX
        surfaceView.scaleY = scaleY
    }

    fun release() { released = true; hdrTarget = null; sdr?.release() }
}

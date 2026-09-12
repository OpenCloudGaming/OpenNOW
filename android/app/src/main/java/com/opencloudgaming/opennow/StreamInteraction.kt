package com.opencloudgaming.opennow

import android.opengl.GLES11Ext
import android.opengl.GLES20
import android.view.MotionEvent
import org.webrtc.GlRectDrawer
import org.webrtc.GlShader
import org.webrtc.GlUtil
import org.webrtc.RendererCommon
import java.nio.FloatBuffer
import kotlin.math.abs
import kotlin.math.min
import kotlin.math.roundToInt
import kotlin.math.sqrt

internal fun streamSharpnessShaderStrength(enabled: Boolean, amount: Float): Float =
    if (enabled) amount.coerceIn(0f, 1f) * STREAM_SHARPNESS_MAX_SHADER_STRENGTH else 0f

private const val STREAM_SHARPNESS_MAX_SHADER_STRENGTH = 0.28f

internal class StreamSharpnessGlDrawer : RendererCommon.GlDrawer {
    @Volatile
    var amount: Float = 0f

    // Keep disabled sharpening on WebRTC's native pass-through path. Enabling the setting still
    // takes effect on the next frame without recreating the SurfaceViewRenderer.
    private val passthroughDrawer = GlRectDrawer()
    private val vertexBuffer: FloatBuffer = GlUtil.createFloatBuffer(
        floatArrayOf(
            -1f, -1f,
            1f, -1f,
            -1f, 1f,
            1f, 1f,
        ),
    )
    private val textureBuffer: FloatBuffer = GlUtil.createFloatBuffer(
        floatArrayOf(
            0f, 0f,
            1f, 0f,
            0f, 1f,
            1f, 1f,
        ),
    )

    private var oesProgram: SharpnessProgram? = null
    private var rgbProgram: SharpnessProgram? = null
    private var yuvProgram: SharpnessProgram? = null

    override fun drawOes(
        oesTextureId: Int,
        texMatrix: FloatArray,
        frameWidth: Int,
        frameHeight: Int,
        viewportX: Int,
        viewportY: Int,
        viewportWidth: Int,
        viewportHeight: Int,
    ) {
        val strength = amount
        if (!streamSharpnessShaderActive(strength)) {
            passthroughDrawer.drawOes(
                oesTextureId,
                texMatrix,
                frameWidth,
                frameHeight,
                viewportX,
                viewportY,
                viewportWidth,
                viewportHeight,
            )
            return
        }
        val program = oesProgram ?: SharpnessProgram(SHARPEN_OES_FRAGMENT_SHADER, TextureMode.Oes).also { oesProgram = it }
        program.draw(
            textureIds = intArrayOf(oesTextureId),
            textureTarget = GLES11Ext.GL_TEXTURE_EXTERNAL_OES,
            texMatrix = texMatrix,
            frameWidth = frameWidth,
            frameHeight = frameHeight,
            viewportX = viewportX,
            viewportY = viewportY,
            viewportWidth = viewportWidth,
            viewportHeight = viewportHeight,
            amount = strength,
            vertexBuffer = vertexBuffer,
            textureBuffer = textureBuffer,
        )
    }

    override fun drawRgb(
        textureId: Int,
        texMatrix: FloatArray,
        frameWidth: Int,
        frameHeight: Int,
        viewportX: Int,
        viewportY: Int,
        viewportWidth: Int,
        viewportHeight: Int,
    ) {
        val strength = amount
        if (!streamSharpnessShaderActive(strength)) {
            passthroughDrawer.drawRgb(
                textureId,
                texMatrix,
                frameWidth,
                frameHeight,
                viewportX,
                viewportY,
                viewportWidth,
                viewportHeight,
            )
            return
        }
        val program = rgbProgram ?: SharpnessProgram(SHARPEN_RGB_FRAGMENT_SHADER, TextureMode.Rgb).also { rgbProgram = it }
        program.draw(
            textureIds = intArrayOf(textureId),
            textureTarget = GLES20.GL_TEXTURE_2D,
            texMatrix = texMatrix,
            frameWidth = frameWidth,
            frameHeight = frameHeight,
            viewportX = viewportX,
            viewportY = viewportY,
            viewportWidth = viewportWidth,
            viewportHeight = viewportHeight,
            amount = strength,
            vertexBuffer = vertexBuffer,
            textureBuffer = textureBuffer,
        )
    }

    override fun drawYuv(
        yuvTextures: IntArray,
        texMatrix: FloatArray,
        frameWidth: Int,
        frameHeight: Int,
        viewportX: Int,
        viewportY: Int,
        viewportWidth: Int,
        viewportHeight: Int,
    ) {
        val strength = amount
        if (!streamSharpnessShaderActive(strength)) {
            passthroughDrawer.drawYuv(
                yuvTextures,
                texMatrix,
                frameWidth,
                frameHeight,
                viewportX,
                viewportY,
                viewportWidth,
                viewportHeight,
            )
            return
        }
        val program = yuvProgram ?: SharpnessProgram(SHARPEN_YUV_FRAGMENT_SHADER, TextureMode.Yuv).also { yuvProgram = it }
        program.draw(
            textureIds = yuvTextures,
            textureTarget = GLES20.GL_TEXTURE_2D,
            texMatrix = texMatrix,
            frameWidth = frameWidth,
            frameHeight = frameHeight,
            viewportX = viewportX,
            viewportY = viewportY,
            viewportWidth = viewportWidth,
            viewportHeight = viewportHeight,
            amount = strength,
            vertexBuffer = vertexBuffer,
            textureBuffer = textureBuffer,
        )
    }

    override fun release() {
        passthroughDrawer.release()
        oesProgram?.release()
        rgbProgram?.release()
        yuvProgram?.release()
        oesProgram = null
        rgbProgram = null
        yuvProgram = null
    }

    private class SharpnessProgram(fragmentShader: String, private val mode: TextureMode) {
        private val shader = GlShader(SHARPEN_VERTEX_SHADER, fragmentShader)
        private val texMatrixLocation = shader.getUniformLocation("tex_mat")
        private val sharpnessLocation = shader.getUniformLocation("sharpness")
        private val texelSizeLocation = shader.getUniformLocation("texel_size")
        private val textureLocations: IntArray = when (mode) {
            TextureMode.Oes,
            TextureMode.Rgb,
            -> intArrayOf(shader.getUniformLocation("tex"))
            TextureMode.Yuv -> intArrayOf(
                shader.getUniformLocation("y_tex"),
                shader.getUniformLocation("u_tex"),
                shader.getUniformLocation("v_tex"),
            )
        }

        fun draw(
            textureIds: IntArray,
            textureTarget: Int,
            texMatrix: FloatArray,
            frameWidth: Int,
            frameHeight: Int,
            viewportX: Int,
            viewportY: Int,
            viewportWidth: Int,
            viewportHeight: Int,
            amount: Float,
            vertexBuffer: FloatBuffer,
            textureBuffer: FloatBuffer,
        ) {
            shader.useProgram()
            GLES20.glViewport(viewportX, viewportY, viewportWidth, viewportHeight)
            shader.setVertexAttribArray("in_pos", 2, vertexBuffer)
            shader.setVertexAttribArray("in_tc", 2, textureBuffer)
            GLES20.glUniformMatrix4fv(texMatrixLocation, 1, false, texMatrix, 0)
            GLES20.glUniform1f(sharpnessLocation, amount.coerceIn(0f, STREAM_SHARPNESS_MAX_SHADER_STRENGTH))
            GLES20.glUniform2f(
                texelSizeLocation,
                1f / frameWidth.coerceAtLeast(1).toFloat(),
                1f / frameHeight.coerceAtLeast(1).toFloat(),
            )
            textureLocations.forEachIndexed { index, location ->
                GLES20.glActiveTexture(GLES20.GL_TEXTURE0 + index)
                GLES20.glBindTexture(textureTarget, textureIds.getOrElse(index) { 0 })
                GLES20.glUniform1i(location, index)
            }
            GLES20.glDrawArrays(GLES20.GL_TRIANGLE_STRIP, 0, 4)
            textureLocations.indices.forEach { index ->
                GLES20.glActiveTexture(GLES20.GL_TEXTURE0 + index)
                GLES20.glBindTexture(textureTarget, 0)
            }
            GlUtil.checkNoGLES2Error("StreamSharpnessGlDrawer.draw")
        }

        fun release() {
            shader.release()
        }
    }

    private enum class TextureMode {
        Oes,
        Rgb,
        Yuv,
    }

    private companion object {
        private const val SHARPEN_VERTEX_SHADER = """
            attribute vec4 in_pos;
            attribute vec2 in_tc;
            uniform mat4 tex_mat;
            varying vec2 tc;

            void main() {
              gl_Position = in_pos;
              tc = (tex_mat * vec4(in_tc, 0.0, 1.0)).xy;
            }
        """

        private const val SHARPEN_BODY = """
            uniform float sharpness;
            uniform vec2 texel_size;
            varying vec2 tc;

            void main() {
              vec4 center = sampleColor(tc);
              if (sharpness <= 0.001) {
                gl_FragColor = center;
                return;
              }
              vec3 north = sampleColor(tc + vec2(0.0, -texel_size.y)).rgb;
              vec3 south = sampleColor(tc + vec2(0.0, texel_size.y)).rgb;
              vec3 west = sampleColor(tc + vec2(-texel_size.x, 0.0)).rgb;
              vec3 east = sampleColor(tc + vec2(texel_size.x, 0.0)).rgb;
              vec3 sharpened = center.rgb * (1.0 + 4.0 * sharpness) - (north + south + west + east) * sharpness;
              gl_FragColor = vec4(clamp(sharpened, 0.0, 1.0), center.a);
            }
        """

        private const val SHARPEN_OES_FRAGMENT_SHADER = """
            #extension GL_OES_EGL_image_external : require
            precision mediump float;
            uniform samplerExternalOES tex;
            vec4 sampleColor(vec2 pos) {
              return texture2D(tex, pos);
            }
        """ + SHARPEN_BODY

        private const val SHARPEN_RGB_FRAGMENT_SHADER = """
            precision mediump float;
            uniform sampler2D tex;
            vec4 sampleColor(vec2 pos) {
              return texture2D(tex, pos);
            }
        """ + SHARPEN_BODY

        private const val SHARPEN_YUV_FRAGMENT_SHADER = """
            precision mediump float;
            uniform sampler2D y_tex;
            uniform sampler2D u_tex;
            uniform sampler2D v_tex;
            vec4 sampleColor(vec2 pos) {
              float y = texture2D(y_tex, pos).r;
              float u = texture2D(u_tex, pos).r - 0.5;
              float v = texture2D(v_tex, pos).r - 0.5;
              return vec4(
                y + 1.403 * v,
                y - 0.344 * u - 0.714 * v,
                y + 1.770 * u,
                1.0
              );
            }
        """ + SHARPEN_BODY
    }
}

internal fun streamSharpnessShaderActive(amount: Float): Boolean =
    amount.isFinite() && amount > STREAM_SHARPNESS_ACTIVE_EPSILON

private const val STREAM_SHARPNESS_ACTIVE_EPSILON = 0.001f

internal sealed interface StreamLivenessAction {
    data object None : StreamLivenessAction
    data class RequestKeyframe(val stalledMs: Long, val attempt: Int) : StreamLivenessAction
    data class RestartTransport(val stalledMs: Long) : StreamLivenessAction
}

internal class StreamLivenessWatchdog(
    private val keyframeAfterMs: Long = MEDIA_STALL_KEYFRAME_AFTER_MS,
    private val keyframeIntervalMs: Long = MEDIA_STALL_KEYFRAME_INTERVAL_MS,
    private val restartAfterMs: Long = MEDIA_STALL_RESTART_AFTER_MS,
) {
    private var lastProgressAtMs: Long? = null
    private var lastBytesReceived: Long? = null
    private var lastFramesDecoded: Long? = null
    private var lastKeyframeRequestAtMs = Long.MIN_VALUE
    private var keyframeAttempts = 0
    var latestObservationProgressed: Boolean = false
        private set

    fun reset() {
        lastProgressAtMs = null
        lastBytesReceived = null
        lastFramesDecoded = null
        lastKeyframeRequestAtMs = Long.MIN_VALUE
        keyframeAttempts = 0
        latestObservationProgressed = false
    }

    fun markConnected(nowMs: Long) {
        lastProgressAtMs = nowMs
        lastKeyframeRequestAtMs = Long.MIN_VALUE
        keyframeAttempts = 0
    }

    fun observe(nowMs: Long, bytesReceived: Long?, framesDecoded: Long?, connected: Boolean): StreamLivenessAction {
        latestObservationProgressed = false
        if (!connected) {
            reset()
            return StreamLivenessAction.None
        }

        val progressed = if (framesDecoded != null) {
            lastFramesDecoded?.let { framesDecoded > it } ?: (framesDecoded > 0)
        } else {
            bytesReceived != null && (lastBytesReceived?.let { bytesReceived > it } ?: (bytesReceived > 0))
        }
        if (bytesReceived != null) lastBytesReceived = bytesReceived
        if (framesDecoded != null) lastFramesDecoded = framesDecoded
        if (progressed) {
            latestObservationProgressed = true
            lastProgressAtMs = nowMs
            lastKeyframeRequestAtMs = Long.MIN_VALUE
            keyframeAttempts = 0
            return StreamLivenessAction.None
        }

        val stalledMs = nowMs - (lastProgressAtMs ?: nowMs.also { lastProgressAtMs = it })
        if (stalledMs >= restartAfterMs) {
            reset()
            return StreamLivenessAction.RestartTransport(stalledMs)
        }
        val keyframeDue = lastKeyframeRequestAtMs == Long.MIN_VALUE ||
            nowMs - lastKeyframeRequestAtMs >= keyframeIntervalMs
        if (stalledMs >= keyframeAfterMs && keyframeDue) {
            lastKeyframeRequestAtMs = nowMs
            keyframeAttempts += 1
            return StreamLivenessAction.RequestKeyframe(stalledMs, keyframeAttempts)
        }
        return StreamLivenessAction.None
    }
}

internal data class StreamRecoveryTiming(
    val keyframeAfterMs: Long,
    val keyframeIntervalMs: Long,
    val restartAfterMs: Long,
)

internal fun streamRecoveryTiming(androidTvProfile: Boolean): StreamRecoveryTiming =
    if (androidTvProfile) {
        StreamRecoveryTiming(
            keyframeAfterMs = TV_MEDIA_STALL_KEYFRAME_AFTER_MS,
            keyframeIntervalMs = TV_MEDIA_STALL_KEYFRAME_INTERVAL_MS,
            restartAfterMs = TV_MEDIA_STALL_RESTART_AFTER_MS,
        )
    } else {
        StreamRecoveryTiming(
            keyframeAfterMs = MEDIA_STALL_KEYFRAME_AFTER_MS,
            keyframeIntervalMs = MEDIA_STALL_KEYFRAME_INTERVAL_MS,
            restartAfterMs = MEDIA_STALL_RESTART_AFTER_MS,
        )
    }

internal fun firstVideoFrameRecoveryTimeoutMs(androidTvProfile: Boolean): Long =
    streamRecoveryTiming(androidTvProfile).restartAfterMs

internal enum class FirstFrameRecoveryStep {
    RetryRequestedProfile,
    RetrySelectedProfile,
    ContinueBoundedTransportRecovery,
}

internal data class DecodedResolutionTransition(
    val previousWidth: Int?,
    val previousHeight: Int?,
    val width: Int,
    val height: Int,
) {
    val isInitial: Boolean
        get() = previousWidth == null || previousHeight == null
}

/** Tracks actual decoder output sizes; every valid change is accepted without transport policy. */
internal class DecodedResolutionTracker {
    private var width: Int? = null
    private var height: Int? = null

    @Synchronized
    fun observe(width: Int, height: Int): DecodedResolutionTransition? {
        if (width <= 0 || height <= 0) return null
        if (this.width == width && this.height == height) return null
        return DecodedResolutionTransition(
            previousWidth = this.width,
            previousHeight = this.height,
            width = width,
            height = height,
        ).also {
            this.width = width
            this.height = height
        }
    }
}

internal fun firstFrameRecoveryStep(
    transportHasStableMedia: Boolean,
    reconnectAttempts: Int,
    selectedProfileRetryApplied: Boolean,
): FirstFrameRecoveryStep = when {
    transportHasStableMedia -> FirstFrameRecoveryStep.ContinueBoundedTransportRecovery
    reconnectAttempts == 0 -> FirstFrameRecoveryStep.RetryRequestedProfile
    !selectedProfileRetryApplied -> FirstFrameRecoveryStep.RetrySelectedProfile
    else -> FirstFrameRecoveryStep.ContinueBoundedTransportRecovery
}

internal fun transportRestartShouldRetrySelectedProfile(
    videoFailure: Boolean,
    reconnectAttempts: Int,
    transportHasStableMedia: Boolean,
): Boolean = videoFailure && reconnectAttempts >= 1 && !transportHasStableMedia

internal fun repeatedStableMediaStallShouldRetrySelectedProfile(
    androidTvProfile: Boolean,
    transportCodec: VideoCodec,
    completedStableMediaStallRestarts: Int,
    selectedProfileRetryApplied: Boolean,
): Boolean =
    androidTvProfile &&
        transportCodec != VideoCodec.H264 &&
        completedStableMediaStallRestarts >= 2 &&
        !selectedProfileRetryApplied

internal fun newStreamLivenessWatchdog(androidTvProfile: Boolean): StreamLivenessWatchdog {
    val timing = streamRecoveryTiming(androidTvProfile)
    return StreamLivenessWatchdog(
        keyframeAfterMs = timing.keyframeAfterMs,
        keyframeIntervalMs = timing.keyframeIntervalMs,
        restartAfterMs = timing.restartAfterMs,
    )
}

internal class FirstVideoFrameWatchdog(
    private val timeoutMs: Long = FIRST_VIDEO_FRAME_TIMEOUT_MS,
) {
    private var bytesWithoutFrameSinceMs: Long? = null
    private var rendered = false

    @Synchronized
    fun reset() {
        bytesWithoutFrameSinceMs = null
        rendered = false
    }

    @Synchronized
    fun markRendered() {
        rendered = true
        bytesWithoutFrameSinceMs = null
    }

    @Synchronized
    fun shouldRecover(nowMs: Long, bytesReceived: Long?, connected: Boolean): Boolean {
        if (!connected || rendered || bytesReceived == null || bytesReceived <= 0L) {
            if (!connected) bytesWithoutFrameSinceMs = null
            return false
        }
        val startedAt = bytesWithoutFrameSinceMs ?: nowMs.also { bytesWithoutFrameSinceMs = it }
        return nowMs - startedAt >= timeoutMs
    }
}

internal data class MouseMotionDelta(
    val dx: Int,
    val dy: Int,
)

/** Applies mouse tuning in float space and retains the wire format's subpixel rounding residual. */
internal class MouseMotionAccumulator(
    private val minimumSendIntervalMs: Long = 8L,
) {
    private var pendingDx = 0f
    private var pendingDy = 0f
    private var lastSendTimeMs = Long.MIN_VALUE

    fun reset() {
        pendingDx = 0f
        pendingDy = 0f
        lastSendTimeMs = Long.MIN_VALUE
    }

    fun add(
        dx: Float,
        dy: Float,
        eventTimeMs: Long,
        sensitivity: Float,
        acceleration: Int,
        force: Boolean = false,
    ): MouseMotionDelta? {
        if (!dx.isFinite() || !dy.isFinite() || !sensitivity.isFinite()) {
            reset()
            return null
        }
        var adjustedDx = dx * sensitivity
        var adjustedDy = dy * sensitivity
        if (acceleration > 1) {
            val speed = sqrt(adjustedDx * adjustedDx + adjustedDy * adjustedDy)
            val strength = (acceleration - 1f) / 149f
            val accelerationFactor = 1f + min(0.6f * strength, (speed / 50f) * strength)
            adjustedDx *= accelerationFactor
            adjustedDy *= accelerationFactor
        }
        if (!adjustedDx.isFinite() || !adjustedDy.isFinite()) {
            reset()
            return null
        }
        pendingDx += adjustedDx
        pendingDy += adjustedDy
        if (!pendingDx.isFinite() || !pendingDy.isFinite()) {
            reset()
            return null
        }

        val elapsedSinceSend = eventTimeMs - lastSendTimeMs
        if (
            !force &&
            lastSendTimeMs != Long.MIN_VALUE &&
            elapsedSinceSend in 0 until minimumSendIntervalMs
        ) {
            return null
        }

        val sendDx = pendingDx.roundToInt()
        val sendDy = pendingDy.roundToInt()
        if (sendDx == 0 && sendDy == 0) return null

        pendingDx -= sendDx
        pendingDy -= sendDy
        lastSendTimeMs = eventTimeMs
        return MouseMotionDelta(sendDx, sendDy)
    }
}

/** A point in the stream's own pixel space, as produced by [streamPointForTouch]. */
internal data class StreamPoint(val x: Float, val y: Float)

/** Phase of a single finger, as the host expects it on the wire. */
internal object TouchPhase {
    const val DOWN = 1
    const val UP = 2
    const val MOVE = 4
    const val CANCEL = 8
}

/** Touch coordinates travel as an unsigned 16-bit fraction of the video area. */
internal const val TOUCH_COORDINATE_MAX = 65535

/** The host tracks at most this many fingers at once. */
internal const val MAX_CONCURRENT_TOUCHES = 8

/** One packet carries at most this many records. */
internal const val MAX_TOUCH_RECORDS_PER_BATCH = 40

/**
 * One finger in one packet. [slot] is the host's finger index — deliberately not the platform's
 * pointer id, see [TouchSlotAllocator].
 */
internal data class TouchRecord(
    val slot: Int,
    val phase: Int,
    val x: Int,
    val y: Int,
    val radiusX: Int = 0,
    val radiusY: Int = 0,
    val timestampUs: Long = 0L,
)

/**
 * Maps platform pointer ids onto the small, dense finger indices the host expects.
 *
 * Android pointer ids are arbitrary and can climb without bound across a session; the host wants
 * the lowest free index, reused as soon as a finger lifts. Forwarding pointer ids directly would
 * make the host see fingers appear at ever-higher indices and eventually run past its own limit.
 *
 * Kept free of `MotionEvent` so it can be tested on the JVM.
 */
internal class TouchSlotAllocator {
    private val slotByPointerId = mutableMapOf<Int, Int>()
    private val usedSlots = mutableSetOf<Int>()

    val activeCount: Int get() = slotByPointerId.size

    /** Slot for [pointerId], allocating the lowest free one. Null when all slots are in use. */
    fun acquire(pointerId: Int): Int? {
        slotByPointerId[pointerId]?.let { return it }
        var slot = 0
        while (slot in usedSlots) slot++
        if (slot >= MAX_CONCURRENT_TOUCHES) return null
        slotByPointerId[pointerId] = slot
        usedSlots.add(slot)
        return slot
    }

    /** Slot for [pointerId] without allocating one. */
    fun peek(pointerId: Int): Int? = slotByPointerId[pointerId]

    /** Frees the slot held by [pointerId], returning it so the caller can still report the lift. */
    fun release(pointerId: Int): Int? {
        val slot = slotByPointerId.remove(pointerId) ?: return null
        usedSlots.remove(slot)
        return slot
    }

    fun activePointerIds(): List<Int> = slotByPointerId.keys.toList()

    fun clear() {
        slotByPointerId.clear()
        usedSlots.clear()
    }
}

/** One finger as the platform reported it, before any mapping. */
internal data class TouchPointerSample(
    val pointerId: Int,
    val x: Float,
    val y: Float,
    val radiusX: Float = 0f,
    val radiusY: Float = 0f,
)

/**
 * Turns one input event's fingers into the records for a single packet.
 *
 * Holds every rule that is easy to get subtly wrong — slot allocation, normalising to the video
 * area, and what to do with a finger that is outside it — and takes no `MotionEvent`, so all of it
 * is testable on the JVM.
 */
internal fun buildTouchBatch(
    allocator: TouchSlotAllocator,
    phase: Int,
    pointers: List<TouchPointerSample>,
    viewWidth: Int,
    viewHeight: Int,
    streamWidth: Int,
    streamHeight: Int,
    stretchToFit: Boolean,
    renderingAspectRatio: Float,
    presentationZoomScale: Float = 1f,
    presentationTranslationX: Float = 0f,
    presentationTranslationY: Float = 0f,
    timestampUs: Long = 0L,
): List<TouchRecord> {
    if (viewWidth <= 0 || viewHeight <= 0 || streamWidth <= 0 || streamHeight <= 0) return emptyList()

    // A lift must always be reported, wherever the finger ended up. Swallowing one leaves the host
    // holding that finger down for the rest of the session.
    val lifting = phase == TouchPhase.UP || phase == TouchPhase.CANCEL
    val records = ArrayList<TouchRecord>(pointers.size)

    for (pointer in pointers) {
        if (records.size >= MAX_TOUCH_RECORDS_PER_BATCH) break

        val point = streamPointForTouch(
            touchX = pointer.x,
            touchY = pointer.y,
            viewWidth = viewWidth,
            viewHeight = viewHeight,
            streamWidth = streamWidth,
            streamHeight = streamHeight,
            stretchToFit = stretchToFit,
            renderingAspectRatio = renderingAspectRatio,
            presentationZoomScale = presentationZoomScale,
            presentationTranslationX = presentationTranslationX,
            presentationTranslationY = presentationTranslationY,
            clamp = false,
        )
        val x = point.x / streamWidth * TOUCH_COORDINATE_MAX
        val y = point.y / streamHeight * TOUCH_COORDINATE_MAX
        val radiusX = pointer.radiusX / streamWidth * TOUCH_COORDINATE_MAX
        val radiusY = pointer.radiusY / streamHeight * TOUCH_COORDINATE_MAX

        val finiteSample = x.isFinite() && y.isFinite() && radiusX.isFinite() && radiusY.isFinite()
        if (!finiteSample && !lifting) continue

        val safeX = if (x.isFinite()) x else 0f
        val safeY = if (y.isFinite()) y else 0f
        val safeRadiusX = if (radiusX.isFinite()) radiusX.coerceAtLeast(0f) else 0f
        val safeRadiusY = if (radiusY.isFinite()) radiusY.coerceAtLeast(0f) else 0f
        val safePointerRadiusX = if (pointer.radiusX.isFinite()) pointer.radiusX.coerceAtLeast(0f) else 0f
        val safePointerRadiusY = if (pointer.radiusY.isFinite()) pointer.radiusY.coerceAtLeast(0f) else 0f
        val outside = safeX < -safeRadiusX || safeX > TOUCH_COORDINATE_MAX + safeRadiusX ||
            safeY < -safeRadiusY || safeY > TOUCH_COORDINATE_MAX + safeRadiusY
        if (outside && !lifting) continue

        val slot = (
            if (lifting) allocator.release(pointer.pointerId) else allocator.acquire(pointer.pointerId)
            ) ?: continue

        records += TouchRecord(
            slot = slot,
            phase = phase,
            x = safeX.roundToInt().coerceIn(0, TOUCH_COORDINATE_MAX),
            y = safeY.roundToInt().coerceIn(0, TOUCH_COORDINATE_MAX),
            radiusX = safePointerRadiusX.roundToInt().coerceAtLeast(0),
            radiusY = safePointerRadiusY.roundToInt().coerceAtLeast(0),
            timestampUs = timestampUs,
        )
    }
    return records
}

/**
 * Maps a touch inside a view of [viewWidth] x [viewHeight] onto the stream's pixel space, undoing
 * the presentation zoom/pan first, then the letterbox/pillarbox bars the renderer adds whenever
 * the view and the stream disagree about aspect ratio.
 *
 * Everything it needs arrives as an argument, and the result is expressed as a fraction of the
 * view — which is why a window resize (PiP, rotation, minimise) needs no cursor bookkeeping at all.
 * The event carries the view size it was measured against, so even a size captured mid-resize maps
 * that event correctly.
 */
internal fun streamPointForTouch(
    touchX: Float,
    touchY: Float,
    viewWidth: Int,
    viewHeight: Int,
    streamWidth: Int,
    streamHeight: Int,
    stretchToFit: Boolean,
    renderingAspectRatio: Float,
    presentationZoomScale: Float = 1f,
    presentationTranslationX: Float = 0f,
    presentationTranslationY: Float = 0f,
    /**
     * Clamping is right for a cursor, which must land somewhere. Native touch passes false so it
     * can tell a finger on the letterbox bar from one at the edge of the picture, and drop it.
     */
    clamp: Boolean = true,
): StreamPoint {
    if (viewWidth <= 0 || viewHeight <= 0 || streamWidth <= 0 || streamHeight <= 0) {
        return StreamPoint(0f, 0f)
    }
    if (!touchX.isFinite() || !touchY.isFinite()) {
        return StreamPoint(Float.NaN, Float.NaN)
    }

    val safeZoomScale = presentationZoomScale
        .takeIf { it.isFinite() && it >= 1f }
        ?: 1f
    val safeTranslationX = presentationTranslationX.takeIf { it.isFinite() } ?: 0f
    val safeTranslationY = presentationTranslationY.takeIf { it.isFinite() } ?: 0f
    val viewCenterX = viewWidth / 2f
    val viewCenterY = viewHeight / 2f
    val untransformedTouchX =
        viewCenterX + (touchX - viewCenterX - safeTranslationX) / safeZoomScale
    val untransformedTouchY =
        viewCenterY + (touchY - viewCenterY - safeTranslationY) / safeZoomScale

    var videoWidth = viewWidth.toFloat()
    var videoHeight = viewHeight.toFloat()
    var offsetX = 0f
    var offsetY = 0f

    // A stretched surface occupies the complete view. Aspect-ratio bars only exist in fit mode.
    if (!stretchToFit) {
        val streamAspectRatio =
            if (renderingAspectRatio.isFinite() && renderingAspectRatio > 0f) {
                renderingAspectRatio
            } else {
                viewAspectOf(streamWidth, streamHeight)
            }
        val viewAspectRatio = viewAspectOf(viewWidth, viewHeight)
        if (viewAspectRatio > streamAspectRatio) {
            // Pillarboxed — bars left and right.
            videoWidth = viewHeight * streamAspectRatio
            offsetX = (viewWidth - videoWidth) / 2f
        } else if (viewAspectRatio < streamAspectRatio) {
            // Letterboxed — bars top and bottom.
            videoHeight = viewWidth / streamAspectRatio
            offsetY = (viewHeight - videoHeight) / 2f
        }
    }

    if (!videoWidth.isFinite() || !videoHeight.isFinite() || videoWidth <= 0f || videoHeight <= 0f) {
        return StreamPoint(Float.NaN, Float.NaN)
    }

    val x = (untransformedTouchX - offsetX) / videoWidth * streamWidth
    val y = (untransformedTouchY - offsetY) / videoHeight * streamHeight
    if (!x.isFinite() || !y.isFinite()) {
        return StreamPoint(Float.NaN, Float.NaN)
    }
    return if (clamp) {
        StreamPoint(x.coerceIn(0f, streamWidth.toFloat()), y.coerceIn(0f, streamHeight.toFloat()))
    } else {
        StreamPoint(x, y)
    }
}

private fun viewAspectOf(width: Int, height: Int): Float = width.toFloat() / height.toFloat()

/** A whole-pixel relative move, the only kind the wire format carries. */
internal data class CursorDelta(val dx: Int, val dy: Int)

/**
 * Our model of where the host's cursor sits while a direct-click drag is active.
 *
 * The protocol has no absolute-positioning packet — [InputEncoder.INPUT_MOUSE_REL] is all there is.
 * At the start of every tap, [reanchorDeltasTo] first sends the largest supported negative movement
 * so the desktop clamps the cursor to its top-left boundary, then sends the target coordinates from
 * that known origin. That removes the old assumption that the host cursor started in the centre,
 * which left every direct click offset whenever a game had moved it elsewhere.
 */
internal class VirtualCursor {
    private var x = 0f
    private var y = 0f
    private var initialized = false
    private var streamWidth = 0
    private var streamHeight = 0

    /** Exposed for tests; production code only ever needs [consumeDeltaTo]. */
    val position: StreamPoint get() = StreamPoint(x, y)

    fun onStreamSize(width: Int, height: Int) {
        if (width <= 0 || height <= 0) {
            // Do not reset to 0,0 if we already have a valid size. This prevents the cursor
            // from re-centering when the stream momentarily reports a 0x0 size during a
            // resolution change or PiP transition.
            if (streamWidth > 0 && streamHeight > 0) return
            // Never anchor to a degenerate size either: a 0x0 report before the first valid size
            // must leave the model uninitialised so the first real size anchors normally.
            if (!initialized) return
        }
        if (!initialized) {
            // Direct click reanchors before pressing; this temporary value only keeps the generic
            // relative cursor model well-defined until that first DOWN arrives.
            x = width / 2f
            y = height / 2f
            initialized = true
        } else if (streamWidth != width || streamHeight != height) {
            if (streamWidth > 0 && streamHeight > 0) {
                x = x / streamWidth * width
                y = y / streamHeight * height
            }
        }
        streamWidth = width
        streamHeight = height
    }

    /**
     * The move to send in order to land on [target], advancing the model by exactly that much —
     * not by [target]. The two differ by the rounding residue, and assigning [target] would swallow
     * that residue every event, letting the model random-walk away from the real cursor over a long
     * drag. Null when the rounded move is zero and there is nothing worth sending.
     */
    fun consumeDeltaTo(target: StreamPoint): CursorDelta? {
        if (!target.x.isFinite() || !target.y.isFinite() || !x.isFinite() || !y.isFinite()) return null
        val dx = (target.x - x).roundToInt()
        val dy = (target.y - y).roundToInt()
        if (dx == 0 && dy == 0) return null
        // Advance the model by what was actually sent, never by [target]: the difference is the
        // rounding residue, and assigning [target] would swallow it every event, letting the model
        // drift away from the host cursor over a long drag.
        x += dx
        y += dy
        return CursorDelta(dx, dy)
    }

    /**
     * Reliable relative moves that place the host cursor at [target] without knowing its current
     * position. The first move is already the protocol's signed-16-bit minimum, so the encoder will
     * transmit it unchanged and any supported single-display stream is guaranteed to hit (0, 0).
     */
    fun reanchorDeltasTo(target: StreamPoint): List<CursorDelta> {
        if (
            !initialized ||
            streamWidth <= 0 ||
            streamHeight <= 0 ||
            !target.x.isFinite() ||
            !target.y.isFinite()
        ) return emptyList()
        val targetX = target.x.roundToInt().coerceIn(0, streamWidth - 1)
        val targetY = target.y.roundToInt().coerceIn(0, streamHeight - 1)
        x = targetX.toFloat()
        y = targetY.toFloat()
        return buildList {
            repeat(2) {
                add(CursorDelta(Short.MIN_VALUE.toInt(), Short.MIN_VALUE.toInt()))
            }
            add(CursorDelta(targetX, targetY))
        }
    }

    fun forget() {
        initialized = false
        streamWidth = 0
        streamHeight = 0
    }
}

internal class TouchMouseState {
    private var activePointerId = -1
    private var downX = 0f
    private var downY = 0f
    private var downTimeMs = 0L
    private var lastX = 0f
    private var lastY = 0f
    private var selecting = false
    private var doubleTapDragCandidate = false
    private var lastTapTimeMs = Long.MIN_VALUE
    private var lastTapX = Float.NaN
    private var lastTapY = Float.NaN
    private val virtualCursor = VirtualCursor()
    private var twoFingerTapCandidate = false
    private val motionAccumulator = MouseMotionAccumulator()
    // 2-finger scroll state
    private var secondPointerId = -1
    private var secondPointerDownY = 0f
    private var secondPointerLastY = 0f
    private var isScrollGesture = false
    private var scrollAccumulator = 0f
    private var scrollGestureOccurred = false

    /**
     * Tears down the in-flight gesture. The cursor model is retained only for an active drag; every
     * new direct-click DOWN independently reanchors at the host boundary before moving to its target.
     */
    fun reset(client: NativeStreamClient?) {
        // Correct for both modes now that direct click also maintains `selecting`. Widening this
        // to `activePointerId >= 0` instead would fire in touchpad mode during an ordinary drag,
        // where a pointer is tracked but no button is held, sending a spurious release.
        if (selecting) client?.setTouchMouseButton(false)
        activePointerId = -1
        selecting = false
        doubleTapDragCandidate = false
        twoFingerTapCandidate = false
        motionAccumulator.reset()
        secondPointerId = -1
        isScrollGesture = false
        scrollAccumulator = 0f
        scrollGestureOccurred = false
    }

    /** Clears any position retained from the previous stream client. */
    fun forgetCursorPosition() {
        virtualCursor.forget()
    }

    private fun moveVirtualCursorTo(target: StreamPoint, client: NativeStreamClient) {
        val delta = virtualCursor.consumeDeltaTo(target) ?: return
        client.sendRawMouseMove(delta.dx, delta.dy)
    }

    private fun reanchorVirtualCursorTo(target: StreamPoint, client: NativeStreamClient): Boolean {
        val deltas = virtualCursor.reanchorDeltasTo(target)
        if (deltas.isEmpty()) return false
        for (delta in deltas) {
            // The reanchor series is order-sensitive: the host must clamp through the top-left
            // boundary before moving to the target. The unordered loss-tolerant channel could
            // deliver these out of order and clamp the cursor back to 0,0, so keep it reliable.
            if (!client.sendRawMouseMove(delta.dx, delta.dy)) {
                virtualCursor.forget()
                return false
            }
        }
        return true
    }

    fun handle(
        event: MotionEvent,
        enabled: Boolean,
        client: NativeStreamClient,
        ignoredPointerIds: Set<Int>,
        directClick: Boolean = false,
        width: Int = 0,
        height: Int = 0,
        stretchToFit: Boolean = false,
        renderingAspectRatio: Float = 0f,
        presentationZoomScale: Float = 1f,
        presentationTranslationX: Float = 0f,
        presentationTranslationY: Float = 0f,
        decodedStreamWidth: Int = 0,
        decodedStreamHeight: Int = 0,
    ): Boolean {
        if (!enabled) {
            reset(client)
            return false
        }

        if (directClick) {
            val settingsRes = streamResolutionPixels(client.settings)
            val streamWidth = if (decodedStreamWidth > 0) decodedStreamWidth else settingsRes.first
            val streamHeight = if (decodedStreamHeight > 0) decodedStreamHeight else settingsRes.second
            virtualCursor.onStreamSize(streamWidth, streamHeight)

            when (event.actionMasked) {
                MotionEvent.ACTION_DOWN, MotionEvent.ACTION_POINTER_DOWN -> {
                    val index = if (event.actionMasked == MotionEvent.ACTION_DOWN) 0 else event.actionIndex
                    if (index in 0 until event.pointerCount && event.getPointerId(index) !in ignoredPointerIds) {
                        val pointerId = event.getPointerId(index)
                        // Guard: only block if the *same* pointer is already being tracked (true dup).
                        // Allow a new pointer if the previous activePointerId is no longer present in the event.
                        if (activePointerId >= 0 && event.findPointerIndex(activePointerId) >= 0) {
                            // Active pointer still in contact — absorb this extra DOWN.
                            return true
                        }
                        // If we get here the old pointer was lifted without a UP event — reset first.
                        if (activePointerId >= 0) {
                            client.setTouchMouseButton(false)
                            selecting = false
                        }

                        activePointerId = pointerId
                        val target = streamPointForTouch(
                            touchX = event.getX(index),
                            touchY = event.getY(index),
                            viewWidth = width,
                            viewHeight = height,
                            streamWidth = streamWidth,
                            streamHeight = streamHeight,
                            stretchToFit = stretchToFit,
                            renderingAspectRatio = renderingAspectRatio,
                            presentationZoomScale = presentationZoomScale,
                            presentationTranslationX = presentationTranslationX,
                            presentationTranslationY = presentationTranslationY,
                        )
                        // Move cursor smoothly to target without forced reanchoring.
                        moveVirtualCursorTo(target, client)

                        selecting = client.setTouchMouseButton(true)
                        if (!selecting) {
                            activePointerId = -1
                            return true
                        }
                        // `selecting` is this class's single record of "we are holding the button
                        // down on the host". Direct click used to leave it false and track the
                        // press only through activePointerId, so reset() — which releases on
                        // `selecting` — could not release it, and backgrounding mid-tap left the
                        // button stuck down with no event left to clear it.
                    }
                    return true
                }
                MotionEvent.ACTION_MOVE -> {
                    if (activePointerId >= 0) {
                        val index = event.findPointerIndex(activePointerId)
                        if (index >= 0) {
                            moveVirtualCursorTo(
                                streamPointForTouch(
                                    touchX = event.getX(index),
                                    touchY = event.getY(index),
                                    viewWidth = width,
                                    viewHeight = height,
                                    streamWidth = streamWidth,
                                    streamHeight = streamHeight,
                                    stretchToFit = stretchToFit,
                                    renderingAspectRatio = renderingAspectRatio,
                                    presentationZoomScale = presentationZoomScale,
                                    presentationTranslationX = presentationTranslationX,
                                    presentationTranslationY = presentationTranslationY,
                                ),
                                client,
                            )
                        }
                    }
                    return true
                }
                MotionEvent.ACTION_UP -> {
                    // Final pointer lifted — always release the button.
                    client.setTouchMouseButton(false)
                    selecting = false
                    activePointerId = -1
                    return true
                }
                MotionEvent.ACTION_POINTER_UP -> {
                    val releasedId = event.getPointerId(event.actionIndex)
                    if (releasedId == activePointerId) {
                        client.setTouchMouseButton(false)
                        selecting = false
                        activePointerId = -1
                    }
                    return true
                }
                MotionEvent.ACTION_CANCEL -> {
                    client.setTouchMouseButton(false)
                    selecting = false
                    activePointerId = -1
                    return true
                }
            }
            return true
        }

        when (event.actionMasked) {
            MotionEvent.ACTION_DOWN -> {
                if (event.getPointerId(0) in ignoredPointerIds) {
                    reset(client)
                    return false
                }
                beginPointer(event, 0)
                twoFingerTapCandidate = false
                return true
            }
            MotionEvent.ACTION_POINTER_DOWN -> {
                if (activePointerId < 0) {
                    val index = event.actionIndex
                    if (index in 0 until event.pointerCount && event.getPointerId(index) !in ignoredPointerIds) {
                        beginPointer(event, index)
                    }
                } else {
                    val newIndex = event.actionIndex
                    val newPointerId = if (newIndex in 0 until event.pointerCount) event.getPointerId(newIndex) else -1
                    if (newPointerId >= 0 && newPointerId !in ignoredPointerIds) {
                        var nonIgnoredCount = 0
                        for (i in 0 until event.pointerCount) {
                            if (event.getPointerId(i) !in ignoredPointerIds) {
                                nonIgnoredCount++
                            }
                        }
                        if (nonIgnoredCount == 2) {
                            twoFingerTapCandidate = true
                            secondPointerId = newPointerId
                            val secIdx = event.findPointerIndex(newPointerId)
                            if (secIdx >= 0) {
                                secondPointerLastY = event.getY(secIdx)
                                secondPointerDownY = secondPointerLastY
                            }
                        }
                    }
                }
                return true
            }
            MotionEvent.ACTION_MOVE -> {
                if (activePointerId < 0) {
                    val index = event.firstPointerIndexNotIn(ignoredPointerIds)
                    if (index >= 0) beginPointer(event, index)
                    return index >= 0
                }
                // Handle 2-finger scroll
                if (secondPointerId >= 0) {
                    val secIdx = event.findPointerIndex(secondPointerId)
                    if (secIdx >= 0) {
                        val secY = event.getY(secIdx)
                        val secDy = secY - secondPointerLastY
                        secondPointerLastY = secY
                        if (!isScrollGesture && abs(secY - secondPointerDownY) > SCROLL_START_SLOP_PX) {
                            isScrollGesture = true
                            scrollGestureOccurred = true
                            twoFingerTapCandidate = false
                            scrollAccumulator = 0f
                            NativeInputDiagnostics.add("touch scroll start")
                        }
                        if (isScrollGesture) {
                            val scrollPxPerNotch = client.settings.mouseScrollSensitivity.toFloat().coerceIn(10f, 100f)
                            scrollAccumulator -= secDy
                            val notches = (scrollAccumulator / scrollPxPerNotch).toInt()
                            if (notches != 0) {
                                client.sendTouchMouseWheel(notches * 120)
                                scrollAccumulator -= notches * scrollPxPerNotch
                            }
                            return true
                        }
                    }
                }
                val index = event.findPointerIndex(activePointerId)
                if (index < 0) return true
                val x = event.getX(index)
                val y = event.getY(index)
                val dx = x - lastX
                val dy = y - lastY
                if (
                    doubleTapDragCandidate &&
                    !selecting &&
                    (abs(x - downX) > TOUCH_MOUSE_DRAG_START_SLOP_PX || abs(y - downY) > TOUCH_MOUSE_DRAG_START_SLOP_PX)
                ) {
                    selecting = client.setTouchMouseButton(true)
                    doubleTapDragCandidate = false
                    if (selecting) {
                        NativeInputDiagnostics.add("touch double tap drag start")
                    }
                }
                sendMouseDelta(dx, dy, event.eventTime, client)
                lastX = x
                lastY = y
                return true
            }
            MotionEvent.ACTION_POINTER_UP -> {
                val index = event.actionIndex
                if (index in 0 until event.pointerCount) {
                    val upId = event.getPointerId(index)
                    if (upId == secondPointerId) {
                        secondPointerId = -1
                        isScrollGesture = false
                        scrollAccumulator = 0f
                    }
                    if (upId == activePointerId) {
                        finishPointer(event, index, client)
                    }
                }
                return true
            }
            MotionEvent.ACTION_UP -> {
                val index = event.findPointerIndex(activePointerId).takeIf { it >= 0 } ?: event.firstPointerIndexNotIn(ignoredPointerIds)
                if (index < 0) return false
                finishPointer(event, index, client)
                return true
            }
            MotionEvent.ACTION_CANCEL -> {
                reset(client)
                return true
            }
        }
        return true
    }

    private fun beginPointer(event: MotionEvent, index: Int) {
        activePointerId = event.getPointerId(index)
        downX = event.getX(index)
        downY = event.getY(index)
        downTimeMs = event.eventTime
        lastX = downX
        lastY = downY
        motionAccumulator.reset()
        selecting = false
        doubleTapDragCandidate = isDoubleTap(event, index)
        if (doubleTapDragCandidate) {
            lastTapTimeMs = Long.MIN_VALUE
        }
    }

    private fun finishPointer(event: MotionEvent, index: Int, client: NativeStreamClient) {
        val x = event.getX(index)
        val y = event.getY(index)
        sendMouseDelta(
            dx = x - lastX,
            dy = y - lastY,
            eventTimeMs = event.eventTime,
            client = client,
            force = true,
        )
        lastX = x
        lastY = y
        val tapDistanceX = abs(x - downX)
        val tapDistanceY = abs(y - downY)
        val wasTap = activePointerId >= 0 &&
            !scrollGestureOccurred &&
            event.eventTime - downTimeMs <= TOUCH_MOUSE_TAP_TIMEOUT_MS &&
            tapDistanceX <= TOUCH_MOUSE_TAP_SLOP_PX &&
            tapDistanceY <= TOUCH_MOUSE_TAP_SLOP_PX
        activePointerId = -1
        doubleTapDragCandidate = false
        scrollGestureOccurred = false
        if (selecting) {
            client.setTouchMouseButton(false)
            selecting = false
            return
        }
        if (wasTap) {
            if (twoFingerTapCandidate) {
                NativeInputDiagnostics.add("touch 2-finger tap right click dx=${tapDistanceX.roundToInt()} dy=${tapDistanceY.roundToInt()}")
                client.sendTouchMouseRightClick()
            } else {
                NativeInputDiagnostics.add("touch tap click dx=${tapDistanceX.roundToInt()} dy=${tapDistanceY.roundToInt()}")
                client.sendTouchMouseClick()
            }
            lastTapTimeMs = event.eventTime
            lastTapX = x
            lastTapY = y
        }
        twoFingerTapCandidate = false
    }

    private fun MotionEvent.firstPointerIndexNotIn(ignoredPointerIds: Set<Int>): Int {
        for (index in 0 until pointerCount) {
            if (getPointerId(index) !in ignoredPointerIds) return index
        }
        return -1
    }

    private fun isDoubleTap(event: MotionEvent, index: Int): Boolean {
        if (lastTapTimeMs == Long.MIN_VALUE) return false
        if (event.eventTime - lastTapTimeMs > TOUCH_MOUSE_DOUBLE_TAP_TIMEOUT_MS) return false
        if (!lastTapX.isFinite() || !lastTapY.isFinite()) return false
        return abs(event.getX(index) - lastTapX) <= TOUCH_MOUSE_DOUBLE_TAP_SLOP_PX &&
            abs(event.getY(index) - lastTapY) <= TOUCH_MOUSE_DOUBLE_TAP_SLOP_PX
    }

    private fun sendMouseDelta(
        dx: Float,
        dy: Float,
        eventTimeMs: Long,
        client: NativeStreamClient,
        force: Boolean = false,
    ) {
        val delta = motionAccumulator.add(
            dx = dx,
            dy = dy,
            eventTimeMs = eventTimeMs,
            sensitivity = client.settings.mouseSensitivity,
            acceleration = client.settings.mouseAcceleration,
            force = force,
        ) ?: return
        client.sendRawMouseMove(delta.dx, delta.dy)
    }

    companion object {
        private const val TOUCH_MOUSE_DRAG_START_SLOP_PX = 10f
        private const val TOUCH_MOUSE_TAP_SLOP_PX = 42f
        private const val TOUCH_MOUSE_TAP_TIMEOUT_MS = 450L
        private const val TOUCH_MOUSE_DOUBLE_TAP_TIMEOUT_MS = 320L
        private const val TOUCH_MOUSE_DOUBLE_TAP_SLOP_PX = 36f
        private const val SCROLL_START_SLOP_PX = 12f
    }
}

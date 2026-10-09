package com.opencloudgaming.opennow

import android.media.MediaCodec
import android.media.MediaFormat
import android.os.SystemClock
import android.os.Handler
import android.os.HandlerThread
import android.view.Surface
import org.webrtc.EncodedImage
import org.webrtc.VideoCodecStatus
import org.webrtc.VideoDecoder
import org.webrtc.VideoFrame
import java.nio.ByteBuffer
import java.util.concurrent.atomic.AtomicBoolean
import java.util.concurrent.atomic.AtomicInteger

/**
 * A receive-only opaque frame. WebRTC schedules its presentation and records decoder statistics;
 * pixels remain in the MediaCodec output buffer until the sink presents it. Dropped frames release
 * the buffer without displaying it. There is deliberately no lossy 8-bit I420 readback of HDR.
 */
internal class MediaCodecSurfaceBuffer(
    private val width: Int,
    private val height: Int,
    private val finish: (Boolean) -> Boolean,
) : VideoFrame.Buffer {
    private val references = AtomicInteger(1)
    private val finished = AtomicBoolean(false)
    override fun getWidth() = width
    override fun getHeight() = height
    override fun toI420(): VideoFrame.I420Buffer? = null
    override fun retain() { check(references.incrementAndGet() > 1) }
    override fun release() {
        if (references.decrementAndGet() == 0 && finished.compareAndSet(false, true)) finish(false)
    }
    fun present(): Boolean = finished.compareAndSet(false, true) && finish(true)
    override fun cropAndScale(cropX: Int, cropY: Int, cropWidth: Int, cropHeight: Int,
        scaleWidth: Int, scaleHeight: Int): VideoFrame.Buffer {
        // Receive sinks must render the complete decoded frame; view layout owns aspect fitting.
        require(cropX == 0 && cropY == 0 && cropWidth == width && cropHeight == height &&
            scaleWidth == width && scaleHeight == height)
        retain()
        return this
    }
}

internal class DecoderSurfaceTarget(val surface: Surface)

internal data class SurfaceDecoderConfiguration(
    val name: String,
    val format: MediaFormat,
    val onStarted: (MediaCodec) -> Unit = {},
)

/** Shared opaque surface decoder for SDR and HDR. Input/output callbacks replace polling. */
internal class MediaCodecSurfaceVideoDecoder(
    private val fps: Int,
    private val surface: () -> DecoderSurfaceTarget?,
    private val hdr: Boolean,
    private val configuration: (Int, Int, Int) -> SurfaceDecoderConfiguration?,
) : VideoDecoder {
    private data class FrameInfo(val timestampNs: Long, val rotation: Int, val queuedAtMs: Long, val size: Int)
    private data class PendingFrame(val buffer: ByteBuffer, val info: FrameInfo)
    private val encodedBuffers = SurfaceEncodedBufferPool()
    private val pendingFrames = java.util.ArrayDeque<PendingFrame>()
    private val inputSlots = java.util.ArrayDeque<Int>()
    private val lock = Any()
    private val queuePolicy = SurfaceDecoderQueuePolicy(fps)
    private val presentationClock = SurfacePresentationClock(fps)
    private var queuedBytes = 0L
    private var producedOutput = false
    private var inputCount = 0
    private var outputCount = 0
    private var outputCallbackCount = 0
    private var presentedCount = 0
    private var codec: MediaCodec? = null
    private var outputSurface: DecoderSurfaceTarget? = null
    private var callback: VideoDecoder.Callback? = null
    private var width = 0
    private var height = 0
    private var outputWidth = 0
    private var outputHeight = 0
    private var rotation = 0
    private var generation = 0
    private var nextPtsUs = 0L
    private val frames = mutableMapOf<Long, FrameInfo>()
    private var needsKeyFrame = true
    private var running = false
    private var callbackThread: HandlerThread? = null
    @Volatile var failureReason: String? = null
        private set

    override fun initDecode(settings: VideoDecoder.Settings?, decodeCallback: VideoDecoder.Callback?): VideoCodecStatus {
        if (settings == null || decodeCallback == null) return VideoCodecStatus.ERR_PARAMETER
        synchronized(lock) {
            if (running) return VideoCodecStatus.ERROR
            width = settings.width
            height = settings.height
            callback = decodeCallback
            failureReason = null
            needsKeyFrame = true
            callbackThread = HandlerThread("OpenNOW-surface-codec").also { it.start() }
            running = true
        }
        return VideoCodecStatus.OK
    }

    override fun decode(frame: EncodedImage?, info: VideoDecoder.DecodeInfo?): VideoCodecStatus = synchronized(lock) {
        if (!running || frame == null) return@synchronized VideoCodecStatus.UNINITIALIZED
        val target = surface()?.takeIf { it.surface.isValid }
        if (target == null) {
            stopCodecLocked()
            return@synchronized VideoCodecStatus.NO_OUTPUT
        }
        val nextWidth = frame.encodedWidth.takeIf { it > 0 } ?: width
        val nextHeight = frame.encodedHeight.takeIf { it > 0 } ?: height
        if (failureReason != null || target !== outputSurface || nextWidth != width || nextHeight != height || frame.rotation != rotation) {
            stopCodecLocked()
        }
        if (needsKeyFrame && frame.frameType != EncodedImage.FrameType.VideoFrameKey) return@synchronized VideoCodecStatus.ERROR
        try {
            if (codec == null) {
                val selected = configuration(nextWidth, nextHeight, fps)
                    ?: return@synchronized failLocked("unsupported size/rate/profile")
                width = nextWidth
                height = nextHeight
                rotation = frame.rotation
                outputWidth = width
                outputHeight = height
                val decoder = MediaCodec.createByCodecName(selected.name)
                // Publish before configuring so failures and late callbacks can be released safely.
                codec = decoder
                val codecGeneration = generation
                decoder.setCallback(codecCallbacks(codecGeneration), Handler(callbackThread!!.looper))
                selected.format.setInteger(MediaFormat.KEY_ROTATION, rotation)
                decoder.configure(selected.format, target.surface, null, 0)
                decoder.setVideoScalingMode(MediaCodec.VIDEO_SCALING_MODE_SCALE_TO_FIT)
                outputSurface = target
                decoder.start()
                selected.onStarted(decoder)
                failureReason = null
                NativeInputDiagnostics.addRetained("surface-decoder",
                    "MediaCodec direct surface decoder=${selected.name} size=${width}x$height fps=$fps hdr=$hdr async=true")
            }
            val nowMs = SystemClock.elapsedRealtime()
            val oldestMs = minOf(frames.values.firstOrNull()?.queuedAtMs ?: nowMs,
                pendingFrames.peekFirst()?.info?.queuedAtMs ?: nowMs)
            queuePolicy.failure(frames.size + pendingFrames.size, queuedBytes + frame.buffer.remaining(),
                nowMs - oldestMs, priming = !producedOutput)?.let { return@synchronized failLocked(it) }
            val frameInfo = FrameInfo(frame.captureTimeNs, frame.rotation, nowMs, frame.buffer.remaining())
            queuedBytes += frameInfo.size
            needsKeyFrame = false
            if (pendingFrames.isEmpty() && inputSlots.isNotEmpty()) {
                // Borrowed native data is safe only while this decode call is still executing.
                queueInputLocked(codec!!, inputSlots.removeFirst(), frame.buffer, frameInfo)
            } else {
                pendingFrames.addLast(PendingFrame(encodedBuffers.copy(frame.buffer), frameInfo))
                feedInputLocked(codec!!)
            }
            VideoCodecStatus.OK
        } catch (error: Exception) {
            failLocked("input/configure ${error.javaClass.simpleName}")
        }
    }

    private fun codecCallbacks(codecGeneration: Int) = object : MediaCodec.Callback() {
        override fun onInputBufferAvailable(decoder: MediaCodec, index: Int) {
            synchronized(lock) {
                if (!isCurrentCodec(decoder, codecGeneration) || failureReason != null) return
                inputSlots.addLast(index)
                try { feedInputLocked(decoder) }
                catch (error: Exception) { failLocked("input callback ${error.javaClass.simpleName}") }
            }
        }

        override fun onOutputBufferAvailable(decoder: MediaCodec, index: Int, info: MediaCodec.BufferInfo) {
            val delivery = synchronized(lock) {
                if (!isCurrentCodec(decoder, codecGeneration)) return
                try {
                    outputCallbackCount++
                    val frame = frames.remove(info.presentationTimeUs)
                    if (frame != null) queuedBytes -= frame.size
                    if (frame == null || failureReason != null || info.flags and MediaCodec.BUFFER_FLAG_CODEC_CONFIG != 0) {
                        if (frame == null) NativeInputDiagnostics.addRetained("surface-unmatched-output",
                            "surface output unmatched ptsUs=${info.presentationTimeUs} flags=${info.flags} " +
                                "oldestPtsUs=${frames.keys.firstOrNull()} newestPtsUs=$nextPtsUs callbacks=$outputCallbackCount")
                        decoder.releaseOutputBuffer(index, false)
                        return
                    }
                    producedOutput = true
                    outputCount++
                    val buffer = MediaCodecSurfaceBuffer(outputWidth, outputHeight) { render ->
                        synchronized(lock) {
                            if (!isCurrentCodec(decoder, codecGeneration)) false
                            else try {
                                val present = render && failureReason == null && surface() === outputSurface && outputSurface?.surface?.isValid == true
                                // WebRTC's callbacks can straddle VSYNC despite steady media FPS.
                                // Preserve the media cadence in Android's monotonic clock domain.
                                if (present) decoder.releaseOutputBuffer(index,
                                    presentationClock.next(frame.timestampNs, System.nanoTime()))
                                else decoder.releaseOutputBuffer(index, false)
                                if (present) {
                                    presentedCount++
                                    retainProgressLocked()
                                }
                                present
                            } catch (error: Exception) {
                                failLocked("output release ${error.javaClass.simpleName}")
                                false
                            }
                        }
                    }
                    Triple(VideoFrame(buffer, frame.rotation, frame.timestampNs),
                        (SystemClock.elapsedRealtime() - frame.queuedAtMs).toInt(), callback)
                } catch (error: Exception) {
                    failLocked("output callback ${error.javaClass.simpleName}")
                    return
                }
            }
            // Do not call WebRTC under the codec lock: rendering/dropped-frame release returns slots.
            try { delivery.third?.onDecodedFrame(delivery.first, delivery.second, null) }
            catch (error: Exception) {
                synchronized(lock) {
                    if (isCurrentCodec(decoder, codecGeneration)) failLocked("frame delivery ${error.javaClass.simpleName}")
                }
            }
            finally { delivery.first.release() }
        }

        override fun onOutputFormatChanged(decoder: MediaCodec, format: MediaFormat) {
            synchronized(lock) {
                if (!isCurrentCodec(decoder, codecGeneration)) return
                try {
                    outputWidth = croppedDimension(format, "crop-left", "crop-right", MediaFormat.KEY_WIDTH, width)
                    outputHeight = croppedDimension(format, "crop-top", "crop-bottom", MediaFormat.KEY_HEIGHT, height)
                    require(outputWidth > 0 && outputHeight > 0)
                    if (hdr && !hdrOutputColorSupported(format.integerOrNull(MediaFormat.KEY_COLOR_STANDARD),
                            format.integerOrNull(MediaFormat.KEY_COLOR_TRANSFER))) {
                        failLocked("HDR output color conversion")
                        return
                    }
                    // Format changes can reset scaling mode. Surface layout owns aspect fitting.
                    decoder.setVideoScalingMode(MediaCodec.VIDEO_SCALING_MODE_SCALE_TO_FIT)
                    NativeInputDiagnostics.add("surface decoder output format=$format hdr=$hdr")
                } catch (error: Exception) { failLocked("format callback ${error.javaClass.simpleName}") }
            }
        }

        override fun onError(decoder: MediaCodec, error: MediaCodec.CodecException) {
            synchronized(lock) {
                if (isCurrentCodec(decoder, codecGeneration)) failLocked("codec callback ${error.errorCode}")
            }
        }
    }

    private fun isCurrentCodec(decoder: MediaCodec, codecGeneration: Int): Boolean =
        running && codec === decoder && generation == codecGeneration

    private fun failLocked(reason: String): VideoCodecStatus {
        failureReason = reason
        NativeInputDiagnostics.addRetained("surface-decoder-error",
            "MediaCodec direct surface failure=$reason hdr=$hdr input=$inputCount output=$outputCount " +
                "presented=$presentedCount callbacks=$outputCallbackCount pending=${pendingFrames.size} inCodec=${frames.size}")
        return VideoCodecStatus.ERROR
    }

    private fun feedInputLocked(decoder: MediaCodec) {
        while (pendingFrames.isNotEmpty() && inputSlots.isNotEmpty()) {
            val index = inputSlots.removeFirst()
            val pending = pendingFrames.removeFirst()
            try {
                queueInputLocked(decoder, index, pending.buffer, pending.info)
            } finally { encodedBuffers.recycle(pending.buffer) }
        }
    }

    private fun queueInputLocked(decoder: MediaCodec, index: Int, source: ByteBuffer, info: FrameInfo) {
        val input = decoder.getInputBuffer(index) ?: error("Missing surface codec input buffer")
        input.clear()
        require(info.size <= input.remaining()) { "Encoded frame exceeds codec buffer" }
        input.put(source.duplicate())
        nextPtsUs = maxOf(nextPtsUs + 1, info.timestampNs / 1000)
        frames[nextPtsUs] = info
        decoder.queueInputBuffer(index, 0, info.size, nextPtsUs, 0)
        inputCount++
    }

    private fun MediaFormat.integerOrNull(key: String): Int? = if (containsKey(key)) getInteger(key) else null
    private fun croppedDimension(format: MediaFormat, start: String, end: String, size: String, fallback: Int): Int =
        if (format.containsKey(start) && format.containsKey(end)) format.getInteger(end) - format.getInteger(start) + 1
        else if (format.containsKey(size)) format.getInteger(size) else fallback

    private fun stopCodecLocked() {
        generation++
        val previous = codec
        codec = null
        outputSurface = null
        frames.clear()
        inputSlots.clear()
        pendingFrames.clear()
        encodedBuffers.clear()
        queuedBytes = 0
        producedOutput = false
        presentationClock.reset()
        inputCount = 0
        outputCount = 0
        outputCallbackCount = 0
        presentedCount = 0
        needsKeyFrame = true
        if (previous != null) {
            runCatching { previous.stop() }
            runCatching { previous.release() }
        }
    }

    override fun release(): VideoCodecStatus {
        val worker = synchronized(lock) {
            running = false
            stopCodecLocked()
            callback = null
            callbackThread.also { callbackThread = null }
        }
        worker?.quitSafely()
        if (worker != null && Thread.currentThread() !== worker) worker.join(1000)
        return if (worker?.isAlive == true && Thread.currentThread() !== worker) VideoCodecStatus.TIMEOUT else VideoCodecStatus.OK
    }

    override fun getImplementationName() = "OpenNOW-MediaCodec-${if (hdr) "HDR10" else "SDR"}-Surface"

    private fun retainProgressLocked() {
        if (presentedCount == 1 || presentedCount % fps.coerceAtLeast(1) == 0) {
            NativeInputDiagnostics.addRetained("surface-decoder-progress",
                "MediaCodec direct surface progress input=$inputCount output=$outputCount presented=$presentedCount " +
                    "pending=${pendingFrames.size} inCodec=${frames.size} queuedBytes=$queuedBytes hdr=$hdr")
        }
    }
}

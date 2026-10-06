package com.opencloudgaming.opennow

import android.media.MediaCodec
import android.media.MediaFormat
import android.os.Build
import android.os.Handler
import android.os.HandlerThread
import android.os.Process
import android.os.SystemClock
import android.view.Surface
import org.webrtc.EncodedImage
import org.webrtc.VideoCodecStatus
import org.webrtc.VideoDecoder
import org.webrtc.VideoFrame
import java.nio.ByteBuffer
import java.util.Locale
import java.util.concurrent.atomic.AtomicBoolean
import java.util.concurrent.atomic.AtomicInteger

/**
 * A receive-only opaque frame. WebRTC schedules its presentation and records decoder statistics;
 * pixels remain in the MediaCodec output buffer until the sink presents it. Dropped frames release
 * the buffer without displaying it. There is deliberately no lossy 8-bit I420 readback of HDR.
 */
internal class HdrSurfaceBuffer(
    private val width: Int,
    private val height: Int,
    private val timedFinish: ((Long) -> Boolean)? = null,
    internal val flow: HdrFrameFlow.Frame? = null,
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
    fun presentAt(timestampNs: Long): Boolean = finished.compareAndSet(false, true) &&
        (timedFinish?.invoke(timestampNs) ?: finish(true))
    override fun cropAndScale(cropX: Int, cropY: Int, cropWidth: Int, cropHeight: Int,
        scaleWidth: Int, scaleHeight: Int): VideoFrame.Buffer {
        // Receive sinks must render the complete HDR frame; view layout owns aspect fitting.
        require(cropX == 0 && cropY == 0 && cropWidth == width && cropHeight == height &&
            scaleWidth == width && scaleHeight == height)
        retain()
        return this
    }
}

internal class HdrSurfaceTarget(val surface: Surface)

internal class HdrSurfaceVideoDecoder(
    private val videoCodec: VideoCodec,
    private val fps: Int,
    private val surface: () -> HdrSurfaceTarget?,
    private val lowLatencyEnabled: Boolean = false,
) : VideoDecoder {
    private data class FrameInfo(val timestampNs: Long, val rotation: Int, val queuedAtMs: Long,
        val queuedAtNs: Long, val submittedAtNs: Long, val copyNs: Long, val flow: HdrFrameFlow.Frame?)
    private data class PendingFrame(val data: ByteArray, val timestampNs: Long, val rotation: Int,
        val queuedAtMs: Long, val queuedAtNs: Long, val copyNs: Long, val flow: HdrFrameFlow.Frame?)
    private val pendingFrames = java.util.ArrayDeque<PendingFrame>()
    private val lock = Any()
    private var codec: MediaCodec? = null
    private var outputSurface: HdrSurfaceTarget? = null
    private var callback: VideoDecoder.Callback? = null
    private var width = 0
    private var height = 0
    private var outputWidth = 0
    private var outputHeight = 0
    private var rotation = 0
    private var generation = 0
    private var nextPtsUs = 0L
    private val frames = HdrFrameMetadata<FrameInfo>()
    private val flow = if (BuildConfig.DEBUG) HdrFrameFlow(fps, NativeInputDiagnostics::addRetained) else null
    private var needsKeyFrame = true
    private var failed = false
    @Volatile private var running = false
    private var outputThread: Thread? = null
    private var callbackThread: HandlerThread? = null
    private var codecHandler: Handler? = null
    private val availableInputs = java.util.ArrayDeque<Int>()
    private var pumpPosted = false
    private val asyncMode = Build.VERSION.SDK_INT >= 30
    private val pumpInput = Runnable {
        synchronized(lock) {
            pumpPosted = false
            val decoder = codec
            if (running && !failed && decoder != null) {
                try { feedInputLocked(decoder) }
                catch (error: Exception) { failLocked("input", error) }
            }
        }
    }
    private var timingStartedNs = 0L
    private var timingCount = 0L
    private var timingQueueNs = 0L
    private var timingCodecNs = 0L
    private var timingCopyNs = 0L
    private var timingQueueMaxNs = 0L
    private var timingCodecMaxNs = 0L
    private var timingPendingMax = 0

    override fun initDecode(settings: VideoDecoder.Settings?, decodeCallback: VideoDecoder.Callback?): VideoCodecStatus {
        if (settings == null || decodeCallback == null) return VideoCodecStatus.ERR_PARAMETER
        synchronized(lock) {
            width = settings.width
            height = settings.height
            callback = decodeCallback
            failed = false
            needsKeyFrame = true
        }
        running = true
        if (asyncMode) {
            callbackThread = HandlerThread("OpenNOW-HDR-codec", Process.THREAD_PRIORITY_DISPLAY).also { it.start() }
            codecHandler = Handler(requireNotNull(callbackThread).looper)
        } else {
            outputThread = Thread({
                Process.setThreadPriority(Process.THREAD_PRIORITY_DISPLAY)
                while (running) {
                    val delivered = drainOutput()
                    if (!delivered) SystemClock.sleep(2)
                }
            }, "OpenNOW-HDR-output").also { it.start() }
        }
        return VideoCodecStatus.OK
    }

    override fun decode(frame: EncodedImage?, info: VideoDecoder.DecodeInfo?): VideoCodecStatus {
        val arrivalNs = System.nanoTime()
        return synchronized(lock) {
            if (!running || frame == null) return@synchronized VideoCodecStatus.UNINITIALIZED
            val target = surface()?.takeIf { it.surface.isValid }
            if (target == null) {
                stopCodecLocked()
                return@synchronized VideoCodecStatus.NO_OUTPUT
            }
            val nextWidth = frame.encodedWidth.takeIf { it > 0 } ?: width
            val nextHeight = frame.encodedHeight.takeIf { it > 0 } ?: height
            if (failed || target !== outputSurface || nextWidth != width || nextHeight != height || frame.rotation != rotation) {
                if (codec != null) NativeInputDiagnostics.addRetained("hdr.reset",
                    "HDR reset failed=$failed surfaceChanged=${target !== outputSurface} " +
                        "size=${width}x$height->${nextWidth}x$nextHeight rotation=$rotation->${frame.rotation} " +
                        "queued=${frames.size}+${pendingFrames.size}")
                stopCodecLocked()
            }
            if (needsKeyFrame && frame.frameType != EncodedImage.FrameType.VideoFrameKey) {
                return@synchronized VideoCodecStatus.ERROR
            }
            try {
                if (codec == null) {
                    val name = StreamHdr.decoderName(nextWidth, nextHeight, fps, videoCodec)
                        ?: return@synchronized VideoCodecStatus.ERROR
                    width = nextWidth
                    height = nextHeight
                    rotation = frame.rotation
                    outputWidth = width
                    outputHeight = height
                    configureCodecLocked(name, target)
                }
                // Some AV1 inputs update reference pictures without producing an output frame.
                // Their timestamp metadata is not decoder backpressure. Bound it separately;
                // only unsent encoded frames can trigger queue-overflow recovery.
                if (pendingFrames.size >= 32) {
                    NativeInputDiagnostics.addRetained("hdr.backlog", "HDR decoder backlog frames=${frames.size} pending=${pendingFrames.size}")
                    failed = true
                    return@synchronized VideoCodecStatus.ERROR
                }
                // NativeToJavaEncodedImage supplies a borrowed direct buffer with no release
                // callback. Retaining its Java wrapper does not retain the C++ payload. Own a
                // compressed-data copy before returning to WebRTC and its RTP buffer pool.
                val copyStartedNs = SystemClock.elapsedRealtimeNanos()
                val payload = copyHdrEncodedPayload(frame.buffer)
                val queuedAtNs = SystemClock.elapsedRealtimeNanos()
                val trace = flow?.input(arrivalNs, frame.captureTimeNs)
                pendingFrames.addLast(PendingFrame(payload, frame.captureTimeNs,
                    frame.rotation, SystemClock.elapsedRealtime(), queuedAtNs, queuedAtNs - copyStartedNs, trace))
                timingPendingMax = maxOf(timingPendingMax, pendingFrames.size)
                needsKeyFrame = false
                if (asyncMode && !pumpPosted) {
                    pumpPosted = true
                    check(requireNotNull(codecHandler).post(pumpInput)) { "HDR callback thread stopped" }
                }
                VideoCodecStatus.OK
            } catch (error: Exception) {
                failed = true
                NativeInputDiagnostics.add("HDR decoder input failed: ${error.javaClass.simpleName}")
                VideoCodecStatus.ERROR
            }
        }
    }

    private fun configureCodecLocked(name: String, target: HdrSurfaceTarget) {
        val profiles = decoderLatencyProfiles(lowLatencyEnabled)
        codec = tryDecoderLatencyProfiles(profiles, { profile, error ->
            NativeInputDiagnostics.addRetained("hdr.configureFallback",
                "HDR rejected profile=$profile error=${error.javaClass.simpleName}")
        }) { profile ->
            val decoder = MediaCodec.createByCodecName(name)
            val epoch = ++generation
            try {
                if (Build.VERSION.SDK_INT >= 31) {
                    val supported = runCatching { decoder.supportedVendorParameters }.getOrDefault(emptyList())
                    NativeInputDiagnostics.add("HDR vendor parameters decoder=$name supported=" +
                        supported.filter { it.contains("latency") || it.contains("fence") || it.contains("instant") || it.contains("picture-order") })
                }
                if (asyncMode) decoder.setCallback(codecCallback(epoch), requireNotNull(codecHandler))
                val format = StreamHdr.format(width, height, fps, videoCodec).apply {
                    setInteger(MediaFormat.KEY_ROTATION, rotation)
                    applyDecoderLatencyProfile(this, name, fps, profile)
                }
                decoder.configure(format, target.surface, null, 0)
                decoder.start()
                NativeInputDiagnostics.addRetained("hdr.configure",
                    "HDR direct surface codec=$videoCodec decoder=$name size=${width}x$height " +
                        "transfer=PQ color=BT2020 async=$asyncMode lowLatency=$lowLatencyEnabled " +
                        "profile=$profile operatingRate=${format.getInteger(MediaFormat.KEY_OPERATING_RATE)}")
                decoder
            } catch (error: Exception) {
                runCatching { decoder.release() }
                throw error
            }
        }
        outputSurface = target
        failed = false
    }

    // A callback from a released/reset codec must never touch the new codec's buffer indices.
    private fun isCurrentLocked(decoder: MediaCodec, epoch: Int): Boolean =
        running && codec === decoder && generation == epoch && !failed

    private fun codecCallback(epoch: Int) = object : MediaCodec.Callback() {
        override fun onInputBufferAvailable(decoder: MediaCodec, index: Int) {
            synchronized(lock) {
                if (!isCurrentLocked(decoder, epoch)) return
                availableInputs.addLast(index)
                try { feedInputLocked(decoder) }
                catch (error: Exception) { failLocked("input", error) }
            }
        }
        override fun onOutputBufferAvailable(decoder: MediaCodec, index: Int, info: MediaCodec.BufferInfo) {
            deliverOutput(decoder, epoch, index, info)
        }
        override fun onOutputFormatChanged(decoder: MediaCodec, format: MediaFormat) {
            synchronized(lock) {
                if (isCurrentLocked(decoder, epoch)) {
                    try { acceptOutputFormatLocked(format) }
                    catch (error: Exception) { failLocked("format", error) }
                }
            }
        }
        override fun onError(decoder: MediaCodec, error: MediaCodec.CodecException) {
            synchronized(lock) {
                if (isCurrentLocked(decoder, epoch)) failLocked("callback", error)
            }
        }
    }

    private fun failLocked(stage: String, error: Exception) {
        failed = true
        NativeInputDiagnostics.addRetained("hdr.error", "HDR decoder $stage failed: ${error.javaClass.simpleName}")
    }

    private fun acceptOutputFormatLocked(format: MediaFormat) {
        outputWidth = croppedDimension(format, "crop-left", "crop-right", MediaFormat.KEY_WIDTH, width)
        outputHeight = croppedDimension(format, "crop-top", "crop-bottom", MediaFormat.KEY_HEIGHT, height)
        // Explicit SDR conversion cannot be displayed as PQ.
        val standard = format.integerOrNull(MediaFormat.KEY_COLOR_STANDARD)
        val transfer = format.integerOrNull(MediaFormat.KEY_COLOR_TRANSFER)
        if (!hdrOutputColorSupported(standard, transfer)) {
            failed = true
            NativeInputDiagnostics.add("HDR decoder rejected output standard=$standard transfer=$transfer")
        } else NativeInputDiagnostics.add("HDR decoder output format=$format")
    }

    /** Compatibility polling is used only below Android 11; never dequeue an asynchronous codec. */
    private fun drainOutput(): Boolean {
        val output = MediaCodec.BufferInfo()
        val decoder: MediaCodec
        val epoch: Int
        val index: Int
        synchronized(lock) {
            decoder = codec ?: return false
            epoch = generation
            if (failed) return false
            try {
                feedInputLocked(decoder)
                index = decoder.dequeueOutputBuffer(output, 0)
                if (index == MediaCodec.INFO_OUTPUT_FORMAT_CHANGED) {
                    acceptOutputFormatLocked(decoder.outputFormat)
                    return true
                }
                if (index < 0) return false
            } catch (error: Exception) {
                failLocked("output", error)
                return false
            }
        }
        deliverOutput(decoder, epoch, index, output)
        return true
    }

    private fun deliverOutput(decoder: MediaCodec, epoch: Int, index: Int, output: MediaCodec.BufferInfo) {
        val outputNs = System.nanoTime()
        val decoded: VideoFrame
        val decodeTime: Int
        val deliveryCallback: VideoDecoder.Callback?
        synchronized(lock) {
            if (!isCurrentLocked(decoder, epoch)) return
            try {
                val frame = frames.remove(output.presentationTimeUs)
                if (frame == null || output.flags and MediaCodec.BUFFER_FLAG_CODEC_CONFIG != 0) {
                    NativeInputDiagnostics.retainCounted("hdr.unmatched") {
                        "HDR output without metadata pts=${output.presentationTimeUs} flags=${output.flags} " +
                            "size=${output.size} tracked=${frames.size} first=${frames.firstTimestamp} last=$nextPtsUs"
                    }
                    decoder.releaseOutputBuffer(index, false)
                    return
                }
                NativeInputDiagnostics.retainCounted("hdr.matched") {
                    "HDR output matched pts=${output.presentationTimeUs} tracked=${frames.size} pending=${pendingFrames.size}"
                }
                frame.flow?.let { flow?.output(it, outputNs) }
                val buffer = HdrSurfaceBuffer(outputWidth, outputHeight, flow = frame.flow,
                    timedFinish = { time -> finishOutput(decoder, epoch, index, true, time, frame.flow) }) { render ->
                    finishOutput(decoder, epoch, index, render, null, frame.flow)
                }
                decoded = VideoFrame(buffer, frame.rotation, frame.timestampNs)
                decodeTime = (SystemClock.elapsedRealtime() - frame.queuedAtMs).toInt()
                recordTimingLocked(frame, SystemClock.elapsedRealtimeNanos())
                deliveryCallback = callback
            } catch (error: Exception) {
                failLocked("output", error)
                return
            }
        }
        // WebRTC/NVST and surface presentation may call back into this decoder.
        try { deliveryCallback?.onDecodedFrame(decoded, decodeTime, null) }
        catch (error: Exception) { synchronized(lock) { if (isCurrentLocked(decoder, epoch)) failLocked("delivery", error) } }
        finally { decoded.release() }
    }

    private fun finishOutput(decoder: MediaCodec, epoch: Int, index: Int, render: Boolean,
        timestampNs: Long?, trace: HdrFrameFlow.Frame?): Boolean = synchronized(lock) {
        if (!isCurrentLocked(decoder, epoch)) false
        else runCatching {
            val present = render && surface() === outputSurface && outputSurface?.surface?.isValid == true
            val before = System.nanoTime()
            if (present && timestampNs != null) decoder.releaseOutputBuffer(index, timestampNs)
            else decoder.releaseOutputBuffer(index, present)
            trace?.released(before, System.nanoTime(), timestampNs, present)
            present
        }.getOrDefault(false)
    }

    private fun feedInputLocked(decoder: MediaCodec) {
        while (pendingFrames.isNotEmpty()) {
            val index = if (asyncMode) availableInputs.pollFirst() ?: return else decoder.dequeueInputBuffer(0)
            if (index < 0) return
            val pending = pendingFrames.removeFirst()
            val input = decoder.getInputBuffer(index) ?: error("Missing HDR input buffer")
            input.clear()
            val size = pending.data.size
            require(size <= input.remaining()) { "HDR input exceeds codec buffer" }
            input.put(pending.data)
            nextPtsUs = hdrPresentationTimeUs(nextPtsUs, pending.timestampNs, fps)
            if (pending.timestampNs <= 0L) NativeInputDiagnostics.retainCounted("hdr.missingCaptureTime") {
                "HDR input missing capture timestamp; codec interval=${1_000_000L / fps.coerceAtLeast(1)}us"
            }
            val submittedAtNs = SystemClock.elapsedRealtimeNanos()
            if (frames.put(nextPtsUs, FrameInfo(pending.timestampNs, pending.rotation, pending.queuedAtMs,
                    pending.queuedAtNs, submittedAtNs, pending.copyNs, pending.flow))) {
                NativeInputDiagnostics.retainCounted("hdr.metadataEvicted") {
                    "HDR bounded metadata for inputs without decoder output"
                }
            }
            pending.flow?.let { flow?.submitted(it, System.nanoTime()) }
            decoder.queueInputBuffer(index, 0, size, nextPtsUs, 0)
        }
    }

    /** Observed MediaCodec turnaround includes driver buffering and callback/poll scheduling, not silicon time. */
    private fun recordTimingLocked(frame: FrameInfo, outputAtNs: Long) {
        if (timingStartedNs == 0L) timingStartedNs = frame.queuedAtNs
        val queueNs = (frame.submittedAtNs - frame.queuedAtNs).coerceAtLeast(0L)
        val codecNs = (outputAtNs - frame.submittedAtNs).coerceAtLeast(0L)
        timingCount++
        timingQueueNs += queueNs
        timingCodecNs += codecNs
        timingCopyNs += frame.copyNs
        timingQueueMaxNs = maxOf(timingQueueMaxNs, queueNs)
        timingCodecMaxNs = maxOf(timingCodecMaxNs, codecNs)
        if (outputAtNs - timingStartedNs < 1_000_000_000L) return
        NativeInputDiagnostics.addRetained("hdr.latency", String.format(Locale.US,
            "HDR timing codec=%s size=%dx%d frames=%d copyMeanMs=%.3f queueMeanMs=%.3f " +
                "codecTurnaroundMeanMs=%.3f totalMeanMs=%.3f queueMaxMs=%.3f " +
                "codecTurnaroundMaxMs=%.3f pendingMax=%d pending=%d async=%s pollSleepMs=%d",
            videoCodec, outputWidth, outputHeight, timingCount,
            timingCopyNs / timingCount / 1e6, timingQueueNs / timingCount / 1e6,
            timingCodecNs / timingCount / 1e6, (timingQueueNs + timingCodecNs) / timingCount / 1e6,
            timingQueueMaxNs / 1e6, timingCodecMaxNs / 1e6, timingPendingMax, pendingFrames.size, asyncMode, if (asyncMode) 0 else 2))
        resetTimingLocked(outputAtNs)
    }

    private fun resetTimingLocked(startedNs: Long = 0L) {
        timingStartedNs = startedNs
        timingCount = 0L
        timingQueueNs = 0L
        timingCodecNs = 0L
        timingCopyNs = 0L
        timingQueueMaxNs = 0L
        timingCodecMaxNs = 0L
        timingPendingMax = pendingFrames.size
    }

    private fun MediaFormat.integerOrNull(key: String): Int? =
        if (containsKey(key)) getInteger(key) else null

    private fun croppedDimension(format: MediaFormat, start: String, end: String, size: String, fallback: Int): Int =
        if (format.containsKey(start) && format.containsKey(end)) format.getInteger(end) - format.getInteger(start) + 1
        else if (format.containsKey(size)) format.getInteger(size) else fallback

    private fun stopCodecLocked() {
        generation++
        val previous = codec
        codec = null
        outputSurface = null
        frames.clear()
        flow?.reset()
        pendingFrames.clear()
        availableInputs.clear()
        codecHandler?.removeCallbacks(pumpInput)
        pumpPosted = false
        resetTimingLocked()
        needsKeyFrame = true
        if (previous != null) {
            runCatching { previous.stop() }
            runCatching { previous.release() }
        }
    }

    override fun release(): VideoCodecStatus {
        synchronized(lock) {
            running = false
            stopCodecLocked()
            callback = null
        }
        callbackThread?.quitSafely()
        val threads = listOfNotNull(outputThread, callbackThread)
        // Never wait while holding lock: queued callbacks may need it to discard old indices.
        threads.filter { it !== Thread.currentThread() }.forEach { it.join(1000) }
        if (threads.any { it !== Thread.currentThread() && it.isAlive }) return VideoCodecStatus.TIMEOUT
        outputThread = null
        callbackThread = null
        codecHandler = null
        return VideoCodecStatus.OK
    }

    override fun getImplementationName() = "OpenNOW-MediaCodec-HDR10-Surface"
}

/** Codec inputs need not produce one output each (for example AV1 hidden reference frames). */
internal class HdrFrameMetadata<T>(private val capacity: Int = 128) {
    init { require(capacity > 0) }
    private val values = linkedMapOf<Long, T>()
    val size: Int get() = values.size
    val firstTimestamp: Long? get() = values.keys.firstOrNull()
    fun put(timestampUs: Long, value: T): Boolean {
        values[timestampUs] = value
        if (values.size <= capacity) return false
        values.remove(values.keys.first())
        return true
    }
    fun remove(timestampUs: Long): T? = values.remove(timestampUs)
    fun clear() = values.clear()
}

internal fun hdrPresentationTimeUs(previousUs: Long, captureTimeNs: Long, fps: Int): Long =
    // WebRTC can supply zero/duplicate capture timestamps. One-microsecond increments
    // are collapsed by some hardware decoders; use a real frame interval in that case.
    maxOf(previousUs + (1_000_000L / fps.coerceAtLeast(1)).coerceAtLeast(1L), captureTimeNs / 1000)

internal fun copyHdrEncodedPayload(source: ByteBuffer): ByteArray =
    ByteArray(source.remaining()).also { source.duplicate().get(it) }

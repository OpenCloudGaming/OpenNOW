package com.opencloudgaming.opennow

import android.content.Context
import android.media.MediaCodec
import android.media.MediaCodecInfo
import android.media.MediaFormat
import android.media.MediaMuxer
import android.net.Uri
import android.opengl.GLES20
import android.os.Build
import android.os.SystemClock
import org.webrtc.EglBase
import org.webrtc.VideoFrame
import org.webrtc.VideoFrameDrawer
import org.webrtc.VideoSink
import java.io.File
import java.io.FileInputStream
import java.io.IOException
import java.nio.ByteBuffer
import java.util.ArrayDeque
import java.util.concurrent.ArrayBlockingQueue
import java.util.concurrent.CountDownLatch
import java.util.concurrent.atomic.AtomicBoolean
import java.util.concurrent.atomic.AtomicLong
import java.util.concurrent.locks.LockSupport
import kotlin.math.max
import kotlin.math.min

/** Lifecycle of the recorder. The worker, rather than the caller of stop(), owns finalization. */
internal enum class StreamRecorderPhase {
    Starting,
    Recording,
    Finalizing,
    Finished,
    Failed,
}

/**
 * Records decoded SDR WebRTC frames and decoder PCM directly to an MP4.
 *
 * The class deliberately has no View, screen-capture permission, or touch/UI dependency. Input callbacks
 * only retain/copy into small bounded queues; all codec, EGL, and SAF I/O work happens on one
 * dedicated worker. [stop] is therefore non-blocking and may safely be called from the UI thread.
 */
internal class AndroidStreamRecorder(
    context: Context,
    private val sharedContext: EglBase.Context,
    private val outputUri: Uri,
    private val width: Int,
    private val height: Int,
    private val fps: Int,
    private val bitrateMbps: Int,
    private val sharpnessAmount: Float,
    private val onPhase: (StreamRecorderPhase, String?) -> Unit,
) : VideoSink {
    private companion object {
        private const val VIDEO_MIME = MediaFormat.MIMETYPE_VIDEO_AVC
        private const val AUDIO_MIME = MediaFormat.MIMETYPE_AUDIO_AAC
        private const val DEFAULT_SAMPLE_RATE = 48_000
        private const val DEFAULT_CHANNELS = 2
        private const val DEFAULT_BITS_PER_SAMPLE = 16
        private const val VIDEO_QUEUE_CAPACITY = 4
        private const val AUDIO_QUEUE_CAPACITY = 64
        private const val AUDIO_INPUTS_PER_TICK = 8
        private const val AUDIO_GAP_REANCHOR_THRESHOLD_NS = 50_000_000L
        private const val PENDING_SAMPLE_CAPACITY = 128
        private const val FINALIZE_TIMEOUT_NS = 15_000_000_000L
    }

    private val appContext = context.applicationContext
    private val resolver = appContext.contentResolver
    private val validConfiguration =
        width > 0 && height > 0 && (width and 1) == 0 && (height and 1) == 0 && fps in 1..60 &&
            (bitrateMbps == 0 || bitrateMbps in 2..50) && sharpnessAmount.isFinite()

    private data class QueuedVideo(val frame: VideoFrame, val ptsNs: Long)

    private class QueuedAudio(
        val bytes: ByteArray,
        val bitsPerSample: Int,
        val sampleRate: Int,
        val channels: Int,
        val firstPtsNs: Long,
    ) {
        var offset: Int = 0

        val bytesPerFrame: Int
            get() = (bitsPerSample / 8).coerceAtLeast(1) * channels.coerceAtLeast(1)

        val durationNs: Long
            get() = (bytes.size / bytesPerFrame).toLong() * 1_000_000_000L / sampleRate.coerceAtLeast(1)
    }

    private data class PendingSample(
        val data: ByteArray,
        val ptsUs: Long,
        val flags: Int,
    )

    private val inputGate = Any()
    private val frameQueue = ArrayBlockingQueue<QueuedVideo>(VIDEO_QUEUE_CAPACITY)
    private val audioQueue = ArrayBlockingQueue<QueuedAudio>(AUDIO_QUEUE_CAPACITY)
    private val started = AtomicBoolean(false)
    private val stopRequested = AtomicBoolean(false)
    private val resourcesReleased = AtomicBoolean(false)
    private val workerFinished = CountDownLatch(1)
    private val audioDroppedBlocks = AtomicLong(0L)
    private val videoDroppedFrames = AtomicLong(0L)
    private val audioDiscontinuityAtNs = AtomicLong(-1L)
    private val reportedAudioQueueOverflow = AtomicBoolean(false)
    @Volatile private var acceptingInput = false
    @Volatile private var workerThread: Thread? = null

    private val phaseGate = Any()
    private var lastPhase: StreamRecorderPhase? = null
    private var lastPhaseMessage: String? = null

    // These are worker-owned after start(). The NVST decoder supplies signed PCM16 at 48 kHz;
    // mono callback packets are expanded to stereo before reaching the fixed AAC input format.
    private var videoEncoder: MediaCodec? = null
    private var audioEncoder: MediaCodec? = null
    private var videoInputSurface: android.view.Surface? = null
    private var eglBase: EglBase? = null
    private var videoDrawer: VideoFrameDrawer? = null
    private var glDrawer: StreamSharpnessGlDrawer? = null

    private var muxer: MediaMuxer? = null
    private var outputPfd: android.os.ParcelFileDescriptor? = null
    private var temporaryOutput: File? = null
    private var muxerStarted = false
    private var videoTrack = -1
    private var audioTrack = -1
    private var videoOutputFormat: MediaFormat? = null
    private var audioOutputFormat: MediaFormat? = null
    private val pendingVideoSamples = ArrayDeque<PendingSample>()
    private val pendingAudioSamples = ArrayDeque<PendingSample>()

    private var videoInputPtsUs = 0L
    private var audioInputPtsUs = 0L
    private var audioCadenceNextPtsNs: Long? = null
    private var nextVideoFramePtsNs = 0L
    private var videoWrittenPtsUs = 0L
    private var audioWrittenPtsUs = 0L
    private var timelineOriginNs = 0L
    private var outputSaved = false
    private var reportedAudioFormatWarning = false
    private var reportedMonoExpansion = false
    private var videoOutputEos = false
    private var audioOutputEos = false

    override fun onFrame(frame: VideoFrame) {
        // WebRTC may recycle the frame as soon as this callback returns. Retain exactly once
        // before enqueueing, and release it either on the worker or on a dropped-frame path.
        // Capture the same elapsedRealtimeNanos domain used by queueAudio(), rather than the
        // transport timestamp carried by VideoFrame.
        val receivedPtsNs = SystemClock.elapsedRealtimeNanos()
        synchronized(inputGate) {
            if (!acceptingInput) return
            frame.retain()
            val queued = QueuedVideo(frame, receivedPtsNs)
            if (!frameQueue.offer(queued)) {
                if (!frameQueue.offer(queued)) {
                    frameQueue.poll()?.let { dropped ->
                        videoDroppedFrames.incrementAndGet()
                        dropped.frame.release()
                    }
                    if (!frameQueue.offer(queued)) {
                        videoDroppedFrames.incrementAndGet()
                        queued.frame.release()
                    }
                }
            }
        }
    }

    /**
     * Copies the callback buffer before returning. Only PCM16/48 kHz is accepted because
     * MediaCodec's AAC input is stereo; mono packets are duplicated sample-for-sample, while
     * malformed or other-rate/bit-depth/channel packets are dropped without blocking the callback.
     */
    fun queueAudio(
        audioData: ByteBuffer,
        bitsPerSample: Int,
        sampleRate: Int,
        numberOfChannels: Int,
        ptsNs: Long,
    ) {
        var firstNotice: String? = null
        var secondNotice: String? = null
        var rejectPacket = false
        fun deferNotice(message: String) {
            if (firstNotice == null) firstNotice = message
            else if (secondNotice == null) secondNotice = message
        }

        synchronized(inputGate) {
            if (!acceptingInput) return
            if (
                bitsPerSample != DEFAULT_BITS_PER_SAMPLE ||
                sampleRate != DEFAULT_SAMPLE_RATE ||
                (numberOfChannels != 1 && numberOfChannels != DEFAULT_CHANNELS)
            ) {
                if (!reportedAudioFormatWarning) {
                    reportedAudioFormatWarning = true
                    deferNotice(
                        "Ignoring audio packet with unsupported format " +
                            "${bitsPerSample}bit/${sampleRate}Hz/${numberOfChannels}ch; " +
                            "expected PCM16/48000Hz with 1 or 2 channels",
                    )
                }
                rejectPacket = true
            } else {
                val source = audioData.duplicate()
                val sourceCopy = ByteArray(source.remaining())
                source.get(sourceCopy)
                if (sourceCopy.isEmpty()) return
                val stereoCopy = if (numberOfChannels == 1) {
                    if ((sourceCopy.size and 1) != 0) {
                        if (!reportedAudioFormatWarning) {
                            reportedAudioFormatWarning = true
                            deferNotice("Ignoring mono PCM packet with an incomplete 16-bit sample")
                        }
                        rejectPacket = true
                        ByteArray(0)
                    } else {
                        ByteArray(sourceCopy.size * 2).also { stereo ->
                            var monoOffset = 0
                            var stereoOffset = 0
                            while (monoOffset < sourceCopy.size) {
                                val low = sourceCopy[monoOffset]
                                val high = sourceCopy[monoOffset + 1]
                                stereo[stereoOffset] = low
                                stereo[stereoOffset + 1] = high
                                stereo[stereoOffset + 2] = low
                                stereo[stereoOffset + 3] = high
                                monoOffset += 2
                                stereoOffset += 4
                            }
                        }
                    }
                } else {
                    sourceCopy
                }
                if (!rejectPacket && stereoCopy.size % (DEFAULT_BITS_PER_SAMPLE / 8 * DEFAULT_CHANNELS) != 0) {
                    if (!reportedAudioFormatWarning) {
                        reportedAudioFormatWarning = true
                        deferNotice("Ignoring stereo PCM packet with an incomplete frame")
                    }
                    rejectPacket = true
                }
                if (!rejectPacket) {
                    if (numberOfChannels == 1 && !reportedMonoExpansion) {
                        reportedMonoExpansion = true
                        deferNotice("Duplicating mono PCM to stereo for AAC")
                    }
                    val queued = QueuedAudio(
                        stereoCopy,
                        bitsPerSample,
                        sampleRate,
                        DEFAULT_CHANNELS,
                        ptsNs,
                    )
                    if (!audioQueue.offer(queued)) {
                        if (!audioQueue.offer(queued)) {
                            val dropped = audioQueue.poll()
                            if (dropped != null) {
                                audioDroppedBlocks.incrementAndGet()
                                audioDiscontinuityAtNs.set(dropped.firstPtsNs + dropped.durationNs)
                                if (reportedAudioQueueOverflow.compareAndSet(false, true)) {
                                    deferNotice(
                                        "Audio input fell behind the encoder; dropping old PCM blocks to preserve sync",
                                    )
                                }
                            }
                            if (!audioQueue.offer(queued)) {
                                audioDroppedBlocks.incrementAndGet()
                                audioDiscontinuityAtNs.set(queued.firstPtsNs + queued.durationNs)
                            }
                        }
                    }
                }
            }
        }
        firstNotice?.let { report(StreamRecorderPhase.Recording, it) }
        secondNotice?.let { report(StreamRecorderPhase.Recording, it) }
    }

    /** Starts the worker and returns without initializing codecs on the calling/UI thread. */
    fun start(): Boolean {
        if (!validConfiguration) {
            report(StreamRecorderPhase.Failed,
                "Recorder needs positive even dimensions, 1-60 FPS, automatic or 2-50 Mbps bitrate, and finite sharpness")
            return false
        }
        if (!started.compareAndSet(false, true)) return false
        timelineOriginNs = SystemClock.elapsedRealtimeNanos()
        synchronized(inputGate) { acceptingInput = true }
        report(StreamRecorderPhase.Starting, null)
        val thread = Thread(::runWorker, "OpenNOW-stream-recorder")
        thread.isDaemon = true
        workerThread = thread
        return try {
            thread.start()
            true
        } catch (error: Throwable) {
            synchronized(inputGate) { acceptingInput = false }
            workerThread = null
            report(StreamRecorderPhase.Failed, failureMessage("Could not start recorder worker", error))
            workerFinished.countDown()
            false
        }
    }

    /**
     * Requests finalization and returns immediately. Repeated calls are harmless; the worker is
     * the only owner that signals EOS, stops the muxer, copies legacy-API output, and releases EGL.
     */
    fun stop() {
        if (!started.get()) return
        synchronized(inputGate) {
            acceptingInput = false
            if (!stopRequested.compareAndSet(false, true)) return
        }
        val worker = workerThread
        if (worker != null) LockSupport.unpark(worker)
    }

    /** Lifecycle alias for integrations that use an explicit release callback. */
    fun release() = stop()

    /** Waits for codecs, muxer, and the recorder-owned EGL child context to be fully released. */
    fun awaitWorkerFinished() {
        var interrupted = false
        while (true) {
            try {
                workerFinished.await()
                break
            } catch (_: InterruptedException) {
                interrupted = true
            }
        }
        if (interrupted) Thread.currentThread().interrupt()
    }

    private fun runWorker() {
        try {
            createVideoEncoder()
            createAudioEncoder()
            report(StreamRecorderPhase.Recording, null)
            recordingLoop()
            finishOutput()
            releaseResources()
            copyLegacyOutputIfNeeded()
            outputSaved = true
            val audioDrops = audioDroppedBlocks.get()
            val videoDrops = videoDroppedFrames.get()
            val finishMessage = buildList {
                if (audioDrops > 0L) add("dropped $audioDrops audio PCM blocks to stay synchronized")
                if (videoDrops > 0L) add("dropped $videoDrops video frames under encoder load")
            }.takeIf { it.isNotEmpty() }?.joinToString("; ")
            report(StreamRecorderPhase.Finished, finishMessage)
        } catch (error: Throwable) {
            releaseResources()
            deleteTemporaryOutput()
            if (!outputSaved) deleteOutputUri()
            report(StreamRecorderPhase.Failed, failureMessage("Stream recording failed", error))
        } finally {
            synchronized(inputGate) { acceptingInput = false }
            workerThread = null
            workerFinished.countDown()
        }
    }

    private fun openOutput() {
        if (Build.VERSION.SDK_INT >= Build.VERSION_CODES.O) {
            outputPfd = resolver.openFileDescriptor(outputUri, "w")
                ?: throw IOException("SAF did not return a writable file descriptor")
            muxer = MediaMuxer(
                outputPfd!!.fileDescriptor,
                MediaMuxer.OutputFormat.MUXER_OUTPUT_MPEG_4,
            )
        } else {
            // File-descriptor MediaMuxer was added in API 26. Keep the temporary file in the app
            // cache and copy only after MediaMuxer.release() has finalized the MP4.
            temporaryOutput = File.createTempFile("opennow-stream-", ".mp4", appContext.cacheDir)
            muxer = MediaMuxer(
                temporaryOutput!!.absolutePath,
                MediaMuxer.OutputFormat.MUXER_OUTPUT_MPEG_4,
            )
        }
    }

    private fun createVideoEncoder() {
        val format = MediaFormat.createVideoFormat(VIDEO_MIME, width, height).apply {
            setInteger(MediaFormat.KEY_COLOR_FORMAT, MediaCodecInfo.CodecCapabilities.COLOR_FormatSurface)
            setInteger(MediaFormat.KEY_BIT_RATE, videoBitrate())
            setInteger(MediaFormat.KEY_FRAME_RATE, fps)
            setInteger(MediaFormat.KEY_I_FRAME_INTERVAL, 2)
        }
        val encoder = MediaCodec.createEncoderByType(VIDEO_MIME)
        try {
            encoder.configure(format, null, null, MediaCodec.CONFIGURE_FLAG_ENCODE)
            val inputSurface = encoder.createInputSurface()
            videoInputSurface = inputSurface
            encoder.start()
            videoEncoder = encoder
            eglBase = EglBase.create(sharedContext, EglBase.CONFIG_RECORDABLE)
            eglBase!!.createSurface(inputSurface)
            eglBase!!.makeCurrent()
            videoDrawer = VideoFrameDrawer()
            glDrawer = StreamSharpnessGlDrawer().also { it.amount = sharpnessAmount.coerceIn(0f, 0.28f) }
            GLES20.glClearColor(0f, 0f, 0f, 1f)
            GLES20.glClear(GLES20.GL_COLOR_BUFFER_BIT)
        } catch (error: Throwable) {
            runCatching { encoder.release() }
            throw error
        }
    }

    private fun createAudioEncoder() {
        val format = MediaFormat.createAudioFormat(
            AUDIO_MIME,
            DEFAULT_SAMPLE_RATE,
            DEFAULT_CHANNELS,
        ).apply {
            setInteger(
                MediaFormat.KEY_AAC_PROFILE,
                MediaCodecInfo.CodecProfileLevel.AACObjectLC,
            )
            setInteger(MediaFormat.KEY_BIT_RATE, 128_000)
            setInteger(MediaFormat.KEY_MAX_INPUT_SIZE, 16 * 1024)
            // The AAC encoder receives signed interleaved PCM16 from the decoder callback; no
            // microphone or platform audio capture API is involved.
        }
        val encoder = MediaCodec.createEncoderByType(AUDIO_MIME)
        try {
            encoder.configure(format, null, null, MediaCodec.CONFIGURE_FLAG_ENCODE)
            encoder.start()
            audioEncoder = encoder
        } catch (error: Throwable) {
            runCatching { encoder.release() }
            throw error
        }
    }

    private fun recordingLoop() {
        var finalizing = false
        var videoInputEos = false
        var audioInputEos = false
        var pendingAudio: QueuedAudio? = null
        var finalizeDeadlineNs = Long.MAX_VALUE

        while (true) {
            var progress = false
            if (stopRequested.get() && !finalizing) {
                finalizing = true
                finalizeDeadlineNs = System.nanoTime() + FINALIZE_TIMEOUT_NS
                report(StreamRecorderPhase.Finalizing, null)
            }

            var audioInputsFed = 0
            while (audioInputsFed < AUDIO_INPUTS_PER_TICK) {
                if (pendingAudio == null) pendingAudio = audioQueue.poll()
                val currentPendingAudio = pendingAudio ?: break
                if (!feedAudio(currentPendingAudio)) break
                if (currentPendingAudio.offset >= currentPendingAudio.bytes.size) pendingAudio = null
                audioInputsFed += 1
                progress = true
            }

            val queuedFrame = frameQueue.poll()
            if (queuedFrame != null) {
                try {
                    renderFrame(queuedFrame.frame, queuedFrame.ptsNs)
                } finally {
                    queuedFrame.frame.release()
                }
                progress = true
            }

            if (finalizing && frameQueue.isEmpty() && !videoInputEos) {
                videoEncoder!!.signalEndOfInputStream()
                videoInputEos = true
                progress = true
            }
            if (
                finalizing && pendingAudio == null && audioQueue.isEmpty() && !audioInputEos &&
                    queueAudioEos()
            ) {
                audioInputEos = true
                progress = true
            }

            if (drainEncoder(audioEncoder!!, isVideo = false)) progress = true
            if (drainEncoder(videoEncoder!!, isVideo = true)) progress = true

            if (
                finalizing && videoInputEos && audioInputEos && videoOutputEos && audioOutputEos
            ) {
                break
            }
            if (finalizing && System.nanoTime() >= finalizeDeadlineNs) {
                throw IOException("Timed out waiting for codec EOS")
            }
            if (!progress) LockSupport.parkNanos(1_000_000L)
        }
    }

    private fun renderFrame(frame: VideoFrame, receivedPtsNs: Long) {
        val scheduledPtsNs = nextVideoFramePtsNs
        if (scheduledPtsNs > 0L && receivedPtsNs + 1_000_000L < scheduledPtsNs) {
            return
        }
        val egl = eglBase ?: throw IllegalStateException("EGL recorder surface is unavailable")
        val drawer = videoDrawer ?: throw IllegalStateException("Video drawer is unavailable")
        val gl = glDrawer ?: throw IllegalStateException("GL drawer is unavailable")
        val ptsUs = normalizedInputPts(receivedPtsNs, videoInputPtsUs)
        videoInputPtsUs = ptsUs

        val sourceWidth = frame.rotatedWidth.coerceAtLeast(1)
        val sourceHeight = frame.rotatedHeight.coerceAtLeast(1)
        val scale = min(width.toFloat() / sourceWidth, height.toFloat() / sourceHeight)
        val viewportWidth = max(1, (sourceWidth * scale).toInt())
        val viewportHeight = max(1, (sourceHeight * scale).toInt())
        val viewportX = (width - viewportWidth) / 2
        val viewportY = (height - viewportHeight) / 2

        egl.makeCurrent()
        GLES20.glViewport(0, 0, width, height)
        GLES20.glClearColor(0f, 0f, 0f, 1f)
        GLES20.glClear(GLES20.GL_COLOR_BUFFER_BIT)
        // VideoFrameDrawer applies VideoFrame rotation when the matrix is null. The viewport is
        // based on rotatedWidth/rotatedHeight, so changed resolutions are letterboxed, never
        // cropped or stretched, while the unused encoder surface remains black.
        drawer.drawFrame(frame, gl, null, viewportX, viewportY, viewportWidth, viewportHeight)
        egl.swapBuffers(ptsUs * 1_000L)
        val frameIntervalNs = 1_000_000_000L / fps
        var nextPtsNs = if (scheduledPtsNs == 0L) receivedPtsNs + frameIntervalNs
        else scheduledPtsNs + frameIntervalNs
        if (nextPtsNs <= receivedPtsNs) {
            nextPtsNs += ((receivedPtsNs - nextPtsNs) / frameIntervalNs + 1L) * frameIntervalNs
        }
        nextVideoFramePtsNs = nextPtsNs
    }

    /** Returns true when an input buffer accepted all or part of [packet]. */
    private fun feedAudio(packet: QueuedAudio): Boolean {
        if (
            packet.bitsPerSample != DEFAULT_BITS_PER_SAMPLE ||
            packet.sampleRate != DEFAULT_SAMPLE_RATE ||
            packet.channels != DEFAULT_CHANNELS
        ) {
            packet.offset = packet.bytes.size
            return true
        }
        val codec = audioEncoder ?: return false
        val inputIndex = codec.dequeueInputBuffer(0)
        if (inputIndex < 0) return false
        val input = codec.getInputBuffer(inputIndex)
            ?: throw IOException("AAC encoder returned a null input buffer")
        input.clear()
        val remaining = packet.bytes.size - packet.offset
        val frameSize = packet.bytesPerFrame.coerceAtLeast(1)
        val inputCapacity = input.remaining()
        val alignedCapacity = inputCapacity - inputCapacity % frameSize
        val amount = min(remaining, alignedCapacity)
        if (amount <= 0) throw IOException("AAC encoder input buffer has no capacity")
        input.put(packet.bytes, packet.offset, amount)
        // Derive normal cadence from PCM frame count; re-anchor only when a long arrival gap
        // signals that old PCM blocks were dropped from an overloaded bounded queue.
        val expectedPtsNs = audioCadenceNextPtsNs
        val discontinuityAtNs = audioDiscontinuityAtNs.get()
        val droppedAudioGap = if (discontinuityAtNs >= 0L && packet.firstPtsNs >= discontinuityAtNs) {
            audioDiscontinuityAtNs.compareAndSet(discontinuityAtNs, -1L)
        } else {
            false
        }
        val chunkPtsNs = if (
            droppedAudioGap || expectedPtsNs == null ||
                packet.firstPtsNs - expectedPtsNs > AUDIO_GAP_REANCHOR_THRESHOLD_NS
        ) {
            packet.firstPtsNs
        } else {
            expectedPtsNs
        }
        val ptsUs = normalizedInputPts(chunkPtsNs, audioInputPtsUs)
        audioInputPtsUs = ptsUs
        codec.queueInputBuffer(inputIndex, 0, amount, ptsUs, 0)
        packet.offset += amount
        val completeFrames = amount / frameSize
        if (completeFrames > 0) {
            audioCadenceNextPtsNs =
                chunkPtsNs + (completeFrames.toLong() * 1_000_000_000L) / packet.sampleRate
        }
        return true
    }

    private fun queueAudioEos(): Boolean {
        val codec = audioEncoder ?: return false
        val inputIndex = codec.dequeueInputBuffer(0)
        if (inputIndex < 0) return false
        val eosPtsUs = audioCadenceNextPtsNs
            ?.let { normalizedInputPts(it, audioInputPtsUs) }
            ?: safeIncrement(audioInputPtsUs)
        audioInputPtsUs = eosPtsUs
        codec.queueInputBuffer(
            inputIndex,
            0,
            0,
            eosPtsUs,
            MediaCodec.BUFFER_FLAG_END_OF_STREAM,
        )
        return true
    }

    private fun drainEncoder(codec: MediaCodec, isVideo: Boolean): Boolean {
        var progress = false
        val info = MediaCodec.BufferInfo()
        while (true) {
            val outputIndex = codec.dequeueOutputBuffer(info, 0)
            when {
                outputIndex == MediaCodec.INFO_TRY_AGAIN_LATER -> return progress
                outputIndex == MediaCodec.INFO_OUTPUT_FORMAT_CHANGED -> {
                    val format = codec.outputFormat
                    if (isVideo) {
                        videoOutputFormat = format
                    } else {
                        audioOutputFormat = format
                    }
                    maybeStartMuxer()
                    progress = true
                }
                outputIndex >= 0 -> {
                    progress = true
                    val flags = info.flags
                    val isCodecConfig = flags and MediaCodec.BUFFER_FLAG_CODEC_CONFIG != 0
                    if (info.size > 0 && !isCodecConfig) {
                        val outputBuffer = codec.getOutputBuffer(outputIndex)
                            ?: throw IOException("${if (isVideo) "Video" else "AAC"} output buffer is null")
                        val data = copyOutput(outputBuffer, info)
                        val ptsUs = normalizedOutputPts(
                            info.presentationTimeUs,
                            isVideo,
                        )
                        val sampleFlags = flags and
                            MediaCodec.BUFFER_FLAG_CODEC_CONFIG.inv() and
                            MediaCodec.BUFFER_FLAG_END_OF_STREAM.inv()
                        enqueueSample(
                            isVideo = isVideo,
                            sample = PendingSample(data, ptsUs, sampleFlags),
                        )
                    }
                    val reachedEos = flags and MediaCodec.BUFFER_FLAG_END_OF_STREAM != 0
                    codec.releaseOutputBuffer(outputIndex, false)
                    if (reachedEos) {
                        if (isVideo) videoOutputEos = true else audioOutputEos = true
                    }
                }
                else -> {
                    // INFO_OUTPUT_BUFFERS_CHANGED is obsolete but can still be returned by older
                    // platform codecs; retrying the non-blocking dequeue is sufficient.
                    progress = true
                }
            }
        }
    }

    private fun copyOutput(buffer: ByteBuffer, info: MediaCodec.BufferInfo): ByteArray {
        val start = info.offset
        val end = info.offset + info.size
        if (start < 0 || info.size < 0 || end < start || end > buffer.capacity()) {
            throw IOException("Codec returned an invalid output buffer range")
        }
        val duplicate = buffer.duplicate()
        duplicate.position(start)
        duplicate.limit(end)
        val copy = ByteArray(info.size)
        duplicate.get(copy)
        return copy
    }

    private fun enqueueSample(isVideo: Boolean, sample: PendingSample) {
        if (muxerStarted) {
            writeSample(isVideo, sample)
            return
        }
        val pending = if (isVideo) pendingVideoSamples else pendingAudioSamples
        if (pending.size >= PENDING_SAMPLE_CAPACITY) {
            throw IOException("Muxer tracks did not become ready before the sample queue filled")
        }
        pending.addLast(sample)
    }

    private fun maybeStartMuxer() {
        if (muxerStarted || videoOutputFormat == null || audioOutputFormat == null) return
        // MediaMuxer itself is deliberately constructed only after both codec output formats are
        // exposed. This also keeps SAF/PFD creation out of the recording path until both tracks
        // are known, while pending encoded samples remain bounded below.
        if (muxer == null) openOutput()
        val output = muxer ?: throw IOException("MediaMuxer is unavailable")
        videoTrack = output.addTrack(videoOutputFormat!!)
        audioTrack = output.addTrack(audioOutputFormat!!)
        output.start()
        muxerStarted = true
        while (pendingVideoSamples.isNotEmpty()) writeSample(true, pendingVideoSamples.removeFirst())
        while (pendingAudioSamples.isNotEmpty()) writeSample(false, pendingAudioSamples.removeFirst())
    }

    private fun writeSample(isVideo: Boolean, sample: PendingSample) {
        val output = muxer ?: throw IOException("MediaMuxer is unavailable")
        val info = MediaCodec.BufferInfo()
        info.set(0, sample.data.size, sample.ptsUs, sample.flags)
        output.writeSampleData(
            if (isVideo) videoTrack else audioTrack,
            ByteBuffer.wrap(sample.data),
            info,
        )
    }

    private fun normalizedInputPts(rawNs: Long, previousUs: Long): Long {
        val relativeNs = rawNs - timelineOriginNs
        val candidate = if (relativeNs <= 0L) 1L else max(1L, relativeNs / 1_000L)
        return max(candidate, safeIncrement(previousUs))
    }

    private fun normalizedOutputPts(rawUs: Long, isVideo: Boolean): Long {
        val previous = if (isVideo) videoWrittenPtsUs else audioWrittenPtsUs
        val candidate = max(1L, rawUs)
        val normalized = max(candidate, safeIncrement(previous))
        if (isVideo) videoWrittenPtsUs = normalized else audioWrittenPtsUs = normalized
        return normalized
    }

    private fun safeIncrement(value: Long): Long =
        if (value >= Long.MAX_VALUE - 1L) Long.MAX_VALUE else value + 1L

    private fun finishOutput() {
        if (!muxerStarted) {
            throw IOException("Both H.264 and AAC output formats were not exposed")
        }
        val output = muxer ?: throw IOException("MediaMuxer is unavailable")
        output.stop()
        output.release()
        muxer = null
        outputPfd?.close()
        outputPfd = null
    }

    private fun releaseResources() {
        if (!resourcesReleased.compareAndSet(false, true)) return
        synchronized(inputGate) { acceptingInput = false }
        while (true) {
            val queued = frameQueue.poll() ?: break
            queued.frame.release()
        }
        audioQueue.clear()
        pendingVideoSamples.clear()
        pendingAudioSamples.clear()
        videoOutputFormat = null
        audioOutputFormat = null
        runCatching { glDrawer?.release() }
        glDrawer = null
        videoDrawer = null
        runCatching { eglBase?.release() }
        eglBase = null
        runCatching { videoInputSurface?.release() }
        videoInputSurface = null
        runCatching { videoEncoder?.stop() }
        runCatching { videoEncoder?.release() }
        videoEncoder = null
        runCatching { audioEncoder?.stop() }
        runCatching { audioEncoder?.release() }
        audioEncoder = null
        runCatching { muxer?.release() }
        muxer = null
        runCatching { outputPfd?.close() }
        outputPfd = null
    }

    private fun copyLegacyOutputIfNeeded() {
        if (Build.VERSION.SDK_INT >= Build.VERSION_CODES.O) return
        val source = temporaryOutput ?: throw IOException("Temporary MP4 was not created")
        if (!source.isFile || source.length() <= 0L) {
            throw IOException("Finalized temporary MP4 is missing or empty")
        }
        val destination = resolver.openOutputStream(outputUri)
            ?: throw IOException("SAF did not return an output stream")
        FileInputStream(source).use { input ->
            destination.use { output -> input.copyTo(output) }
        }
        deleteTemporaryOutput()
    }

    private fun deleteTemporaryOutput() {
        temporaryOutput?.let { runCatching { it.delete() } }
        temporaryOutput = null
    }

    private fun deleteOutputUri() {
        runCatching { resolver.delete(outputUri, null, null) }
    }

    private fun videoBitrate(): Int {
        if (bitrateMbps > 0) return (bitrateMbps.coerceIn(2, 50) * 1_000_000L).toInt()
        val pixelsPerSecond = width.toLong() * height.toLong() * fps.toLong()
        return (pixelsPerSecond * 7L / 100L)
            .coerceIn(2_000_000L, 20_000_000L)
            .coerceAtMost(Int.MAX_VALUE.toLong())
            .toInt()
    }

    private fun failureMessage(prefix: String, error: Throwable): String =
        "$prefix: ${error.message ?: error.javaClass.simpleName}"

    private fun report(phase: StreamRecorderPhase, message: String?) {
        synchronized(phaseGate) {
            if (lastPhase == phase && lastPhaseMessage == message) return
            lastPhase = phase
            lastPhaseMessage = message
        }
        runCatching { onPhase(phase, message) }
    }
}

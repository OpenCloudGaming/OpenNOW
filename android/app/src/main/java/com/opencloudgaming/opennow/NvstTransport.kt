package com.opencloudgaming.opennow

import android.media.AudioAttributes
import android.media.AudioFormat
import android.media.AudioTrack
import android.media.MediaCodec
import android.media.MediaFormat
import android.os.SystemClock
import androidx.annotation.Keep
import kotlinx.serialization.json.*
import org.webrtc.*
import java.nio.ByteBuffer
import java.nio.ByteOrder
import java.util.concurrent.ArrayBlockingQueue
import java.util.concurrent.ThreadPoolExecutor
import java.util.concurrent.TimeUnit
import java.util.concurrent.atomic.AtomicInteger
import java.util.concurrent.atomic.AtomicLong
import kotlin.math.roundToInt

internal data class NvstNetworkMetrics(val jitterMs: Double?, val lossPct: Double?, val pingMs: Int?)

internal fun parseNvstNetworkMetrics(detail: String): NvstNetworkMetrics {
    val values = detail.split(',')
    fun value(index: Int) = values.getOrNull(index)?.toDoubleOrNull()?.takeIf { it.isFinite() && it >= 0 }
    return NvstNetworkMetrics(value(0), value(1)?.takeIf { it <= 100 },
        value(2)?.takeIf { it <= Int.MAX_VALUE }?.roundToInt())
}

internal const val NVST_OPT_IN_VERSION = 1

/** Disable the 1.6.3 automatic opt-in once. Later explicit choices survive restarts. */
internal fun AppSettings.withCurrentNvstOptInDefault(): AppSettings =
    if (nvstOptInVersion >= NVST_OPT_IN_VERSION) this else copy(
        stream = stream.copy(experimentalNvst = false),
        nvstOptInVersion = NVST_OPT_IN_VERSION,
    )

/** RTSPS is a control endpoint; it must never become the RTP/ICE peer fallback. */
internal fun JsonObject.isRtspSessionConnection(): Boolean {
    val resource = (get("resourcePath") as? JsonPrimitive)?.contentOrNull.orEmpty()
    return resource.startsWith("rtsps://") || resource.startsWith("rtsp://") ||
        (get("usage") as? JsonPrimitive)?.contentOrNull == "16" ||
        (get("appLevelProtocol") as? JsonPrimitive)?.contentOrNull == "6"
}

internal fun webRtcMediaFallbackConnections(connections: List<JsonObject>): List<JsonObject> =
    connections.filter {
        (it["usage"] as? JsonPrimitive)?.contentOrNull == "14" && !it.isRtspSessionConnection()
    }.sortedByDescending { (it["port"] as? JsonPrimitive)?.contentOrNull?.toIntOrNull() ?: 0 }

internal fun nvstRtspEndpoints(connections: JsonArray?): List<String> = connections.orEmpty().mapNotNull { raw ->
    val item = raw as? JsonObject ?: return@mapNotNull null
    fun value(key: String) = (item[key] as? JsonPrimitive)?.contentOrNull
    val resource = value("resourcePath").orEmpty()
    if (resource.startsWith("rtsps://") || resource.startsWith("rtsp://")) return@mapNotNull resource
    if (!item.isRtspSessionConnection()) return@mapNotNull null
    val host = value("ip")?.takeIf { it.isNotBlank() } ?: return@mapNotNull null
    val port = value("port")?.toIntOrNull()?.takeIf { it in 1..65535 } ?: 322
    "rtsps://${if (':' in host && !host.startsWith("[")) "[$host]" else host}:$port"
}.distinct()

internal fun nvstSessionContext(session: SessionInfo, settings: StreamSettings): String = buildJsonObject {
    put("session", buildJsonObject {
        put("sessionId", session.sessionId)
        put("serverIp", session.serverIp)
        session.mediaConnectionInfo?.let { put("mediaConnectionInfo", OpenNowJson.encodeToJsonElement(it)) }
        put("rtspsEndpoints", JsonArray(session.rtspsEndpoints.map(::JsonPrimitive)))
    })
    put("settings", buildJsonObject {
        put("resolution", settings.resolution)
        put("fps", settings.fps)
        put("codec", settings.codec.name)
        put("colorQuality", OpenNowJson.encodeToJsonElement(settings.colorQuality))
        put("maxBitrateMbps", settings.maxBitrateMbps)
        put("networkAdaptation", buildJsonObject {
            val bitrate = StreamNetworkAdaptation.bitrateRange(settings.maxBitrateMbps)
            put("dynamicStreamingMode", StreamNetworkAdaptation.DYNAMIC_STREAMING_MODE)
            put("minimumBitrateKbps", bitrate.minimumKbps)
            put("initialBitrateKbps", bitrate.initialKbps)
        })
        put("hdrEnabled", settings.hdrEnabled)
    })
    put("shortcuts", JsonObject(emptyMap()))
}.toString()

internal fun loadSettingsWithNvstDefault(raw: String?, persistMigration: (AppSettings) -> Unit = {}): AppSettings {
    val loaded = raw?.let { runCatching { OpenNowJson.decodeFromString<AppSettings>(it) }.getOrNull() } ?: AppSettings()
    val migrated = loaded.withCurrentNvstOptInDefault()
    if (migrated != loaded) persistMigration(migrated)
    return migrated
}

internal fun nvstDecoderInitialSize(session: SessionInfo, settings: StreamSettings): Pair<Int, Int> =
    parseResolutionPixelsOrNull(session.negotiatedStreamProfile?.resolution) ?: streamResolutionPixels(settings)

internal fun nvstEncodedImage(
    buffer: ByteBuffer,
    timestampNs: Long,
    keyframe: Boolean,
    release: Runnable,
): EncodedImage = EncodedImage.builder()
    .setBuffer(buffer, release)
    // NVST access units do not carry parsed dimensions here. Zero means unspecified to WebRTC;
    // MediaCodec learns the actual size from the bitstream. Stamping requested dimensions caused
    // AndroidVideoDecoder to reinitialize on EVERY frame after a server resolution fallback.
    .setEncodedWidth(0)
    .setEncodedHeight(0)
    .setCaptureTimeNs(timestampNs)
    .setFrameType(if (keyframe) EncodedImage.FrameType.VideoFrameKey else EncodedImage.FrameType.VideoFrameDelta)
    .setRotation(0)
    .createEncodedImage()

@Keep
internal object NvstBridge {
    init { System.loadLibrary("opennow_nvst") }
    external fun create(): Long
    external fun run(handle: Long, context: String, callback: NvstTransport)
    external fun stop(handle: Long)
    external fun input(handle: Long, bytes: ByteArray, partial: Boolean): Boolean
    external fun microphoneEnabled(handle: Long, enabled: Boolean)
    external fun microphone(handle: Long, samples: ShortArray, timestamp: Long, restart: Boolean): Boolean
    external fun keyframe(handle: Long)
}

/** One worker owns negotiation, decoder input, and audio; shutdown never blocks the UI thread. */
@Keep
internal class NvstTransport(
    context: android.content.Context,
    private val session: SessionInfo,
    private val settings: StreamSettings,
    private val decoderFactory: VideoDecoderFactory,
    private val sink: () -> VideoSink?,
    private val event: (String, String) -> Unit,
    private val stats: (StreamRuntimeStats) -> Unit,
    private val rumble: (GamepadRumbleCommand) -> Unit,
) {
    private val handle = NvstBridge.create()
    private val wifiPerformance = StreamWifiPerformanceLock(context)
    @Volatile private var stopped = false
    @Volatile var inputReady = false
        private set
    @Volatile var muted = false
    private val microphone = NvstMicrophoneCapture(context.applicationContext, settings.microphoneDeviceId,
        ready = { inputReady && !stopped },
        send = { samples, timestamp, restart -> NvstBridge.microphone(handle, samples, timestamp, restart) },
        error = { detail ->
            NvstBridge.microphoneEnabled(handle, false)
            event("microphone-error", detail)
        })
    fun setMicrophoneEnabled(enabled: Boolean) {
        val capture = enabled && !stopped && settings.microphoneMode != MicrophoneMode.Disabled
        if (microphone.enabled == capture) return
        NvstBridge.microphoneEnabled(handle, capture)
        microphone.enabled = capture
    }
    private val finished = java.util.concurrent.CountDownLatch(1)
    private var decoder: VideoDecoder? = null
    private var audio: NvstAudioOutput? = null
    private var needsKeyframe = true
    private val firstDecoded = java.util.concurrent.atomic.AtomicBoolean()
    private val lastDecodedAt = AtomicLong(SystemClock.elapsedRealtime())
    private val decodedFrames = AtomicInteger()
    private val receivedFrames = AtomicInteger()
    private val receivedBytes = AtomicLong()
    private var statsAt = SystemClock.elapsedRealtime()
    private var codecName: String? = null
    @Volatile private var decodedResolution: String? = null
    @Volatile private var decodeTimeMs: Double? = null
    private val decoderBuffers = ArrayBlockingQueue<ByteBuffer>(8)
    private val initialDecoderSize = nvstDecoderInitialSize(session, settings)
    private val inputWorker = ThreadPoolExecutor(1, 1, 0, TimeUnit.MILLISECONDS,
        ArrayBlockingQueue(128), { task -> Thread({
            android.os.Process.setThreadPriority(android.os.Process.THREAD_PRIORITY_DISPLAY)
            task.run()
        }, "OpenNOW-NVST-input") }, ThreadPoolExecutor.AbortPolicy())

    fun start() {
        Thread({
            try {
                android.os.Process.setThreadPriority(android.os.Process.THREAD_PRIORITY_DISPLAY)
                wifiPerformance.acquire()
                microphone.start()
                NvstBridge.run(handle, nvstSessionContext(session, settings), this)
            } catch (error: Exception) {
                if (!stopped) event("error", "NVST failed: ${error.javaClass.simpleName}")
            } finally {
                wifiPerformance.release()
                stopped = true
                inputReady = false
                inputWorker.shutdownNow()
                microphone.stop()
                microphone.awaitStopped()
                runCatching { decoder?.release() }
                runCatching { audio?.close() }
                finished.countDown()
            }
        }, "OpenNOW-NVST-media").start()
    }

    fun stop() {
        microphone.stop()
        stopped = true
        inputReady = false
        inputWorker.shutdownNow()
        NvstBridge.stop(handle)
    }

    fun awaitStopped() { finished.await() }

    fun sendInput(bytes: ByteArray, partial: Boolean): Boolean {
        if (stopped || !inputReady) return false
        return try {
            inputWorker.execute {
                if (!stopped && !NvstBridge.input(handle, bytes, partial)) {
                    NativeInputDiagnostics.retain("nvst.input", "NVST input send rejected")
                }
            }
            true
        } catch (_: java.util.concurrent.RejectedExecutionException) { false }
    }

    @Keep fun onNativeInput(bytes: ByteArray) {
        if (!stopped && inputReady) NvstHaptics.parse(bytes).forEach(rumble)
    }

    @Keep fun onNativeEvent(kind: String, detail: String) {
        if (stopped) return
        when (kind) {
            "input-ready" -> inputReady = true
            "input-unavailable", "error" -> inputReady = false
        }
        if (kind == "connected") {
            statsAt = SystemClock.elapsedRealtime()
            lastDecodedAt.set(statsAt)
        }
        if (kind == "network") {
            val now = SystemClock.elapsedRealtime()
            if (now - lastDecodedAt.get() > 8_000) {
                event("error", "NVST decoder made no progress for eight seconds")
                return
            }
            val seconds = ((now - statsAt) / 1000.0).coerceAtLeast(0.001)
            val network = parseNvstNetworkMetrics(detail)
            val decodedFps = (decodedFrames.getAndSet(0) / seconds).toInt()
            stats(StreamRuntimeStats(
                codec = codecName,
                fps = decodedFps,
                decodeMs = decodeTimeMs,
                receivedFps = (receivedFrames.getAndSet(0) / seconds).toInt(),
                decodedFps = decodedFps,
                bitrateKbps = (receivedBytes.getAndSet(0) * 8 / seconds / 1000).toInt(),
                jitterMs = network.jitterMs,
                packetLossPct = network.lossPct,
                pingMs = network.pingMs,
                resolution = decodedResolution,
            ))
            statsAt = now
        } else event(kind, detail)
    }

    @Keep fun onNativeMedia(codec: String, bytes: ByteBuffer, timestampNs: Long, keyframe: Boolean, contiguous: Boolean): Boolean {
        if (stopped) return false
        if (codec.equals("opus", true)) {
            val output = audio ?: NvstAudioOutput().also { audio = it }
            output.feed(bytes, timestampNs / 1000, muted)
            return true
        }
        receivedFrames.incrementAndGet()
        receivedBytes.addAndGet(bytes.remaining().toLong())
        if (!contiguous) needsKeyframe = true
        if (needsKeyframe && !keyframe) return false
        val activeDecoder = decoder ?: run {
            val created = decoderFactory.createDecoder(VideoCodecInfo(codec, emptyMap(), emptyList()))
                ?: error("No Android decoder for negotiated NVST codec $codec")
            decoder = created
            val (width, height) = initialDecoderSize
            val status = created.initDecode(VideoDecoder.Settings(2, width, height), VideoDecoder.Callback { frame, decodeMs, _ ->
                if (!stopped) {
                    decodedResolution = "${frame.rotatedWidth}x${frame.rotatedHeight}"
                    decodeTimeMs = decodeMs?.toDouble()
                    decodedFrames.incrementAndGet()
                    lastDecodedAt.set(SystemClock.elapsedRealtime())
                    sink()?.onFrame(frame)
                    if (firstDecoded.compareAndSet(false, true)) event("streaming", "")
                }
            })
            check(status == VideoCodecStatus.OK) { "NVST decoder initialization failed: $status" }
            codecName = codec
            created
        }
        val reusable = decoderBuffers.poll()
        val buffer = if (reusable != null && reusable.capacity() >= bytes.remaining()) reusable
            else ByteBuffer.allocateDirect(bytes.remaining())
        buffer.clear()
        buffer.put(bytes)
        buffer.flip()
        val image = nvstEncodedImage(buffer, timestampNs, keyframe) { decoderBuffers.offer(buffer) }
        return try {
            val status = activeDecoder.decode(image, VideoDecoder.DecodeInfo(false, 0))
            val accepted = status == VideoCodecStatus.OK
            needsKeyframe = !accepted
            accepted
        } finally { image.release() }
    }
}

/** Android's Opus decoder; partial writes retain PCM within a bounded 100 ms queue. */
private class NvstAudioOutput {
    private val codec = MediaCodec.createDecoderByType(MediaFormat.MIMETYPE_AUDIO_OPUS)
    private val track: AudioTrack
    private val info = MediaCodec.BufferInfo()
    private val pending = java.util.ArrayDeque<ByteBuffer>()
    private var pendingBytes = 0

    init {
        var openedTrack: AudioTrack? = null
        try {
            val format = MediaFormat.createAudioFormat(MediaFormat.MIMETYPE_AUDIO_OPUS, 48000, 2)
            val head = ByteBuffer.allocate(19).order(ByteOrder.LITTLE_ENDIAN)
                .put("OpusHead".toByteArray(Charsets.US_ASCII)).put(1).put(2).putShort(0).putInt(48000).putShort(0).put(0)
            format.setByteBuffer("csd-0", ByteBuffer.wrap(head.array()))
            format.setByteBuffer("csd-1", ByteBuffer.allocate(8).order(ByteOrder.nativeOrder()).putLong(0).apply { flip() })
            format.setByteBuffer("csd-2", ByteBuffer.allocate(8).order(ByteOrder.nativeOrder()).putLong(80_000_000).apply { flip() })
            codec.configure(format, null, null, 0)
            codec.start()
            openedTrack = AudioTrack.Builder()
                .setAudioAttributes(AudioAttributes.Builder().setUsage(AudioAttributes.USAGE_GAME).setContentType(AudioAttributes.CONTENT_TYPE_MOVIE).build())
                .setAudioFormat(AudioFormat.Builder().setSampleRate(48000).setChannelMask(AudioFormat.CHANNEL_OUT_STEREO).setEncoding(AudioFormat.ENCODING_PCM_16BIT).build())
                .setBufferSizeInBytes(maxOf(3840, AudioTrack.getMinBufferSize(48000, AudioFormat.CHANNEL_OUT_STEREO, AudioFormat.ENCODING_PCM_16BIT)))
                .setTransferMode(AudioTrack.MODE_STREAM)
                .apply { if (android.os.Build.VERSION.SDK_INT >= 26) setPerformanceMode(AudioTrack.PERFORMANCE_MODE_LOW_LATENCY) }
                .build()
            check(openedTrack.state == AudioTrack.STATE_INITIALIZED) { "NVST audio output unavailable" }
            openedTrack.play()
            track = openedTrack
        } catch (error: Exception) {
            openedTrack?.release()
            codec.release()
            throw error
        }
    }

    fun feed(bytes: ByteBuffer, ptsUs: Long, muted: Boolean) {
        track.setVolume(if (muted) 0f else 1f)
        drainPcm()
        val index = codec.dequeueInputBuffer(0)
        if (index >= 0) {
            val size = bytes.remaining()
            codec.getInputBuffer(index)!!.apply { clear(); put(bytes) }
            codec.queueInputBuffer(index, 0, size, ptsUs, 0)
        }
        repeat(8) {
            val output = codec.dequeueOutputBuffer(info, 0)
            if (output < 0) return
            try {
                codec.getOutputBuffer(output)?.let { pcm ->
                    pcm.position(info.offset)
                    pcm.limit(info.offset + info.size)
                    if (pending.isEmpty()) track.write(pcm, pcm.remaining(), AudioTrack.WRITE_NON_BLOCKING)
                    if (pcm.hasRemaining()) {
                        // A stalled output cannot retain unbounded audio or block the video worker.
                        while (pending.isNotEmpty() && pendingBytes + pcm.remaining() > 19200) {
                            pendingBytes -= pending.removeFirst().remaining()
                        }
                        if (pcm.remaining() <= 19200) {
                            val retained = ByteBuffer.allocateDirect(pcm.remaining()).apply { put(pcm); flip() }
                            pendingBytes += retained.remaining()
                            pending.addLast(retained)
                        }
                    }
                }
            } finally { codec.releaseOutputBuffer(output, false) }
        }
    }

    private fun drainPcm() {
        while (pending.isNotEmpty()) {
            val pcm = pending.first()
            val written = track.write(pcm, pcm.remaining(), AudioTrack.WRITE_NON_BLOCKING)
            if (written <= 0) return
            pendingBytes -= written
            if (pcm.hasRemaining()) return
            pending.removeFirst()
        }
    }

    fun close() {
        try { codec.stop() } finally {
            try { codec.release() } finally { track.release() }
        }
    }
}

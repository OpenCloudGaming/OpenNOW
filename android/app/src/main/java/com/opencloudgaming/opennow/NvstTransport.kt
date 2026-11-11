package com.opencloudgaming.opennow

import android.os.SystemClock
import androidx.annotation.Keep
import kotlinx.serialization.json.*
import org.webrtc.*
import java.nio.ByteBuffer
import java.net.URI
import java.util.concurrent.ArrayBlockingQueue
import java.util.concurrent.ThreadPoolExecutor
import java.util.concurrent.TimeUnit
import java.util.concurrent.atomic.AtomicInteger
import java.util.concurrent.atomic.AtomicLong
import kotlin.math.roundToInt

internal data class NvstNetworkMetrics(
    val jitterMs: Double?,
    val lossPct: Double?,
    val pingMs: Int?,
    val duplicateStunResponses: Long? = null,
)

internal fun parseNvstNetworkMetrics(detail: String): NvstNetworkMetrics {
    val values = detail.split(',')
    fun value(index: Int) = values.getOrNull(index)?.toDoubleOrNull()?.takeIf { it.isFinite() && it >= 0 }
    return NvstNetworkMetrics(
        value(0), value(1)?.takeIf { it <= 100 },
        value(2)?.takeIf { it <= Int.MAX_VALUE }?.roundToInt(),
        values.getOrNull(3)?.toLongOrNull()?.takeIf { it >= 0 },
    )
}

/** Allocation metadata cannot opt a user into experimental NVST, including on reconnect. */
internal fun SessionInfo.usesNvstTransport(settings: StreamSettings): Boolean =
    settings.experimentalNvst

/** A retained native-only allocation needs a new WebRTC allocation or an explicit opt-in. */
internal fun SessionInfo.nvstTransportOptInError(settings: StreamSettings): String? =
    if (nativeTransportRequired && !usesNvstTransport(settings)) {
        "This cloud session requires NVST, but NVST is disabled. Start a new session with Website client mode, or explicitly enable NVST in Settings. The cloud session is retained."
    } else null

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
        (get("appLevelProtocol") as? JsonPrimitive)?.contentOrNull in listOf("1", "6")
}

internal fun webRtcMediaFallbackConnections(connections: List<JsonObject>): List<JsonObject> =
    connections.filter {
        (it["usage"] as? JsonPrimitive)?.contentOrNull == "14" && !it.isRtspSessionConnection()
    }.sortedByDescending { (it["port"] as? JsonPrimitive)?.contentOrNull?.toIntOrNull() ?: 0 }

internal fun nvstRtspEndpoints(connections: JsonArray?): List<String> = connections.orEmpty().mapNotNull { raw ->
    val item = raw as? JsonObject ?: return@mapNotNull null
    fun value(key: String) = (item[key] as? JsonPrimitive)?.contentOrNull
    val usage = value("usage")?.toIntOrNull()
    val protocol = value("appLevelProtocol")?.toIntOrNull()
    // Keep the provider's order and exclude media even when its resource looks
    // like a signalling URL. Older responses may omit usage altogether.
    if (usage != null && usage != 16 && !(usage == 14 && protocol in listOf(1, 6))) return@mapNotNull null
    if (usage == null && !item.isRtspSessionConnection()) return@mapNotNull null
    val address = value("ip")?.trim()?.takeIf { it.isNotEmpty() }
        ?: value("resourcePath")?.trim()?.takeIf { it.isNotEmpty() } ?: return@mapNotNull null
    val scheme = if (protocol == 1) "rtsp" else "rtsps"
    val endpoint = runCatching { URI(if ("://" in address) address else {
        val host = if (address.count { it == ':' } > 1 && !address.startsWith("[")) "[$address]" else address
        "$scheme://$host"
    }) }.getOrNull() ?: return@mapNotNull null
    if (endpoint.scheme !in listOf("rtsp", "rtsps") || endpoint.rawUserInfo != null || endpoint.rawQuery != null || endpoint.rawFragment != null) return@mapNotNull null
    val host = endpoint.host?.takeIf { it.isNotBlank() } ?: return@mapNotNull null
    val port = value("port")?.toIntOrNull()?.takeIf { it in 1..65535 }
        ?: endpoint.port.takeIf { it in 1..65535 } ?: 322
    "${endpoint.scheme}://${if (':' in host && !host.startsWith("[")) "[$host]" else host}:$port"
}.distinct().take(20)

internal fun nvstSessionContext(session: SessionInfo, settings: StreamSettings): String = buildJsonObject {
    put("session", buildJsonObject {
        put("sessionId", session.sessionId)
        put("serverIp", session.serverIp)
        if (isAllianceCloudMatchBase(session.streamingBaseUrl)) put("alliancePartner", true)
        session.mediaConnectionInfo?.let { put("mediaConnectionInfo", OpenNowJson.encodeToJsonElement(it)) }
        session.bundleConnectionInfo?.let { bundle ->
            putJsonArray("connectionInfo") {
                add(buildJsonObject {
                    put("ip", bundle.ip)
                    put("port", bundle.port)
                    put("usage", 17)
                })
            }
        }
        put("rtspsEndpoints", JsonArray(session.rtspsEndpoints.map(::JsonPrimitive)))
    })
    put("settings", buildJsonObject {
        put("resolution", settings.resolution)
        put("fps", settings.fps)
        put("codec", settings.codec.name)
        put("colorQuality", OpenNowJson.encodeToJsonElement(settings.colorQuality))
        put("maxBitrateMbps", settings.maxBitrateMbps)
        put("networkAdaptation", buildJsonObject {
            val bitrate = StreamNetworkAdaptation.nvstBitrateRange(settings)
            put("dynamicStreamingMode", StreamNetworkAdaptation.dynamicStreamingMode(settings))
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

/** Video/negotiation and audio have independent workers; both finish before codec release. */
@Keep
internal class NvstTransport(
    context: android.content.Context,
    private val session: SessionInfo,
    private val settings: StreamSettings,
    private val lowLatencyGameAudio: Boolean,
    private val decoderFactory: VideoDecoderFactory,
    private val sink: () -> VideoSink?,
    private val event: (String, String) -> Unit,
    private val stats: (StreamRuntimeStats) -> Unit,
    private val rumble: (GamepadRumbleCommand) -> Unit,
    private val recordingAudio: (ByteBuffer) -> Unit = {},
    private val recordingMicrophone: (ShortArray) -> Unit = {},
) {
    private val handle = NvstBridge.create()
    @Volatile private var stopped = false
    @Volatile var inputReady = false
        private set
    @Volatile var muted = false
    private val microphone = MicrophonePcmCapture(context.applicationContext, settings.microphoneDeviceId,
        ready = { inputReady && !stopped },
        send = { samples, timestamp, restart ->
            recordingMicrophone(samples)
            NvstBridge.microphone(handle, samples, timestamp, restart)
        },
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
    @Volatile private var audio: NvstAudioOutput? = null
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
    private var decoderInput: NvstDecoderInput? = null
    private val decodeInfo = VideoDecoder.DecodeInfo(false, 0)
    private val decodedSize = DecodedResolutionTracker()
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
                microphone.start()
                NvstBridge.run(handle, nvstSessionContext(session, settings), this)
            } catch (error: Exception) {
                if (!stopped) {
                    NativeInputDiagnostics.addRetained("nvst.failure", boundedJavaCrashTrace(error))
                    event("error", nvstFailureDetail(error))
                }
            } finally {
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
        if (kind == "stage") NativeInputDiagnostics.add(detail)
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
            network.duplicateStunResponses?.let { duplicates ->
                NativeInputDiagnostics.retain(
                    "nvst.rtt",
                    "NVST ping source=ice-first-response duplicateResponsesIgnored=$duplicates",
                )
            }
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
            audioOperation {
                val output = audio ?: run {
                    android.os.Process.setThreadPriority(android.os.Process.THREAD_PRIORITY_AUDIO)
                    NvstAudioOutput(lowLatencyGameAudio, recordingAudio).also {
                        audio = it
                        NativeInputDiagnostics.addRetained("nvst.audio-worker", "NVST audio independentWorker=true pollMs=5 queuePackets=8")
                    }
                }
                output.feed(bytes, timestampNs / 1000, muted)
            }
            return true
        }
        receivedFrames.incrementAndGet()
        receivedBytes.addAndGet(bytes.remaining().toLong())
        if (!contiguous) needsKeyframe = true
        if (needsKeyframe && !keyframe) return false
        val activeDecoder = decoder ?: nvstDecoderOperation("initialization") {
            val created = decoderFactory.createDecoder(VideoCodecInfo(codec, emptyMap(), emptyList()))
                ?: error("No Android decoder for negotiated NVST codec $codec")
            decoder = created
            val (width, height) = initialDecoderSize
            val status = created.initDecode(VideoDecoder.Settings(2, width, height), VideoDecoder.Callback { frame, decodeMs, _ ->
                if (!stopped) {
                    decodedSize.observe(frame.rotatedWidth, frame.rotatedHeight)?.let { size ->
                        decodedResolution = "${size.width}x${size.height}"
                    }
                    decodeTimeMs = decodeMs?.toDouble()
                    decodedFrames.incrementAndGet()
                    lastDecodedAt.set(SystemClock.elapsedRealtime())
                    sink()?.onFrame(frame)
                    if (firstDecoded.compareAndSet(false, true)) event("streaming", "")
                }
            })
            check(status == VideoCodecStatus.OK) { "NVST decoder initialization failed: $status" }
            val borrowedInput = created.supportsBorrowedNvstInput()
            decoderInput = NvstDecoderInput(borrowedInput)
            NativeInputDiagnostics.addRetained("nvst.decoder-input",
                "NVST decoder input borrowed=$borrowedInput implementation=${created.implementationName}")
            codecName = codec
            created
        }
        val image = checkNotNull(decoderInput).frame(bytes, timestampNs, keyframe)
        return try {
            val accepted = nvstDecoderOperation("decode") {
                val status = activeDecoder.decode(image, decodeInfo)
                if (status != VideoCodecStatus.OK) NativeInputDiagnostics.retainThrottled("nvst.decoder-status", 1_000) {
                    "NVST decoder status=$status implementation=${activeDecoder.implementationName}"
                }
                nvstDecodeAccepted(status)
            }
            needsKeyframe = !accepted
            accepted
        } finally { image.release() }
    }

    @Keep fun onNativeAudioTick() {
        if (!stopped) audioOperation { audio?.drain() }
    }

    private inline fun audioOperation(block: () -> Unit) {
        try { block() } catch (error: Exception) {
            NativeInputDiagnostics.addRetained("nvst.audio-failure", boundedJavaCrashTrace(error))
            throw error
        }
    }
}

package com.opencloudgaming.opennow

import android.content.Context
import android.media.AudioAttributes
import android.media.MediaCodecInfo
import android.media.MediaCodecList
import android.opengl.GLES11Ext
import android.opengl.GLES20
import android.os.Build
import android.os.CombinedVibration
import android.os.SystemClock
import android.os.VibrationEffect
import android.os.Vibrator
import android.os.VibratorManager
import android.util.Log
import android.view.InputDevice
import android.view.KeyEvent
import android.view.MotionEvent
import kotlinx.coroutines.CoroutineScope
import kotlinx.coroutines.Dispatchers
import kotlinx.coroutines.Job
import kotlinx.coroutines.SupervisorJob
import kotlinx.coroutines.delay
import kotlinx.coroutines.launch
import kotlinx.coroutines.sync.Mutex
import kotlinx.coroutines.sync.withLock
import kotlinx.serialization.json.JsonObject
import kotlinx.serialization.json.buildJsonObject
import kotlinx.serialization.json.contentOrNull
import kotlinx.serialization.json.intOrNull
import kotlinx.serialization.json.jsonObject
import kotlinx.serialization.json.jsonPrimitive
import kotlinx.serialization.json.put
import okhttp3.OkHttpClient
import okhttp3.Request
import okhttp3.Response
import okhttp3.WebSocket
import okhttp3.WebSocketListener
import okio.ByteString
import org.webrtc.AudioTrack
import org.webrtc.DataChannel
import org.webrtc.DefaultVideoDecoderFactory
import org.webrtc.DefaultVideoEncoderFactory
import org.webrtc.EglBase
import org.webrtc.GlShader
import org.webrtc.GlUtil
import org.webrtc.HardwareVideoDecoderFactory
import org.webrtc.IceCandidate
import org.webrtc.MediaConstraints
import org.webrtc.MediaStream
import org.webrtc.MediaStreamTrack
import org.webrtc.PeerConnection
import org.webrtc.PeerConnectionFactory
import org.webrtc.Predicate
import org.webrtc.RTCStats
import org.webrtc.RTCStatsCollectorCallback
import org.webrtc.RendererCommon
import org.webrtc.RtpCapabilities
import org.webrtc.RtpReceiver
import org.webrtc.RtpTransceiver
import org.webrtc.SdpObserver
import org.webrtc.SessionDescription
import org.webrtc.SurfaceViewRenderer
import org.webrtc.VideoCodecInfo
import org.webrtc.VideoDecoder
import org.webrtc.VideoDecoderFactory
import org.webrtc.VideoTrack
import org.webrtc.audio.AudioDeviceModule
import org.webrtc.audio.JavaAudioDeviceModule
import java.nio.ByteBuffer
import java.nio.ByteOrder
import java.nio.FloatBuffer
import java.security.SecureRandom
import java.util.Locale
import kotlin.math.abs
import kotlin.math.max
import kotlin.math.min
import kotlin.math.roundToInt
import kotlin.math.sqrt

object NativeCodecProbe {
    init {
        runCatching { System.loadLibrary("opennow_native") }
    }

    external fun nativeRuntimeSummary(): String
    external fun nativeDecoderAvailable(mimeType: String): Boolean
}

private object WebRtcRuntime {
    @Volatile
    private var initialized = false

    fun ensureInitialized(context: Context) {
        if (initialized) return
        synchronized(this) {
            if (initialized) return
            PeerConnectionFactory.initialize(
                PeerConnectionFactory.InitializationOptions.builder(context.applicationContext)
                    .setEnableInternalTracer(false)
                    .createInitializationOptions(),
            )
            initialized = true
        }
    }
}

object CodecProbe {
    fun report(context: Context): RuntimeCodecReport {
        WebRtcRuntime.ensureInitialized(context)
        val packageManager = context.packageManager
        val isTv = packageManager.hasSystemFeature("android.software.leanback")
        val renderer = listOf(Build.HARDWARE, Build.BOARD, Build.DEVICE, Build.MODEL, Build.MANUFACTURER)
            .joinToString(" ")
            .lowercase(Locale.US)
        val lowPower = renderer.contains("powervr") || renderer.contains("ge8320") || renderer.contains("ge83")
        val webRtcDecoders = probeWebRtcDecoders()
        val capabilities = VideoCodec.entries.map { codec ->
            val mime = codec.mimeType()
            val decoders = codecInfos(mime, encoder = false)
            val encoders = codecInfos(mime, encoder = true)
            val webRtc = webRtcDecoders[codec]
            val nativeDecoderAvailable = runCatching { NativeCodecProbe.nativeDecoderAvailable(mime) }.getOrNull()
            val preferredDecoder = decoders.firstOrNull(::isHardwareCodec) ?: decoders.firstOrNull()
            val preferredEncoder = encoders.firstOrNull(::isHardwareCodec) ?: encoders.firstOrNull()
            CodecCapability(
                codec = codec,
                decoderAvailable = decoders.isNotEmpty(),
                encoderAvailable = encoders.isNotEmpty(),
                hardwareDecoder = decoders.any(::isHardwareCodec),
                hardwareEncoder = encoders.any(::isHardwareCodec),
                decoderName = preferredDecoder?.name,
                encoderName = preferredEncoder?.name,
                realtimeSafe = decoders.any { isRealtimeSafeDecoder(codec, it) },
                nativeDecoderAvailable = nativeDecoderAvailable,
                webRtcDecoderAvailable = webRtc?.decoderAvailable,
                webRtcHardwareDecoderAvailable = webRtc?.hardwareDecoderAvailable,
                webRtcDecoderName = webRtc?.decoderName,
                webRtcCodecProfiles = webRtc?.profiles.orEmpty(),
            )
        }
        return RuntimeCodecReport(
            capabilities = capabilities,
            nativeRuntimeSummary = runCatching { NativeCodecProbe.nativeRuntimeSummary() }.getOrElse { "{\"nativeLibrary\":\"unavailable\"}" },
            androidTvProfile = isTv,
            lowPowerGpuProfile = lowPower,
        )
    }

    private fun codecInfos(mime: String, encoder: Boolean): List<MediaCodecInfo> {
        val list = if (Build.VERSION.SDK_INT >= 21) {
            MediaCodecList(MediaCodecList.ALL_CODECS).codecInfos.toList()
        } else {
            emptyList()
        }
        return list.filter { info ->
            info.isEncoder == encoder && info.supportedTypes.any { it.equals(mime, ignoreCase = true) }
        }
    }

    private data class WebRtcCodecProbe(
        val decoderAvailable: Boolean,
        val hardwareDecoderAvailable: Boolean,
        val decoderName: String?,
        val profiles: List<String>,
    )

    private fun probeWebRtcDecoders(): Map<VideoCodec, WebRtcCodecProbe> {
        val eglBase = runCatching { EglBase.create() }.getOrNull() ?: return emptyMap()
        return try {
            val streamingFactory = OpenNowVideoDecoderFactory(eglBase.eglBaseContext)
            val hardwareFactory = openNowHardwareVideoDecoderFactory(eglBase.eglBaseContext)
            val streamingSupported = streamingFactory.supportedCodecsByVideoCodec()
            val hardwareSupported = hardwareFactory.supportedCodecsByVideoCodec()
            VideoCodec.entries.associateWith { codec ->
                val defaultInfos = streamingSupported[codec].orEmpty()
                val hardwareInfos = hardwareSupported[codec].orEmpty()
                val decoderName = streamingFactory.firstDecoderName(defaultInfos)
                WebRtcCodecProbe(
                    decoderAvailable = decoderName != null,
                    hardwareDecoderAvailable = hardwareFactory.firstDecoderName(hardwareInfos) != null,
                    decoderName = decoderName,
                    profiles = defaultInfos.map(::formatWebRtcCodecInfo).distinct(),
                )
            }
        } catch (_: Throwable) {
            emptyMap()
        } finally {
            eglBase.release()
        }
    }

    private fun VideoDecoderFactory.supportedCodecsByVideoCodec(): Map<VideoCodec, List<VideoCodecInfo>> =
        getSupportedCodecs()
            .groupBy { info -> info.name.toVideoCodec() }
            .mapNotNull { (codec, infos) -> codec?.let { it to infos } }
            .toMap()

    private fun VideoDecoderFactory.firstDecoderName(infos: List<VideoCodecInfo>): String? {
        for (info in infos) {
            val decoder = runCatching { createDecoder(info) }.getOrNull() ?: continue
            return try {
                decoder.getImplementationName()
            } finally {
                runCatching { decoder.release() }
            }
        }
        return null
    }

    private fun String.toVideoCodec(): VideoCodec? =
        when (uppercase(Locale.US)) {
            "AVC", "H264", "H.264" -> VideoCodec.H264
            "HEVC", "H265", "H.265" -> VideoCodec.H265
            "AV01", "AV1" -> VideoCodec.AV1
            else -> null
        }

    private fun formatWebRtcCodecInfo(info: VideoCodecInfo): String {
        val profile = info.params["profile-level-id"]
        val packetization = info.params["packetization-mode"]
        return listOfNotNull(info.name.toVideoCodec()?.name ?: info.name, profile?.let { "profile=$it" }, packetization?.let { "packet=$it" })
            .joinToString(" ")
    }

    private fun isHardwareCodec(info: MediaCodecInfo): Boolean {
        val name = info.name.lowercase(Locale.US)
        if (name.contains("google") || name.contains("sw") || name.contains("software")) return false
        return if (Build.VERSION.SDK_INT >= 29) {
            info.isHardwareAccelerated
        } else {
            true
        }
    }

    internal fun isOpenNowHardwareDecoderAllowed(info: MediaCodecInfo): Boolean {
        if (!isHardwareCodec(info)) return false
        val name = info.name.lowercase(Locale.US)
        if (name.contains("google") || name.contains("software") || name.contains("sw")) return false
        if (name.contains("exynos")) return false
        return true
    }

    private fun isRealtimeSafeDecoder(codec: VideoCodec, info: MediaCodecInfo): Boolean {
        if (!isHardwareCodec(info)) return false
        val name = info.name.lowercase(Locale.US)
        return when (codec) {
            VideoCodec.H264 -> true
            // Android WebRTC HEVC/AV1 decode is still device-fragile here. Exynos HEVC black-screens
            // and Google AV1 falls back to a laggy software path even when the codec list advertises it.
            VideoCodec.H265 -> !name.contains("exynos")
            VideoCodec.AV1 -> !name.contains("google")
        }
    }

    private fun VideoCodec.mimeType(): String =
        when (this) {
            VideoCodec.H264 -> "video/avc"
            VideoCodec.H265 -> "video/hevc"
            VideoCodec.AV1 -> "video/av01"
        }
}

private fun openNowHardwareVideoDecoderFactory(sharedContext: EglBase.Context): VideoDecoderFactory =
    HardwareVideoDecoderFactory(
        sharedContext,
        Predicate<MediaCodecInfo> { info -> CodecProbe.isOpenNowHardwareDecoderAllowed(info) },
    )

private class OpenNowVideoDecoderFactory(sharedContext: EglBase.Context) : VideoDecoderFactory {
    private val defaultFactory = DefaultVideoDecoderFactory(sharedContext)
    private val hardwareFactory = openNowHardwareVideoDecoderFactory(sharedContext)

    override fun createDecoder(info: VideoCodecInfo): VideoDecoder? {
        val codec = info.name.toOpenNowVideoCodec()
        val decoder = if (codec == VideoCodec.H265 || codec == VideoCodec.AV1) {
            hardwareFactory.createDecoder(info)
        } else {
            defaultFactory.createDecoder(info)
        }
        if ((codec == VideoCodec.H265 || codec == VideoCodec.AV1) && decoder != null) {
            NativeInputDiagnostics.add("native MediaCodec decoder selected codec=${codec.name} implementation=${decoder.getImplementationName()}")
        }
        return decoder
    }

    override fun getSupportedCodecs(): Array<VideoCodecInfo> {
        val defaultCodecs = defaultFactory.getSupportedCodecs()
            .filterNot { it.name.toOpenNowVideoCodec() in ADVANCED_STREAM_CODECS }
        val nativeAdvancedCodecs = hardwareFactory.getSupportedCodecs()
            .filter { it.name.toOpenNowVideoCodec() in ADVANCED_STREAM_CODECS }
        return (defaultCodecs + nativeAdvancedCodecs)
            .distinctBy { it.stableKey() }
            .toTypedArray()
    }

    private fun VideoCodecInfo.stableKey(): String =
        "${name.uppercase(Locale.US)}:${params.toSortedMap()}"

    private companion object {
        private val ADVANCED_STREAM_CODECS = setOf(VideoCodec.H265, VideoCodec.AV1)
    }
}

private fun String.toOpenNowVideoCodec(): VideoCodec? =
    when (uppercase(Locale.US)) {
        "AVC", "H264", "H.264" -> VideoCodec.H264
        "HEVC", "H265", "H.265" -> VideoCodec.H265
        "AV01", "AV1" -> VideoCodec.AV1
        else -> null
    }

private fun VideoCodec.webRtcCodecName(): String =
    when (this) {
        VideoCodec.H264 -> "H264"
        VideoCodec.H265 -> "H265"
        VideoCodec.AV1 -> "AV1"
    }

private fun RtpCapabilities.CodecCapability.openNowCodecName(): String? {
    val fromMime = mimeType
        ?.substringAfter("/", "")
        ?.takeIf { it.isNotBlank() }
        ?.toOpenNowVideoCodec()
        ?.webRtcCodecName()
    if (fromMime != null) return fromMime
    return name?.toOpenNowVideoCodec()?.webRtcCodecName() ?: name?.uppercase(Locale.US)
}

private fun RtpCapabilities.CodecCapability.codecParameterInt(name: String): Int? =
    parameters
        ?.entries
        ?.firstOrNull { it.key.equals(name, ignoreCase = true) }
        ?.value
        ?.toIntOrNull()

private fun RtpCapabilities.CodecCapability.h265ProfilePriority(preferTenBit: Boolean): Int {
    val profile = codecParameterInt("profile-id")
    return if (preferTenBit) {
        when (profile) {
            2 -> 0
            1 -> 1
            else -> 2
        }
    } else {
        when (profile) {
            1 -> 0
            null -> 1
            2 -> 2
            else -> 3
        }
    }
}

private fun RtpCapabilities.CodecCapability.preferenceKey(): String =
    "${openNowCodecName().orEmpty()}:${parameters.orEmpty().toSortedMap()}"

private fun StreamSettings.prefersTenBitVideo(): Boolean =
    hdrEnabled ||
        colorQuality == ColorQuality.TenBit420 ||
        colorQuality == ColorQuality.TenBit444

private val WEBRTC_AUXILIARY_VIDEO_CODECS = setOf("RTX", "RED", "ULPFEC", "FLEXFEC-03")

sealed interface SignalingEvent {
    data object Connected : SignalingEvent
    data class Disconnected(val reason: String) : SignalingEvent
    data class Offer(val sdp: String) : SignalingEvent
    data class RemoteIce(val candidate: IceCandidate) : SignalingEvent
    data class Error(val message: String) : SignalingEvent
    data class Log(val message: String) : SignalingEvent
}

class GfnSignalingClient(
    private val session: SessionInfo,
    private val settings: StreamSettings,
    private val http: OkHttpClient = defaultHttpClient(),
    private val onEvent: (SignalingEvent) -> Unit,
) {
    private var webSocket: WebSocket? = null
    private val scope = CoroutineScope(SupervisorJob() + Dispatchers.IO)
    private var heartbeatJob: Job? = null
    private var peerId = 0
    private var remotePeerId = 1
    private val peerName = "peer-${java.util.UUID.randomUUID().toString().replace("-", "").take(12)}"
    private var ackCounter = 0

    fun connect() {
        val url = buildSignInUrl()
        val host = url.removePrefix("wss://").substringBefore("/")
        val request = Request.Builder()
            .url(url)
            .header("Sec-WebSocket-Protocol", "x-nv-sessionid.${session.sessionId}")
            .header("Host", host)
            .header("Origin", "https://play.geforcenow.com")
            .header("User-Agent", "Mozilla/5.0 (Windows NT 10.0; Win64; x64) Chrome/131.0.0.0 Safari/537.36")
            .build()
        webSocket = http.newWebSocket(
            request,
            object : WebSocketListener() {
                override fun onOpen(webSocket: WebSocket, response: Response) {
                    sendPeerInfo()
                    heartbeatJob?.cancel()
                    heartbeatJob = scope.launch {
                        while (true) {
                            delay(5000)
                            sendJson("""{"hb":1}""")
                        }
                    }
                    onEvent(SignalingEvent.Connected)
                }

                override fun onMessage(webSocket: WebSocket, text: String) = handleMessage(text)
                override fun onMessage(webSocket: WebSocket, bytes: ByteString) = handleMessage(bytes.utf8())

                override fun onClosed(webSocket: WebSocket, code: Int, reason: String) {
                    heartbeatJob?.cancel()
                    onEvent(SignalingEvent.Disconnected(reason.ifBlank { "socket closed" }))
                }

                override fun onFailure(webSocket: WebSocket, t: Throwable, response: Response?) {
                    heartbeatJob?.cancel()
                    onEvent(SignalingEvent.Error(t.message ?: "Signaling failed"))
                }
            },
        )
    }

    fun sendAnswer(sdp: String, nvstSdp: String?) {
        val msg = buildJsonObject {
            put("type", "answer")
            put("sdp", sdp)
            if (nvstSdp != null) put("nvstSdp", nvstSdp)
        }.toString()
        sendPeerMessage(msg)
    }

    fun sendIceCandidate(candidate: IceCandidate) {
        if (candidate.sdp.contains(" tcp ", ignoreCase = true)) return
        val msg = buildJsonObject {
            put("candidate", candidate.sdp)
            put("sdpMid", candidate.sdpMid)
            put("sdpMLineIndex", candidate.sdpMLineIndex)
        }.toString()
        sendPeerMessage(msg)
    }

    fun requestKeyframe(reason: String, backlogFrames: Int, attempt: Int) {
        val msg = buildJsonObject {
            put("type", "request_keyframe")
            put("reason", reason)
            put("backlogFrames", backlogFrames)
            put("attempt", attempt)
        }.toString()
        sendPeerMessage(msg)
    }

    fun disconnect() {
        heartbeatJob?.cancel()
        webSocket?.close(1000, "closed")
        webSocket = null
    }

    private fun buildSignInUrl(): String {
        val base = session.signalingUrl.ifBlank {
            val host = if (session.signalingServer.contains(":")) session.signalingServer else "${session.signalingServer}:443"
            "wss://$host/nvst/"
        }
        val normalized = base.replace("wss://", "").trimEnd('/')
        return "wss://$normalized/sign_in?peer_id=$peerName&version=2&peer_role=1&pairing_id=${session.sessionId}"
    }

    private fun handleMessage(text: String) {
        val parsed = runCatching { OpenNowJson.parseToJsonElement(text).jsonObject }.getOrNull()
        if (parsed == null) {
            onEvent(SignalingEvent.Log("Ignoring non-JSON signaling packet"))
            return
        }
        parsed["peer_info"]?.jsonObject?.let { info ->
            if (info["name"]?.jsonPrimitive?.contentOrNull == peerName) {
                peerId = info["id"]?.jsonPrimitive?.intOrNull ?: peerId
            }
        }
        parsed["ackid"]?.jsonPrimitive?.intOrNull?.let { ack ->
            val shouldAck = parsed["peer_info"]?.jsonObject?.get("id")?.jsonPrimitive?.intOrNull != peerId
            if (shouldAck) sendJson("""{"ack":$ack}""")
        }
        if (parsed["hb"] != null) {
            sendJson("""{"hb":1}""")
            return
        }
        val peerMsg = parsed["peer_msg"]?.jsonObject ?: return
        remotePeerId = peerMsg["from"]?.jsonPrimitive?.intOrNull ?: remotePeerId
        val msg = peerMsg["msg"]?.jsonPrimitive?.contentOrNull ?: return
        val payload = runCatching { OpenNowJson.parseToJsonElement(msg).jsonObject }.getOrNull() ?: return
        when {
            payload["type"]?.jsonPrimitive?.contentOrNull == "offer" -> {
                val sdp = payload["sdp"]?.jsonPrimitive?.contentOrNull
                if (sdp != null) onEvent(SignalingEvent.Offer(sdp))
            }
            payload["candidate"]?.jsonPrimitive?.contentOrNull != null -> {
                onEvent(
                    SignalingEvent.RemoteIce(
                        IceCandidate(
                            payload["sdpMid"]?.jsonPrimitive?.contentOrNull,
                            payload["sdpMLineIndex"]?.jsonPrimitive?.intOrNull ?: 0,
                            payload["candidate"]?.jsonPrimitive?.contentOrNull.orEmpty(),
                        ),
                    ),
                )
            }
        }
    }

    private fun sendPeerInfo() {
        val (width, height) = streamResolutionPixels(settings)
        sendJson(
            """
            {"ackid":${nextAckId()},"peer_info":{"browser":"Chrome","browserVersion":"131","connected":true,"id":$peerId,"name":"$peerName","peerRole":0,"resolution":"${width}x$height","version":2}}
            """.trimIndent(),
        )
    }

    private fun sendPeerMessage(message: String) {
        val escaped = message.replace("\\", "\\\\").replace("\"", "\\\"")
        sendJson("""{"peer_msg":{"from":$peerId,"to":$remotePeerId,"msg":"$escaped"},"ackid":${nextAckId()}}""")
    }

    private fun sendJson(text: String) {
        webSocket?.send(text)
    }

    private fun nextAckId(): Int {
        ackCounter += 1
        return ackCounter
    }
}

object NativeStreamInputRouter {
    @Volatile
    private var client: NativeStreamClient? = null
    @Volatile
    private var touchMouseEnabled = false
    @Volatile
    private var captureAllTouch = false
    @Volatile
    private var systemMenuHandler: (() -> Unit)? = null
    @Volatile
    private var systemBackHandler: (() -> Unit)? = null
    @Volatile
    private var streamUiActive = false
    @Volatile
    private var streamChromePassthroughBounds: TouchPassthroughBounds? = null
    @Volatile
    private var streamPanelPassthroughBounds: TouchPassthroughBounds? = null
    @Volatile
    private var touchControllerPassthroughBounds: Map<String, TouchPassthroughBounds> = emptyMap()
    @Volatile
    private var touchControllerVisible = false
    @Volatile
    private var uiTouchPassthroughActive = false
    private val nativeUiTouchPointerIds = mutableSetOf<Int>()
    private val touchMouseState = TouchMouseState()

    fun attach(next: NativeStreamClient) {
        client = next
    }

    fun detach(next: NativeStreamClient) {
        if (client === next) {
            client = null
        }
    }

    fun setTouchMouseEnabled(enabled: Boolean) {
        touchMouseEnabled = enabled
        if (!enabled) {
            touchMouseState.reset(client)
        }
    }

    fun setCaptureAllTouch(enabled: Boolean) {
        captureAllTouch = enabled
    }

    fun setSystemMenuHandler(handler: (() -> Unit)?) {
        systemMenuHandler = handler
    }

    fun setSystemBackHandler(handler: (() -> Unit)?) {
        systemBackHandler = handler
    }

    fun setStreamUiActive(active: Boolean) {
        streamUiActive = active
    }

    fun normalizedStreamUiKeyCode(event: KeyEvent): Int? {
        if (!streamUiActive) return null
        return when (event.keyCode) {
            KeyEvent.KEYCODE_BUTTON_A -> KeyEvent.KEYCODE_DPAD_CENTER
            KeyEvent.KEYCODE_BUTTON_B,
            KeyEvent.KEYCODE_BUTTON_SELECT -> KeyEvent.KEYCODE_BACK
            else -> null
        }
    }

    fun normalizedAppUiKeyCode(event: KeyEvent): Int? {
        return normalizedAppUiKeyCode(event.keyCode, streamUiActive)
    }

    fun normalizedAppUiKeyCode(keyCode: Int, streamUiActive: Boolean): Int? {
        if (streamUiActive) return null
        return when (keyCode) {
            KeyEvent.KEYCODE_BUTTON_A -> KeyEvent.KEYCODE_DPAD_CENTER
            else -> null
        }
    }

    fun isControllerAppBackKey(event: KeyEvent): Boolean =
        isControllerAppBackKey(
            keyCode = event.keyCode,
            controllerSource = event.isControllerInputDevice(),
            streamUiActive = streamUiActive,
        )

    fun isControllerAppBackKey(keyCode: Int, controllerSource: Boolean, streamUiActive: Boolean): Boolean =
        !streamUiActive &&
            (keyCode == KeyEvent.KEYCODE_BUTTON_B ||
                (keyCode == KeyEvent.KEYCODE_BACK && controllerSource))

    fun setUiTouchPassthroughBounds(left: Int, top: Int, right: Int, bottom: Int) {
        streamChromePassthroughBounds = TouchPassthroughBounds(left, top, right, bottom)
    }

    fun clearUiTouchPassthroughBounds() {
        streamChromePassthroughBounds = null
        uiTouchPassthroughActive = false
        nativeUiTouchPointerIds.clear()
    }

    fun setStreamPanelTouchPassthroughBounds(left: Int, top: Int, right: Int, bottom: Int) {
        streamPanelPassthroughBounds = TouchPassthroughBounds(left, top, right, bottom)
    }

    fun clearStreamPanelTouchPassthroughBounds() {
        streamPanelPassthroughBounds = null
        uiTouchPassthroughActive = false
        nativeUiTouchPointerIds.clear()
    }

    fun setTouchControllerPassthroughBounds(left: Int, top: Int, right: Int, bottom: Int) {
        touchControllerPassthroughBounds = mapOf("default" to TouchPassthroughBounds(left, top, right, bottom))
    }

    fun setTouchControllerPassthroughBound(id: String, left: Int, top: Int, right: Int, bottom: Int) {
        touchControllerPassthroughBounds = touchControllerPassthroughBounds.toMutableMap().also {
            it[id] = TouchPassthroughBounds(left, top, right, bottom)
        }
    }

    fun clearTouchControllerPassthroughBound(id: String) {
        if (id !in touchControllerPassthroughBounds) return
        touchControllerPassthroughBounds = touchControllerPassthroughBounds.toMutableMap().also { it.remove(id) }
    }

    fun setTouchControllerVisible(visible: Boolean) {
        touchControllerVisible = visible
        if (!visible) {
            touchControllerPassthroughBounds = emptyMap()
            uiTouchPassthroughActive = false
            nativeUiTouchPointerIds.clear()
        }
    }

    fun clearTouchControllerPassthroughBounds() {
        touchControllerPassthroughBounds = emptyMap()
        touchControllerVisible = false
        uiTouchPassthroughActive = false
        nativeUiTouchPointerIds.clear()
    }

    fun cancelTouchMouse() {
        touchMouseState.reset(client)
    }

    fun isNativeUiTouchGestureActive(): Boolean =
        nativeUiTouchPointerIds.isNotEmpty()

    fun shouldForwardTouchBeforeViews(event: MotionEvent, width: Int, height: Int): Boolean {
        if (
            client == null ||
            streamUiActive ||
            !touchMouseEnabled ||
            !captureAllTouch ||
            width <= 0 ||
            height <= 0 ||
            !event.isFingerTouchEvent()
        ) {
            return false
        }
        updateNativeUiTouchPointers(event, width, height)
        if (!eventHasStreamTouchPointer(event, width, height)) return false
        return event.pointerCount == 1 || nativeUiTouchPointerIds.isNotEmpty()
    }

    fun shouldCaptureTouchBeforeViews(event: MotionEvent, width: Int, height: Int): Boolean =
        shouldForwardTouchBeforeViews(event, width, height) &&
            nativeUiTouchPointerIds.isEmpty()

    fun dispatchTouch(event: MotionEvent, width: Int, height: Int): Boolean {
        val current = client ?: return false
        if (streamUiActive) return false
        if (!event.isFingerTouchEvent()) return false
        updateNativeUiTouchPointers(event, width, height)
        return touchMouseState.handle(
            event = event,
            enabled = touchMouseEnabled && width > 0 && height > 0,
            client = current,
            ignoredPointerIds = nativeUiTouchPointerIds,
        )
    }

    fun dispatchExternalMouseTouch(event: MotionEvent, width: Int, height: Int): Boolean {
        if (streamUiActive) return false
        if (!event.isExternalMousePointerEvent()) return false
        if (shouldPassTouchToNativeUi(event, width, height)) return false
        return client?.dispatchMotion(event) == true
    }

    fun dispatchKey(event: KeyEvent): Boolean {
        if (event.action == KeyEvent.ACTION_DOWN && event.repeatCount == 0 && event.isStreamSystemMenuKey()) {
            systemMenuHandler?.invoke()
            return systemMenuHandler != null
        }
        if (event.action == KeyEvent.ACTION_DOWN && event.repeatCount == 1 && event.isStreamLongStartShortcutKey()) {
            client?.releaseGamepadButtonForShortcut(event)
            systemMenuHandler?.invoke()
            return systemMenuHandler != null
        }
        if (event.action == KeyEvent.ACTION_DOWN && event.repeatCount == 0 && event.isStreamControlsShortcutKey()) {
            systemMenuHandler?.invoke()
            return systemMenuHandler != null
        }
        if (event.action == KeyEvent.ACTION_DOWN && event.repeatCount == 0 && event.isStreamExitShortcutKey()) {
            systemBackHandler?.invoke()
            return systemBackHandler != null
        }
        if (event.action == KeyEvent.ACTION_UP && event.isStreamExitShortcutKey()) {
            return systemBackHandler != null
        }
        if (streamUiActive) return false
        val current = client ?: return false
        if (event.shouldConsumeAsStreamKeyboard()) {
            current.dispatchKey(event)
            return true
        }
        return current.dispatchKey(event)
    }

    fun dispatchMotion(event: MotionEvent): Boolean {
        if (streamUiActive && event.isExternalMousePointerEvent()) {
            return false
        }
        if (streamUiActive && event.isNativeUiNavigationMotion()) {
            return false
        }
        return client?.dispatchMotion(event) == true
    }

    private fun KeyEvent.isStreamSystemMenuKey(): Boolean =
        keyCode == KeyEvent.KEYCODE_MENU

    private fun KeyEvent.isStreamLongStartShortcutKey(): Boolean =
        !streamUiActive &&
            keyCode == KeyEvent.KEYCODE_BUTTON_START &&
            (isControllerSource() || GamepadButtonMapping.isControllerButtonKeyCode(keyCode))

    private fun KeyEvent.isStreamControlsShortcutKey(): Boolean =
        !streamUiActive &&
            !isControllerInputDevice() &&
            !isHardwareKeyboardSource() &&
            isDpadSource() &&
            (keyCode == KeyEvent.KEYCODE_ENTER ||
                keyCode == KeyEvent.KEYCODE_NUMPAD_ENTER ||
                keyCode == KeyEvent.KEYCODE_DPAD_CENTER)

    private fun KeyEvent.isStreamExitShortcutKey(): Boolean =
        keyCode == KeyEvent.KEYCODE_BACK ||
            ((keyCode == KeyEvent.KEYCODE_BUTTON_B || keyCode == KeyEvent.KEYCODE_BUTTON_SELECT) && !isControllerInputDevice()) ||
            (keyCode == KeyEvent.KEYCODE_ESCAPE && !isHardwareKeyboardSource())

    private fun KeyEvent.isControllerSource(): Boolean =
        AndroidControllerInput.hasControllerSource(source)

    private fun KeyEvent.isControllerInputDevice(): Boolean =
        AndroidControllerInput.isControllerEvent(source, deviceId)

    private fun KeyEvent.isDpadSource(): Boolean =
        (source and InputDevice.SOURCE_DPAD) == InputDevice.SOURCE_DPAD

    private fun KeyEvent.isHardwareKeyboardSource(): Boolean =
        !isControllerInputDevice() &&
            ((source and InputDevice.SOURCE_KEYBOARD) == InputDevice.SOURCE_KEYBOARD ||
                InputDevice.getDevice(deviceId)?.keyboardType == InputDevice.KEYBOARD_TYPE_ALPHABETIC)

    private fun KeyEvent.shouldConsumeAsStreamKeyboard(): Boolean =
        (action == KeyEvent.ACTION_DOWN || action == KeyEvent.ACTION_UP) &&
            !isControllerInputDevice() &&
            !isAndroidSystemKey() &&
            (isHardwareKeyboardSource() || keyCode.isTextEntryKeyCode())

    private fun KeyEvent.isAndroidSystemKey(): Boolean =
        keyCode == KeyEvent.KEYCODE_VOLUME_UP ||
            keyCode == KeyEvent.KEYCODE_VOLUME_DOWN ||
            keyCode == KeyEvent.KEYCODE_VOLUME_MUTE ||
            keyCode == KeyEvent.KEYCODE_POWER ||
            keyCode == KeyEvent.KEYCODE_HOME

    private fun Int.isKeyboardLikeKeyCode(): Boolean =
        this == KeyEvent.KEYCODE_ENTER ||
            this == KeyEvent.KEYCODE_NUMPAD_ENTER ||
            this == KeyEvent.KEYCODE_ESCAPE ||
            this == KeyEvent.KEYCODE_DEL ||
            this == KeyEvent.KEYCODE_TAB ||
            this == KeyEvent.KEYCODE_SPACE ||
            this == KeyEvent.KEYCODE_DPAD_LEFT ||
            this == KeyEvent.KEYCODE_DPAD_UP ||
            this == KeyEvent.KEYCODE_DPAD_RIGHT ||
            this == KeyEvent.KEYCODE_DPAD_DOWN ||
            this == KeyEvent.KEYCODE_PAGE_UP ||
            this == KeyEvent.KEYCODE_PAGE_DOWN ||
            this == KeyEvent.KEYCODE_FORWARD_DEL ||
            this == KeyEvent.KEYCODE_INSERT ||
            this == KeyEvent.KEYCODE_MOVE_HOME ||
            this == KeyEvent.KEYCODE_MOVE_END ||
            this == KeyEvent.KEYCODE_SHIFT_LEFT ||
            this == KeyEvent.KEYCODE_SHIFT_RIGHT ||
            this == KeyEvent.KEYCODE_CTRL_LEFT ||
            this == KeyEvent.KEYCODE_CTRL_RIGHT ||
            this == KeyEvent.KEYCODE_ALT_LEFT ||
            this == KeyEvent.KEYCODE_ALT_RIGHT ||
            this == KeyEvent.KEYCODE_CAPS_LOCK ||
            this == KeyEvent.KEYCODE_NUM_LOCK ||
            this == KeyEvent.KEYCODE_SCROLL_LOCK ||
            this == KeyEvent.KEYCODE_MINUS ||
            this == KeyEvent.KEYCODE_EQUALS ||
            this == KeyEvent.KEYCODE_LEFT_BRACKET ||
            this == KeyEvent.KEYCODE_RIGHT_BRACKET ||
            this == KeyEvent.KEYCODE_BACKSLASH ||
            this == KeyEvent.KEYCODE_SEMICOLON ||
            this == KeyEvent.KEYCODE_APOSTROPHE ||
            this == KeyEvent.KEYCODE_COMMA ||
            this == KeyEvent.KEYCODE_PERIOD ||
            this == KeyEvent.KEYCODE_SLASH ||
            this == KeyEvent.KEYCODE_GRAVE ||
            this in KeyEvent.KEYCODE_A..KeyEvent.KEYCODE_Z ||
            this in KeyEvent.KEYCODE_0..KeyEvent.KEYCODE_9 ||
            this in KeyEvent.KEYCODE_NUMPAD_0..KeyEvent.KEYCODE_NUMPAD_9 ||
            this in KeyEvent.KEYCODE_F1..KeyEvent.KEYCODE_F12

    private fun Int.isTextEntryKeyCode(): Boolean =
        this == KeyEvent.KEYCODE_ENTER ||
            this == KeyEvent.KEYCODE_NUMPAD_ENTER ||
            this == KeyEvent.KEYCODE_ESCAPE ||
            this == KeyEvent.KEYCODE_DEL ||
            this == KeyEvent.KEYCODE_TAB ||
            this == KeyEvent.KEYCODE_SPACE ||
            this == KeyEvent.KEYCODE_FORWARD_DEL ||
            this == KeyEvent.KEYCODE_SHIFT_LEFT ||
            this == KeyEvent.KEYCODE_SHIFT_RIGHT ||
            this == KeyEvent.KEYCODE_CTRL_LEFT ||
            this == KeyEvent.KEYCODE_CTRL_RIGHT ||
            this == KeyEvent.KEYCODE_ALT_LEFT ||
            this == KeyEvent.KEYCODE_ALT_RIGHT ||
            this == KeyEvent.KEYCODE_CAPS_LOCK ||
            this == KeyEvent.KEYCODE_MINUS ||
            this == KeyEvent.KEYCODE_EQUALS ||
            this == KeyEvent.KEYCODE_LEFT_BRACKET ||
            this == KeyEvent.KEYCODE_RIGHT_BRACKET ||
            this == KeyEvent.KEYCODE_BACKSLASH ||
            this == KeyEvent.KEYCODE_SEMICOLON ||
            this == KeyEvent.KEYCODE_APOSTROPHE ||
            this == KeyEvent.KEYCODE_COMMA ||
            this == KeyEvent.KEYCODE_PERIOD ||
            this == KeyEvent.KEYCODE_SLASH ||
            this == KeyEvent.KEYCODE_GRAVE ||
            this in KeyEvent.KEYCODE_A..KeyEvent.KEYCODE_Z ||
            this in KeyEvent.KEYCODE_0..KeyEvent.KEYCODE_9 ||
            this in KeyEvent.KEYCODE_NUMPAD_0..KeyEvent.KEYCODE_NUMPAD_9 ||
            this in KeyEvent.KEYCODE_F1..KeyEvent.KEYCODE_F12

    private fun MotionEvent.isNativeUiNavigationMotion(): Boolean =
        isFromSource(InputDevice.SOURCE_JOYSTICK) ||
            isFromSource(InputDevice.SOURCE_GAMEPAD) ||
            AndroidControllerInput.isControllerEvent(source, deviceId)

    private fun MotionEvent.isFromSource(source: Int): Boolean = (this.source and source) == source

    private fun MotionEvent.isFingerTouchEvent(): Boolean =
        isFromSource(InputDevice.SOURCE_TOUCHSCREEN) &&
            !isFromSource(InputDevice.SOURCE_MOUSE) &&
            !isFromSource(InputDevice.SOURCE_MOUSE_RELATIVE)

    private fun MotionEvent.isExternalMousePointerEvent(): Boolean {
        val controllerSource = isFromSource(InputDevice.SOURCE_JOYSTICK) || isFromSource(InputDevice.SOURCE_GAMEPAD)
        return isFromSource(InputDevice.SOURCE_MOUSE) ||
            isFromSource(InputDevice.SOURCE_MOUSE_RELATIVE) ||
            (isFromSource(InputDevice.SOURCE_TOUCHPAD) && !controllerSource)
    }

    private fun shouldPassTouchToNativeUi(event: MotionEvent, width: Int, height: Int): Boolean {
        if (event.isFingerTouchEvent()) {
            updateNativeUiTouchPointers(event, width, height)
            return eventHasNativeUiTouchPointer(event, width, height) &&
                !eventHasStreamTouchPointer(event, width, height)
        }
        when (event.actionMasked) {
            MotionEvent.ACTION_DOWN -> {
                uiTouchPassthroughActive =
                    pointerTouchesNativeUi(event, 0, width, height)
                return uiTouchPassthroughActive
            }
            MotionEvent.ACTION_UP, MotionEvent.ACTION_CANCEL -> {
                val wasActive = uiTouchPassthroughActive
                uiTouchPassthroughActive = false
                return wasActive
            }
            else -> if (uiTouchPassthroughActive) {
                return true
            }
        }
        return false
    }

    private fun updateNativeUiTouchPointers(event: MotionEvent, width: Int, height: Int) {
        if (!event.isFingerTouchEvent()) return
        when (event.actionMasked) {
            MotionEvent.ACTION_DOWN -> {
                nativeUiTouchPointerIds.clear()
                if (pointerTouchesNativeUi(event, 0, width, height)) {
                    nativeUiTouchPointerIds += event.getPointerId(0)
                }
            }
            MotionEvent.ACTION_POINTER_DOWN -> {
                val index = event.actionIndex
                if (index in 0 until event.pointerCount && pointerTouchesNativeUi(event, index, width, height)) {
                    nativeUiTouchPointerIds += event.getPointerId(index)
                }
            }
            MotionEvent.ACTION_UP,
            MotionEvent.ACTION_CANCEL,
            -> nativeUiTouchPointerIds.clear()
        }
        uiTouchPassthroughActive = nativeUiTouchPointerIds.isNotEmpty()
    }

    private fun eventHasNativeUiTouchPointer(event: MotionEvent, width: Int, height: Int): Boolean =
        (0 until event.pointerCount).any { index ->
            isNativeUiTouchPointer(event, index, width, height)
        }

    private fun eventHasStreamTouchPointer(event: MotionEvent, width: Int, height: Int): Boolean =
        (0 until event.pointerCount).any { index ->
            !isNativeUiTouchPointer(event, index, width, height)
        }

    private fun isNativeUiTouchPointer(event: MotionEvent, index: Int, width: Int, height: Int): Boolean =
        event.getPointerId(index) in nativeUiTouchPointerIds ||
            pointerTouchesNativeUi(event, index, width, height)

    private fun pointerTouchesNativeUi(event: MotionEvent, index: Int, width: Int, height: Int): Boolean {
        if (index !in 0 until event.pointerCount) return false
        val x = event.getX(index)
        val y = event.getY(index)
        return streamChromePassthroughBounds?.contains(x, y) == true ||
            streamPanelPassthroughBounds?.contains(x, y) == true ||
            touchControllerContains(x, y, width, height)
    }

    private fun touchControllerContains(x: Float, y: Float, width: Int, height: Int): Boolean {
        val bounds = touchControllerPassthroughBounds
        if (bounds.isNotEmpty()) return bounds.values.any { it.contains(x, y) }
        return touchControllerVisible &&
            width > 0 &&
            height > 0 &&
            y >= height * TOUCH_CONTROLLER_FALLBACK_TOP_RATIO
    }

    private data class TouchPassthroughBounds(
        val left: Int,
        val top: Int,
        val right: Int,
        val bottom: Int,
    ) {
        fun contains(x: Float, y: Float): Boolean =
            x >= left - EDGE_SLOP_PX &&
                x <= right + EDGE_SLOP_PX &&
                y >= top - EDGE_SLOP_PX &&
                y <= bottom + EDGE_SLOP_PX

        companion object {
            private const val EDGE_SLOP_PX = 24
        }
    }

    private const val TOUCH_CONTROLLER_FALLBACK_TOP_RATIO = 0.52f
}

object NativeInputDiagnostics {
    private const val MAX_LINES = 80
    private val lines = ArrayDeque<String>()

    @Synchronized
    fun add(message: String) {
        if (lines.size >= MAX_LINES) {
            lines.removeFirst()
        }
        lines.addLast("${SystemClock.elapsedRealtime()} $message")
        Log.d("OpenNOWInput", message)
    }

    @Synchronized
    fun snapshot(): String =
        if (lines.isEmpty()) {
            "input.diagnostics=empty"
        } else {
            buildString {
                appendLine("input.diagnostics:")
                lines.forEach { appendLine(it) }
            }.trimEnd()
    }
}

enum class InputDataChannelRole {
    Reliable,
    PartiallyReliable,
    Other,
}

object InputDataChannelLabels {
    fun classify(label: String): InputDataChannelRole =
        when (label.lowercase(Locale.US)) {
            "input_channel_v1",
            "input_channel",
            -> InputDataChannelRole.Reliable
            "input_channel_partially_reliable",
            "input_channel_pr",
            -> InputDataChannelRole.PartiallyReliable
            else -> InputDataChannelRole.Other
        }
}

internal object AndroidControllerInput {
    fun hasControllerSource(source: Int): Boolean =
        source.hasSource(InputDevice.SOURCE_GAMEPAD) ||
            source.hasSource(InputDevice.SOURCE_JOYSTICK)

    fun isControllerDevice(device: InputDevice?): Boolean =
        device != null && isControllerDevice(device.sources, device.name)

    fun isControllerDevice(source: Int, deviceName: String?): Boolean =
        hasControllerSource(source) ||
            (source.hasSource(InputDevice.SOURCE_DPAD) && isKnownControllerName(deviceName))

    fun isControllerEvent(source: Int, deviceId: Int): Boolean =
        hasControllerSource(source) ||
            isControllerDevice(InputDevice.getDevice(deviceId))

    fun isKnownControllerName(name: String?): Boolean {
        val normalized = name.orEmpty().lowercase(Locale.US)
        return normalized.contains("stadia controller") ||
            normalized == "stadia" ||
            normalized.contains("google stadia")
    }

    fun isPrimaryActivationKey(keyCode: Int): Boolean =
        keyCode == KeyEvent.KEYCODE_DPAD_CENTER ||
            keyCode == KeyEvent.KEYCODE_ENTER ||
            keyCode == KeyEvent.KEYCODE_NUMPAD_ENTER

    private fun Int.hasSource(source: Int): Boolean = (this and source) == source
}

internal data class GamepadRumbleCommand(
    val controllerId: Int,
    val weakMagnitude: Int,
    val strongMagnitude: Int,
)

internal object HapticsPacketParser {
    fun parse(bytes: ByteArray): GamepadRumbleCommand? {
        if (bytes.size < 2) return null
        val view = ByteBuffer.wrap(bytes).order(ByteOrder.LITTLE_ENDIAN)
        val firstWord = view.getShort(0).toInt() and 0xffff
        if (firstWord == LEGACY_HAPTIC_SUBMESSAGE_TYPE) {
            return parseLegacy(bytes, 2)
        }

        return when (firstWord and 0xff) {
            WRAPPER_SINGLE_EVENT -> parseSubMessage(bytes, 1)
            WRAPPER_BATCHED_EVENT,
            WRAPPER_LEGACY_INPUT,
            WRAPPER_TIMESTAMPED_SINGLE,
            WRAPPER_TIMESTAMPED_BATCHED,
            WRAPPER_RESERVED,
            -> null
            else -> parseLegacy(bytes, 0)
        }
    }

    private fun parseSubMessage(bytes: ByteArray, offset: Int): GamepadRumbleCommand? {
        if (offset < 0 || offset + 4 > bytes.size) return null
        val type = ByteBuffer.wrap(bytes).order(ByteOrder.LITTLE_ENDIAN).getInt(offset)
        return when (type) {
            LEGACY_HAPTIC_SUBMESSAGE_TYPE -> parseLegacy(bytes, offset + 4)
            OC_HAPTIC_SUBMESSAGE_TYPE -> parseOc(bytes, offset + 4)
            else -> null
        }
    }

    private fun parseLegacy(bytes: ByteArray, offset: Int): GamepadRumbleCommand? {
        if (offset < 0 || offset + 10 > bytes.size) return null
        val view = ByteBuffer.wrap(bytes).order(ByteOrder.LITTLE_ENDIAN)
        val kind = view.getShort(offset).toInt() and 0xffff
        if (kind != 1) return null
        val length = view.getShort(offset + 2).toInt() and 0xffff
        if (length < 6) return null
        return GamepadRumbleCommand(
            controllerId = view.getShort(offset + 4).toInt() and 0xffff,
            weakMagnitude = view.getShort(offset + 6).toInt() and 0xffff,
            strongMagnitude = view.getShort(offset + 8).toInt() and 0xffff,
        )
    }

    private fun parseOc(bytes: ByteArray, offset: Int): GamepadRumbleCommand? {
        if (offset < 0 || offset + 9 > bytes.size) return null
        val controllerByte = bytes[offset].toInt() and 0xff
        if (controllerByte !in 6 until 10) return null
        val reportKind = bytes[offset + 3].toInt() and 0xff
        val flags = bytes[offset + 4].toInt() and 0xff
        if (reportKind != 5 || (flags and 0xfe) != 0) return null
        return GamepadRumbleCommand(
            controllerId = controllerByte - 6,
            weakMagnitude = (bytes[offset + 7].toInt() and 0xff) shl 8,
            strongMagnitude = (bytes[offset + 8].toInt() and 0xff) shl 8,
        )
    }

    private const val LEGACY_HAPTIC_SUBMESSAGE_TYPE = 267
    private const val OC_HAPTIC_SUBMESSAGE_TYPE = 17
    private const val WRAPPER_BATCHED_EVENT = 32
    private const val WRAPPER_LEGACY_INPUT = 33
    private const val WRAPPER_SINGLE_EVENT = 34
    private const val WRAPPER_TIMESTAMPED_SINGLE = 35
    private const val WRAPPER_TIMESTAMPED_BATCHED = 36
    private const val WRAPPER_RESERVED = 255
}

internal object GamepadButtonMapping {
    const val DPAD_UP = 0x0001
    const val DPAD_DOWN = 0x0002
    const val DPAD_LEFT = 0x0004
    const val DPAD_RIGHT = 0x0008
    const val START = 0x0010
    const val BACK = 0x0020
    const val LEFT_THUMB = 0x0040
    const val RIGHT_THUMB = 0x0080
    const val LEFT_SHOULDER = 0x0100
    const val RIGHT_SHOULDER = 0x0200
    const val GUIDE = 0x0400
    const val A = 0x1000
    const val B = 0x2000
    const val X = 0x4000
    const val Y = 0x8000

    fun maskForKeyCode(keyCode: Int, controllerActivation: Boolean = false): Int? = when (keyCode) {
        KeyEvent.KEYCODE_DPAD_UP -> DPAD_UP
        KeyEvent.KEYCODE_DPAD_DOWN -> DPAD_DOWN
        KeyEvent.KEYCODE_DPAD_LEFT -> DPAD_LEFT
        KeyEvent.KEYCODE_DPAD_RIGHT -> DPAD_RIGHT
        KeyEvent.KEYCODE_DPAD_CENTER,
        KeyEvent.KEYCODE_ENTER,
        KeyEvent.KEYCODE_NUMPAD_ENTER,
        -> if (controllerActivation) A else null
        KeyEvent.KEYCODE_BUTTON_START -> START
        KeyEvent.KEYCODE_BUTTON_SELECT -> BACK
        KeyEvent.KEYCODE_BUTTON_THUMBL -> LEFT_THUMB
        KeyEvent.KEYCODE_BUTTON_THUMBR -> RIGHT_THUMB
        KeyEvent.KEYCODE_BUTTON_L1 -> LEFT_SHOULDER
        KeyEvent.KEYCODE_BUTTON_R1 -> RIGHT_SHOULDER
        KeyEvent.KEYCODE_BUTTON_MODE -> GUIDE
        KeyEvent.KEYCODE_BUTTON_A -> A
        KeyEvent.KEYCODE_BUTTON_B -> B
        KeyEvent.KEYCODE_BUTTON_X -> X
        KeyEvent.KEYCODE_BUTTON_Y -> Y
        else -> null
    }

    fun isControllerButtonKeyCode(keyCode: Int): Boolean =
        keyCode == KeyEvent.KEYCODE_BUTTON_L2 ||
            keyCode == KeyEvent.KEYCODE_BUTTON_R2 ||
            keyCode in KeyEvent.KEYCODE_BUTTON_A..KeyEvent.KEYCODE_BUTTON_MODE
}

internal fun streamSharpnessShaderStrength(enabled: Boolean, amount: Float): Float =
    if (enabled) amount.coerceIn(0f, 1f) * STREAM_SHARPNESS_MAX_SHADER_STRENGTH else 0f

private const val STREAM_SHARPNESS_MAX_SHADER_STRENGTH = 0.28f

private class StreamSharpnessGlDrawer : RendererCommon.GlDrawer {
    @Volatile
    var amount: Float = 0f

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
            amount = amount,
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
            amount = amount,
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
            amount = amount,
            vertexBuffer = vertexBuffer,
            textureBuffer = textureBuffer,
        )
    }

    override fun release() {
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

    fun reset() {
        lastProgressAtMs = null
        lastBytesReceived = null
        lastFramesDecoded = null
        lastKeyframeRequestAtMs = Long.MIN_VALUE
        keyframeAttempts = 0
    }

    fun markConnected(nowMs: Long) {
        lastProgressAtMs = nowMs
        lastKeyframeRequestAtMs = Long.MIN_VALUE
        keyframeAttempts = 0
    }

    fun observe(nowMs: Long, bytesReceived: Long?, framesDecoded: Long?, connected: Boolean): StreamLivenessAction {
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

private class TouchMouseState {
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

    fun reset(client: NativeStreamClient?) {
        if (selecting) client?.setTouchMouseButton(false)
        activePointerId = -1
        selecting = false
        doubleTapDragCandidate = false
    }

    fun handle(event: MotionEvent, enabled: Boolean, client: NativeStreamClient, ignoredPointerIds: Set<Int>): Boolean {
        if (!enabled) {
            reset(client)
            return false
        }

        when (event.actionMasked) {
            MotionEvent.ACTION_DOWN -> {
                if (event.getPointerId(0) in ignoredPointerIds) {
                    reset(client)
                    return false
                }
                beginPointer(event, 0)
                return true
            }
            MotionEvent.ACTION_POINTER_DOWN -> {
                if (activePointerId < 0) {
                    val index = event.actionIndex
                    if (index in 0 until event.pointerCount && event.getPointerId(index) !in ignoredPointerIds) {
                        beginPointer(event, index)
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
                sendMouseDelta(dx, dy, client)
                lastX = x
                lastY = y
                return true
            }
            MotionEvent.ACTION_POINTER_UP -> {
                val index = event.actionIndex
                if (index in 0 until event.pointerCount && event.getPointerId(index) == activePointerId) {
                    finishPointer(event, index, client)
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
        selecting = false
        doubleTapDragCandidate = isDoubleTap(event, index)
        if (doubleTapDragCandidate) {
            lastTapTimeMs = Long.MIN_VALUE
        }
    }

    private fun finishPointer(event: MotionEvent, index: Int, client: NativeStreamClient) {
        val x = event.getX(index)
        val y = event.getY(index)
        val tapDistanceX = abs(x - downX)
        val tapDistanceY = abs(y - downY)
        val wasTap = activePointerId >= 0 &&
            event.eventTime - downTimeMs <= TOUCH_MOUSE_TAP_TIMEOUT_MS &&
            tapDistanceX <= TOUCH_MOUSE_TAP_SLOP_PX &&
            tapDistanceY <= TOUCH_MOUSE_TAP_SLOP_PX
        activePointerId = -1
        doubleTapDragCandidate = false
        if (selecting) {
            client.setTouchMouseButton(false)
            selecting = false
            return
        }
        if (wasTap) {
            NativeInputDiagnostics.add("touch tap click dx=${tapDistanceX.roundToInt()} dy=${tapDistanceY.roundToInt()}")
            client.sendTouchMouseClick()
            lastTapTimeMs = event.eventTime
            lastTapX = x
            lastTapY = y
        }
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
        client: NativeStreamClient,
        partiallyReliable: Boolean = true,
    ) {
        val ix = dx.roundToInt()
        val iy = dy.roundToInt()
        if (ix != 0 || iy != 0) {
            client.sendTouchMouseMove(ix, iy, partiallyReliable)
        }
    }

    companion object {
        private const val TOUCH_MOUSE_DRAG_START_SLOP_PX = 10f
        private const val TOUCH_MOUSE_TAP_SLOP_PX = 42f
        private const val TOUCH_MOUSE_TAP_TIMEOUT_MS = 450L
        private const val TOUCH_MOUSE_DOUBLE_TAP_TIMEOUT_MS = 320L
        private const val TOUCH_MOUSE_DOUBLE_TAP_SLOP_PX = 36f
    }
}

class NativeStreamClient(
    context: Context,
    private val onState: (String) -> Unit,
    private val onError: (String) -> Unit,
    private val onSafeVideoFallbackRequired: (String) -> Unit = {},
    private val onSessionRecoveryRequired: (String) -> Unit = {},
    private val onStats: (StreamRuntimeStats) -> Unit = {},
) {
    private val appContext = context.applicationContext
    private val eglBase: EglBase = EglBase.create()
    private val scope = CoroutineScope(SupervisorJob() + Dispatchers.Main.immediate)
    private val inputEncoder = InputEncoder()
    private val audioDeviceModule: AudioDeviceModule =
        JavaAudioDeviceModule.builder(appContext)
            .setUseLowLatency(true)
            .setUseStereoOutput(true)
            .setAudioAttributes(
                AudioAttributes.Builder()
                    .setUsage(AudioAttributes.USAGE_GAME)
                    .setContentType(AudioAttributes.CONTENT_TYPE_MUSIC)
                    .build(),
            )
            .createAudioDeviceModule()
    private var factory: PeerConnectionFactory? = null
    private var peerConnection: PeerConnection? = null
    private var signaling: GfnSignalingClient? = null
    private var reliableInput: DataChannel? = null
    private var partiallyReliableInput: DataChannel? = null
    private var partiallyReliableGamepadMask = 0
    private var hapticsAdvertised: Boolean? = null
    private var videoTrack: VideoTrack? = null
    private var audioTrack: AudioTrack? = null
    private var renderer: SurfaceViewRenderer? = null
    private var rendererSharpnessDrawer: StreamSharpnessGlDrawer? = null
    private var heartbeatJob: Job? = null
    private var gamepadKeepaliveJob: Job? = null
    private var statsJob: Job? = null
    private var iceRecoveryJob: Job? = null
    private var offerTimeoutJob: Job? = null
    private var settings: StreamSettings = StreamSettings()
    private var session: SessionInfo? = null
    private var transportGeneration = 0
    private var reconnectAttempts = 0
    private var videoSafeFallbackApplied = false
    private var sessionRecoveryRequested = false
    private var lastIceState: PeerConnection.IceConnectionState? = null
    private var audioMuted = false
    private var virtualButtons = 0
    private var virtualLeftTrigger = 0
    private var virtualRightTrigger = 0
    private var virtualLeftStickActive = false
    private var virtualLeftStickX = 0
    private var virtualLeftStickY = 0
    private var virtualRightStickActive = false
    private var virtualRightStickX = 0
    private var virtualRightStickY = 0
    private var virtualControllerVisible = false
    private var physicalControllerConnected = false
    private var physicalControllerActive = false
    private var activeControllerId = 0
    private val controllerSlots = linkedMapOf<Int, Int>()
    private var physicalButtons = 0
    private var physicalHatButtons = 0
    private var physicalLeftTriggerButtonPressed = false
    private var physicalRightTriggerButtonPressed = false
    private var lastLeftTrigger = 0
    private var lastRightTrigger = 0
    private var lastLeftStickX = 0
    private var lastLeftStickY = 0
    private var lastRightStickX = 0
    private var lastRightStickY = 0
    private var mouseLastDeviceId = Int.MIN_VALUE
    private var mouseLastSource = 0
    private var mouseLastX = 0f
    private var mouseLastY = 0f
    private var mousePositionValid = false
    private var mouseSuppressNextAbsoluteDelta = false
    private var inputDropLogged = false
    private var externalMouseEventLogged = false
    private var externalMouseMoveSentLogged = false
    private var externalMouseAbsoluteJumpLogged = false
    private var hardwareKeyboardEventLogged = false
    private var lastStatsSample: StreamStatsSample? = null
    private val livenessWatchdog = StreamLivenessWatchdog()
    private val textSendMutex = Mutex()
    private var guideAutoReleaseJob: Job? = null
    private val lastRumbleEffectAtMs = LongArray(GAMEPAD_MAX_CONTROLLERS)
    private val hapticsSupportLogged = BooleanArray(GAMEPAD_MAX_CONTROLLERS)
    private var lastHapticsWarningAtMs = 0L
    private var lastHapticsAdvertisementAtMs = 0L
    private var phoneRumbleFallbackEnabled = true
    private var phoneRumbleSupportLogged = false

    private data class RumbleEffectProfile(
        val weakAmplitude: Int,
        val strongAmplitude: Int,
        val combinedAmplitude: Int,
    ) {
        val isStop: Boolean
            get() = weakAmplitude <= 0 && strongAmplitude <= 0 && combinedAmplitude <= 0
    }

    private data class StreamStatsSample(
        val atMs: Double,
        val bytesReceived: Long,
        val framesDecoded: Long,
    )

    private data class RuntimeStatsSnapshot(
        val stats: StreamRuntimeStats,
        val bytesReceived: Long?,
        val framesDecoded: Long?,
    )

    init {
        WebRtcRuntime.ensureInitialized(appContext)
        factory = PeerConnectionFactory.builder()
            .setOptions(PeerConnectionFactory.Options())
            .setAudioDeviceModule(audioDeviceModule)
            .setVideoDecoderFactory(OpenNowVideoDecoderFactory(eglBase.eglBaseContext))
            .setVideoEncoderFactory(DefaultVideoEncoderFactory(eglBase.eglBaseContext, true, true))
            .createPeerConnectionFactory()
    }

    fun createRenderer(context: Context, settings: StreamSettings, stretchToFill: Boolean = false): SurfaceViewRenderer =
        SurfaceViewRenderer(context).also {
            renderer?.let { oldRenderer ->
                videoTrack?.removeSink(oldRenderer)
                oldRenderer.release()
            }
            val sharpnessDrawer = if (settings.streamSharpeningEnabled) {
                StreamSharpnessGlDrawer().also { drawer ->
                    drawer.amount = streamSharpnessShaderStrength(true, settings.streamSharpeningAmount)
                }
            } else {
                null
            }
            rendererSharpnessDrawer = sharpnessDrawer
            if (sharpnessDrawer != null) {
                it.init(eglBase.eglBaseContext, null, EglBase.CONFIG_PLAIN, sharpnessDrawer)
            } else {
                it.init(eglBase.eglBaseContext, null)
            }
            it.setEnableHardwareScaler(false)
            it.setMirror(false)
            it.setStreamScaling()
            renderer = it
            videoTrack?.addSink(it)
        }

    fun updateRendererSettings(settings: StreamSettings, stretchToFill: Boolean = false) {
        this.settings = this.settings.copy(
            mouseSensitivity = settings.mouseSensitivity,
            mouseAcceleration = settings.mouseAcceleration,
            streamSharpeningEnabled = settings.streamSharpeningEnabled,
            streamSharpeningAmount = settings.streamSharpeningAmount,
        )
        rendererSharpnessDrawer?.amount = streamSharpnessShaderStrength(settings.streamSharpeningEnabled, settings.streamSharpeningAmount)
        renderer?.setStreamScaling()
    }

    private fun SurfaceViewRenderer.setStreamScaling() {
        setScalingType(RendererCommon.ScalingType.SCALE_ASPECT_FIT)
    }

    fun updateHapticsSettings(phoneFallbackEnabled: Boolean) {
        if (phoneRumbleFallbackEnabled == phoneFallbackEnabled) return
        phoneRumbleFallbackEnabled = phoneFallbackEnabled
        if (!phoneFallbackEnabled) {
            cancelPhoneRumble()
        }
        updateHapticsAdvertisement(force = true)
    }

    fun start(session: SessionInfo, settings: StreamSettings) {
        this.session = session
        this.settings = settings
        transportGeneration += 1
        reconnectAttempts = 0
        videoSafeFallbackApplied = false
        sessionRecoveryRequested = false
        lastStatsSample = null
        livenessWatchdog.reset()
        onStats(StreamRuntimeStats())
        audioDeviceModule.setSpeakerMute(audioMuted)
        closeTransport(clearInputState = false)
        startTransport(session, settings, transportGeneration)
    }

    fun stop() {
        transportGeneration += 1
        reconnectAttempts = 0
        sessionRecoveryRequested = false
        livenessWatchdog.reset()
        closeTransport(clearInputState = true)
        emitState("Stopped")
    }

    fun release() {
        stop()
        renderer?.release()
        renderer = null
        rendererSharpnessDrawer = null
        factory?.dispose()
        factory = null
        audioDeviceModule.release()
        eglBase.release()
    }

    private fun resetInputState() {
        virtualButtons = 0
        virtualLeftTrigger = 0
        virtualRightTrigger = 0
        virtualLeftStickActive = false
        virtualLeftStickX = 0
        virtualLeftStickY = 0
        virtualRightStickActive = false
        virtualRightStickX = 0
        virtualRightStickY = 0
        virtualControllerVisible = false
        physicalControllerConnected = false
        physicalControllerActive = false
        physicalButtons = 0
        physicalHatButtons = 0
        physicalLeftTriggerButtonPressed = false
        physicalRightTriggerButtonPressed = false
        guideAutoReleaseJob?.cancel()
        guideAutoReleaseJob = null
        stopAllGamepadRumble()
        lastLeftTrigger = 0
        lastRightTrigger = 0
        lastLeftStickX = 0
        lastLeftStickY = 0
        lastRightStickX = 0
        lastRightStickY = 0
        activeControllerId = 0
        controllerSlots.clear()
        mousePositionValid = false
        mouseSuppressNextAbsoluteDelta = false
        inputDropLogged = false
        externalMouseEventLogged = false
        externalMouseMoveSentLogged = false
        externalMouseAbsoluteJumpLogged = false
        hardwareKeyboardEventLogged = false
        inputEncoder.resetGamepadSequences()
    }

    fun dispatchKey(event: KeyEvent): Boolean {
        if (event.isGamepadEvent() && dispatchGamepadKey(event)) {
            return true
        }
        val key = InputEncoder.mapKeyEvent(event)
        val hardwareKeyboard = event.isHardwareKeyboardSource()
        if (hardwareKeyboard && !hardwareKeyboardEventLogged) {
            hardwareKeyboardEventLogged = true
            NativeInputDiagnostics.add("hardware keyboard event action=${event.action} key=${event.keyCode} scan=${event.scanCode} source=${event.source} device=${event.deviceId} mapped=${key != null}")
        }
        val packet = key?.let { if (event.action == KeyEvent.ACTION_DOWN) inputEncoder.encodeKeyDown(it) else inputEncoder.encodeKeyUp(it) }
        if (packet == null) {
            if (hardwareKeyboard && (event.action == KeyEvent.ACTION_DOWN || event.action == KeyEvent.ACTION_UP)) {
                NativeInputDiagnostics.add("hardware keyboard consumed unmapped key=${event.keyCode} action=${event.action}")
                return true
            }
            return false
        }
        val sent = sendReliableInput(packet)
        if (hardwareKeyboard && !sent) {
            NativeInputDiagnostics.add("hardware keyboard consumed without send key=${event.keyCode} reliable=${reliableInput?.state()} partial=${partiallyReliableInput?.state()}")
        }
        return sent || hardwareKeyboard
    }

    fun dispatchMotion(event: MotionEvent): Boolean {
        if (event.isGamepadMotionEvent()) {
            return dispatchJoystick(event)
        }
        if (event.isMouseLikePointer()) {
            return dispatchMouseLikePointer(event)
        }
        return false
    }

    fun sendTouchMouseMove(dx: Int, dy: Int, partiallyReliable: Boolean = true): Boolean {
        val adjusted = adjustedMouseDelta(dx, dy)
        return sendInput(inputEncoder.encodeMouseMove(adjusted.first, adjusted.second), partiallyReliable = partiallyReliable)
    }

    private fun dispatchMouseLikePointer(event: MotionEvent): Boolean {
        if (!externalMouseEventLogged) {
            externalMouseEventLogged = true
            val relativeDx = if (Build.VERSION.SDK_INT >= 26) event.getAxisValue(MotionEvent.AXIS_RELATIVE_X) else 0f
            val relativeDy = if (Build.VERSION.SDK_INT >= 26) event.getAxisValue(MotionEvent.AXIS_RELATIVE_Y) else 0f
            NativeInputDiagnostics.add(
                "external mouse event action=${event.actionMasked} source=${event.source} device=${event.deviceId} buttons=${event.buttonState} relativeDx=$relativeDx relativeDy=$relativeDy",
            )
        }
        when (event.actionMasked) {
            MotionEvent.ACTION_HOVER_MOVE,
            MotionEvent.ACTION_MOVE,
            -> {
                val relativeDx = if (Build.VERSION.SDK_INT >= 26) event.getAxisValue(MotionEvent.AXIS_RELATIVE_X) else 0f
                val relativeDy = if (Build.VERSION.SDK_INT >= 26) event.getAxisValue(MotionEvent.AXIS_RELATIVE_Y) else 0f
                if (abs(relativeDx) >= 0.5f || abs(relativeDy) >= 0.5f) {
                    val sent = sendTouchMouseMove(relativeDx.roundToInt(), relativeDy.roundToInt())
                    if (sent && !externalMouseMoveSentLogged) {
                        externalMouseMoveSentLogged = true
                        NativeInputDiagnostics.add("external mouse move sent source=${event.source} device=${event.deviceId} mode=relative")
                    }
                    mousePositionValid = false
                } else if (event.isRelativeMousePointer()) {
                    val positionDx = event.x
                    val positionDy = event.y
                    if (abs(positionDx) >= 0.5f || abs(positionDy) >= 0.5f) {
                        val sent = sendTouchMouseMove(positionDx.roundToInt(), positionDy.roundToInt())
                        if (sent && !externalMouseMoveSentLogged) {
                            externalMouseMoveSentLogged = true
                            NativeInputDiagnostics.add("external mouse move sent source=${event.source} device=${event.deviceId} mode=relativePosition")
                        }
                    }
                    mousePositionValid = false
                } else if (mousePositionValid && mouseLastDeviceId == event.deviceId && mouseLastSource == event.source) {
                    val dx = event.x - mouseLastX
                    val dy = event.y - mouseLastY
                    if (abs(dx) >= 0.5f || abs(dy) >= 0.5f) {
                        val discontinuous = mouseSuppressNextAbsoluteDelta ||
                            abs(dx) > EXTERNAL_MOUSE_ABSOLUTE_DELTA_LIMIT_PX ||
                            abs(dy) > EXTERNAL_MOUSE_ABSOLUTE_DELTA_LIMIT_PX
                        if (discontinuous) {
                            if (!externalMouseAbsoluteJumpLogged) {
                                externalMouseAbsoluteJumpLogged = true
                                NativeInputDiagnostics.add("external mouse absolute delta rebased source=${event.source} device=${event.deviceId} dx=${dx.roundToInt()} dy=${dy.roundToInt()}")
                            }
                        } else {
                            val sent = sendTouchMouseMove(dx.roundToInt(), dy.roundToInt())
                            if (sent && !externalMouseMoveSentLogged) {
                                externalMouseMoveSentLogged = true
                                NativeInputDiagnostics.add("external mouse move sent source=${event.source} device=${event.deviceId} mode=absoluteDelta")
                            }
                        }
                        mouseSuppressNextAbsoluteDelta = false
                    }
                } else {
                    mouseSuppressNextAbsoluteDelta = false
                }
                if (!event.isRelativeMousePointer()) {
                    rememberMousePosition(event)
                }
            }
            MotionEvent.ACTION_DOWN -> {
                mouseSuppressNextAbsoluteDelta = true
                rememberMousePosition(event)
                sendReliableInput(inputEncoder.encodeMouseButton(InputEncoder.INPUT_MOUSE_BUTTON_DOWN, event.primaryMouseButton()))
            }
            MotionEvent.ACTION_UP,
            MotionEvent.ACTION_CANCEL,
            -> {
                mousePositionValid = false
                mouseSuppressNextAbsoluteDelta = true
                sendReliableInput(inputEncoder.encodeMouseButton(InputEncoder.INPUT_MOUSE_BUTTON_UP, event.primaryMouseButton()))
            }
            MotionEvent.ACTION_BUTTON_PRESS -> {
                mouseSuppressNextAbsoluteDelta = true
                rememberMousePosition(event)
                val handled = sendReliableInput(inputEncoder.encodeMouseButton(InputEncoder.INPUT_MOUSE_BUTTON_DOWN, event.actionButton.toGfnMouseButton()))
                if (!handled) {
                    NativeInputDiagnostics.add("external mouse button consumed without send action=press button=${event.actionButton} reliable=${reliableInput?.state()} partial=${partiallyReliableInput?.state()}")
                }
                return true
            }
            MotionEvent.ACTION_BUTTON_RELEASE -> {
                mousePositionValid = false
                mouseSuppressNextAbsoluteDelta = true
                val handled = sendReliableInput(inputEncoder.encodeMouseButton(InputEncoder.INPUT_MOUSE_BUTTON_UP, event.actionButton.toGfnMouseButton()))
                if (!handled) {
                    NativeInputDiagnostics.add("external mouse button consumed without send action=release button=${event.actionButton} reliable=${reliableInput?.state()} partial=${partiallyReliableInput?.state()}")
                }
                return true
            }
            MotionEvent.ACTION_SCROLL -> {
                val vertical = event.getAxisValue(MotionEvent.AXIS_VSCROLL)
                if (abs(vertical) >= 0.01f) {
                    sendReliableInput(inputEncoder.encodeMouseWheel((vertical * 120).roundToInt()))
                }
            }
        }
        return true
    }

    private fun rememberMousePosition(event: MotionEvent) {
        mouseLastDeviceId = event.deviceId
        mouseLastSource = event.source
        mouseLastX = event.x
        mouseLastY = event.y
        mousePositionValid = true
    }

    private fun adjustedMouseDelta(dx: Int, dy: Int): Pair<Int, Int> {
        var adjustedDx = dx * settings.mouseSensitivity
        var adjustedDy = dy * settings.mouseSensitivity
        if (settings.mouseAcceleration > 1) {
            val speed = sqrt(adjustedDx * adjustedDx + adjustedDy * adjustedDy)
            val strength = (settings.mouseAcceleration - 1f) / 149f
            val accelFactor = 1f + min(0.6f * strength, (speed / 50f) * strength)
            adjustedDx *= accelFactor
            adjustedDy *= accelFactor
        }
        return adjustedDx.roundToInt() to adjustedDy.roundToInt()
    }

    fun sendTouchMouseClick(delayBeforeDownMs: Long = 0L) {
        scope.launch {
            if (delayBeforeDownMs > 0) {
                delay(delayBeforeDownMs)
            }
            if (!setTouchMouseButton(true)) return@launch
            delay(160L)
            setTouchMouseButton(false)
        }
    }

    fun sendKeyCode(keyCode: Int) {
        val down = KeyEvent(SystemClock.uptimeMillis(), SystemClock.uptimeMillis(), KeyEvent.ACTION_DOWN, keyCode, 0)
        val up = KeyEvent(SystemClock.uptimeMillis(), SystemClock.uptimeMillis(), KeyEvent.ACTION_UP, keyCode, 0)
        dispatchKey(down)
        dispatchKey(up)
    }

    fun sendText(text: String) {
        if (text.isEmpty()) return
        val textToSend = text.take(STREAM_TEXT_SEND_MAX_CHARS)
        scope.launch {
            textSendMutex.withLock {
                textToSend.forEach { char ->
                    sendTextChar(char)
                }
            }
        }
    }

    private fun sendKeyboardPayload(payload: InputEncoder.KeyboardPayload, isDown: Boolean): Boolean =
        sendReliableInput(if (isDown) inputEncoder.encodeKeyDown(payload) else inputEncoder.encodeKeyUp(payload))

    private suspend fun sendTextChar(char: Char) {
        val spec = InputEncoder.mapTextCharToKeySpec(char) ?: return
        if (spec.shift) {
            sendKeyboardPayloadWithRetry(InputEncoder.shiftLeftPayload(modifiers = 0x01), isDown = true)
        }
        val modifiers = if (spec.shift) 0x01 else 0
        sendKeyboardPayloadWithRetry(spec.toKeyboardPayload(modifiers), isDown = true)
        sendKeyboardPayloadWithRetry(spec.toKeyboardPayload(modifiers), isDown = false)
        if (spec.shift) {
            sendKeyboardPayloadWithRetry(InputEncoder.shiftLeftPayload(modifiers = 0), isDown = false)
        }
        delay(STREAM_TEXT_KEY_DELAY_MS)
    }

    private suspend fun sendKeyboardPayloadWithRetry(payload: InputEncoder.KeyboardPayload, isDown: Boolean): Boolean {
        repeat(STREAM_TEXT_SEND_ATTEMPTS) { attempt ->
            if (sendKeyboardPayload(payload, isDown)) {
                delay(STREAM_TEXT_PACKET_DELAY_MS)
                return true
            }
            if (attempt < STREAM_TEXT_SEND_ATTEMPTS - 1) {
                delay(STREAM_TEXT_RETRY_DELAY_MS)
            }
        }
        NativeInputDiagnostics.add(
            "overlay keyboard dropped key=${payload.keycode} action=${if (isDown) "down" else "up"} reliable=${reliableInput?.state()} partial=${partiallyReliableInput?.state()}",
        )
        return false
    }

    fun setAudioMuted(muted: Boolean) {
        audioMuted = muted
        audioDeviceModule.setSpeakerMute(muted)
        audioTrack?.setEnabled(!muted)
    }

    fun setTouchMouseButton(pressed: Boolean): Boolean {
        val packet = inputEncoder.encodeMouseButton(
            if (pressed) InputEncoder.INPUT_MOUSE_BUTTON_DOWN else InputEncoder.INPUT_MOUSE_BUTTON_UP,
            1,
        )
        val reliableSent = sendInput(packet, partiallyReliable = false)
        val partialSent = sendInput(packet, partiallyReliable = true)
        NativeInputDiagnostics.add(
            "touch mouse button ${if (pressed) "down" else "up"} reliableSent=$reliableSent partialSent=$partialSent reliable=${reliableInput?.state()} partial=${partiallyReliableInput?.state()}",
        )
        return reliableSent || partialSent
    }

    fun setVirtualButton(buttonMask: Int, pressed: Boolean) {
        virtualButtons = if (pressed) virtualButtons or buttonMask else virtualButtons and buttonMask.inv()
        sendCurrentGamepadState()
    }

    fun setVirtualTrigger(left: Boolean, pressed: Boolean) {
        if (left) {
            virtualLeftTrigger = if (pressed) 255 else 0
        } else {
            virtualRightTrigger = if (pressed) 255 else 0
        }
        sendCurrentGamepadState()
    }

    fun setVirtualLeftStick(x: Float, y: Float) {
        val normalized = applyDeadzone(x, y, deadzone = 0.08f)
        virtualLeftStickActive = normalized.first != 0f || normalized.second != 0f
        virtualLeftStickX = normalizeToInt16(normalized.first)
        virtualLeftStickY = normalizeToInt16(-normalized.second)
        sendCurrentGamepadState()
    }

    fun setVirtualRightStick(x: Float, y: Float) {
        val normalized = applyDeadzone(x, y, deadzone = 0.08f)
        virtualRightStickActive = normalized.first != 0f || normalized.second != 0f
        virtualRightStickX = normalizeToInt16(normalized.first)
        virtualRightStickY = normalizeToInt16(-normalized.second)
        sendCurrentGamepadState()
    }

    fun setVirtualControllerVisible(visible: Boolean) {
        if (virtualControllerVisible == visible) return
        virtualControllerVisible = visible
        sendCurrentGamepadState()
    }

    fun releaseGamepadButtonForShortcut(sourceEvent: KeyEvent): Boolean {
        val upEvent = KeyEvent(
            sourceEvent.downTime,
            SystemClock.uptimeMillis(),
            KeyEvent.ACTION_UP,
            sourceEvent.keyCode,
            0,
            sourceEvent.metaState,
            sourceEvent.deviceId,
            sourceEvent.scanCode,
            sourceEvent.flags,
            sourceEvent.source,
        )
        return dispatchKey(upEvent)
    }

    private fun startTransport(session: SessionInfo, settings: StreamSettings, generation: Int) {
        inputDropLogged = false
        lastIceState = null
        lastStatsSample = null
        emitStats(StreamRuntimeStats())
        audioDeviceModule.setSpeakerMute(audioMuted)
        emitState(if (reconnectAttempts > 0) "Reconnecting signaling" else "Connecting signaling")
        signaling = GfnSignalingClient(session, settings = settings) { event ->
            handleSignaling(event, generation)
        }.also { it.connect() }
    }

    private fun closeTransport(clearInputState: Boolean, cancelRecovery: Boolean = true) {
        if (cancelRecovery) {
            iceRecoveryJob?.cancel()
            iceRecoveryJob = null
        }
        heartbeatJob?.cancel()
        gamepadKeepaliveJob?.cancel()
        statsJob?.cancel()
        offerTimeoutJob?.cancel()
        heartbeatJob = null
        gamepadKeepaliveJob = null
        statsJob = null
        offerTimeoutJob = null
        lastStatsSample = null
        lastIceState = null
        livenessWatchdog.reset()
        signaling?.disconnect()
        signaling = null
        reliableInput = null
        partiallyReliableInput = null
        partiallyReliableGamepadMask = 0
        hapticsAdvertised = null
        lastHapticsAdvertisementAtMs = 0L
        if (clearInputState) resetInputState()
        videoTrack?.removeSink(renderer)
        videoTrack = null
        audioTrack = null
        peerConnection?.close()
        peerConnection?.dispose()
        peerConnection = null
    }

    private fun handleSignaling(event: SignalingEvent, generation: Int) {
        if (generation != transportGeneration) return
        when (event) {
            SignalingEvent.Connected -> {
                emitState("Waiting for offer")
                startOfferTimeout(generation)
            }
            is SignalingEvent.Disconnected -> scheduleTransportReconnect("Signaling disconnected: ${event.reason}", SIGNALING_RECONNECT_DELAY_MS, generation)
            is SignalingEvent.Error -> scheduleTransportReconnect("Signaling failed: ${event.message}", SIGNALING_RECONNECT_DELAY_MS, generation)
            is SignalingEvent.Log -> emitState(event.message)
            is SignalingEvent.RemoteIce -> peerConnection?.addIceCandidate(event.candidate)
            is SignalingEvent.Offer -> handleOffer(event.sdp, generation)
        }
    }

    private fun handleOffer(rawOffer: String, generation: Int) {
        val currentSession = session ?: return
        offerTimeoutJob?.cancel()
        offerTimeoutJob = null
        val fixed = prepareRemoteOffer(rawOffer, currentSession)
        val preferred = SdpTools.preferCodec(fixed, settings)
        val pc = ensurePeerConnection(currentSession, generation)
        ensureInputDataChannels(pc, preferred)
        inputEncoder.setProtocolVersion(SdpTools.parseInputProtocolVersion(preferred))
        partiallyReliableGamepadMask = SdpTools.parsePartiallyReliableGamepadMask(preferred)
        pc.setRemoteDescription(
            object : SimpleSdpObserver() {
                override fun onSetSuccess() {
                    applyVideoCodecPreferences(pc)
                    pc.createAnswer(
                        object : SimpleSdpObserver() {
                            override fun onCreateSuccess(description: SessionDescription?) {
                                if (generation != transportGeneration) return
                                val rawDescription = description ?: run {
                                    failStream("WebRTC returned an empty answer", generation)
                                    return
                                }
                                val munged = SdpTools.mungeAnswerSdp(rawDescription.description, settings.maxBitrateMbps * 1000)
                                if (settings.codec != VideoCodec.H264 && !SdpTools.negotiatesCodec(munged, settings.codec)) {
                                    NativeInputDiagnostics.add("local answer did not negotiate requested codec=${settings.codec}; requesting safe fallback")
                                    if (
                                        requestSafeVideoFallback(
                                            message = "${settings.codec} was requested but WebRTC did not negotiate it; restarting with safe H264 profile",
                                            diagnosticReason = "codec negotiation",
                                        )
                                    ) {
                                        return
                                    }
                                    failStream("${settings.codec} requested but not negotiated in local SDP", generation)
                                    return
                                }
                                val answer = SessionDescription(SessionDescription.Type.ANSWER, munged)
                                pc.setLocalDescription(
                                    object : SimpleSdpObserver() {
                                        override fun onSetSuccess() {
                                            if (generation != transportGeneration) return
                                            val nvst = SdpTools.buildNvstSdp(
                                                offerSdp = preferred,
                                                settings = settings,
                                                localAnswer = munged,
                                            )
                                            signaling?.sendAnswer(munged, nvst)
                                            emitState("Streaming")
                                            startHeartbeat()
                                            startGamepadKeepalive()
                                            startStatsPolling()
                                        }

                                        override fun onSetFailure(error: String?) {
                                            failStream(error ?: "Failed to set local description", generation)
                                        }
                                    },
                                    answer,
                                )
                            }

                            override fun onCreateFailure(error: String?) {
                                failStream(error ?: "Failed to create WebRTC answer", generation)
                            }
                        },
                        MediaConstraints(),
                    )
                }

                override fun onSetFailure(error: String?) {
                    failStream(error ?: "Failed to apply server offer", generation)
                }
            },
            SessionDescription(SessionDescription.Type.OFFER, preferred),
        )
    }

    private fun prepareRemoteOffer(rawOffer: String, session: SessionInfo): String {
        var prepared = SdpTools.fixServerIp(rawOffer, session.serverIp)
        if (settings.codec == VideoCodec.H265) {
            val maxLevels = h265ReceiverMaxLevelsByProfile()
            if (maxLevels.isNotEmpty()) {
                val rewritten = SdpTools.rewriteH265LevelIdByProfile(prepared, maxLevels)
                if (rewritten.replacements > 0) {
                    NativeInputDiagnostics.add("h265 level-id clamped replacements=${rewritten.replacements} maxLevels=$maxLevels")
                    prepared = rewritten.sdp
                }
            }
            if (!supportsH265TierFlagOne()) {
                val rewritten = SdpTools.rewriteH265TierFlag(prepared, 0)
                if (rewritten.replacements > 0) {
                    NativeInputDiagnostics.add("h265 tier-flag rewritten replacements=${rewritten.replacements}")
                    prepared = rewritten.sdp
                }
            }
        }
        return prepared
    }

    private fun applyVideoCodecPreferences(pc: PeerConnection) {
        val preferences = receiverCodecPreferences(settings.codec)
        if (preferences.isEmpty()) return
        val transceiver = pc.transceivers.firstOrNull {
            it.mediaType == MediaStreamTrack.MediaType.MEDIA_TYPE_VIDEO ||
                it.receiver?.track()?.kind() == MediaStreamTrack.VIDEO_TRACK_KIND
        } ?: return
        val result = transceiver.setCodecPreferences(preferences)
        if (result.isSuccess) {
            NativeInputDiagnostics.add("codec preferences applied codec=${settings.codec} count=${preferences.size}")
        } else {
            NativeInputDiagnostics.add("codec preferences failed codec=${settings.codec} error=${result.error()?.message.orEmpty()}")
        }
    }

    private fun receiverCodecPreferences(codec: VideoCodec): List<RtpCapabilities.CodecCapability> {
        val receiverCaps = runCatching {
            requireNotNull(factory).getRtpReceiverCapabilities(MediaStreamTrack.MediaType.MEDIA_TYPE_VIDEO).codecs
        }.getOrNull().orEmpty()
        val target = codec.webRtcCodecName()
        val preferred = receiverCaps
            .filter { it.openNowCodecName() == target }
            .let { caps -> if (codec == VideoCodec.H265) caps.sortedBy { it.h265ProfilePriority(settings.prefersTenBitVideo()) } else caps }
        if (preferred.isEmpty()) return emptyList()
        val auxiliary = receiverCaps.filter { it.openNowCodecName() in WEBRTC_AUXILIARY_VIDEO_CODECS }
        return (preferred + auxiliary).distinctBy { it.preferenceKey() }
    }

    private fun h265ReceiverMaxLevelsByProfile(): Map<Int, Int> =
        receiverH265Capabilities()
            .mapNotNull { capability ->
                val profile = capability.codecParameterInt("profile-id") ?: return@mapNotNull null
                val level = capability.codecParameterInt("level-id") ?: return@mapNotNull null
                profile to level
            }
            .groupBy({ it.first }, { it.second })
            .mapValues { (_, levels) -> levels.maxOrNull() ?: 0 }
            .filterValues { it > 0 }

    private fun supportsH265TierFlagOne(): Boolean =
        receiverH265Capabilities().any { it.codecParameterInt("tier-flag") == 1 }

    private fun receiverH265Capabilities(): List<RtpCapabilities.CodecCapability> =
        runCatching {
            requireNotNull(factory).getRtpReceiverCapabilities(MediaStreamTrack.MediaType.MEDIA_TYPE_VIDEO).codecs
        }.getOrNull().orEmpty()
            .filter { it.openNowCodecName() == VideoCodec.H265.webRtcCodecName() }

    private fun startOfferTimeout(generation: Int) {
        offerTimeoutJob?.cancel()
        offerTimeoutJob = scope.launch {
            delay(OFFER_TIMEOUT_MS)
            if (generation != transportGeneration || peerConnection != null) return@launch
            offerTimeoutJob = null
            NativeInputDiagnostics.add("video offer timeout codec=${settings.codec} resolution=${settings.resolution} bitrate=${settings.maxBitrateMbps}")
            if (
                requestSafeVideoFallback(
                    message = "Timed out waiting for video offer; restarting with safe H264 profile",
                    diagnosticReason = "offer timeout",
                )
            ) {
                return@launch
            }
            restartTransport("Timed out waiting for video offer")
        }
    }

    private fun ensurePeerConnection(session: SessionInfo, generation: Int): PeerConnection {
        peerConnection?.let { return it }
        val ice = session.iceServers.map {
            PeerConnection.IceServer.builder(it.urls).apply {
                if (it.username != null) setUsername(it.username)
                if (it.credential != null) setPassword(it.credential)
            }.createIceServer()
        }
        val config = PeerConnection.RTCConfiguration(ice).apply {
            sdpSemantics = PeerConnection.SdpSemantics.UNIFIED_PLAN
            continualGatheringPolicy = PeerConnection.ContinualGatheringPolicy.GATHER_CONTINUALLY
            tcpCandidatePolicy = PeerConnection.TcpCandidatePolicy.DISABLED
            bundlePolicy = PeerConnection.BundlePolicy.MAXBUNDLE
            rtcpMuxPolicy = PeerConnection.RtcpMuxPolicy.REQUIRE
        }
        val pc = requireNotNull(factory).createPeerConnection(config, object : PeerConnection.Observer {
            override fun onSignalingChange(state: PeerConnection.SignalingState?) = Unit
            override fun onIceConnectionChange(state: PeerConnection.IceConnectionState?) {
                handleIceConnectionChange(state, generation)
            }
            override fun onIceConnectionReceivingChange(receiving: Boolean) = Unit
            override fun onIceGatheringChange(state: PeerConnection.IceGatheringState?) = Unit
            override fun onIceCandidate(candidate: IceCandidate?) {
                if (candidate != null) signaling?.sendIceCandidate(candidate)
            }
            override fun onIceCandidatesRemoved(candidates: Array<out IceCandidate>?) = Unit
            override fun onAddStream(stream: MediaStream?) {
                stream?.videoTracks?.firstOrNull()?.let(::attachVideo)
                stream?.audioTracks?.firstOrNull()?.let {
                    audioTrack = it
                    it.setEnabled(!audioMuted)
                }
            }
            override fun onRemoveStream(stream: MediaStream?) = Unit
            override fun onDataChannel(channel: DataChannel?) {
                if (channel != null) attachDataChannel(channel)
            }
            override fun onRenegotiationNeeded() = Unit
            override fun onAddTrack(receiver: RtpReceiver?, streams: Array<out MediaStream>?) {
                val track = receiver?.track()
                if (track is VideoTrack) attachVideo(track)
                if (track is AudioTrack) {
                    audioTrack = track
                    track.setEnabled(!audioMuted)
                }
            }
            override fun onTrack(transceiver: RtpTransceiver?) {
                val track = transceiver?.receiver?.track()
                if (track is VideoTrack) attachVideo(track)
                if (track is AudioTrack) {
                    audioTrack = track
                    track.setEnabled(!audioMuted)
                }
            }
        }) ?: error("Failed to create PeerConnection")
        peerConnection = pc
        return pc
    }

    private fun handleIceConnectionChange(state: PeerConnection.IceConnectionState?, generation: Int) {
        scope.launch {
            if (generation != transportGeneration) return@launch
            lastIceState = state
            when (state) {
                PeerConnection.IceConnectionState.CONNECTED,
                PeerConnection.IceConnectionState.COMPLETED,
                -> {
                    iceRecoveryJob?.cancel()
                    iceRecoveryJob = null
                    reconnectAttempts = 0
                    livenessWatchdog.markConnected(SystemClock.elapsedRealtime())
                    emitState("Streaming")
                }
                PeerConnection.IceConnectionState.DISCONNECTED -> {
                    emitState("ICE_DISCONNECTED")
                    scheduleTransportReconnect("ICE disconnected", ICE_DISCONNECTED_GRACE_MS, generation)
                }
                PeerConnection.IceConnectionState.FAILED -> {
                    emitState("ICE_FAILED")
                    scheduleTransportReconnect("ICE failed", ICE_FAILED_RECONNECT_DELAY_MS, generation)
                }
                PeerConnection.IceConnectionState.CHECKING,
                PeerConnection.IceConnectionState.NEW,
                -> emitState(state.toIceStatusLabel())
                PeerConnection.IceConnectionState.CLOSED -> Unit
                null -> Unit
            }
        }
    }

    private fun scheduleTransportReconnect(reason: String, delayMs: Long, generation: Int) {
        if (generation != transportGeneration || iceRecoveryJob?.isActive == true) return
        iceRecoveryJob = scope.launch {
            if (delayMs > 0) delay(delayMs)
            if (generation != transportGeneration) return@launch
            if (reason == "ICE disconnected" && lastIceState != PeerConnection.IceConnectionState.DISCONNECTED) return@launch
            restartTransport(reason)
        }
    }

    private fun restartTransport(reason: String) {
        val currentSession = session ?: return
        val currentSettings = settings
        if (reconnectAttempts >= MAX_TRANSPORT_RECONNECT_ATTEMPTS) {
            requestSessionRecovery("$reason. Stream reconnect failed after $MAX_TRANSPORT_RECONNECT_ATTEMPTS attempts.")
            return
        }
        reconnectAttempts += 1
        transportGeneration += 1
        val generation = transportGeneration
        emitState("Reconnecting stream ($reconnectAttempts/$MAX_TRANSPORT_RECONNECT_ATTEMPTS)")
        closeTransport(clearInputState = false, cancelRecovery = false)
        iceRecoveryJob = null
        startTransport(currentSession, currentSettings, generation)
    }

    private fun requestSessionRecovery(message: String) {
        if (sessionRecoveryRequested) return
        sessionRecoveryRequested = true
        transportGeneration += 1
        closeTransport(clearInputState = false)
        emitState("Recovering cloud session")
        emitSessionRecoveryRequired(message)
    }

    private fun failStream(message: String, generation: Int? = null) {
        if (generation != null && generation != transportGeneration) return
        transportGeneration += 1
        closeTransport(clearInputState = true)
        emitError(message)
    }

    private fun emitState(message: String) {
        scope.launch { onState(message) }
    }

    private fun emitError(message: String) {
        scope.launch { onError(message) }
    }

    private fun emitSafeVideoFallbackRequired(message: String) {
        scope.launch { onSafeVideoFallbackRequired(message) }
    }

    private fun emitSessionRecoveryRequired(message: String) {
        scope.launch { onSessionRecoveryRequired(message) }
    }

    private fun PeerConnection.IceConnectionState.toIceStatusLabel(): String = "ICE_${name}"

    private fun emitStats(stats: StreamRuntimeStats) {
        scope.launch { onStats(stats) }
    }

    private fun ensureInputDataChannels(pc: PeerConnection, offerSdp: String) {
        if (reliableInput == null) {
            val reliableInit = DataChannel.Init().apply {
                ordered = true
            }
            pc.createDataChannel("input_channel_v1", reliableInit)?.let(::attachDataChannel)
        }

        if (partiallyReliableInput == null) {
            val thresholdMs = SdpTools.parsePartialReliableThresholdMs(offerSdp)
            val partialInit = DataChannel.Init().apply {
                ordered = false
                maxRetransmitTimeMs = thresholdMs
            }
            pc.createDataChannel("input_channel_partially_reliable", partialInit)?.let(::attachDataChannel)
        }
    }

    private fun attachVideo(track: VideoTrack) {
        videoTrack?.removeSink(renderer)
        videoTrack = track
        track.setEnabled(true)
        renderer?.let { track.addSink(it) }
    }

    private fun attachDataChannel(channel: DataChannel) {
        val label = channel.label()
        val normalizedLabel = label.lowercase(Locale.US)
        val role = InputDataChannelLabels.classify(label)
        NativeInputDiagnostics.add("data channel attached label=$normalizedLabel role=$role state=${channel.state()}")
        when (role) {
            InputDataChannelRole.Reliable -> reliableInput = channel
            InputDataChannelRole.PartiallyReliable -> partiallyReliableInput = channel
            InputDataChannelRole.Other -> return
        }
        channel.registerObserver(object : DataChannel.Observer {
            override fun onBufferedAmountChange(previousAmount: Long) = Unit
            override fun onStateChange() {
                if (channel.state() == DataChannel.State.OPEN) {
                    inputDropLogged = false
                    NativeInputDiagnostics.add("input channel open label=$normalizedLabel")
                    updateHapticsAdvertisement(force = true)
                    schedulePrimeConnectedGamepadState(reason = "channel open $normalizedLabel")
                }
            }
            override fun onMessage(buffer: DataChannel.Buffer) {
                handleInputChannelMessage(buffer)
            }
        })
    }

    private fun handleInputChannelMessage(buffer: DataChannel.Buffer) {
        val bytes = buffer.data.duplicate().let { data ->
            ByteArray(data.remaining()).also(data::get)
        }
        if (bytes.isEmpty()) return
        if (handleInputHandshakeMessage(bytes)) return
        HapticsPacketParser.parse(bytes)?.let { command ->
            applyGamepadRumble(command.controllerId, command.weakMagnitude, command.strongMagnitude)
        }
    }

    private fun handleInputHandshakeMessage(bytes: ByteArray): Boolean {
        val firstWord = if (bytes.size >= 2) {
            ByteBuffer.wrap(bytes).order(ByteOrder.LITTLE_ENDIAN).short.toInt() and 0xffff
        } else {
            bytes[0].toInt() and 0xff
        }
        val version = when {
            firstWord == INPUT_HANDSHAKE_MAGIC_WORD -> {
                if (bytes.size >= 4) {
                    ByteBuffer.wrap(bytes).order(ByteOrder.LITTLE_ENDIAN).getShort(2).toInt() and 0xffff
                } else {
                    DEFAULT_INPUT_PROTOCOL_VERSION
                }
            }
            (bytes[0].toInt() and 0xff) == INPUT_HANDSHAKE_MARKER -> firstWord
            else -> return false
        }.coerceAtLeast(1)

        inputEncoder.setProtocolVersion(version)
        inputEncoder.resetGamepadSequences()
        NativeInputDiagnostics.add("input handshake protocol=$version bytes=${bytes.size}")
        updateHapticsAdvertisement(force = true)
        schedulePrimeConnectedGamepadState(reason = "input handshake")
        return true
    }

    private fun startHeartbeat() {
        heartbeatJob?.cancel()
        heartbeatJob = scope.launch {
            while (true) {
                delay(1000)
                sendReliableInput(inputEncoder.encodeHeartbeat())
            }
        }
    }

    private fun startGamepadKeepalive() {
        gamepadKeepaliveJob?.cancel()
        gamepadKeepaliveJob = scope.launch {
            var connectedScanCountdown = 0
            primeConnectedGamepadState(reason = "keepalive start")
            while (true) {
                delay(100L)
                connectedScanCountdown -= 1
                if (connectedScanCountdown <= 0) {
                    connectedScanCountdown = 10
                    refreshConnectedPhysicalControllers()
                }
                if (hasAnyControllerState()) {
                    sendCurrentGamepadState()
                }
                updateHapticsAdvertisement()
            }
        }
    }

    private fun startStatsPolling() {
        statsJob?.cancel()
        statsJob = scope.launch {
            while (true) {
                pollRuntimeStats()
                delay(1000L)
            }
        }
    }

    private fun pollRuntimeStats() {
        val pc = peerConnection ?: return
        val generation = transportGeneration
        pc.getStats(RTCStatsCollectorCallback { report ->
            val snapshot = buildRuntimeStatsSnapshot(report.timestampUs / 1000.0, report.statsMap.values)
            scope.launch {
                if (generation != transportGeneration) return@launch
                handleMediaLiveness(snapshot)
                onStats(snapshot.stats)
            }
        })
    }

    private fun buildRuntimeStatsSnapshot(timestampMs: Double, stats: Collection<RTCStats>): RuntimeStatsSnapshot {
        val inboundVideo = stats.firstOrNull { stat ->
            val members = stat.members
            stat.type == "inbound-rtp" &&
                (members["kind"] == "video" || members["mediaType"] == "video")
        }
        val activePair = stats.firstOrNull { stat ->
            val members = stat.members
            stat.type == "candidate-pair" &&
                members["state"] == "succeeded" &&
                members["nominated"] == true
        }
        val codecId = inboundVideo?.members?.get("codecId") as? String
        val codec = codecId
            ?.let { id -> stats.firstOrNull { stat -> stat.id == id } }
            ?.members
            ?.get("mimeType")
            ?.let(::formatStatsCodec)

        val members = inboundVideo?.members.orEmpty()
        val bytesReceived = members["bytesReceived"].statsLong()
        val framesDecoded = members["framesDecoded"].statsLong()
        val explicitFps = members["framesPerSecond"].statsDouble()
        val width = members["frameWidth"].statsLong()
        val height = members["frameHeight"].statsLong()
        val previous = lastStatsSample
        val elapsedSeconds = previous?.let { (timestampMs - it.atMs) / 1000.0 }?.takeIf { it > 0.0 }
        val bitrateKbps = if (previous != null && bytesReceived != null && elapsedSeconds != null) {
            (((bytesReceived - previous.bytesReceived).coerceAtLeast(0) * 8.0) / elapsedSeconds / 1000.0)
                .roundToInt()
                .coerceAtLeast(0)
        } else {
            null
        }
        val derivedFps = if (previous != null && framesDecoded != null && elapsedSeconds != null) {
            ((framesDecoded - previous.framesDecoded).coerceAtLeast(0) / elapsedSeconds).roundToInt()
        } else {
            null
        }
        if (bytesReceived != null || framesDecoded != null) {
            lastStatsSample = StreamStatsSample(
                atMs = timestampMs,
                bytesReceived = bytesReceived ?: previous?.bytesReceived ?: 0L,
                framesDecoded = framesDecoded ?: previous?.framesDecoded ?: 0L,
            )
        }

        val pingMs = activePair?.members?.get("currentRoundTripTime")
            .statsDouble()
            ?.let { (it * 1000.0).roundToInt().coerceAtLeast(0) }
        val resolution = if (width != null && height != null && width > 0 && height > 0) {
            "${width}x$height"
        } else {
            null
        }

        return RuntimeStatsSnapshot(
            stats = StreamRuntimeStats(
                bitrateKbps = bitrateKbps,
                pingMs = pingMs,
                fps = explicitFps?.roundToInt()?.takeIf { it > 0 } ?: derivedFps?.takeIf { it > 0 },
                resolution = resolution,
                codec = codec,
            ),
            bytesReceived = bytesReceived,
            framesDecoded = framesDecoded,
        )
    }

    private fun handleMediaLiveness(snapshot: RuntimeStatsSnapshot) {
        val connected = lastIceState == PeerConnection.IceConnectionState.CONNECTED ||
            lastIceState == PeerConnection.IceConnectionState.COMPLETED
        when (val action = livenessWatchdog.observe(SystemClock.elapsedRealtime(), snapshot.bytesReceived, snapshot.framesDecoded, connected)) {
            StreamLivenessAction.None -> Unit
            is StreamLivenessAction.RequestKeyframe -> {
                signaling?.requestKeyframe(
                    reason = "media_stall",
                    backlogFrames = 0,
                    attempt = action.attempt,
                )
                emitState("Recovering video")
                NativeInputDiagnostics.add("media stall keyframe requested stalledMs=${action.stalledMs} attempt=${action.attempt}")
            }
            is StreamLivenessAction.RestartTransport -> {
                if (
                    requestSafeVideoFallback(
                        message = "Decoder stalled; restarting with safe H264 profile",
                        diagnosticReason = "media stall",
                    )
                ) {
                    return
                }
                NativeInputDiagnostics.add("media stall transport restart stalledMs=${action.stalledMs}")
                restartTransport("Media stalled for ${action.stalledMs / 1000}s")
            }
        }
    }

    private fun requestSafeVideoFallback(message: String, diagnosticReason: String): Boolean {
        val fallback = settings.androidSafeVideoFallback()
        if (videoSafeFallbackApplied || settings == fallback) return false
        videoSafeFallbackApplied = true
        settings = fallback
        NativeInputDiagnostics.add(
            "$diagnosticReason safe video fallback codec=${fallback.codec} resolution=${fallback.resolution} fps=${fallback.fps} bitrate=${fallback.maxBitrateMbps}",
        )
        transportGeneration += 1
        closeTransport(clearInputState = false)
        emitState("Restarting cloud session with safe H264 profile")
        emitSafeVideoFallbackRequired(message)
        return true
    }

    private fun formatStatsCodec(value: Any?): String? {
        val raw = value?.toString()?.substringAfter("/", value.toString())?.trim()?.uppercase(Locale.US) ?: return null
        return when (raw) {
            "AVC", "H264", "H.264" -> "H264"
            "HEVC", "H265", "H.265" -> "H265"
            "AV01", "AV1" -> "AV1"
            else -> raw.takeIf { it.isNotBlank() }
        }
    }

    private fun dispatchJoystick(event: MotionEvent): Boolean {
        val controllerId = controllerIdFor(event)
        activeControllerId = controllerId
        if (!physicalControllerActive) {
            NativeInputDiagnostics.add("physical gamepad motion source=${event.source} device=${event.deviceId} slot=$controllerId")
        }
        physicalControllerConnected = true
        physicalControllerActive = true
        val lx = event.getAxisValue(MotionEvent.AXIS_X)
        val ly = event.getAxisValue(MotionEvent.AXIS_Y)
        val rx = event.getAxisValue(MotionEvent.AXIS_Z).takeIf { abs(it) > 0.001f } ?: event.getAxisValue(MotionEvent.AXIS_RX)
        val ry = event.getAxisValue(MotionEvent.AXIS_RZ).takeIf { abs(it) > 0.001f } ?: event.getAxisValue(MotionEvent.AXIS_RY)
        val lt = max(
            max(event.getAxisValue(MotionEvent.AXIS_LTRIGGER), normalizeTriggerAxis(event.getAxisValue(MotionEvent.AXIS_BRAKE))),
            if (physicalLeftTriggerButtonPressed) 1f else 0f,
        )
        val rt = max(
            max(event.getAxisValue(MotionEvent.AXIS_RTRIGGER), normalizeTriggerAxis(event.getAxisValue(MotionEvent.AXIS_GAS))),
            if (physicalRightTriggerButtonPressed) 1f else 0f,
        )
        val left = applyDeadzone(lx, ly)
        val right = applyDeadzone(rx, ry)
        physicalHatButtons = event.hatDpadButtons()
        lastLeftTrigger = normalizeToUint8(lt)
        lastRightTrigger = normalizeToUint8(rt)
        lastLeftStickX = normalizeToInt16(left.first)
        lastLeftStickY = normalizeToInt16(-left.second)
        lastRightStickX = normalizeToInt16(right.first)
        lastRightStickY = normalizeToInt16(-right.second)
        return sendCurrentGamepadState(controllerId = controllerId)
    }

    private fun dispatchGamepadKey(event: KeyEvent): Boolean {
        if (event.action != KeyEvent.ACTION_DOWN && event.action != KeyEvent.ACTION_UP) return false
        val pressed = event.action == KeyEvent.ACTION_DOWN
        val controllerInputDevice = event.isControllerInputDevice()
        val mask = GamepadButtonMapping.maskForKeyCode(
            event.keyCode,
            controllerActivation = controllerInputDevice,
        )
        if (mask != null) {
            activeControllerId = controllerIdFor(event)
            if (!physicalControllerActive) {
                NativeInputDiagnostics.add("physical gamepad key source=${event.source} device=${event.deviceId} slot=$activeControllerId key=${event.keyCode}")
            }
            physicalControllerConnected = true
            physicalControllerActive = true
            physicalButtons = if (pressed) physicalButtons or mask else physicalButtons and mask.inv()
            val sent = sendCurrentGamepadState(controllerId = activeControllerId)
            updateGuideAutoRelease(mask, pressed, activeControllerId)
            return sent
        }
        when (event.keyCode) {
            KeyEvent.KEYCODE_BUTTON_L2 -> {
                activeControllerId = controllerIdFor(event)
                if (!physicalControllerActive) {
                    NativeInputDiagnostics.add("physical gamepad key source=${event.source} device=${event.deviceId} slot=$activeControllerId key=${event.keyCode}")
                }
                physicalControllerConnected = true
                physicalControllerActive = true
                physicalLeftTriggerButtonPressed = pressed
                lastLeftTrigger = if (pressed) 255 else 0
                return sendCurrentGamepadState(controllerId = activeControllerId)
            }
            KeyEvent.KEYCODE_BUTTON_R2 -> {
                activeControllerId = controllerIdFor(event)
                if (!physicalControllerActive) {
                    NativeInputDiagnostics.add("physical gamepad key source=${event.source} device=${event.deviceId} slot=$activeControllerId key=${event.keyCode}")
                }
                physicalControllerConnected = true
                physicalControllerActive = true
                physicalRightTriggerButtonPressed = pressed
                lastRightTrigger = if (pressed) 255 else 0
                return sendCurrentGamepadState(controllerId = activeControllerId)
            }
        }
        return false
    }

    private fun sendCurrentGamepadState(controllerId: Int = activeControllerId): Boolean {
        val partiallyReliable = canSendGamepadPartiallyReliable(controllerId)
        val packet = inputEncoder.encodeGamepadState(
            controllerId = controllerId,
            buttons = physicalButtons or physicalHatButtons or virtualButtons,
            leftTrigger = max(lastLeftTrigger, virtualLeftTrigger),
            rightTrigger = max(lastRightTrigger, virtualRightTrigger),
            leftStickX = effectiveLeftStickX(),
            leftStickY = effectiveLeftStickY(),
            rightStickX = effectiveRightStickX(),
            rightStickY = effectiveRightStickY(),
            bitmap = currentGamepadBitmap(controllerId),
            partiallyReliable = partiallyReliable,
        )
        return sendInput(packet, partiallyReliable = partiallyReliable, fallbackToReliable = !partiallyReliable)
    }

    private fun updateGuideAutoRelease(mask: Int, pressed: Boolean, controllerId: Int) {
        if (mask != GamepadButtonMapping.GUIDE) return
        guideAutoReleaseJob?.cancel()
        if (!pressed) {
            guideAutoReleaseJob = null
            return
        }
        guideAutoReleaseJob = scope.launch {
            delay(GAMEPAD_GUIDE_AUTO_RELEASE_MS)
            if ((physicalButtons and GamepadButtonMapping.GUIDE) == 0) return@launch
            physicalButtons = physicalButtons and GamepadButtonMapping.GUIDE.inv()
            sendCurrentGamepadState(controllerId = controllerId)
            NativeInputDiagnostics.add("physical gamepad guide auto-release slot=$controllerId")
        }
    }

    private fun effectiveLeftStickX(): Int = if (virtualLeftStickActive) virtualLeftStickX else lastLeftStickX
    private fun effectiveLeftStickY(): Int = if (virtualLeftStickActive) virtualLeftStickY else lastLeftStickY
    private fun effectiveRightStickX(): Int = if (virtualRightStickActive) virtualRightStickX else lastRightStickX
    private fun effectiveRightStickY(): Int = if (virtualRightStickActive) virtualRightStickY else lastRightStickY

    private fun hasAnyControllerState(): Boolean =
        physicalControllerConnected ||
            physicalControllerActive ||
            virtualControllerVisible ||
            physicalButtons != 0 ||
            physicalHatButtons != 0 ||
            virtualButtons != 0 ||
            lastLeftTrigger != 0 ||
            lastRightTrigger != 0 ||
            virtualLeftTrigger != 0 ||
            virtualRightTrigger != 0 ||
            lastLeftStickX != 0 ||
            lastLeftStickY != 0 ||
            lastRightStickX != 0 ||
            lastRightStickY != 0 ||
            virtualLeftStickActive ||
            virtualRightStickActive

    private fun sendInput(bytes: ByteArray, partiallyReliable: Boolean): Boolean =
        sendInput(bytes, partiallyReliable, fallbackToReliable = true)

    private fun sendReliableInput(bytes: ByteArray): Boolean {
        if (sendInput(bytes, partiallyReliable = false)) return true
        val sentPartial = sendInput(bytes, partiallyReliable = true, fallbackToReliable = false)
        if (sentPartial) {
            NativeInputDiagnostics.add("reliable input used partial fallback reliable=${reliableInput?.state()} partial=${partiallyReliableInput?.state()} bytes=${bytes.size}")
        }
        return sentPartial
    }

    private fun sendInput(bytes: ByteArray, partiallyReliable: Boolean, fallbackToReliable: Boolean): Boolean {
        val channel = if (partiallyReliable && partiallyReliableInput?.state() == DataChannel.State.OPEN) {
            partiallyReliableInput
        } else if (partiallyReliable && !fallbackToReliable) {
            null
        } else {
            reliableInput
        }
        if (channel?.state() != DataChannel.State.OPEN) {
            if (!inputDropLogged) {
                inputDropLogged = true
                NativeInputDiagnostics.add(
                    "input dropped noOpenChannel requestedPartial=$partiallyReliable reliable=${reliableInput?.state()} partial=${partiallyReliableInput?.state()} bytes=${bytes.size}",
                )
            }
            return false
        }
        return channel.send(DataChannel.Buffer(ByteBuffer.wrap(bytes), true))
    }

    private fun refreshConnectedPhysicalControllers() {
        val connectedDevices = mutableListOf<InputDevice>()
        InputDevice.getDeviceIds().forEach { deviceId ->
            val device = InputDevice.getDevice(deviceId) ?: return@forEach
            if (AndroidControllerInput.isControllerDevice(device)) {
                connectedDevices += device
            }
        }
        val connected = connectedDevices.isNotEmpty()
        val connectionChanged = connected != physicalControllerConnected
        if (connectionChanged) {
            NativeInputDiagnostics.add(
                "physical gamepad connected=$connected devices=${connectedDevices.joinToString { "${it.id}:${it.name}" }}",
            )
        }
        physicalControllerConnected = connected
        if (connected && !physicalControllerActive && controllerSlots.isEmpty()) {
            activeControllerId = controllerIdFor(connectedDevices.first().id)
        }
        if (!connected && physicalControllerActive) {
            physicalControllerActive = false
            physicalButtons = 0
            physicalHatButtons = 0
            physicalLeftTriggerButtonPressed = false
            physicalRightTriggerButtonPressed = false
            lastLeftTrigger = 0
            lastRightTrigger = 0
            lastLeftStickX = 0
            lastLeftStickY = 0
            lastRightStickX = 0
            lastRightStickY = 0
            sendCurrentGamepadState()
        }
        updateHapticsAdvertisement(force = connectionChanged)
    }

    private fun schedulePrimeConnectedGamepadState(reason: String) {
        scope.launch {
            primeConnectedGamepadState(reason)
        }
    }

    private fun primeConnectedGamepadState(reason: String) {
        refreshConnectedPhysicalControllers()
        if (!hasAnyControllerState()) return
        val sent = sendCurrentGamepadState()
        NativeInputDiagnostics.add(
            "gamepad state prime reason=$reason sent=$sent connected=$physicalControllerConnected active=$physicalControllerActive slot=$activeControllerId reliable=${reliableInput?.state()} partial=${partiallyReliableInput?.state()}",
        )
    }

    private fun updateHapticsAdvertisement(force: Boolean = false) {
        if (reliableInput?.state() != DataChannel.State.OPEN) return
        val enabled = hapticsOutputAvailable()
        val now = SystemClock.elapsedRealtime()
        if (!force && hapticsAdvertised == enabled && now - lastHapticsAdvertisementAtMs < HAPTICS_ADVERTISEMENT_REFRESH_MS) return
        if (sendReliableInput(inputEncoder.encodeHapticsEnabled(enabled))) {
            hapticsAdvertised = enabled
            lastHapticsAdvertisementAtMs = now
            NativeInputDiagnostics.add("gamepad haptics advertised enabled=$enabled force=$force")
        }
    }

    private fun hapticsOutputAvailable(): Boolean =
        hapticControllerDevices().isNotEmpty() || hasPhoneRumbleFallback()

    private fun hapticControllerDevices(): List<InputDevice> =
        buildList {
            InputDevice.getDeviceIds().forEach { deviceId ->
                val device = InputDevice.getDevice(deviceId) ?: return@forEach
                if (!AndroidControllerInput.isControllerDevice(device)) return@forEach
                if (device.hasControllerRumble()) add(device)
            }
        }

    private fun findHapticControllerDevice(controllerId: Int): InputDevice? {
        val devices = hapticControllerDevices()
        if (devices.isEmpty()) return null
        devices.firstOrNull { controllerSlots[it.id] == controllerId }?.let { return it }
        if (controllerId in 0 until GAMEPAD_MAX_CONTROLLERS) {
            devices.getOrNull(controllerId)?.let { return it }
        }
        return devices.singleOrNull()
    }

    @Suppress("DEPRECATION")
    private fun applyGamepadRumble(controllerId: Int, weakMagnitude16: Int, strongMagnitude16: Int) {
        val slot = controllerId.coerceIn(0, GAMEPAD_MAX_CONTROLLERS - 1)
        val profile = buildRumbleEffectProfile(weakMagnitude16, strongMagnitude16)
        val isStop = profile.isStop
        val device = findHapticControllerDevice(slot)
        val usePhoneFallback = device == null && hasPhoneRumbleFallback()
        if (device == null && !usePhoneFallback) {
            logHapticsWarning("input haptics no vibrator controller=$controllerId phoneFallback=$phoneRumbleFallbackEnabled")
            return
        }
        val now = SystemClock.elapsedRealtime()
        if (!isStop && lastRumbleEffectAtMs[slot] != 0L && now - lastRumbleEffectAtMs[slot] <= RUMBLE_THROTTLE_MS) {
            return
        }
        lastRumbleEffectAtMs[slot] = if (isStop) 0L else now

        if (isStop) {
            device?.let(::cancelControllerRumble)
            cancelPhoneRumble()
            return
        }
        if (device != null && !hapticsSupportLogged[slot]) {
            hapticsSupportLogged[slot] = true
            NativeInputDiagnostics.add("gamepad haptics available controller=$slot device=${device.id}:${device.name}")
        }
        if (Build.VERSION.SDK_INT >= Build.VERSION_CODES.O) {
            if (device != null) {
                vibrateController(device, profile)
            } else {
                vibratePhoneRumble(profile)
            }
        } else {
            @Suppress("DEPRECATION")
            if (device != null) {
                device.vibrator.vibrate(RUMBLE_EFFECT_MS)
            } else {
                vibratePhoneRumbleLegacy()
            }
        }
    }

    private fun stopAllGamepadRumble() {
        hapticControllerDevices().forEach { device ->
            cancelControllerRumble(device)
        }
        cancelPhoneRumble()
        for (index in 0 until GAMEPAD_MAX_CONTROLLERS) {
            lastRumbleEffectAtMs[index] = 0L
            hapticsSupportLogged[index] = false
        }
        phoneRumbleSupportLogged = false
        lastHapticsWarningAtMs = 0L
    }

    private fun logHapticsWarning(message: String) {
        val now = SystemClock.elapsedRealtime()
        if (now - lastHapticsWarningAtMs < HAPTICS_LOG_INTERVAL_MS) return
        lastHapticsWarningAtMs = now
        NativeInputDiagnostics.add(message)
    }

    private fun buildRumbleEffectProfile(weakMagnitude16: Int, strongMagnitude16: Int): RumbleEffectProfile {
        val weak = weakMagnitude16.coerceIn(0, 65535) / 65535f
        val strong = strongMagnitude16.coerceIn(0, 65535) / 65535f
        val combined = (strong * 0.78f + weak * 0.48f).coerceIn(0f, 1f)
        return RumbleEffectProfile(
            weakAmplitude = rumbleAmplitude(weak, weight = 0.72f),
            strongAmplitude = rumbleAmplitude(strong, weight = 1f),
            combinedAmplitude = rumbleAmplitude(combined, weight = 1f),
        )
    }

    private fun rumbleAmplitude(value: Float, weight: Float): Int {
        val scaled = (value.coerceIn(0f, 1f) * weight.coerceIn(0f, 1f) * 255f).roundToInt()
        return if (scaled <= 0) 0 else scaled.coerceIn(1, 255)
    }

    @Suppress("DEPRECATION")
    private fun InputDevice.hasControllerRumble(): Boolean {
        if (Build.VERSION.SDK_INT >= Build.VERSION_CODES.S) {
            val manager = vibratorManager
            if (manager.vibratorIds.any { manager.getVibrator(it).hasVibrator() }) return true
        }
        return vibrator.hasVibrator()
    }

    private fun vibrateController(device: InputDevice, profile: RumbleEffectProfile) {
        if (Build.VERSION.SDK_INT >= Build.VERSION_CODES.S) {
            val manager = device.vibratorManager
            val vibratorIds = manager.vibratorIds.filter { manager.getVibrator(it).hasVibrator() }
            if (vibratorIds.size >= 2) {
                val combination = CombinedVibration.startParallel()
                var addedEffect = false
                if (profile.strongAmplitude > 0) {
                    combination.addVibrator(vibratorIds[0], createRumbleEffect(profile.strongAmplitude))
                    addedEffect = true
                }
                if (profile.weakAmplitude > 0) {
                    combination.addVibrator(vibratorIds[1], createRumbleEffect(profile.weakAmplitude))
                    addedEffect = true
                }
                if (addedEffect) {
                    manager.vibrate(combination.combine())
                    return
                }
            }
            if (vibratorIds.isNotEmpty() && profile.combinedAmplitude > 0) {
                manager.vibrate(CombinedVibration.createParallel(createRumbleEffect(profile.combinedAmplitude)))
                return
            }
        }
        @Suppress("DEPRECATION")
        device.vibrator.vibrate(createRumbleEffect(profile.combinedAmplitude))
    }

    @Suppress("DEPRECATION")
    private fun cancelControllerRumble(device: InputDevice) {
        if (Build.VERSION.SDK_INT >= Build.VERSION_CODES.S) {
            val manager = device.vibratorManager
            if (manager.vibratorIds.isNotEmpty()) {
                manager.cancel()
                return
            }
        }
        device.vibrator.cancel()
    }

    private fun hasPhoneRumbleFallback(): Boolean {
        if (!phoneRumbleFallbackEnabled) return false
        return if (Build.VERSION.SDK_INT >= Build.VERSION_CODES.S) {
            val manager = appContext.getSystemService(VibratorManager::class.java) ?: return false
            manager.vibratorIds.any { manager.getVibrator(it).hasVibrator() }
        } else {
            @Suppress("DEPRECATION")
            (appContext.getSystemService(Context.VIBRATOR_SERVICE) as? Vibrator)?.hasVibrator() == true
        }
    }

    private fun createRumbleEffect(amplitude: Int): VibrationEffect =
        VibrationEffect.createOneShot(RUMBLE_EFFECT_MS, amplitude.coerceIn(1, 255))

    private fun vibratePhoneRumble(profile: RumbleEffectProfile) {
        if (!phoneRumbleSupportLogged) {
            phoneRumbleSupportLogged = true
            NativeInputDiagnostics.add("gamepad haptics using phone fallback")
        }
        if (Build.VERSION.SDK_INT >= Build.VERSION_CODES.S) {
            val manager = appContext.getSystemService(VibratorManager::class.java)
            if (manager != null && manager.vibratorIds.any { manager.getVibrator(it).hasVibrator() }) {
                manager.vibrate(CombinedVibration.createParallel(createRumbleEffect(profile.combinedAmplitude)))
                return
            }
        }
        @Suppress("DEPRECATION")
        (appContext.getSystemService(Context.VIBRATOR_SERVICE) as? Vibrator)?.vibrate(createRumbleEffect(profile.combinedAmplitude))
    }

    @Suppress("DEPRECATION")
    private fun vibratePhoneRumbleLegacy() {
        if (!phoneRumbleSupportLogged) {
            phoneRumbleSupportLogged = true
            NativeInputDiagnostics.add("gamepad haptics using phone fallback")
        }
        (appContext.getSystemService(Context.VIBRATOR_SERVICE) as? Vibrator)?.vibrate(RUMBLE_EFFECT_MS)
    }

    private fun cancelPhoneRumble() {
        if (Build.VERSION.SDK_INT >= Build.VERSION_CODES.S) {
            appContext.getSystemService(VibratorManager::class.java)?.cancel()
            return
        }
        @Suppress("DEPRECATION")
        (appContext.getSystemService(Context.VIBRATOR_SERVICE) as? Vibrator)?.cancel()
    }

    private fun controllerIdFor(event: KeyEvent): Int = controllerIdFor(event.deviceId)
    private fun controllerIdFor(event: MotionEvent): Int = controllerIdFor(event.deviceId)

    private fun controllerIdFor(deviceId: Int): Int {
        val stableDeviceId = if (deviceId >= 0) deviceId else 0
        controllerSlots[stableDeviceId]?.let { return it }
        val used = controllerSlots.values.toSet()
        val slot = (0 until 4).firstOrNull { it !in used } ?: 0
        controllerSlots[stableDeviceId] = slot
        return slot
    }

    private fun currentGamepadBitmap(controllerId: Int): Int {
        val connected = physicalControllerConnected ||
            physicalControllerActive ||
            virtualControllerVisible ||
            virtualButtons != 0 ||
            virtualLeftTrigger != 0 ||
            virtualRightTrigger != 0 ||
            virtualLeftStickActive ||
            virtualRightStickActive
        if (!connected) return 0
        val id = controllerId.coerceIn(0, 3)
        return (1 shl id) or (1 shl (id + 8))
    }

    private fun canSendGamepadPartiallyReliable(controllerId: Int): Boolean {
        if (partiallyReliableInput?.state() != DataChannel.State.OPEN) return false
        val mask = 1 shl (controllerId and 0x1f)
        return (partiallyReliableGamepadMask and mask) != 0
    }

    private fun MotionEvent.isFromSource(source: Int): Boolean = (this.source and source) == source
    private fun MotionEvent.isMouseLikePointer(): Boolean {
        val controllerSource = isFromSource(InputDevice.SOURCE_JOYSTICK) || isFromSource(InputDevice.SOURCE_GAMEPAD)
        return isFromSource(InputDevice.SOURCE_MOUSE) ||
            isFromSource(InputDevice.SOURCE_MOUSE_RELATIVE) ||
            (isFromSource(InputDevice.SOURCE_TOUCHPAD) && !controllerSource)
    }

    private fun MotionEvent.isRelativeMousePointer(): Boolean =
        isFromSource(InputDevice.SOURCE_MOUSE_RELATIVE)

    private fun MotionEvent.primaryMouseButton(): Int =
        when {
            actionButton != 0 -> actionButton.toGfnMouseButton()
            buttonState != 0 -> buttonState.toGfnMouseButton()
            else -> 1
        }

    private fun MotionEvent.hatDpadButtons(): Int {
        var mask = 0
        val hatX = getAxisValue(MotionEvent.AXIS_HAT_X)
        val hatY = getAxisValue(MotionEvent.AXIS_HAT_Y)
        if (hatY <= -0.5f) mask = mask or GamepadButtonMapping.DPAD_UP
        if (hatY >= 0.5f) mask = mask or GamepadButtonMapping.DPAD_DOWN
        if (hatX <= -0.5f) mask = mask or GamepadButtonMapping.DPAD_LEFT
        if (hatX >= 0.5f) mask = mask or GamepadButtonMapping.DPAD_RIGHT
        return mask
    }

    private fun KeyEvent.isGamepadEvent(): Boolean {
        val controllerInputDevice = isControllerInputDevice()
        return (controllerInputDevice &&
            (GamepadButtonMapping.maskForKeyCode(keyCode) != null ||
                AndroidControllerInput.isPrimaryActivationKey(keyCode) ||
                keyCode == KeyEvent.KEYCODE_BUTTON_L2 ||
                keyCode == KeyEvent.KEYCODE_BUTTON_R2)) ||
            GamepadButtonMapping.isControllerButtonKeyCode(keyCode)
    }

    private fun KeyEvent.isHardwareKeyboardSource(): Boolean =
        !isControllerInputDevice() &&
            ((source and InputDevice.SOURCE_KEYBOARD) == InputDevice.SOURCE_KEYBOARD ||
                InputDevice.getDevice(deviceId)?.keyboardType == InputDevice.KEYBOARD_TYPE_ALPHABETIC)

    private fun MotionEvent.isGamepadMotionEvent(): Boolean =
        isFromSource(InputDevice.SOURCE_JOYSTICK) ||
            isFromSource(InputDevice.SOURCE_GAMEPAD) ||
            (AndroidControllerInput.isControllerEvent(source, deviceId) && !isMouseLikePointer())

    private fun KeyEvent.isControllerInputDevice(): Boolean =
        AndroidControllerInput.isControllerEvent(source, deviceId)

    private fun Int.toGfnMouseButton(): Int = when {
        this and MotionEvent.BUTTON_PRIMARY != 0 -> 1
        this and MotionEvent.BUTTON_TERTIARY != 0 -> 2
        this and MotionEvent.BUTTON_SECONDARY != 0 -> 3
        this and MotionEvent.BUTTON_BACK != 0 -> 4
        this and MotionEvent.BUTTON_FORWARD != 0 -> 5
        else -> 1
    }

    private companion object {
        private const val EXTERNAL_MOUSE_ABSOLUTE_DELTA_LIMIT_PX = 240f
        private const val GAMEPAD_MAX_CONTROLLERS = 4
        private const val RUMBLE_EFFECT_MS = 90L
        private const val RUMBLE_THROTTLE_MS = 35L
        private const val HAPTICS_ADVERTISEMENT_REFRESH_MS = 5000L
        private const val HAPTICS_LOG_INTERVAL_MS = 5000L
    }

    private fun applyDeadzone(x: Float, y: Float, deadzone: Float = 0.15f): Pair<Float, Float> {
        val magnitude = kotlin.math.sqrt((x * x + y * y).toDouble()).toFloat()
        if (magnitude < deadzone) return 0f to 0f
        val scaled = ((magnitude - deadzone) / (1f - deadzone)).coerceIn(0f, 1f)
        return (x / magnitude) * scaled to (y / magnitude) * scaled
    }

    private fun normalizeToInt16(value: Float): Int = (value.coerceIn(-1f, 1f) * 32767).roundToInt().coerceIn(-32768, 32767)
    private fun normalizeToUint8(value: Float): Int = (value.coerceIn(0f, 1f) * 255).roundToInt().coerceIn(0, 255)
    private fun normalizeTriggerAxis(value: Float): Float = if (value < 0f) ((value + 1f) / 2f).coerceIn(0f, 1f) else value.coerceIn(0f, 1f)
}

open class SimpleSdpObserver : SdpObserver {
    override fun onCreateSuccess(description: SessionDescription?) = Unit
    override fun onSetSuccess() = Unit
    override fun onCreateFailure(error: String?) = Unit
    override fun onSetFailure(error: String?) = Unit
}

object SdpTools {
    data class RewriteResult(val sdp: String, val replacements: Int)

    fun fixServerIp(sdp: String, serverIp: String): String {
        val ip = extractPublicIp(serverIp) ?: return sdp
        return sdp
            .replace("c=IN IP4 0.0.0.0", "c=IN IP4 $ip")
            .replace(Regex("(a=candidate:\\S+\\s+\\d+\\s+\\w+\\s+\\d+\\s+)0\\.0\\.0\\.0(\\s+)"), "$1$ip$2")
    }

    fun preferCodec(sdp: String, settings: StreamSettings): String =
        preferCodec(sdp, settings.codec, settings.prefersTenBitVideo())

    fun preferCodec(sdp: String, codec: VideoCodec): String =
        preferCodec(sdp, codec, preferTenBit = codec != VideoCodec.H265)

    private fun preferCodec(sdp: String, codec: VideoCodec, preferTenBit: Boolean): String {
        val target = when (codec) {
            VideoCodec.H264 -> "H264"
            VideoCodec.H265 -> "H265"
            VideoCodec.AV1 -> "AV1"
        }
        val lineEnding = if (sdp.contains("\r\n")) "\r\n" else "\n"
        val lines = sdp.split(Regex("\\r?\\n"))
        var inVideo = false
        val codecByPt = mutableMapOf<String, String>()
        val rtxApt = mutableMapOf<String, String>()
        val fmtpByPt = mutableMapOf<String, String>()
        lines.forEach { line ->
            if (line.startsWith("m=video")) inVideo = true else if (line.startsWith("m=") && inVideo) inVideo = false
            if (inVideo && line.startsWith("a=rtpmap:")) {
                val rest = line.substringAfter(":")
                val pt = rest.substringBefore(" ")
                val name = rest.substringAfter(" ").substringBefore("/").uppercase(Locale.US).let { if (it == "HEVC") "H265" else it }
                codecByPt[pt] = name
            }
            if (inVideo && line.startsWith("a=fmtp:")) {
                val rest = line.substringAfter(":")
                val pt = rest.substringBefore(" ")
                val params = rest.substringAfter(" ", "")
                fmtpByPt[pt] = params
                Regex("(?:^|;)\\s*apt=(\\d+)").find(params)?.groupValues?.getOrNull(1)?.let { rtxApt[pt] = it }
            }
        }
        val preferred = codecByPt.filterValues { it == target }.keys.toMutableList()
        if (preferred.isEmpty()) return sdp
        if (codec == VideoCodec.H265) {
            preferred.sortBy { pt -> h265ProfilePriority(fmtpByPt[pt], preferTenBit) }
        }
        val allowed = preferred.toMutableSet()
        rtxApt.forEach { (rtx, apt) ->
            if (apt in preferred && codecByPt[rtx] == "RTX") allowed += rtx
        }
        val output = mutableListOf<String>()
        inVideo = false
        lines.forEach { line ->
            if (line.startsWith("m=video")) {
                inVideo = true
                val parts = line.split(Regex("\\s+"))
                val ordered = preferred + parts.drop(3).filter { it in allowed && it !in preferred }
                output += if (ordered.isNotEmpty()) (parts.take(3) + ordered).joinToString(" ") else line
                return@forEach
            }
            if (line.startsWith("m=") && inVideo) inVideo = false
            if (inVideo && (line.startsWith("a=rtpmap:") || line.startsWith("a=fmtp:") || line.startsWith("a=rtcp-fb:"))) {
                val pt = line.substringAfter(":").substringBefore(" ")
                if (pt !in allowed) return@forEach
            }
            output += line
        }
        return output.joinToString(lineEnding)
    }

    fun rewriteH265TierFlag(sdp: String, tierFlag: Int): RewriteResult {
        val payloads = h265PayloadTypes(sdp)
        if (payloads.isEmpty()) return RewriteResult(sdp, 0)
        val lineEnding = if (sdp.contains("\r\n")) "\r\n" else "\n"
        var replacements = 0
        val output = sdp.split(Regex("\\r?\\n")).map { line ->
            if (!line.startsWith("a=fmtp:")) return@map line
            val pt = line.substringAfter(":").substringBefore(" ")
            if (pt !in payloads) return@map line
            val next = line.replace(Regex("tier-flag=1", RegexOption.IGNORE_CASE), "tier-flag=$tierFlag")
            if (next != line) replacements += 1
            next
        }
        return RewriteResult(output.joinToString(lineEnding), replacements)
    }

    fun rewriteH265LevelIdByProfile(sdp: String, maxLevelByProfile: Map<Int, Int>): RewriteResult {
        val payloads = h265PayloadTypes(sdp)
        if (payloads.isEmpty() || maxLevelByProfile.isEmpty()) return RewriteResult(sdp, 0)
        val lineEnding = if (sdp.contains("\r\n")) "\r\n" else "\n"
        var replacements = 0
        val output = sdp.split(Regex("\\r?\\n")).map { line ->
            if (!line.startsWith("a=fmtp:")) return@map line
            val rest = line.substringAfter(":")
            val pt = rest.substringBefore(" ")
            val params = rest.substringAfter(" ", "")
            if (pt !in payloads || params.isBlank()) return@map line
            val profile = Regex("(?:^|;)\\s*profile-id=(\\d+)", RegexOption.IGNORE_CASE)
                .find(params)
                ?.groupValues
                ?.getOrNull(1)
                ?.toIntOrNull()
                ?: return@map line
            val level = Regex("(?:^|;)\\s*level-id=(\\d+)", RegexOption.IGNORE_CASE)
                .find(params)
                ?.groupValues
                ?.getOrNull(1)
                ?.toIntOrNull()
                ?: return@map line
            val maxLevel = maxLevelByProfile[profile] ?: return@map line
            if (level <= maxLevel) return@map line
            val next = line.replace(Regex("(level-id=)(\\d+)", RegexOption.IGNORE_CASE), "$1$maxLevel")
            if (next != line) replacements += 1
            next
        }
        return RewriteResult(output.joinToString(lineEnding), replacements)
    }

    fun negotiatesCodec(sdp: String, codec: VideoCodec): Boolean {
        val target = when (codec) {
            VideoCodec.H264 -> "H264"
            VideoCodec.H265 -> "H265"
            VideoCodec.AV1 -> "AV1"
        }
        var inVideo = false
        sdp.split(Regex("\\r?\\n")).forEach { line ->
            if (line.startsWith("m=video")) {
                inVideo = true
                return@forEach
            }
            if (line.startsWith("m=") && inVideo) {
                inVideo = false
            }
            if (!inVideo || !line.startsWith("a=rtpmap:")) return@forEach
            val codecName = line.substringAfter(" ")
                .substringBefore("/")
                .uppercase(Locale.US)
                .let { if (it == "HEVC") "H265" else it }
            if (codecName == target) return true
        }
        return false
    }

    private fun h265PayloadTypes(sdp: String): Set<String> {
        var inVideo = false
        val payloads = mutableSetOf<String>()
        sdp.split(Regex("\\r?\\n")).forEach { line ->
            if (line.startsWith("m=video")) {
                inVideo = true
                return@forEach
            }
            if (line.startsWith("m=") && inVideo) {
                inVideo = false
            }
            if (!inVideo || !line.startsWith("a=rtpmap:")) return@forEach
            val rest = line.substringAfter(":")
            val pt = rest.substringBefore(" ")
            val codecName = rest.substringAfter(" ")
                .substringBefore("/")
                .uppercase(Locale.US)
                .let { if (it == "HEVC") "H265" else it }
            if (pt.isNotBlank() && codecName == "H265") payloads += pt
        }
        return payloads
    }

    private fun h265ProfilePriority(fmtp: String?, preferTenBit: Boolean): Int {
        val profileId = Regex("(?:^|;)\\s*profile-id=(\\d+)")
            .find(fmtp.orEmpty())
            ?.groupValues
            ?.getOrNull(1)
        return if (preferTenBit) {
            when (profileId) {
                "2" -> 0
                "1" -> 1
                else -> 2
            }
        } else {
            when (profileId) {
                "1" -> 0
                null -> 1
                "2" -> 2
                else -> 3
            }
        }
    }

    private fun StreamSettings.prefersTenBitVideo(): Boolean =
        hdrEnabled ||
            colorQuality == ColorQuality.TenBit420 ||
            colorQuality == ColorQuality.TenBit444

    fun mungeAnswerSdp(sdp: String, maxBitrateKbps: Int): String {
        val lineEnding = if (sdp.contains("\r\n")) "\r\n" else "\n"
        val out = mutableListOf<String>()
        val lines = sdp.split(Regex("\\r?\\n"))
        lines.forEachIndexed { index, line ->
            val rewritten = if (line.startsWith("a=fmtp:") && line.contains("minptime=") && !line.contains("stereo=1")) "$line;stereo=1" else line
            out += rewritten
            if ((line.startsWith("m=video") || line.startsWith("m=audio")) && !lines.getOrNull(index + 1).orEmpty().startsWith("b=")) {
                out += if (line.startsWith("m=video")) "b=AS:$maxBitrateKbps" else "b=AS:128"
            }
        }
        return out.joinToString(lineEnding)
    }

    fun parseInputProtocolVersion(sdp: String): Int =
        Regex("a=ri\\.version:(\\d+)").find(sdp)?.groupValues?.getOrNull(1)?.toIntOrNull()
            ?: DEFAULT_INPUT_PROTOCOL_VERSION

    fun parsePartialReliableThresholdMs(sdp: String): Int =
        Regex("a=ri\\.partialReliableThresholdMs:(\\d+)")
            .find(sdp)
            ?.groupValues
            ?.getOrNull(1)
            ?.toIntOrNull()
            ?.coerceIn(1, 5000)
            ?: 30

    fun parsePartiallyReliableGamepadMask(sdp: String): Int =
        parseRiIntegerAttribute(
            sdp,
            "ri.enablePartiallyReliableTransferGamepad",
            PARTIALLY_RELIABLE_GAMEPAD_MASK_ALL,
        )

    fun buildNvstSdp(offerSdp: String, settings: StreamSettings, localAnswer: String): String {
        val (width, height) = streamResolutionPixels(settings)
        val ufrag = Regex("a=ice-ufrag:([^\\r\\n]+)").find(localAnswer)?.groupValues?.getOrNull(1)?.trim().orEmpty()
        val pwd = Regex("a=ice-pwd:([^\\r\\n]+)").find(localAnswer)?.groupValues?.getOrNull(1)?.trim().orEmpty()
        val fingerprint = Regex("a=fingerprint:sha-256 ([^\\r\\n]+)").find(localAnswer)?.groupValues?.getOrNull(1)?.trim().orEmpty()
        val threshold = Regex("a=ri\\.partialReliableThresholdMs:(\\d+)").find(offerSdp)?.groupValues?.getOrNull(1)?.toIntOrNull() ?: 30
        val bitDepth = if (settings.hdrEnabled || settings.colorQuality == ColorQuality.TenBit420 || settings.colorQuality == ColorQuality.TenBit444) 10 else 8
        val maxBitrate = settings.maxBitrateMbps * 1000
        val minBitrate = max(5000, (maxBitrate * 0.35f).roundToInt())
        val initialBitrate = max(minBitrate, (maxBitrate * 0.7f).roundToInt())
        val isHighFps = settings.fps >= 90
        val is120Fps = settings.fps == 120
        val is240Fps = settings.fps >= 240
        val isAv1 = settings.codec == VideoCodec.AV1
        return buildList {
            add("v=0")
            add("o=SdpTest test_id_13 14 IN IPv4 127.0.0.1")
            add("s=-")
            add("t=0 0")
            add("a=general.icePassword:$pwd")
            add("a=general.iceUserNameFragment:$ufrag")
            add("a=general.dtlsFingerprint:$fingerprint")
            add("m=video 0 RTP/AVP")
            add("a=msid:fbc-video-0")
            add("a=vqos.fec.rateDropWindow:10")
            add("a=vqos.fec.minRequiredFecPackets:2")
            add("a=vqos.fec.repairMinPercent:5")
            add("a=vqos.fec.repairPercent:5")
            add("a=vqos.fec.repairMaxPercent:35")
            add("a=vqos.dynamicStreamingMode:0")
            add("a=vqos.drc.enable:0")
            add("a=video.dx9EnableNv12:1")
            add("a=video.dx9EnableHdr:1")
            add("a=vqos.qpg.enable:1")
            add("a=vqos.resControl.qp.qpg.featureSetting:7")
            add("a=bwe.useOwdCongestionControl:1")
            add("a=video.enableRtpNack:1")
            add("a=vqos.bw.txRxLag.minFeedbackTxDeltaMs:200")
            add("a=vqos.drc.bitrateIirFilterFactor:18")
            add("a=video.packetSize:1140")
            add("a=packetPacing.minNumPacketsPerGroup:15")
            if (isHighFps) {
                add("a=vqos.dfc.enable:1")
                add("a=vqos.dfc.decodeFpsAdjPercent:85")
                add("a=vqos.dfc.targetDownCooldownMs:250")
                add("a=vqos.dfc.dfcAlgoVersion:${if (is120Fps || is240Fps) 2 else 1}")
                add("a=vqos.dfc.minTargetFps:${if (is120Fps || is240Fps) 100 else 60}")
                add("a=vqos.resControl.dfc.useClientFpsPerf:0")
                add("a=vqos.dfc.adjustResAndFps:0")
                add("a=bwe.iirFilterFactor:8")
                add("a=video.encoderFeatureSetting:47")
                add("a=video.encoderPreset:6")
                add("a=vqos.resControl.cpmRtc.badNwSkipFramesCount:600")
                add("a=vqos.resControl.cpmRtc.decodeTimeThresholdMs:9")
                add("a=video.fbcDynamicFpsGrabTimeoutMs:${if (is120Fps) 6 else 18}")
                add("a=vqos.resControl.cpmRtc.serverResolutionUpdateCoolDownCount:${if (is120Fps) 6000 else 12000}")
            } else {
                add("a=vqos.dfc.enable:0")
                add("a=vqos.dfc.adjustResAndFps:0")
            }
            if (is240Fps) {
                add("a=video.enableNextCaptureMode:1")
                add("a=vqos.maxStreamFpsEstimate:240")
                add("a=video.videoSplitEncodeStripsPerFrame:3")
                add("a=video.updateSplitEncodeStateDynamically:1")
                add("a=vqos.rtcPreemptiveIdrSettings.minBurstNackSize:65535")
                add("a=vqos.rtcPreemptiveIdrSettings.minNackPacketCaptureAgeMs:65535")
            }
            add("a=vqos.adjustStreamingFpsDuringOutOfFocus:0")
            add("a=vqos.resControl.cpmRtc.ignoreOutOfFocusWindowState:1")
            add("a=vqos.resControl.perfHistory.rtcIgnoreOutOfFocusWindowState:1")
            add("a=vqos.resControl.cpmRtc.featureMask:0")
            add("a=vqos.resControl.cpmRtc.enable:0")
            add("a=vqos.resControl.cpmRtc.minResolutionPercent:100")
            add("a=vqos.resControl.cpmRtc.resolutionChangeHoldonMs:999999")
            add("a=packetPacing.numGroups:${if (is120Fps) 3 else 5}")
            add("a=packetPacing.maxDelayUs:1000")
            add("a=packetPacing.minNumPacketsFrame:10")
            add("a=video.rtpNackQueueLength:1024")
            add("a=video.rtpNackQueueMaxPackets:512")
            add("a=video.rtpNackMaxPacketCount:25")
            add("a=vqos.drc.qpMaxResThresholdAdj:4")
            add("a=vqos.grc.qpMaxResThresholdAdj:4")
            add("a=vqos.drc.iirFilterFactor:100")
            if (isAv1) {
                add("a=vqos.drc.minQpHeadroom:20")
                add("a=vqos.drc.lowerQpThreshold:100")
                add("a=vqos.drc.upperQpThreshold:200")
                add("a=vqos.drc.minAdaptiveQpThreshold:180")
                add("a=vqos.drc.qpCodecThresholdAdj:0")
                add("a=vqos.drc.qpMaxResThresholdAdj:20")
                add("a=vqos.dfc.minQpHeadroom:20")
                add("a=vqos.dfc.qpLowerLimit:100")
                add("a=vqos.dfc.qpMaxUpperLimit:200")
                add("a=vqos.dfc.qpMinUpperLimit:180")
                add("a=vqos.dfc.qpMaxResThresholdAdj:20")
                add("a=vqos.dfc.qpCodecThresholdAdj:0")
                add("a=vqos.grc.minQpHeadroom:20")
                add("a=vqos.grc.lowerQpThreshold:100")
                add("a=vqos.grc.upperQpThreshold:200")
                add("a=vqos.grc.minAdaptiveQpThreshold:180")
                add("a=vqos.grc.qpMaxResThresholdAdj:20")
                add("a=vqos.grc.qpCodecThresholdAdj:0")
                add("a=video.minQp:25")
                add("a=video.enableAv1RcPrecisionFactor:1")
            }
            add("a=video.clientViewportWd:$width")
            add("a=video.clientViewportHt:$height")
            add("a=video.maxFPS:${settings.fps}")
            add("a=video.initialBitrateKbps:$initialBitrate")
            add("a=video.initialPeakBitrateKbps:$maxBitrate")
            add("a=vqos.bw.maximumBitrateKbps:$maxBitrate")
            add("a=vqos.bw.minimumBitrateKbps:$minBitrate")
            add("a=vqos.bw.peakBitrateKbps:$maxBitrate")
            add("a=vqos.bw.serverPeakBitrateKbps:$maxBitrate")
            add("a=vqos.bw.enableBandwidthEstimation:1")
            add("a=vqos.bw.disableBitrateLimit:0")
            add("a=vqos.grc.maximumBitrateKbps:$maxBitrate")
            add("a=vqos.grc.enable:0")
            add("a=video.maxNumReferenceFrames:4")
            add("a=video.mapRtpTimestampsToFrames:1")
            add("a=video.encoderCscMode:3")
            add("a=video.dynamicRangeMode:0")
            add("a=video.bitDepth:$bitDepth")
            add("a=video.scalingFeature1:${if (isAv1) 1 else 0}")
            add("a=video.prefilterParams.prefilterModel:0")
            add("m=audio 0 RTP/AVP")
            add("a=msid:audio")
            add("m=mic 0 RTP/AVP")
            add("a=msid:mic")
            add("a=rtpmap:0 PCMU/8000")
            add("m=application 0 RTP/AVP")
            add("a=msid:input_1")
            add("a=ri.partialReliableThresholdMs:$threshold")
            add("a=ri.hidDeviceMask:4294967295")
            add("a=ri.enablePartiallyReliableTransferGamepad:15")
            add("a=ri.enablePartiallyReliableTransferHid:4294967295")
            add("")
        }.joinToString("\n")
    }

    private fun extractPublicIp(hostOrIp: String): String? {
        if (Regex("^\\d{1,3}(\\.\\d{1,3}){3}$").matches(hostOrIp)) return hostOrIp
        val first = hostOrIp.substringBefore(".")
        val parts = first.split("-")
        return if (parts.size == 4 && parts.all { it.all(Char::isDigit) }) parts.joinToString(".") else null
    }

    private fun parseRiIntegerAttribute(sdp: String, attribute: String, fallback: Int): Int {
        val escaped = Regex.escape(attribute)
        val raw = Regex("a=$escaped:([^\\r\\n]+)", RegexOption.IGNORE_CASE)
            .find(sdp)
            ?.groupValues
            ?.getOrNull(1)
            ?.trim()
            ?: return fallback
        val parsed = if (raw.startsWith("0x", ignoreCase = true)) {
            raw.drop(2).toIntOrNull(16)
        } else {
            raw.toIntOrNull()
        }
        return parsed ?: fallback
    }

    private const val PARTIALLY_RELIABLE_GAMEPAD_MASK_ALL = 0x0f
}

class InputEncoder {
    private var protocolVersion = 3
    private val gamepadSequences = mutableMapOf<Int, Int>()

    fun setProtocolVersion(version: Int) {
        protocolVersion = version.coerceAtLeast(1)
    }

    fun resetGamepadSequences() {
        gamepadSequences.clear()
    }

    fun encodeHeartbeat(): ByteArray = ByteBuffer.allocate(4).order(ByteOrder.LITTLE_ENDIAN).putInt(INPUT_HEARTBEAT).array()

    fun encodeKeyDown(key: KeyboardPayload): ByteArray = encodeKey(INPUT_KEY_DOWN, key)
    fun encodeKeyUp(key: KeyboardPayload): ByteArray = encodeKey(INPUT_KEY_UP, key)

    fun encodeMouseMove(dx: Int, dy: Int): ByteArray {
        val bytes = ByteArray(22)
        ByteBuffer.wrap(bytes).order(ByteOrder.LITTLE_ENDIAN).putInt(INPUT_MOUSE_REL)
        ByteBuffer.wrap(bytes).order(ByteOrder.BIG_ENDIAN)
            .putShort(4, dx.coerceIn(-32768, 32767).toShort())
            .putShort(6, dy.coerceIn(-32768, 32767).toShort())
            .putShort(8, 0.toShort())
            .putInt(10, 0)
            .putLong(14, timestampUs())
        return wrapMouseMove(bytes)
    }

    fun encodeMouseButton(type: Int, button: Int): ByteArray {
        val bytes = ByteArray(18)
        ByteBuffer.wrap(bytes).order(ByteOrder.LITTLE_ENDIAN).putInt(type)
        bytes[4] = button.coerceIn(1, 5).toByte()
        bytes[5] = 0
        ByteBuffer.wrap(bytes).order(ByteOrder.BIG_ENDIAN).putInt(6, 0).putLong(10, timestampUs())
        return wrapSingle(bytes)
    }

    fun encodeMouseWheel(delta: Int): ByteArray {
        val bytes = ByteArray(22)
        ByteBuffer.wrap(bytes).order(ByteOrder.LITTLE_ENDIAN).putInt(INPUT_MOUSE_WHEEL)
        ByteBuffer.wrap(bytes).order(ByteOrder.BIG_ENDIAN)
            .putShort(4, 0.toShort())
            .putShort(6, delta.coerceIn(-32768, 32767).toShort())
            .putShort(8, 0.toShort())
            .putInt(10, 0)
            .putLong(14, timestampUs())
        return wrapSingle(bytes)
    }

    fun encodeHapticsEnabled(enabled: Boolean): ByteArray {
        val bytes = ByteArray(6)
        ByteBuffer.wrap(bytes).order(ByteOrder.LITTLE_ENDIAN).putInt(INPUT_HAPTICS_ENABLED)
        ByteBuffer.wrap(bytes).order(ByteOrder.BIG_ENDIAN).putShort(4, (if (enabled) 1 else 0).toShort())
        return wrapSingle(bytes)
    }

    fun encodeGamepadState(
        controllerId: Int,
        buttons: Int,
        leftTrigger: Int,
        rightTrigger: Int,
        leftStickX: Int,
        leftStickY: Int,
        rightStickX: Int,
        rightStickY: Int,
        bitmap: Int,
        partiallyReliable: Boolean,
        timestampUs: Long = timestampUs(),
    ): ByteArray {
        val bytes = ByteArray(38)
        val le = ByteBuffer.wrap(bytes).order(ByteOrder.LITTLE_ENDIAN)
        le.putInt(0, INPUT_GAMEPAD)
        le.putShort(4, 26.toShort())
        le.putShort(6, (controllerId and 0x03).toShort())
        le.putShort(8, bitmap.toShort())
        le.putShort(10, 20.toShort())
        le.putShort(12, buttons.toShort())
        le.putShort(14, ((leftTrigger and 0xff) or ((rightTrigger and 0xff) shl 8)).toShort())
        le.putShort(16, leftStickX.toShort())
        le.putShort(18, leftStickY.toShort())
        le.putShort(20, rightStickX.toShort())
        le.putShort(22, rightStickY.toShort())
        le.putShort(24, 0.toShort())
        le.putShort(26, 85.toShort())
        le.putShort(28, 0.toShort())
        le.putLong(30, timestampUs)
        return if (partiallyReliable) wrapGamepadPartiallyReliable(bytes, controllerId) else wrapGamepadReliable(bytes)
    }

    private fun encodeKey(type: Int, key: KeyboardPayload): ByteArray {
        val bytes = ByteArray(18)
        ByteBuffer.wrap(bytes).order(ByteOrder.LITTLE_ENDIAN).putInt(type)
        ByteBuffer.wrap(bytes).order(ByteOrder.BIG_ENDIAN)
            .putShort(4, key.keycode.toShort())
            .putShort(6, key.modifiers.toShort())
            .putShort(8, key.scancode.toShort())
            .putLong(10, key.timestampUs)
        return wrapSingle(bytes)
    }

    private fun wrapSingle(payload: ByteArray): ByteArray {
        if (protocolVersion <= 2) return payload
        return ByteArray(10 + payload.size).also {
            it[0] = 0x23
            ByteBuffer.wrap(it).order(ByteOrder.BIG_ENDIAN).putLong(1, timestampUs())
            it[9] = 0x22
            payload.copyInto(it, 10)
        }
    }

    private fun wrapMouseMove(payload: ByteArray): ByteArray {
        if (protocolVersion <= 2) return payload
        return ByteArray(12 + payload.size).also {
            it[0] = 0x23
            ByteBuffer.wrap(it).order(ByteOrder.BIG_ENDIAN).putLong(1, timestampUs())
            it[9] = 0x21
            ByteBuffer.wrap(it).order(ByteOrder.BIG_ENDIAN).putShort(10, payload.size.toShort())
            payload.copyInto(it, 12)
        }
    }

    private fun wrapGamepadReliable(payload: ByteArray): ByteArray {
        if (protocolVersion <= 2) return payload
        return ByteArray(12 + payload.size).also {
            it[0] = 0x23
            ByteBuffer.wrap(it).order(ByteOrder.BIG_ENDIAN).putLong(1, timestampUs())
            it[9] = 0x21
            ByteBuffer.wrap(it).order(ByteOrder.BIG_ENDIAN).putShort(10, payload.size.toShort())
            payload.copyInto(it, 12)
        }
    }

    private fun wrapGamepadPartiallyReliable(payload: ByteArray, index: Int): ByteArray {
        if (protocolVersion <= 2) return payload
        val seq = gamepadSequences[index] ?: 1
        gamepadSequences[index] = (seq + 1) and 0xffff
        return ByteArray(16 + payload.size).also {
            it[0] = 0x23
            val be = ByteBuffer.wrap(it).order(ByteOrder.BIG_ENDIAN)
            be.putLong(1, timestampUs())
            it[9] = 0x26
            it[10] = (index and 0xff).toByte()
            be.putShort(11, seq.toShort())
            it[13] = 0x21
            be.putShort(14, payload.size.toShort())
            payload.copyInto(it, 16)
        }
    }

    data class KeyboardPayload(
        val keycode: Int,
        val scancode: Int,
        val modifiers: Int,
        val timestampUs: Long = timestampUs(),
    )

    data class TextKeySpec(
        val keycode: Int,
        val scancode: Int,
        val shift: Boolean = false,
    ) {
        fun toKeyboardPayload(modifiers: Int): KeyboardPayload =
            KeyboardPayload(keycode, scancode, modifiers)
    }

    companion object {
        const val INPUT_HEARTBEAT = 2
        const val INPUT_KEY_DOWN = 3
        const val INPUT_KEY_UP = 4
        const val INPUT_MOUSE_REL = 7
        const val INPUT_MOUSE_BUTTON_DOWN = 8
        const val INPUT_MOUSE_BUTTON_UP = 9
        const val INPUT_MOUSE_WHEEL = 10
        const val INPUT_GAMEPAD = 12
        const val INPUT_HAPTICS_ENABLED = 13

        fun mapKeyEvent(event: KeyEvent): KeyboardPayload? {
            if (event.action != KeyEvent.ACTION_DOWN && event.action != KeyEvent.ACTION_UP) return null
            return mapKeyboardPayload(
                keyCode = event.keyCode,
                unicode = event.unicodeChar,
                scanCode = event.scanCode,
                shift = event.isShiftPressed,
                ctrl = event.isCtrlPressed,
                alt = event.isAltPressed,
                meta = event.isMetaPressed,
                capsLock = event.isCapsLockOn,
                numLock = event.isNumLockOn,
            )
        }

        internal fun mapKeyboardPayload(
            keyCode: Int,
            unicode: Int,
            scanCode: Int,
            shift: Boolean = false,
            ctrl: Boolean = false,
            alt: Boolean = false,
            meta: Boolean = false,
            capsLock: Boolean = false,
            numLock: Boolean = false,
            timestampUs: Long = timestampUs(),
        ): KeyboardPayload? {
            val vk = virtualKey(keyCode, unicode)
            val resolvedScanCode = if (scanCode > 0) scanCode else fallbackScanCode(keyCode)
            if (vk == null || resolvedScanCode == null) return null
            var modifiers = 0
            if (shift) modifiers = modifiers or 0x01
            if (ctrl) modifiers = modifiers or 0x02
            if (alt) modifiers = modifiers or 0x04
            if (meta) modifiers = modifiers or 0x08
            if (capsLock) modifiers = modifiers or 0x10
            if (numLock) modifiers = modifiers or 0x20
            return KeyboardPayload(vk, resolvedScanCode, modifiers, timestampUs)
        }

        internal fun mapTextCharToKeySpec(char: Char): TextKeySpec? {
            val mapped = when (char) {
                in 'a'..'z' -> textKeySpecFromAndroidKeyCode(KeyEvent.KEYCODE_A + (char - 'a'))
                in 'A'..'Z' -> textKeySpecFromAndroidKeyCode(KeyEvent.KEYCODE_A + (char - 'A'), shift = true)
                in '0'..'9' -> textKeySpecFromAndroidKeyCode(KeyEvent.KEYCODE_0 + (char - '0'))
                '\n', '\r' -> textKeySpecFromAndroidKeyCode(KeyEvent.KEYCODE_ENTER)
                else -> textBaseKeyCodes[char]?.let(::textKeySpecFromAndroidKeyCode)
                    ?: textShiftedKeyCodes[char]?.let { textKeySpecFromAndroidKeyCode(it, shift = true) }
            }
            return mapped
        }

        internal fun shiftLeftPayload(modifiers: Int): KeyboardPayload =
            KeyboardPayload(0xa0, fallbackScanCode(KeyEvent.KEYCODE_SHIFT_LEFT) ?: 0x002a, modifiers)

        private fun textKeySpecFromAndroidKeyCode(keyCode: Int, shift: Boolean = false): TextKeySpec? {
            val payload = mapKeyboardPayload(
                keyCode = keyCode,
                unicode = 0,
                scanCode = 0,
                shift = shift,
                timestampUs = 0L,
            ) ?: return null
            return TextKeySpec(payload.keycode, payload.scancode, shift)
        }

        private val textBaseKeyCodes = mapOf(
            ' ' to KeyEvent.KEYCODE_SPACE,
            '-' to KeyEvent.KEYCODE_MINUS,
            '=' to KeyEvent.KEYCODE_EQUALS,
            '[' to KeyEvent.KEYCODE_LEFT_BRACKET,
            ']' to KeyEvent.KEYCODE_RIGHT_BRACKET,
            '\\' to KeyEvent.KEYCODE_BACKSLASH,
            ';' to KeyEvent.KEYCODE_SEMICOLON,
            '\'' to KeyEvent.KEYCODE_APOSTROPHE,
            ',' to KeyEvent.KEYCODE_COMMA,
            '.' to KeyEvent.KEYCODE_PERIOD,
            '/' to KeyEvent.KEYCODE_SLASH,
            '`' to KeyEvent.KEYCODE_GRAVE,
        )

        private val textShiftedKeyCodes = mapOf(
            '!' to KeyEvent.KEYCODE_1,
            '@' to KeyEvent.KEYCODE_2,
            '#' to KeyEvent.KEYCODE_3,
            '$' to KeyEvent.KEYCODE_4,
            '%' to KeyEvent.KEYCODE_5,
            '^' to KeyEvent.KEYCODE_6,
            '&' to KeyEvent.KEYCODE_7,
            '*' to KeyEvent.KEYCODE_8,
            '(' to KeyEvent.KEYCODE_9,
            ')' to KeyEvent.KEYCODE_0,
            '_' to KeyEvent.KEYCODE_MINUS,
            '+' to KeyEvent.KEYCODE_EQUALS,
            '{' to KeyEvent.KEYCODE_LEFT_BRACKET,
            '}' to KeyEvent.KEYCODE_RIGHT_BRACKET,
            '|' to KeyEvent.KEYCODE_BACKSLASH,
            ':' to KeyEvent.KEYCODE_SEMICOLON,
            '"' to KeyEvent.KEYCODE_APOSTROPHE,
            '<' to KeyEvent.KEYCODE_COMMA,
            '>' to KeyEvent.KEYCODE_PERIOD,
            '?' to KeyEvent.KEYCODE_SLASH,
            '~' to KeyEvent.KEYCODE_GRAVE,
        )

        private fun virtualKey(keyCode: Int, unicode: Int): Int? =
            when (keyCode) {
                KeyEvent.KEYCODE_ENTER -> 0x0d
                KeyEvent.KEYCODE_ESCAPE -> 0x1b
                KeyEvent.KEYCODE_DEL -> 0x08
                KeyEvent.KEYCODE_TAB -> 0x09
                KeyEvent.KEYCODE_SPACE -> 0x20
                KeyEvent.KEYCODE_DPAD_LEFT -> 0x25
                KeyEvent.KEYCODE_DPAD_UP -> 0x26
                KeyEvent.KEYCODE_DPAD_RIGHT -> 0x27
                KeyEvent.KEYCODE_DPAD_DOWN -> 0x28
                KeyEvent.KEYCODE_PAGE_UP -> 0x21
                KeyEvent.KEYCODE_PAGE_DOWN -> 0x22
                KeyEvent.KEYCODE_FORWARD_DEL -> 0x2e
                KeyEvent.KEYCODE_INSERT -> 0x2d
                KeyEvent.KEYCODE_MOVE_HOME -> 0x24
                KeyEvent.KEYCODE_MOVE_END -> 0x23
                KeyEvent.KEYCODE_SHIFT_LEFT,
                KeyEvent.KEYCODE_SHIFT_RIGHT,
                -> 0x10
                KeyEvent.KEYCODE_CTRL_LEFT,
                KeyEvent.KEYCODE_CTRL_RIGHT,
                -> 0x11
                KeyEvent.KEYCODE_ALT_LEFT,
                KeyEvent.KEYCODE_ALT_RIGHT,
                -> 0x12
                KeyEvent.KEYCODE_CAPS_LOCK -> 0x14
                KeyEvent.KEYCODE_NUM_LOCK -> 0x90
                KeyEvent.KEYCODE_SCROLL_LOCK -> 0x91
                KeyEvent.KEYCODE_MINUS -> 0xbd
                KeyEvent.KEYCODE_EQUALS -> 0xbb
                KeyEvent.KEYCODE_LEFT_BRACKET -> 0xdb
                KeyEvent.KEYCODE_RIGHT_BRACKET -> 0xdd
                KeyEvent.KEYCODE_BACKSLASH -> 0xdc
                KeyEvent.KEYCODE_SEMICOLON -> 0xba
                KeyEvent.KEYCODE_APOSTROPHE -> 0xde
                KeyEvent.KEYCODE_COMMA -> 0xbc
                KeyEvent.KEYCODE_PERIOD -> 0xbe
                KeyEvent.KEYCODE_SLASH -> 0xbf
                KeyEvent.KEYCODE_GRAVE -> 0xc0
                in KeyEvent.KEYCODE_A..KeyEvent.KEYCODE_Z -> 0x41 + (keyCode - KeyEvent.KEYCODE_A)
                in KeyEvent.KEYCODE_0..KeyEvent.KEYCODE_9 -> 0x30 + (keyCode - KeyEvent.KEYCODE_0)
                in KeyEvent.KEYCODE_NUMPAD_0..KeyEvent.KEYCODE_NUMPAD_9 -> 0x60 + (keyCode - KeyEvent.KEYCODE_NUMPAD_0)
                in KeyEvent.KEYCODE_F1..KeyEvent.KEYCODE_F12 -> 0x70 + (keyCode - KeyEvent.KEYCODE_F1)
                else -> unicode.takeIf { it in 1..255 }?.let { Character.toUpperCase(it.toChar()).code }
            }

        private fun fallbackScanCode(keyCode: Int): Int? =
            when (keyCode) {
                KeyEvent.KEYCODE_A -> 0x001e
                KeyEvent.KEYCODE_B -> 0x0030
                KeyEvent.KEYCODE_C -> 0x002e
                KeyEvent.KEYCODE_D -> 0x0020
                KeyEvent.KEYCODE_E -> 0x0012
                KeyEvent.KEYCODE_F -> 0x0021
                KeyEvent.KEYCODE_G -> 0x0022
                KeyEvent.KEYCODE_H -> 0x0023
                KeyEvent.KEYCODE_I -> 0x0017
                KeyEvent.KEYCODE_J -> 0x0024
                KeyEvent.KEYCODE_K -> 0x0025
                KeyEvent.KEYCODE_L -> 0x0026
                KeyEvent.KEYCODE_M -> 0x0032
                KeyEvent.KEYCODE_N -> 0x0031
                KeyEvent.KEYCODE_O -> 0x0018
                KeyEvent.KEYCODE_P -> 0x0019
                KeyEvent.KEYCODE_Q -> 0x0010
                KeyEvent.KEYCODE_R -> 0x0013
                KeyEvent.KEYCODE_S -> 0x001f
                KeyEvent.KEYCODE_T -> 0x0014
                KeyEvent.KEYCODE_U -> 0x0016
                KeyEvent.KEYCODE_V -> 0x002f
                KeyEvent.KEYCODE_W -> 0x0011
                KeyEvent.KEYCODE_X -> 0x002d
                KeyEvent.KEYCODE_Y -> 0x0015
                KeyEvent.KEYCODE_Z -> 0x002c
                KeyEvent.KEYCODE_1 -> 0x0002
                KeyEvent.KEYCODE_2 -> 0x0003
                KeyEvent.KEYCODE_3 -> 0x0004
                KeyEvent.KEYCODE_4 -> 0x0005
                KeyEvent.KEYCODE_5 -> 0x0006
                KeyEvent.KEYCODE_6 -> 0x0007
                KeyEvent.KEYCODE_7 -> 0x0008
                KeyEvent.KEYCODE_8 -> 0x0009
                KeyEvent.KEYCODE_9 -> 0x000a
                KeyEvent.KEYCODE_0 -> 0x000b
                KeyEvent.KEYCODE_NUMPAD_7 -> 0x0047
                KeyEvent.KEYCODE_NUMPAD_8 -> 0x0048
                KeyEvent.KEYCODE_NUMPAD_9 -> 0x0049
                KeyEvent.KEYCODE_NUMPAD_4 -> 0x004b
                KeyEvent.KEYCODE_NUMPAD_5 -> 0x004c
                KeyEvent.KEYCODE_NUMPAD_6 -> 0x004d
                KeyEvent.KEYCODE_NUMPAD_1 -> 0x004f
                KeyEvent.KEYCODE_NUMPAD_2 -> 0x0050
                KeyEvent.KEYCODE_NUMPAD_3 -> 0x0051
                KeyEvent.KEYCODE_NUMPAD_0 -> 0x0052
                KeyEvent.KEYCODE_ENTER -> 0x001c
                KeyEvent.KEYCODE_NUMPAD_ENTER -> 0x011c
                KeyEvent.KEYCODE_ESCAPE -> 0x0001
                KeyEvent.KEYCODE_SPACE -> 0x0039
                KeyEvent.KEYCODE_TAB -> 0x000f
                KeyEvent.KEYCODE_DEL -> 0x000e
                KeyEvent.KEYCODE_DPAD_LEFT -> 0x014b
                KeyEvent.KEYCODE_DPAD_UP -> 0x0148
                KeyEvent.KEYCODE_DPAD_RIGHT -> 0x014d
                KeyEvent.KEYCODE_DPAD_DOWN -> 0x0150
                KeyEvent.KEYCODE_PAGE_UP -> 0x0149
                KeyEvent.KEYCODE_PAGE_DOWN -> 0x0151
                KeyEvent.KEYCODE_FORWARD_DEL -> 0x0153
                KeyEvent.KEYCODE_INSERT -> 0x0152
                KeyEvent.KEYCODE_MOVE_HOME -> 0x0147
                KeyEvent.KEYCODE_MOVE_END -> 0x014f
                KeyEvent.KEYCODE_SHIFT_LEFT -> 0x002a
                KeyEvent.KEYCODE_SHIFT_RIGHT -> 0x0036
                KeyEvent.KEYCODE_CTRL_LEFT -> 0x001d
                KeyEvent.KEYCODE_CTRL_RIGHT -> 0x011d
                KeyEvent.KEYCODE_ALT_LEFT -> 0x0038
                KeyEvent.KEYCODE_ALT_RIGHT -> 0x0138
                KeyEvent.KEYCODE_CAPS_LOCK -> 0x003a
                KeyEvent.KEYCODE_NUM_LOCK -> 0x0145
                KeyEvent.KEYCODE_SCROLL_LOCK -> 0x0046
                KeyEvent.KEYCODE_MINUS -> 0x000c
                KeyEvent.KEYCODE_EQUALS -> 0x000d
                KeyEvent.KEYCODE_LEFT_BRACKET -> 0x001a
                KeyEvent.KEYCODE_RIGHT_BRACKET -> 0x001b
                KeyEvent.KEYCODE_BACKSLASH -> 0x002b
                KeyEvent.KEYCODE_SEMICOLON -> 0x0027
                KeyEvent.KEYCODE_APOSTROPHE -> 0x0028
                KeyEvent.KEYCODE_COMMA -> 0x0033
                KeyEvent.KEYCODE_PERIOD -> 0x0034
                KeyEvent.KEYCODE_SLASH -> 0x0035
                KeyEvent.KEYCODE_GRAVE -> 0x0029
                else -> null
            }
    }
}

private fun timestampUs(): Long = SystemClock.elapsedRealtimeNanos() / 1000L

private const val DEFAULT_INPUT_PROTOCOL_VERSION = 2
private const val INPUT_HANDSHAKE_MARKER = 0x0e
private const val INPUT_HANDSHAKE_MAGIC_WORD = 526
private const val ICE_DISCONNECTED_GRACE_MS = 3500L
private const val ICE_FAILED_RECONNECT_DELAY_MS = 250L
private const val SIGNALING_RECONNECT_DELAY_MS = 1000L
private const val MAX_TRANSPORT_RECONNECT_ATTEMPTS = 3
private const val OFFER_TIMEOUT_MS = 12_000L
private const val MEDIA_STALL_KEYFRAME_AFTER_MS = 5_000L
private const val MEDIA_STALL_KEYFRAME_INTERVAL_MS = 2_500L
private const val MEDIA_STALL_RESTART_AFTER_MS = 14_000L
private const val GAMEPAD_GUIDE_AUTO_RELEASE_MS = 160L
private const val STREAM_TEXT_SEND_MAX_CHARS = 4096
private const val STREAM_TEXT_SEND_ATTEMPTS = 3
private const val STREAM_TEXT_PACKET_DELAY_MS = 4L
private const val STREAM_TEXT_KEY_DELAY_MS = 10L
private const val STREAM_TEXT_RETRY_DELAY_MS = 16L

private fun Any?.statsDouble(): Double? =
    when (this) {
        is Number -> toDouble()
        is String -> toDoubleOrNull()
        else -> null
    }

private fun Any?.statsLong(): Long? =
    when (this) {
        is Number -> toLong()
        is String -> toLongOrNull()
        else -> null
    }

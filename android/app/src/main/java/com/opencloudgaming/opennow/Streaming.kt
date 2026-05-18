package com.opencloudgaming.opennow

import android.content.Context
import android.media.AudioAttributes
import android.media.MediaCodecInfo
import android.media.MediaCodecList
import android.os.Build
import android.os.SystemClock
import android.util.Log
import android.view.InputDevice
import android.view.KeyEvent
import android.view.KeyCharacterMap
import android.view.MotionEvent
import kotlinx.coroutines.CoroutineScope
import kotlinx.coroutines.Dispatchers
import kotlinx.coroutines.Job
import kotlinx.coroutines.SupervisorJob
import kotlinx.coroutines.delay
import kotlinx.coroutines.launch
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
import org.webrtc.IceCandidate
import org.webrtc.MediaConstraints
import org.webrtc.MediaStream
import org.webrtc.PeerConnection
import org.webrtc.PeerConnectionFactory
import org.webrtc.RTCStats
import org.webrtc.RTCStatsCollectorCallback
import org.webrtc.RendererCommon
import org.webrtc.RtpReceiver
import org.webrtc.RtpTransceiver
import org.webrtc.SdpObserver
import org.webrtc.SessionDescription
import org.webrtc.SurfaceViewRenderer
import org.webrtc.VideoTrack
import org.webrtc.audio.AudioDeviceModule
import org.webrtc.audio.JavaAudioDeviceModule
import java.nio.ByteBuffer
import java.nio.ByteOrder
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
}

object CodecProbe {
    fun report(context: Context): RuntimeCodecReport {
        val packageManager = context.packageManager
        val isTv = packageManager.hasSystemFeature("android.software.leanback")
        val renderer = listOf(Build.HARDWARE, Build.BOARD, Build.DEVICE, Build.MODEL, Build.MANUFACTURER)
            .joinToString(" ")
            .lowercase(Locale.US)
        val lowPower = renderer.contains("powervr") || renderer.contains("ge8320") || renderer.contains("ge83")
        val capabilities = VideoCodec.entries.map { codec ->
            val mime = codec.mimeType()
            val decoders = codecInfos(mime, encoder = false)
            val encoders = codecInfos(mime, encoder = true)
            CodecCapability(
                codec = codec,
                decoderAvailable = decoders.isNotEmpty(),
                encoderAvailable = encoders.isNotEmpty(),
                hardwareDecoder = decoders.any(::isHardwareCodec),
                hardwareEncoder = encoders.any(::isHardwareCodec),
                decoderName = decoders.firstOrNull()?.name,
                encoderName = encoders.firstOrNull()?.name,
                realtimeSafe = decoders.any { isRealtimeSafeDecoder(codec, it) },
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

    private fun isHardwareCodec(info: MediaCodecInfo): Boolean {
        val name = info.name.lowercase(Locale.US)
        if (name.contains("google") || name.contains("sw") || name.contains("software")) return false
        return if (Build.VERSION.SDK_INT >= 29) {
            info.isHardwareAccelerated
        } else {
            true
        }
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
    private var touchControllerPassthroughBounds: TouchPassthroughBounds? = null
    @Volatile
    private var touchControllerVisible = false
    @Volatile
    private var uiTouchPassthroughActive = false
    private val touchMouseState = TouchMouseState()

    fun attach(next: NativeStreamClient) {
        client = next
    }

    fun detach(next: NativeStreamClient) {
        if (client === next) client = null
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

    fun setUiTouchPassthroughBounds(left: Int, top: Int, right: Int, bottom: Int) {
        streamChromePassthroughBounds = TouchPassthroughBounds(left, top, right, bottom)
    }

    fun clearUiTouchPassthroughBounds() {
        streamChromePassthroughBounds = null
        uiTouchPassthroughActive = false
    }

    fun setStreamPanelTouchPassthroughBounds(left: Int, top: Int, right: Int, bottom: Int) {
        streamPanelPassthroughBounds = TouchPassthroughBounds(left, top, right, bottom)
    }

    fun clearStreamPanelTouchPassthroughBounds() {
        streamPanelPassthroughBounds = null
        uiTouchPassthroughActive = false
    }

    fun setTouchControllerPassthroughBounds(left: Int, top: Int, right: Int, bottom: Int) {
        touchControllerPassthroughBounds = TouchPassthroughBounds(left, top, right, bottom)
    }

    fun setTouchControllerVisible(visible: Boolean) {
        touchControllerVisible = visible
        if (!visible) {
            touchControllerPassthroughBounds = null
            uiTouchPassthroughActive = false
        }
    }

    fun clearTouchControllerPassthroughBounds() {
        touchControllerPassthroughBounds = null
        touchControllerVisible = false
        uiTouchPassthroughActive = false
    }

    fun shouldForwardTouchBeforeViews(event: MotionEvent, width: Int, height: Int): Boolean =
        client != null &&
            touchMouseEnabled &&
            width > 0 &&
            height > 0 &&
            event.isFingerTouchEvent() &&
            event.pointerCount == 1 &&
            !shouldPassTouchToNativeUi(event, width, height)

    fun shouldCaptureTouchBeforeViews(event: MotionEvent, width: Int, height: Int): Boolean =
        shouldForwardTouchBeforeViews(event, width, height) &&
            (captureAllTouch || event.isFingerTouchEvent())

    fun dispatchTouch(event: MotionEvent, width: Int, height: Int): Boolean {
        val current = client ?: return false
        if (!event.isFingerTouchEvent()) return false
        return touchMouseState.handle(event, touchMouseEnabled && width > 0 && height > 0, current, width, height)
    }

    fun dispatchExternalMouseTouch(event: MotionEvent, width: Int, height: Int): Boolean {
        if (streamUiActive) return false
        if (!event.isExternalMousePointerEvent()) return false
        if (shouldPassTouchToNativeUi(event, width, height)) return false
        return client?.dispatchMotion(event) == true
    }

    fun dispatchCapturedExternalMouse(event: MotionEvent): Boolean {
        if (streamUiActive) return false
        if (!event.isExternalMousePointerEvent()) return false
        return client?.dispatchMotion(event) == true
    }

    fun dispatchKey(event: KeyEvent): Boolean {
        if (event.action == KeyEvent.ACTION_DOWN && event.repeatCount == 0 && event.isStreamSystemMenuKey()) {
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
        val current = client ?: return false
        if (event.shouldConsumeAsStreamKeyboard()) {
            current.dispatchKey(event)
            return true
        }
        if (streamUiActive && event.isNativeUiNavigationKey()) {
            return false
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
        keyCode == KeyEvent.KEYCODE_BUTTON_MODE ||
            keyCode == KeyEvent.KEYCODE_MENU

    private fun KeyEvent.isStreamControlsShortcutKey(): Boolean =
        !streamUiActive &&
            !isHardwareKeyboardSource() &&
            isDpadSource() &&
            (keyCode == KeyEvent.KEYCODE_ENTER ||
                keyCode == KeyEvent.KEYCODE_NUMPAD_ENTER ||
                keyCode == KeyEvent.KEYCODE_DPAD_CENTER)

    private fun KeyEvent.isStreamExitShortcutKey(): Boolean =
        keyCode == KeyEvent.KEYCODE_BACK ||
            (keyCode == KeyEvent.KEYCODE_ESCAPE && !isHardwareKeyboardSource())

    private fun KeyEvent.isNativeUiNavigationKey(): Boolean =
        keyCode == KeyEvent.KEYCODE_DPAD_UP ||
            keyCode == KeyEvent.KEYCODE_DPAD_DOWN ||
            keyCode == KeyEvent.KEYCODE_DPAD_LEFT ||
            keyCode == KeyEvent.KEYCODE_DPAD_RIGHT ||
            keyCode == KeyEvent.KEYCODE_DPAD_CENTER ||
            keyCode == KeyEvent.KEYCODE_ENTER ||
            keyCode == KeyEvent.KEYCODE_NUMPAD_ENTER ||
            keyCode == KeyEvent.KEYCODE_SPACE ||
            keyCode == KeyEvent.KEYCODE_TAB ||
            keyCode == KeyEvent.KEYCODE_BACK ||
            keyCode == KeyEvent.KEYCODE_ESCAPE ||
            keyCode == KeyEvent.KEYCODE_DEL ||
            keyCode == KeyEvent.KEYCODE_BUTTON_A ||
            keyCode == KeyEvent.KEYCODE_BUTTON_B ||
            keyCode == KeyEvent.KEYCODE_BUTTON_SELECT ||
            keyCode == KeyEvent.KEYCODE_BUTTON_START

    private fun KeyEvent.isControllerSource(): Boolean =
        (source and InputDevice.SOURCE_GAMEPAD) == InputDevice.SOURCE_GAMEPAD ||
            (source and InputDevice.SOURCE_JOYSTICK) == InputDevice.SOURCE_JOYSTICK

    private fun KeyEvent.isDpadSource(): Boolean =
        (source and InputDevice.SOURCE_DPAD) == InputDevice.SOURCE_DPAD

    private fun KeyEvent.isHardwareKeyboardSource(): Boolean =
        !isControllerSource() &&
            ((source and InputDevice.SOURCE_KEYBOARD) == InputDevice.SOURCE_KEYBOARD ||
                InputDevice.getDevice(deviceId)?.keyboardType == InputDevice.KEYBOARD_TYPE_ALPHABETIC)

    private fun KeyEvent.shouldConsumeAsStreamKeyboard(): Boolean =
        (action == KeyEvent.ACTION_DOWN || action == KeyEvent.ACTION_UP) &&
            !isControllerSource() &&
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
            this in KeyEvent.KEYCODE_F1..KeyEvent.KEYCODE_F12

    private fun MotionEvent.isNativeUiNavigationMotion(): Boolean =
        isFromSource(InputDevice.SOURCE_JOYSTICK) ||
            isFromSource(InputDevice.SOURCE_GAMEPAD)

    private fun MotionEvent.isFromSource(source: Int): Boolean = (this.source and source) == source

    private fun MotionEvent.isFingerTouchEvent(): Boolean =
        isFromSource(InputDevice.SOURCE_TOUCHSCREEN) &&
            !isFromSource(InputDevice.SOURCE_MOUSE) &&
            !isFromSource(InputDevice.SOURCE_MOUSE_RELATIVE)

    private fun MotionEvent.isExternalMousePointerEvent(): Boolean =
        isFromSource(InputDevice.SOURCE_MOUSE) ||
            isFromSource(InputDevice.SOURCE_MOUSE_RELATIVE)

    private fun shouldPassTouchToNativeUi(event: MotionEvent, width: Int, height: Int): Boolean {
        when (event.actionMasked) {
            MotionEvent.ACTION_DOWN -> {
                uiTouchPassthroughActive =
                    streamChromePassthroughBounds?.contains(event.x, event.y) == true ||
                    streamPanelPassthroughBounds?.contains(event.x, event.y) == true ||
                    touchControllerContains(event, width, height)
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

    private fun touchControllerContains(event: MotionEvent, width: Int, height: Int): Boolean {
        val bounds = touchControllerPassthroughBounds
        if (bounds != null) return bounds.contains(event.x, event.y)
        return touchControllerVisible &&
            width > 0 &&
            height > 0 &&
            event.y >= height * TOUCH_CONTROLLER_FALLBACK_TOP_RATIO
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

    fun handle(event: MotionEvent, enabled: Boolean, client: NativeStreamClient, width: Int, height: Int): Boolean {
        if (!enabled) {
            reset(client)
            return false
        }

        when (event.actionMasked) {
            MotionEvent.ACTION_DOWN -> {
                activePointerId = event.getPointerId(0)
                downX = event.x
                downY = event.y
                downTimeMs = event.eventTime
                lastX = event.x
                lastY = event.y
                selecting = false
                doubleTapDragCandidate = isDoubleTap(event)
                if (doubleTapDragCandidate) {
                    lastTapTimeMs = Long.MIN_VALUE
                }
                return true
            }
            MotionEvent.ACTION_POINTER_DOWN -> {
                reset(client)
                return true
            }
            MotionEvent.ACTION_MOVE -> {
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
            MotionEvent.ACTION_UP -> {
                val tapDistanceX = abs(event.x - downX)
                val tapDistanceY = abs(event.y - downY)
                val wasTap = activePointerId >= 0 &&
                    event.eventTime - downTimeMs <= TOUCH_MOUSE_TAP_TIMEOUT_MS &&
                    tapDistanceX <= TOUCH_MOUSE_TAP_SLOP_PX &&
                    tapDistanceY <= TOUCH_MOUSE_TAP_SLOP_PX
                activePointerId = -1
                doubleTapDragCandidate = false
                if (selecting) {
                    client.setTouchMouseButton(false)
                    selecting = false
                    return true
                }
                if (wasTap) {
                    NativeInputDiagnostics.add("touch tap click dx=${tapDistanceX.roundToInt()} dy=${tapDistanceY.roundToInt()}")
                    client.sendTouchMouseClick()
                    lastTapTimeMs = event.eventTime
                    lastTapX = event.x
                    lastTapY = event.y
                }
                return true
            }
            MotionEvent.ACTION_CANCEL -> {
                reset(client)
                return true
            }
        }
        return true
    }

    private fun isDoubleTap(event: MotionEvent): Boolean {
        if (lastTapTimeMs == Long.MIN_VALUE) return false
        if (event.eventTime - lastTapTimeMs > TOUCH_MOUSE_DOUBLE_TAP_TIMEOUT_MS) return false
        if (!lastTapX.isFinite() || !lastTapY.isFinite()) return false
        return abs(event.x - lastTapX) <= TOUCH_MOUSE_DOUBLE_TAP_SLOP_PX &&
            abs(event.y - lastTapY) <= TOUCH_MOUSE_DOUBLE_TAP_SLOP_PX
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
    private var videoTrack: VideoTrack? = null
    private var audioTrack: AudioTrack? = null
    private var renderer: SurfaceViewRenderer? = null
    private var heartbeatJob: Job? = null
    private var gamepadKeepaliveJob: Job? = null
    private var statsJob: Job? = null
    private var iceRecoveryJob: Job? = null
    private var settings: StreamSettings = StreamSettings()
    private var session: SessionInfo? = null
    private var transportGeneration = 0
    private var reconnectAttempts = 0
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
    private var mouseLastX = 0f
    private var mouseLastY = 0f
    private var mousePositionValid = false
    private var inputDropLogged = false
    private var externalMouseEventLogged = false
    private var externalMouseMoveSentLogged = false
    private var hardwareKeyboardEventLogged = false
    private var lastStatsSample: StreamStatsSample? = null

    private data class StreamStatsSample(
        val atMs: Double,
        val bytesReceived: Long,
        val framesDecoded: Long,
    )

    init {
        PeerConnectionFactory.initialize(
            PeerConnectionFactory.InitializationOptions.builder(appContext)
                .setEnableInternalTracer(false)
                .createInitializationOptions(),
        )
        factory = PeerConnectionFactory.builder()
            .setOptions(PeerConnectionFactory.Options())
            .setAudioDeviceModule(audioDeviceModule)
            .setVideoDecoderFactory(DefaultVideoDecoderFactory(eglBase.eglBaseContext))
            .setVideoEncoderFactory(DefaultVideoEncoderFactory(eglBase.eglBaseContext, true, true))
            .createPeerConnectionFactory()
    }

    fun createRenderer(context: Context): SurfaceViewRenderer =
        SurfaceViewRenderer(context).also {
            it.init(eglBase.eglBaseContext, null)
            it.setEnableHardwareScaler(true)
            it.setMirror(false)
            it.setScalingType(RendererCommon.ScalingType.SCALE_ASPECT_FIT)
            renderer = it
            videoTrack?.addSink(it)
        }

    fun start(session: SessionInfo, settings: StreamSettings) {
        this.session = session
        this.settings = settings
        transportGeneration += 1
        reconnectAttempts = 0
        lastStatsSample = null
        onStats(StreamRuntimeStats())
        audioDeviceModule.setSpeakerMute(audioMuted)
        closeTransport(clearInputState = false)
        startTransport(session, settings, transportGeneration)
    }

    fun stop() {
        transportGeneration += 1
        reconnectAttempts = 0
        closeTransport(clearInputState = true)
        emitState("Stopped")
    }

    fun release() {
        stop()
        renderer?.release()
        renderer = null
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
        lastLeftTrigger = 0
        lastRightTrigger = 0
        lastLeftStickX = 0
        lastLeftStickY = 0
        lastRightStickX = 0
        lastRightStickY = 0
        activeControllerId = 0
        controllerSlots.clear()
        mousePositionValid = false
        inputDropLogged = false
        externalMouseEventLogged = false
        externalMouseMoveSentLogged = false
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
        val sent = sendInput(packet, partiallyReliable = false)
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
                } else if (mousePositionValid && mouseLastDeviceId == event.deviceId) {
                    val dx = event.x - mouseLastX
                    val dy = event.y - mouseLastY
                    if (abs(dx) >= 0.5f || abs(dy) >= 0.5f) {
                        val sent = sendTouchMouseMove(dx.roundToInt(), dy.roundToInt())
                        if (sent && !externalMouseMoveSentLogged) {
                            externalMouseMoveSentLogged = true
                            NativeInputDiagnostics.add("external mouse move sent source=${event.source} device=${event.deviceId} mode=absoluteDelta")
                        }
                    }
                }
                mouseLastDeviceId = event.deviceId
                mouseLastX = event.x
                mouseLastY = event.y
                mousePositionValid = true
            }
            MotionEvent.ACTION_DOWN -> {
                mousePositionValid = true
                mouseLastDeviceId = event.deviceId
                mouseLastX = event.x
                mouseLastY = event.y
                sendInput(inputEncoder.encodeMouseButton(InputEncoder.INPUT_MOUSE_BUTTON_DOWN, event.primaryMouseButton()), false)
            }
            MotionEvent.ACTION_UP,
            MotionEvent.ACTION_CANCEL,
            -> {
                mousePositionValid = false
                sendInput(inputEncoder.encodeMouseButton(InputEncoder.INPUT_MOUSE_BUTTON_UP, event.primaryMouseButton()), false)
            }
            MotionEvent.ACTION_BUTTON_PRESS -> {
                val handled = sendInput(inputEncoder.encodeMouseButton(InputEncoder.INPUT_MOUSE_BUTTON_DOWN, event.actionButton.toGfnMouseButton()), false)
                if (!handled) {
                    NativeInputDiagnostics.add("external mouse button consumed without send action=press button=${event.actionButton} reliable=${reliableInput?.state()} partial=${partiallyReliableInput?.state()}")
                }
                return true
            }
            MotionEvent.ACTION_BUTTON_RELEASE -> {
                val handled = sendInput(inputEncoder.encodeMouseButton(InputEncoder.INPUT_MOUSE_BUTTON_UP, event.actionButton.toGfnMouseButton()), false)
                if (!handled) {
                    NativeInputDiagnostics.add("external mouse button consumed without send action=release button=${event.actionButton} reliable=${reliableInput?.state()} partial=${partiallyReliableInput?.state()}")
                }
                return true
            }
            MotionEvent.ACTION_SCROLL -> {
                val vertical = event.getAxisValue(MotionEvent.AXIS_VSCROLL)
                if (abs(vertical) >= 0.01f) {
                    sendInput(inputEncoder.encodeMouseWheel((vertical * 120).roundToInt()), false)
                }
            }
        }
        return true
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
        val keyMap = KeyCharacterMap.load(KeyCharacterMap.VIRTUAL_KEYBOARD)
        val events = keyMap.getEvents(text.toCharArray()) ?: return
        events.forEach(::dispatchKey)
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
        heartbeatJob = null
        gamepadKeepaliveJob = null
        statsJob = null
        lastStatsSample = null
        lastIceState = null
        signaling?.disconnect()
        signaling = null
        reliableInput = null
        partiallyReliableInput = null
        partiallyReliableGamepadMask = 0
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
            SignalingEvent.Connected -> emitState("Waiting for offer")
            is SignalingEvent.Disconnected -> scheduleTransportReconnect("Signaling disconnected: ${event.reason}", SIGNALING_RECONNECT_DELAY_MS, generation)
            is SignalingEvent.Error -> scheduleTransportReconnect("Signaling failed: ${event.message}", SIGNALING_RECONNECT_DELAY_MS, generation)
            is SignalingEvent.Log -> emitState(event.message)
            is SignalingEvent.RemoteIce -> peerConnection?.addIceCandidate(event.candidate)
            is SignalingEvent.Offer -> handleOffer(event.sdp, generation)
        }
    }

    private fun handleOffer(rawOffer: String, generation: Int) {
        val currentSession = session ?: return
        val fixed = SdpTools.fixServerIp(rawOffer, currentSession.serverIp)
        val preferred = SdpTools.preferCodec(fixed, settings.codec)
        val pc = ensurePeerConnection(currentSession, generation)
        ensureInputDataChannels(pc, preferred)
        inputEncoder.setProtocolVersion(SdpTools.parseInputProtocolVersion(preferred))
        partiallyReliableGamepadMask = SdpTools.parsePartiallyReliableGamepadMask(preferred)
        pc.setRemoteDescription(
            object : SimpleSdpObserver() {
                override fun onSetSuccess() {
                    pc.createAnswer(
                        object : SimpleSdpObserver() {
                            override fun onCreateSuccess(description: SessionDescription?) {
                                if (generation != transportGeneration) return
                                val rawDescription = description ?: run {
                                    failStream("WebRTC returned an empty answer", generation)
                                    return
                                }
                                val munged = SdpTools.mungeAnswerSdp(rawDescription.description, settings.maxBitrateMbps * 1000)
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
                    emitState("Streaming")
                }
                PeerConnection.IceConnectionState.DISCONNECTED -> {
                    emitState("Connection interrupted")
                    scheduleTransportReconnect("ICE disconnected", ICE_DISCONNECTED_GRACE_MS, generation)
                }
                PeerConnection.IceConnectionState.FAILED -> {
                    emitState("Reconnecting stream")
                    scheduleTransportReconnect("ICE failed", ICE_FAILED_RECONNECT_DELAY_MS, generation)
                }
                PeerConnection.IceConnectionState.CHECKING,
                PeerConnection.IceConnectionState.NEW,
                -> emitState("Connecting media")
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
            failStream("$reason. Stream reconnect failed after $MAX_TRANSPORT_RECONNECT_ATTEMPTS attempts.")
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
        val label = channel.label().lowercase(Locale.US)
        NativeInputDiagnostics.add("input channel attached label=$label state=${channel.state()}")
        if (label.contains("partial") || label.contains("pr")) {
            partiallyReliableInput = channel
        } else {
            reliableInput = channel
        }
        channel.registerObserver(object : DataChannel.Observer {
            override fun onBufferedAmountChange(previousAmount: Long) = Unit
            override fun onStateChange() {
                if (channel.state() == DataChannel.State.OPEN) {
                    inputDropLogged = false
                    NativeInputDiagnostics.add("input channel open label=$label")
                    sendInput(inputEncoder.encodeHapticsEnabled(true), partiallyReliable = false)
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
            else -> return
        }.coerceAtLeast(1)

        inputEncoder.setProtocolVersion(version)
        inputEncoder.resetGamepadSequences()
        NativeInputDiagnostics.add("input handshake protocol=$version bytes=${bytes.size}")
    }

    private fun startHeartbeat() {
        heartbeatJob?.cancel()
        heartbeatJob = scope.launch {
            while (true) {
                delay(1000)
                sendInput(inputEncoder.encodeHeartbeat(), partiallyReliable = false)
            }
        }
    }

    private fun startGamepadKeepalive() {
        gamepadKeepaliveJob?.cancel()
        gamepadKeepaliveJob = scope.launch {
            var connectedScanCountdown = 0
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
        pc.getStats(RTCStatsCollectorCallback { report ->
            val stats = buildRuntimeStats(report.timestampUs / 1000.0, report.statsMap.values)
            scope.launch {
                onStats(stats)
            }
        })
    }

    private fun buildRuntimeStats(timestampMs: Double, stats: Collection<RTCStats>): StreamRuntimeStats {
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

        return StreamRuntimeStats(
            bitrateKbps = bitrateKbps,
            pingMs = pingMs,
            fps = explicitFps?.roundToInt()?.takeIf { it > 0 } ?: derivedFps?.takeIf { it > 0 },
            resolution = resolution,
            codec = codec,
        )
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
        val mask = event.keyCode.toGamepadButtonMask()
        if (mask != null) {
            activeControllerId = controllerIdFor(event)
            if (!physicalControllerActive) {
                NativeInputDiagnostics.add("physical gamepad key source=${event.source} device=${event.deviceId} slot=$activeControllerId key=${event.keyCode}")
            }
            physicalControllerConnected = true
            physicalControllerActive = true
            physicalButtons = if (pressed) physicalButtons or mask else physicalButtons and mask.inv()
            return sendCurrentGamepadState(controllerId = activeControllerId)
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
            if (device.sources.hasSource(InputDevice.SOURCE_GAMEPAD) ||
                device.sources.hasSource(InputDevice.SOURCE_JOYSTICK)
            ) {
                connectedDevices += device
            }
        }
        val connected = connectedDevices.isNotEmpty()
        if (connected != physicalControllerConnected) {
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
    private fun MotionEvent.isMouseLikePointer(): Boolean =
        isFromSource(InputDevice.SOURCE_MOUSE) ||
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
        if (hatY <= -0.5f) mask = mask or 0x0001
        if (hatY >= 0.5f) mask = mask or 0x0002
        if (hatX <= -0.5f) mask = mask or 0x0004
        if (hatX >= 0.5f) mask = mask or 0x0008
        return mask
    }

    private fun KeyEvent.isGamepadEvent(): Boolean {
        val controllerSource =
            (source and InputDevice.SOURCE_GAMEPAD) == InputDevice.SOURCE_GAMEPAD ||
                (source and InputDevice.SOURCE_JOYSTICK) == InputDevice.SOURCE_JOYSTICK
        if (!controllerSource) return false
        return keyCode.toGamepadButtonMask() != null ||
            keyCode == KeyEvent.KEYCODE_BUTTON_L2 ||
            keyCode == KeyEvent.KEYCODE_BUTTON_R2 ||
            keyCode == KeyEvent.KEYCODE_BUTTON_MODE
    }

    private fun KeyEvent.isHardwareKeyboardSource(): Boolean =
        !isGamepadEvent() &&
            ((source and InputDevice.SOURCE_KEYBOARD) == InputDevice.SOURCE_KEYBOARD ||
                InputDevice.getDevice(deviceId)?.keyboardType == InputDevice.KEYBOARD_TYPE_ALPHABETIC)

    private fun MotionEvent.isGamepadMotionEvent(): Boolean =
        isFromSource(InputDevice.SOURCE_JOYSTICK) ||
            isFromSource(InputDevice.SOURCE_GAMEPAD)

    private fun Int.hasSource(source: Int): Boolean = (this and source) == source

    private fun Int.toGamepadButtonMask(): Int? = when (this) {
        KeyEvent.KEYCODE_DPAD_UP -> 0x0001
        KeyEvent.KEYCODE_DPAD_DOWN -> 0x0002
        KeyEvent.KEYCODE_DPAD_LEFT -> 0x0004
        KeyEvent.KEYCODE_DPAD_RIGHT -> 0x0008
        KeyEvent.KEYCODE_BUTTON_START -> 0x0010
        KeyEvent.KEYCODE_BUTTON_SELECT -> 0x0020
        KeyEvent.KEYCODE_BUTTON_THUMBL -> 0x0040
        KeyEvent.KEYCODE_BUTTON_THUMBR -> 0x0080
        KeyEvent.KEYCODE_BUTTON_L1 -> 0x0100
        KeyEvent.KEYCODE_BUTTON_R1 -> 0x0200
        KeyEvent.KEYCODE_BUTTON_MODE -> 0x0400
        KeyEvent.KEYCODE_BUTTON_A -> 0x1000
        KeyEvent.KEYCODE_BUTTON_B -> 0x2000
        KeyEvent.KEYCODE_BUTTON_X -> 0x4000
        KeyEvent.KEYCODE_BUTTON_Y -> 0x8000
        else -> null
    }
    private fun Int.toGfnMouseButton(): Int = when {
        this and MotionEvent.BUTTON_PRIMARY != 0 -> 1
        this and MotionEvent.BUTTON_TERTIARY != 0 -> 2
        this and MotionEvent.BUTTON_SECONDARY != 0 -> 3
        this and MotionEvent.BUTTON_BACK != 0 -> 4
        this and MotionEvent.BUTTON_FORWARD != 0 -> 5
        else -> 1
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
    fun fixServerIp(sdp: String, serverIp: String): String {
        val ip = extractPublicIp(serverIp) ?: return sdp
        return sdp
            .replace("c=IN IP4 0.0.0.0", "c=IN IP4 $ip")
            .replace(Regex("(a=candidate:\\S+\\s+\\d+\\s+\\w+\\s+\\d+\\s+)0\\.0\\.0\\.0(\\s+)"), "$1$ip$2")
    }

    fun preferCodec(sdp: String, codec: VideoCodec): String {
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
            preferred.sortBy { pt ->
                when (Regex("(?:^|;)\\s*profile-id=(\\d+)").find(fmtpByPt[pt].orEmpty())?.groupValues?.getOrNull(1)) {
                    "2" -> 0
                    "1" -> 1
                    else -> 2
                }
            }
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
        val bitDepth = if (settings.colorQuality == ColorQuality.TenBit420 || settings.colorQuality == ColorQuality.TenBit444) 10 else 8
        val maxBitrate = settings.maxBitrateMbps * 1000
        val minBitrate = max(5000, (maxBitrate * 0.35f).roundToInt())
        val initialBitrate = max(minBitrate, (maxBitrate * 0.7f).roundToInt())
        return listOf(
            "v=0",
            "o=SdpTest test_id_13 14 IN IPv4 127.0.0.1",
            "s=-",
            "t=0 0",
            "a=general.icePassword:$pwd",
            "a=general.iceUserNameFragment:$ufrag",
            "a=general.dtlsFingerprint:$fingerprint",
            "m=video 0 RTP/AVP",
            "a=msid:fbc-video-0",
            "a=vqos.dynamicStreamingMode:0",
            "a=vqos.drc.enable:0",
            "a=vqos.dfc.enable:${if (settings.fps >= 90) 1 else 0}",
            "a=video.enableRtpNack:1",
            "a=video.packetSize:1140",
            "a=video.clientViewportWd:$width",
            "a=video.clientViewportHt:$height",
            "a=video.maxFPS:${settings.fps}",
            "a=video.initialBitrateKbps:$initialBitrate",
            "a=video.initialPeakBitrateKbps:$maxBitrate",
            "a=vqos.bw.maximumBitrateKbps:$maxBitrate",
            "a=vqos.bw.minimumBitrateKbps:$minBitrate",
            "a=vqos.bw.peakBitrateKbps:$maxBitrate",
            "a=vqos.bw.serverPeakBitrateKbps:$maxBitrate",
            "a=vqos.bw.enableBandwidthEstimation:1",
            "a=vqos.bw.disableBitrateLimit:0",
            "a=vqos.resControl.cpmRtc.enable:0",
            "a=vqos.resControl.cpmRtc.minResolutionPercent:100",
            "a=video.bitDepth:$bitDepth",
            "m=audio 0 RTP/AVP",
            "a=msid:audio",
            "m=mic 0 RTP/AVP",
            "a=msid:mic",
            "a=rtpmap:0 PCMU/8000",
            "m=application 0 RTP/AVP",
            "a=msid:input_1",
            "a=ri.partialReliableThresholdMs:$threshold",
            "a=ri.hidDeviceMask:4294967295",
            "a=ri.enablePartiallyReliableTransferGamepad:15",
            "a=ri.enablePartiallyReliableTransferHid:4294967295",
            "",
        ).joinToString("\n")
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
        le.putLong(30, timestampUs())
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
            val vk = virtualKey(event.keyCode, event.unicodeChar)
            val scancode = if (event.scanCode > 0) event.scanCode else fallbackScanCode(event.keyCode)
            if (vk == null || scancode == null) return null
            var modifiers = 0
            if (event.isShiftPressed) modifiers = modifiers or 0x01
            if (event.isCtrlPressed) modifiers = modifiers or 0x02
            if (event.isAltPressed) modifiers = modifiers or 0x04
            if (event.isMetaPressed) modifiers = modifiers or 0x08
            if (event.isCapsLockOn) modifiers = modifiers or 0x10
            if (event.isNumLockOn) modifiers = modifiers or 0x20
            return KeyboardPayload(vk, scancode, modifiers)
        }

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
                KeyEvent.KEYCODE_ENTER -> 0x001c
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

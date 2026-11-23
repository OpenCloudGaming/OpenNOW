// Adapted from OpenStroid Android. See android/app/src/sideload/assets/licenses/OpenStroid-NOTICE.txt.
package com.opencloudgaming.opennow.boosteroid

import android.content.Context
import android.media.AudioAttributes
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
import android.view.SurfaceHolder
import android.view.View
import com.opencloudgaming.opennow.AbsoluteMousePosition
import com.opencloudgaming.opennow.AndroidControllerInput
import com.opencloudgaming.opennow.AndroidControllerMouseAssist
import com.opencloudgaming.opennow.AndroidGamepadAxisAvailability
import com.opencloudgaming.opennow.AndroidGamepadAxisMapping
import com.opencloudgaming.opennow.AndroidGamepadRawAxes
import com.opencloudgaming.opennow.BuildConfig
import com.opencloudgaming.opennow.GamepadButtonMapping
import com.opencloudgaming.opennow.MouseButtonPressSources
import com.opencloudgaming.opennow.OpenNowJson
import com.opencloudgaming.opennow.OpenNowVideoDecoderFactory
import com.opencloudgaming.opennow.SimpleSdpObserver
import com.opencloudgaming.opennow.SteamMenuChord
import com.opencloudgaming.opennow.SteamOverlayChordState
import com.opencloudgaming.opennow.StreamLivenessAction
import com.opencloudgaming.opennow.StreamRuntimeStats
import com.opencloudgaming.opennow.StreamSettings
import com.opencloudgaming.opennow.TouchMouseOutput
import com.opencloudgaming.opennow.TouchOverlayInput
import com.opencloudgaming.opennow.VideoCodec
import com.opencloudgaming.opennow.VirtualStickSourceState
import com.opencloudgaming.opennow.WEBRTC_AUXILIARY_VIDEO_CODECS
import com.opencloudgaming.opennow.WebRtcRuntime
import com.opencloudgaming.opennow.androidSafeVideoFallback
import com.opencloudgaming.opennow.isAndroidTvProfile
import com.opencloudgaming.opennow.newStreamLivenessWatchdog
import com.opencloudgaming.opennow.openNowCodecName
import com.opencloudgaming.opennow.preferenceKey
import com.opencloudgaming.opennow.streamDiagnosticId
import com.opencloudgaming.opennow.streamResolutionPixels
import com.opencloudgaming.opennow.webRtcCodecName
import java.nio.ByteBuffer
import java.util.Locale
import kotlin.math.abs
import kotlin.math.max
import kotlin.math.min
import kotlin.math.roundToInt
import kotlin.math.sqrt
import kotlinx.coroutines.CoroutineScope
import kotlinx.coroutines.Dispatchers
import kotlinx.coroutines.Job
import kotlinx.coroutines.SupervisorJob
import kotlinx.coroutines.cancel
import kotlinx.coroutines.delay
import kotlinx.coroutines.launch
import kotlinx.coroutines.sync.Mutex
import kotlinx.coroutines.sync.withLock
import kotlinx.coroutines.withContext
import kotlinx.serialization.json.JsonObject
import kotlinx.serialization.json.buildJsonObject
import kotlinx.serialization.json.put
import org.webrtc.AudioTrack
import org.webrtc.DataChannel
import org.webrtc.DefaultVideoEncoderFactory
import org.webrtc.EglBase
import org.webrtc.IceCandidate
import org.webrtc.MediaConstraints
import org.webrtc.MediaStream
import org.webrtc.MediaStreamTrack
import org.webrtc.PeerConnection
import org.webrtc.PeerConnectionFactory
import org.webrtc.RTCStats
import org.webrtc.RTCStatsCollectorCallback
import org.webrtc.RendererCommon
import org.webrtc.RtpCapabilities
import org.webrtc.RtpReceiver
import org.webrtc.RtpTransceiver
import org.webrtc.SessionDescription
import org.webrtc.SurfaceViewRenderer
import org.webrtc.VideoTrack
import org.webrtc.audio.AudioDeviceModule
import org.webrtc.audio.JavaAudioDeviceModule

internal class BoosteroidStreamClient(
    context: Context,
    private val onState: (String) -> Unit,
    private val onError: (String) -> Unit,
    private val onSafeVideoFallbackRequired: (String) -> Unit = {},
    private val onSessionRecoveryRequired: (String) -> Unit = {},
    private val onStats: (StreamRuntimeStats) -> Unit = {},
    private val onRemoteCursor: (StreamRemoteCursorState) -> Unit = {},
    private val onFirstFrame: () -> Unit = {},
) : TouchOverlayInput, TouchMouseOutput {
    private val appContext = context.applicationContext
    private val eglBase: EglBase = EglBase.create()
    private val gatewayHttp = boosteroidHttpClient()
    private val scope = CoroutineScope(SupervisorJob() + Dispatchers.Main.immediate)
    private val audioDeviceModule: AudioDeviceModule =
        JavaAudioDeviceModule.builder(appContext)
            .setUseLowLatency(true)
            .setUseStereoOutput(true)
            .setAudioTrackErrorCallback(object : JavaAudioDeviceModule.AudioTrackErrorCallback {
                override fun onWebRtcAudioTrackInitError(errorMessage: String) {
                    recordBoosteroidInputDiagnostic("stream audio track init error=${errorMessage.take(240)}")
                }

                override fun onWebRtcAudioTrackStartError(
                    errorCode: JavaAudioDeviceModule.AudioTrackStartErrorCode,
                    errorMessage: String,
                ) {
                    recordBoosteroidInputDiagnostic("stream audio track start error=$errorCode message=${errorMessage.take(240)}")
                }

                override fun onWebRtcAudioTrackError(errorMessage: String) {
                    recordBoosteroidInputDiagnostic("stream audio track runtime error=${errorMessage.take(240)}")
                }
            })
            .setAudioTrackStateCallback(object : JavaAudioDeviceModule.AudioTrackStateCallback {
                override fun onWebRtcAudioTrackStart() {
                    recordBoosteroidInputDiagnostic("stream audio track started low-latency mode")
                }

                override fun onWebRtcAudioTrackStop() {
                    recordBoosteroidInputDiagnostic("stream audio track stopped")
                }
            })
            .setAudioAttributes(
                AudioAttributes.Builder()
                    .setUsage(AudioAttributes.USAGE_GAME)
                    .setContentType(AudioAttributes.CONTENT_TYPE_MUSIC)
                    .build(),
            )
            .createAudioDeviceModule()
    private var factory: PeerConnectionFactory? = null
    private var peerConnection: PeerConnection? = null
    private var signaling: BoosteroidGatewayClient? = null
    private var boosteroidInputChannel: DataChannel? = null
    private var videoTrack: VideoTrack? = null
    private var audioTrack: AudioTrack? = null
    private var renderer: SurfaceViewRenderer? = null
    private var rendererSurfaceCallback: SurfaceHolder.Callback? = null
    private var rendererSinkAttached = false
    private var gamepadKeepaliveJob: Job? = null
    private var statsJob: Job? = null
    private var remoteIcePollingJob: Job? = null
    private var iceRecoveryJob: Job? = null
    private var offerTimeoutJob: Job? = null
    private var negotiationJob: Job? = null
    private var connectionTimeoutJob: Job? = null
    private var currentSettings = StreamSettings()
    override val settings: StreamSettings get() = currentSettings
    private var session: BoosteroidSession? = null
    @Volatile private var transportGeneration = 0
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
    private var steamMenuChordButtons = 0
    private val physicalSteamOverlayChord = SteamOverlayChordState()
    private val virtualSteamOverlayChord = SteamOverlayChordState()
    private var physicalLeftTriggerButtonPressed = false
    private var physicalRightTriggerButtonPressed = false
    private var lastLeftTrigger = 0
    private var lastRightTrigger = 0
    private var lastLeftStickX = 0
    private var lastLeftStickY = 0
    private var lastRightStickX = 0
    private var lastRightStickY = 0
    private var controllerMouseAssistActive = false
    private var controllerMouseAssistAutoArmed = false
    private var controllerMouseMoveLogged = false
    private var controllerMouseLeftButtonDown = false
    private var controllerMouseRightButtonDown = false
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
    private var physicalGamepadAxisLogged = false
    private var lastStatsSample: StreamStatsSample? = null
    private val livenessWatchdog = newStreamLivenessWatchdog(isAndroidTvProfile(appContext))
    private val textSendMutex = Mutex()
    private val mouseSources = MouseButtonPressSources()
    private val virtualKeySources = MouseButtonPressSources()
    private val virtualButtonSources = MouseButtonPressSources()
    private val virtualTriggerSources = MouseButtonPressSources()
    private val virtualStickSources = VirtualStickSourceState()
    private var clickSequence = 0L
    private var guideAutoReleaseJob: Job? = null
    private var steamMenuChordJob: Job? = null
    private var physicalSteamOverlayChordReleaseJob: Job? = null
    private var virtualSteamOverlayChordReleaseJob: Job? = null
    private val lastRumbleEffectAtMs = LongArray(GAMEPAD_MAX_CONTROLLERS)
    private val hapticsSupportLogged = BooleanArray(GAMEPAD_MAX_CONTROLLERS)
    private var lastHapticsWarningAtMs = 0L
    private var lastHapticsAdvertisementAtMs = 0L
    private var phoneRumbleSupportLogged = false
    private var boosteroidKeyboardConnected = false
    private var boosteroidMouseConnected = false
    private var boosteroidMouseAbsolute = true
    private var boosteroidMouseX = 0.5
    private var boosteroidMouseY = 0.5
    private var boosteroidCursorName = "default"
    private var legacyRelativeMouseFallbackLogged = false
    private val boosteroidCursorResources = mutableSetOf<String>()
    private val boosteroidCursorBitmaps = android.util.LruCache<String, DecodedBoosteroidCursor>(24)
    private val boosteroidCursorMissedAtMs = mutableMapOf<String, Long>()
    private val pressedBoosteroidMouseButtons = mutableSetOf<Int>()
    private var boosteroidInputEventCount = 0
    private var boosteroidInputCommandId = 0L
    private var boosteroidRelativeRemainderX = 0.0
    private var boosteroidRelativeRemainderY = 0.0
    private var gatewayCodec = ""
    private val pressedKeyboardCodes = linkedSetOf<Int>()
    private val announcedControllerSlots = mutableSetOf<Int>()
    private val controllerNamesBySlot = mutableMapOf<Int, String>()
    private val boosteroidControllerIds = mutableMapOf<Int, Int>()
    private val lastBoosteroidControllerStates = mutableMapOf<Int, BoosteroidControllerState>()
    private val remoteIceCandidates = mutableSetOf<String>()

    private data class BoosteroidControllerState(
        val buttons: Int,
        val hat: Int,
        val leftTrigger: Int,
        val rightTrigger: Int,
        val leftStickX: Int,
        val leftStickY: Int,
        val rightStickX: Int,
        val rightStickY: Int,
    )

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

    private fun recordStreamDiagnostic(message: String) {
        recordBoosteroidInputDiagnostic("stream $message")
    }

    init {
        WebRtcRuntime.ensureInitialized(appContext)
        factory = PeerConnectionFactory.builder()
            .setOptions(PeerConnectionFactory.Options())
            .setAudioDeviceModule(audioDeviceModule)
            .setVideoDecoderFactory(OpenNowVideoDecoderFactory(eglBase.eglBaseContext, requestedFps = { settings.fps }))
            .setVideoEncoderFactory(DefaultVideoEncoderFactory(eglBase.eglBaseContext, true, true))
            .createPeerConnectionFactory()
    }

    fun createRenderer(context: Context): SurfaceViewRenderer =
        SurfaceViewRenderer(context).also {
            renderer?.let(::releaseRendererInternal)
            val events = object : RendererCommon.RendererEvents {
                override fun onFirstFrameRendered() {
                    scope.launch { onFirstFrame() }
                }

                override fun onFrameResolutionChanged(width: Int, height: Int, rotation: Int) = Unit
            }
            it.init(eglBase.eglBaseContext, events)
            it.setEnableHardwareScaler(true)
            it.setMirror(false)
            it.setScalingType(RendererCommon.ScalingType.SCALE_ASPECT_FIT)
            renderer = it
            rendererSurfaceCallback = object : SurfaceHolder.Callback {
                override fun surfaceCreated(holder: SurfaceHolder) {
                    attachRendererSinkIfAvailable(it)
                }

                override fun surfaceChanged(holder: SurfaceHolder, format: Int, width: Int, height: Int) {
                    attachRendererSinkIfAvailable(it)
                }

                override fun surfaceDestroyed(holder: SurfaceHolder) {
                    detachRendererSink(it)
                }
            }.also(it.holder::addCallback)
            attachRendererSinkIfAvailable(it)
        }

    fun releaseRenderer(candidate: SurfaceViewRenderer) {
        if (renderer !== candidate) return
        releaseRendererInternal(candidate)
        renderer = null
    }

    private fun attachRendererSinkIfAvailable(candidate: SurfaceViewRenderer) {
        if (renderer !== candidate || rendererSinkAttached || candidate.holder.surface?.isValid != true) return
        val track = videoTrack ?: return
        track.addSink(candidate)
        rendererSinkAttached = true
        recordStreamDiagnostic("video renderer sink attached")
    }

    private fun detachRendererSink(candidate: SurfaceViewRenderer) {
        if (renderer !== candidate || !rendererSinkAttached) return
        videoTrack?.removeSink(candidate)
        rendererSinkAttached = false
        recordStreamDiagnostic("video renderer sink detached surface=${candidate.holder.surface?.isValid == true}")
    }

    private fun releaseRendererInternal(candidate: SurfaceViewRenderer) {
        detachRendererSink(candidate)
        rendererSurfaceCallback?.let(candidate.holder::removeCallback)
        rendererSurfaceCallback = null
        candidate.alpha = 0f
        candidate.visibility = View.GONE
        candidate.release()
    }

    fun setControllerMouseAssist(enabled: Boolean) {
        setControllerMouseAssistActive(enabled)
    }

    fun start(session: BoosteroidSession, settings: StreamSettings) {
        this.session = session
        this.currentSettings = settings
        transportGeneration += 1
        reconnectAttempts = 0
        videoSafeFallbackApplied = false
        sessionRecoveryRequested = false
        lastStatsSample = null
        livenessWatchdog.reset()
        onStats(StreamRuntimeStats())
        audioDeviceModule.setSpeakerMute(audioMuted)
        closeTransport(clearInputState = false)
        recordStreamDiagnostic(
            "start session=${streamDiagnosticId(session.sessionId)} gateway=${session.gatewayHost.take(96)} " +
                "settings=${settings.resolution}/${settings.fps}/${settings.codec} bitrate=${settings.maxBitrateMbps}",
        )
        startTransport(session, settings, transportGeneration)
    }

    fun stop() {
        session = null
        transportGeneration += 1
        reconnectAttempts = 0
        sessionRecoveryRequested = false
        livenessWatchdog.reset()
        closeTransport(clearInputState = true)
        emitState("Stopped")
    }

    fun release() {
        stop()
        scope.cancel()
        renderer?.let(::releaseRenderer)
        factory?.dispose()
        factory = null
        audioDeviceModule.release()
        eglBase.release()
    }

    private fun resetInputState() {
        mouseSources.clear()
        virtualKeySources.clear()
        virtualButtonSources.clear()
        virtualTriggerSources.clear()
        virtualStickSources.clear()
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
        steamMenuChordButtons = 0
        physicalSteamOverlayChord.reset()
        virtualSteamOverlayChord.reset()
        physicalLeftTriggerButtonPressed = false
        physicalRightTriggerButtonPressed = false
        guideAutoReleaseJob?.cancel()
        guideAutoReleaseJob = null
        steamMenuChordJob?.cancel()
        steamMenuChordJob = null
        physicalSteamOverlayChordReleaseJob?.cancel()
        physicalSteamOverlayChordReleaseJob = null
        virtualSteamOverlayChordReleaseJob?.cancel()
        virtualSteamOverlayChordReleaseJob = null
        stopAllGamepadRumble()
        lastLeftTrigger = 0
        lastRightTrigger = 0
        lastLeftStickX = 0
        lastLeftStickY = 0
        lastRightStickX = 0
        lastRightStickY = 0
        controllerMouseAssistActive = false
        controllerMouseAssistAutoArmed = false
        controllerMouseMoveLogged = false
        controllerMouseLeftButtonDown = false
        controllerMouseRightButtonDown = false
        activeControllerId = 0
        controllerSlots.clear()
        mousePositionValid = false
        mouseSuppressNextAbsoluteDelta = false
        inputDropLogged = false
        externalMouseEventLogged = false
        externalMouseMoveSentLogged = false
        externalMouseAbsoluteJumpLogged = false
        hardwareKeyboardEventLogged = false
        physicalGamepadAxisLogged = false
        boosteroidMouseConnected = false
        boosteroidKeyboardConnected = false
        boosteroidMouseAbsolute = true
        boosteroidMouseX = 0.5
        boosteroidMouseY = 0.5
        boosteroidCursorName = "default"
        legacyRelativeMouseFallbackLogged = false
        boosteroidCursorResources.clear()
        boosteroidCursorBitmaps.evictAll()
        boosteroidCursorMissedAtMs.clear()
        pressedBoosteroidMouseButtons.clear()
        boosteroidInputEventCount = 0
        boosteroidRelativeRemainderX = 0.0
        boosteroidRelativeRemainderY = 0.0
        pressedKeyboardCodes.clear()
        announcedControllerSlots.clear()
        controllerNamesBySlot.clear()
        boosteroidControllerIds.clear()
        lastBoosteroidControllerStates.clear()
    }

    fun dispatchKey(event: KeyEvent): Boolean {
        if (event.isGamepadEvent() && dispatchGamepadKey(event)) {
            return true
        }
        val key = BoosteroidKeyMapper.mapKeyEvent(event)
        val hardwareKeyboard = event.isHardwareKeyboardSource()
        if (hardwareKeyboard && !hardwareKeyboardEventLogged) {
            hardwareKeyboardEventLogged = true
            recordBoosteroidInputDiagnostic("hardware keyboard event action=${event.action} key=${event.keyCode} scan=${event.scanCode} source=${event.source} device=${event.deviceId} mapped=${key != null}")
        }
        if (key == null) {
            if (hardwareKeyboard && (event.action == KeyEvent.ACTION_DOWN || event.action == KeyEvent.ACTION_UP)) {
                recordBoosteroidInputDiagnostic("hardware keyboard consumed unmapped key=${event.keyCode} action=${event.action}")
                return true
            }
            return false
        }
        val sent = sendKeyboardButton(key.keycode, event.action == KeyEvent.ACTION_DOWN)
        if (hardwareKeyboard && !sent) {
            recordBoosteroidInputDiagnostic("hardware keyboard consumed without send key=${event.keyCode} control=${signaling != null} data=${boosteroidInputChannel?.state()}")
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

    private fun sendTouchMouseMove(dx: Int, dy: Int): Boolean {
        val adjusted = adjustedMouseDelta(dx, dy)
        return sendBoosteroidMouseMove(adjusted.first, adjusted.second)
    }

    private fun sendKeyboardButton(code: Int, pressed: Boolean): Boolean {
        if (code <= 0) return false
        ensureBoosteroidKeyboardConnected()
        if (pressed) {
            if (!pressedKeyboardCodes.add(code)) return true
        } else {
            pressedKeyboardCodes.remove(code)
        }
        return sendBoosteroidRttInput(buildJsonObject {
            put("type", "keyboard")
            put("action", "button")
            put("isPressed", pressed)
            put("code", code)
        })
    }

    private fun releasePressedKeyboard(reason: String) {
        if (pressedKeyboardCodes.isEmpty()) return
        val keys = pressedKeyboardCodes.toList()
        pressedKeyboardCodes.clear()
        keys.forEach { code ->
            sendBoosteroidInput(buildJsonObject {
                put("type", "keyboard")
                put("action", "button")
                put("isPressed", false)
                put("code", code)
                put("time", System.currentTimeMillis())
            })
        }
        recordBoosteroidInputDiagnostic("released ${keys.size} pressed keyboard key(s): $reason")
    }

    private fun ensureBoosteroidKeyboardConnected() {
        if (boosteroidKeyboardConnected) return
        boosteroidKeyboardConnected = true
        sendBoosteroidInput(buildJsonObject {
            put("type", "keyboard")
            put("action", "connected")
        })
    }

    private fun ensureBoosteroidMouseConnected() {
        if (boosteroidMouseConnected) return
        boosteroidMouseConnected = true
        sendBoosteroidInput(buildJsonObject {
            put("type", "cursor")
            put("action", "missed")
        })
        sendBoosteroidInput(buildJsonObject {
            put("type", "mouse")
            put("action", "connected")
            put("LeftBtnState", false)
            put("MiddleBtnState", false)
            put("RightBtnState", false)
        })
    }

    private fun sendBoosteroidMouseMove(dx: Int, dy: Int): Boolean {
        ensureBoosteroidMouseConnected()
        val (width, height) = streamResolutionPixels(settings)
        boosteroidMouseX = (boosteroidMouseX + dx.toDouble() / width.coerceAtLeast(1)).coerceIn(0.0, 1.0)
        boosteroidMouseY = (boosteroidMouseY + dy.toDouble() / height.coerceAtLeast(1)).coerceIn(0.0, 1.0)
        emitBoosteroidCursorState()
        return sendBoosteroidRttInput(buildJsonObject {
            put("type", "mouse")
            put("action", "move")
            put("X", boosteroidMouseX)
            put("Y", boosteroidMouseY)
            put("offsetX", dx)
            put("offsetY", dy)
            put("isVisible", boosteroidMouseAbsolute)
        })
    }

    private fun sendBoosteroidMouseWheel(deltaY: Int): Boolean {
        ensureBoosteroidMouseConnected()
        return sendBoosteroidRttInput(buildJsonObject {
            put("type", "mouse")
            put("action", "wheel")
            put("deltaY", deltaY.coerceIn(-1, 1))
        })
    }

    private fun dispatchMouseLikePointer(event: MotionEvent): Boolean {
        if (!externalMouseEventLogged) {
            externalMouseEventLogged = true
            val relativeDx = if (Build.VERSION.SDK_INT >= 26) event.getAxisValue(MotionEvent.AXIS_RELATIVE_X) else 0f
            val relativeDy = if (Build.VERSION.SDK_INT >= 26) event.getAxisValue(MotionEvent.AXIS_RELATIVE_Y) else 0f
            recordBoosteroidInputDiagnostic(
                "external mouse event action=${event.actionMasked} source=${event.source} device=${event.deviceId} buttons=${event.buttonState} relativeDx=$relativeDx relativeDy=$relativeDy",
            )
        }
        when (event.actionMasked) {
            MotionEvent.ACTION_HOVER_MOVE,
            MotionEvent.ACTION_MOVE,
            -> {
                val relativeDx = if (Build.VERSION.SDK_INT >= 26) event.getAxisValue(MotionEvent.AXIS_RELATIVE_X) else 0f
                val relativeDy = if (Build.VERSION.SDK_INT >= 26) event.getAxisValue(MotionEvent.AXIS_RELATIVE_Y) else 0f
                if (boosteroidMouseAbsolute && !event.isRelativeMousePointer()) {
                    val activeRenderer = renderer
                    val surfaceWidth = activeRenderer?.width?.takeIf { it > 0 }
                    val surfaceHeight = activeRenderer?.height?.takeIf { it > 0 }
                    if (activeRenderer != null && surfaceWidth != null && surfaceHeight != null) {
                        val location = IntArray(2)
                        activeRenderer.getLocationOnScreen(location)
                        val rendererX = event.rawX - location[0]
                        val rendererY = event.rawY - location[1]
                        val sent = sendBoosteroidAbsoluteMouseMove(rendererX, rendererY, surfaceWidth, surfaceHeight)
                        if (sent && !externalMouseMoveSentLogged) {
                            externalMouseMoveSentLogged = true
                            recordBoosteroidInputDiagnostic("external mouse move sent source=${event.source} device=${event.deviceId} mode=absolute")
                        }
                    }
                    rememberMousePosition(event)
                } else if (abs(relativeDx) >= 0.5f || abs(relativeDy) >= 0.5f) {
                    val sent = sendBoosteroidPhysicalRelativeMouseMove(relativeDx, relativeDy)
                    if (sent && !externalMouseMoveSentLogged) {
                        externalMouseMoveSentLogged = true
                        recordBoosteroidInputDiagnostic("external mouse move sent source=${event.source} device=${event.deviceId} mode=relative")
                    }
                    mousePositionValid = false
                } else if (event.isRelativeMousePointer()) {
                    val positionDx = event.x
                    val positionDy = event.y
                    if (abs(positionDx) >= 0.5f || abs(positionDy) >= 0.5f) {
                        val sent = sendBoosteroidPhysicalRelativeMouseMove(positionDx, positionDy)
                        if (sent && !externalMouseMoveSentLogged) {
                            externalMouseMoveSentLogged = true
                            recordBoosteroidInputDiagnostic("external mouse move sent source=${event.source} device=${event.deviceId} mode=relativePosition")
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
                                recordBoosteroidInputDiagnostic("external mouse absolute delta rebased source=${event.source} device=${event.deviceId} dx=${dx.roundToInt()} dy=${dy.roundToInt()}")
                            }
                        } else {
                            val sent = sendBoosteroidPhysicalRelativeMouseMove(dx, dy)
                            if (sent && !externalMouseMoveSentLogged) {
                                externalMouseMoveSentLogged = true
                                recordBoosteroidInputDiagnostic("external mouse move sent source=${event.source} device=${event.deviceId} mode=absoluteDelta")
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
                sendMouseButton(event.primaryMouseButton(), pressed = true, source = "external mouse")
            }
            MotionEvent.ACTION_UP,
            MotionEvent.ACTION_CANCEL,
            -> {
                mousePositionValid = false
                mouseSuppressNextAbsoluteDelta = true
                sendMouseButton(event.primaryMouseButton(), pressed = false, source = "external mouse")
            }
            MotionEvent.ACTION_BUTTON_PRESS -> {
                mouseSuppressNextAbsoluteDelta = true
                rememberMousePosition(event)
                val handled = sendMouseButton(event.actionButton.toBoosteroidMouseButton(), pressed = true, source = "external mouse")
                if (!handled) {
                    recordBoosteroidInputDiagnostic("external mouse button consumed without send action=press button=${event.actionButton} control=${signaling != null} data=${boosteroidInputChannel?.state()}")
                }
                return true
            }
            MotionEvent.ACTION_BUTTON_RELEASE -> {
                mousePositionValid = false
                mouseSuppressNextAbsoluteDelta = true
                val handled = sendMouseButton(event.actionButton.toBoosteroidMouseButton(), pressed = false, source = "external mouse")
                if (!handled) {
                    recordBoosteroidInputDiagnostic("external mouse button consumed without send action=release button=${event.actionButton} control=${signaling != null} data=${boosteroidInputChannel?.state()}")
                }
                return true
            }
            MotionEvent.ACTION_SCROLL -> {
                val vertical = event.getAxisValue(MotionEvent.AXIS_VSCROLL)
                if (abs(vertical) >= 0.01f) {
                    sendBoosteroidMouseWheel(if (vertical > 0f) -1 else 1)
                }
            }
        }
        return true
    }

    private fun sendBoosteroidAbsoluteMouseMove(x: Float, y: Float, surfaceWidth: Int, surfaceHeight: Int): Boolean {
        ensureBoosteroidMouseConnected()
        val nextX = (x / surfaceWidth.coerceAtLeast(1)).toDouble().coerceIn(0.0, 1.0)
        val nextY = (y / surfaceHeight.coerceAtLeast(1)).toDouble().coerceIn(0.0, 1.0)
        val (streamWidth, streamHeight) = streamResolutionPixels(settings)
        val offsetX = ((nextX - boosteroidMouseX) * streamWidth).roundToInt()
        val offsetY = ((nextY - boosteroidMouseY) * streamHeight).roundToInt()
        boosteroidMouseX = nextX
        boosteroidMouseY = nextY
        emitBoosteroidCursorState()
        return sendBoosteroidRttInput(buildJsonObject {
            put("type", "mouse")
            put("action", "move")
            put("X", nextX)
            put("Y", nextY)
            put("offsetX", offsetX)
            put("offsetY", offsetY)
            put("isVisible", true)
        })
    }

    private fun sendBoosteroidPhysicalRelativeMouseMove(dx: Float, dy: Float): Boolean {
        val surfaceWidth = renderer?.width?.takeIf { it > 0 } ?: return true
        val surfaceHeight = renderer?.height?.takeIf { it > 0 } ?: return true
        val (streamWidth, streamHeight) = streamResolutionPixels(settings)
        val adjusted = adjustedMouseDeltaPrecise(dx, dy)
        val totalX = adjusted.first * streamWidth / surfaceWidth + boosteroidRelativeRemainderX
        val totalY = adjusted.second * streamHeight / surfaceHeight + boosteroidRelativeRemainderY
        val offsetX = totalX.toInt()
        val offsetY = totalY.toInt()
        boosteroidRelativeRemainderX = totalX - offsetX
        boosteroidRelativeRemainderY = totalY - offsetY
        if (offsetX == 0 && offsetY == 0) return true
        return sendBoosteroidMouseMove(offsetX, offsetY)
    }

    private fun rememberMousePosition(event: MotionEvent) {
        mouseLastDeviceId = event.deviceId
        mouseLastSource = event.source
        mouseLastX = event.x
        mouseLastY = event.y
        mousePositionValid = true
    }

    private fun adjustedMouseDelta(dx: Int, dy: Int): Pair<Int, Int> {
        val adjusted = adjustedMouseDeltaPrecise(dx.toFloat(), dy.toFloat())
        return adjusted.first.roundToInt() to adjusted.second.roundToInt()
    }

    private fun adjustedMouseDeltaPrecise(dx: Float, dy: Float): Pair<Double, Double> {
        var adjustedDx = dx.toDouble() * settings.mouseSensitivity
        var adjustedDy = dy.toDouble() * settings.mouseSensitivity
        if (settings.mouseAcceleration > 1) {
            val speed = sqrt(adjustedDx * adjustedDx + adjustedDy * adjustedDy)
            val strength = (settings.mouseAcceleration - 1.0) / 149.0
            val accelFactor = 1.0 + min(0.6 * strength, (speed / 50.0) * strength)
            adjustedDx *= accelFactor
            adjustedDy *= accelFactor
        }
        return adjustedDx to adjustedDy
    }

    override fun sendTouchMouseClick() = clickMouseButton(1)

    override fun sendTouchMouseRightClick() = clickMouseButton(3)

    private fun clickMouseButton(button: Int) {
        val pressSource = "click-${++clickSequence}"
        val generation = transportGeneration
        scope.launch {
            if (!setVirtualMouseButtonFromSource(button, pressSource, true)) return@launch
            try {
                delay(160L)
            } finally {
                if (generation == transportGeneration) setVirtualMouseButtonFromSource(button, pressSource, false)
            }
        }
    }

    override fun sendRawMouseMove(dx: Int, dy: Int): Boolean = sendBoosteroidMouseMove(dx, dy)

    override fun sendTouchMouseWheel(delta: Int) {
        sendBoosteroidMouseWheel(delta)
    }

    override fun setDirectClickMouseButton(pressed: Boolean): Boolean = setTouchMouseButton(pressed)

    override fun sendDirectClickMousePosition(position: AbsoluteMousePosition): Boolean =
        sendBoosteroidAbsoluteMouseMove(
            position.x.toFloat(), position.y.toFloat(), position.width, position.height,
        )

    override fun sendDirectClickMouseClick(position: AbsoluteMousePosition) {
        if (sendDirectClickMousePosition(position)) sendTouchMouseClick()
    }

    override fun setVirtualMouseButtonFromSource(button: Int, sourceId: String, pressed: Boolean): Boolean =
        mouseSources.update(button, sourceId, pressed) {
            // OpenNOW uses SDL numbering. Boosteroid's wire protocol starts at zero.
            sendMouseButton(button - 1, it, sourceId)
        }

    override fun setVirtualKeyboardKeyFromSource(keyCode: Int, sourceId: String, pressed: Boolean) {
        val key = BoosteroidKeyMapper.mapKeyEvent(KeyEvent(KeyEvent.ACTION_DOWN, keyCode)) ?: return
        virtualKeySources.update(key.keycode, sourceId, pressed) { sendKeyboardButton(key.keycode, it) }
    }

    override fun setVirtualButtonFromSource(buttonMask: Int, sourceId: String, pressed: Boolean) {
        virtualButtonSources.update(buttonMask, sourceId, pressed) {
            applyVirtualButton(buttonMask, it)
            true
        }
    }

    override fun setVirtualTriggerFromSource(left: Boolean, sourceId: String, pressed: Boolean) {
        virtualTriggerSources.update(if (left) 0 else 1, sourceId, pressed) {
            applyVirtualTrigger(left, it)
            true
        }
    }

    override fun setVirtualLeftStickFromSource(sourceId: String, x: Float, y: Float) {
        val stick = virtualStickSources.update(sourceId, x, y)
        applyVirtualLeftStick(stick.x, stick.y)
    }

    fun releaseInput() {
        releasePressedKeyboard("focus changed")
        releaseBoosteroidMouseButtons("focus changed")
        resetInputState()
        sendCurrentGamepadState()
    }

    fun sendKeyCode(keyCode: Int) {
        val down = KeyEvent(SystemClock.uptimeMillis(), SystemClock.uptimeMillis(), KeyEvent.ACTION_DOWN, keyCode, 0)
        val up = KeyEvent(SystemClock.uptimeMillis(), SystemClock.uptimeMillis(), KeyEvent.ACTION_UP, keyCode, 0)
        dispatchKey(down)
        dispatchKey(up)
    }

    fun submitText(text: String) {
        val textToSend = text.take(STREAM_TEXT_SEND_MAX_CHARS)
        val generation = transportGeneration
        scope.launch {
            textSendMutex.withLock {
                if (generation != transportGeneration) return@withLock
                if (!sendClipboardText(textToSend)) {
                    for (char in textToSend) {
                        if (generation != transportGeneration) return@withLock
                        sendTextChar(char)
                    }
                } else {
                    delay(80L)
                }
                if (generation == transportGeneration) sendKeyCode(KeyEvent.KEYCODE_ENTER)
            }
        }
    }

    private fun sendClipboardText(text: String): Boolean {
        val value = text.take(STREAM_TEXT_SEND_MAX_CHARS)
        if (value.isEmpty()) return false
        return signaling?.sendEvent(buildJsonObject {
            put("type", "clipboard")
            put("action", "paste")
            put("value", value)
        }) == true
    }

    private fun sendKeyboardPayload(payload: BoosteroidKeyMapper.KeyboardPayload, isDown: Boolean): Boolean =
        sendKeyboardButton(payload.keycode, isDown)

    private suspend fun sendTextChar(char: Char) {
        val spec = BoosteroidKeyMapper.mapTextCharToKeySpec(char) ?: return
        if (spec.shift) {
            sendKeyboardPayloadWithRetry(BoosteroidKeyMapper.shiftLeftPayload(modifiers = 0x01), isDown = true)
        }
        val modifiers = if (spec.shift) 0x01 else 0
        sendKeyboardPayloadWithRetry(spec.toKeyboardPayload(modifiers), isDown = true)
        sendKeyboardPayloadWithRetry(spec.toKeyboardPayload(modifiers), isDown = false)
        if (spec.shift) {
            sendKeyboardPayloadWithRetry(BoosteroidKeyMapper.shiftLeftPayload(modifiers = 0), isDown = false)
        }
        delay(STREAM_TEXT_KEY_DELAY_MS)
    }

    private suspend fun sendKeyboardPayloadWithRetry(payload: BoosteroidKeyMapper.KeyboardPayload, isDown: Boolean): Boolean {
        repeat(STREAM_TEXT_SEND_ATTEMPTS) { attempt ->
            if (sendKeyboardPayload(payload, isDown)) {
                delay(STREAM_TEXT_PACKET_DELAY_MS)
                return true
            }
            if (attempt < STREAM_TEXT_SEND_ATTEMPTS - 1) {
                delay(STREAM_TEXT_RETRY_DELAY_MS)
            }
        }
        recordBoosteroidInputDiagnostic(
            "overlay keyboard dropped key=${payload.keycode} action=${if (isDown) "down" else "up"} control=${signaling != null} data=${boosteroidInputChannel?.state()}",
        )
        return false
    }

    fun setAudioMuted(muted: Boolean) {
        audioMuted = muted
        audioDeviceModule.setSpeakerMute(muted)
        audioTrack?.setEnabled(!muted)
    }

    override fun setTouchMouseButton(pressed: Boolean): Boolean =
        setVirtualMouseButtonFromSource(1, "touch mouse", pressed)

    private fun sendMouseButton(button: Int, pressed: Boolean, source: String): Boolean {
        ensureBoosteroidMouseConnected()
        if (pressed) pressedBoosteroidMouseButtons += button else pressedBoosteroidMouseButtons -= button
        val sent = sendBoosteroidRttInput(buildJsonObject {
            put("type", "mouse")
            put("action", "button")
            put("isPressed", pressed)
            put("btn", button.coerceIn(0, 4))
        })
        recordBoosteroidInputDiagnostic(
            "$source button=$button ${if (pressed) "down" else "up"} sent=$sent control=${signaling != null} data=${boosteroidInputChannel?.state()}",
        )
        return sent
    }

    private fun releaseBoosteroidMouseButtons(reason: String) {
        if (pressedBoosteroidMouseButtons.isEmpty()) return
        val buttons = pressedBoosteroidMouseButtons.toList()
        pressedBoosteroidMouseButtons.clear()
        buttons.forEach { button ->
            sendBoosteroidInput(buildJsonObject {
                put("type", "mouse")
                put("action", "button")
                put("isPressed", false)
                put("btn", button)
                put("time", System.currentTimeMillis())
            })
        }
        recordBoosteroidInputDiagnostic("released ${buttons.size} pressed mouse button(s): $reason")
    }

    private fun setControllerMouseAssistActive(active: Boolean, autoArmed: Boolean = false) {
        if (controllerMouseAssistActive == active && controllerMouseAssistAutoArmed == (autoArmed && active)) return
        if (!active) releaseControllerMouseButtons()
        controllerMouseAssistActive = active
        controllerMouseAssistAutoArmed = autoArmed && active
        controllerMouseMoveLogged = false
        recordBoosteroidInputDiagnostic("controller mouse assist ${if (active) "enabled" else "disabled"} auto=$controllerMouseAssistAutoArmed")
    }

    private fun releaseControllerMouseButtons() {
        if (controllerMouseLeftButtonDown) {
            controllerMouseLeftButtonDown = false
            sendMouseButton(button = 0, pressed = false, source = "controller mouse")
        }
        if (controllerMouseRightButtonDown) {
            controllerMouseRightButtonDown = false
            sendMouseButton(button = 2, pressed = false, source = "controller mouse")
        }
    }

    private fun setControllerMouseButton(button: Int, pressed: Boolean): Boolean {
        when (button) {
            0 -> {
                if (controllerMouseLeftButtonDown == pressed) return true
                controllerMouseLeftButtonDown = pressed
            }
            2 -> {
                if (controllerMouseRightButtonDown == pressed) return true
                controllerMouseRightButtonDown = pressed
            }
            else -> return false
        }
        val sent = sendMouseButton(button = button, pressed = pressed, source = "controller mouse")
        if (!pressed && controllerMouseAssistAutoArmed && button == 0) {
            setControllerMouseAssistActive(false)
        }
        return sent
    }

    override fun setVirtualButton(buttonMask: Int, pressed: Boolean) =
        setVirtualButtonFromSource(buttonMask, "primary-$buttonMask", pressed)

    private fun applyVirtualButton(buttonMask: Int, pressed: Boolean) {
        virtualButtons = if (pressed) virtualButtons or buttonMask else virtualButtons and buttonMask.inv()
        val steamOverlayChordActivated = virtualSteamOverlayChord.update(virtualButtons)
        sendCurrentGamepadState()
        if (steamOverlayChordActivated) {
            scheduleVirtualSteamOverlayChordRelease()
        }
    }

    fun openSteamMenu() {
        steamMenuChordJob?.cancel()
        val controllerId = activeControllerId
        steamMenuChordButtons = SteamMenuChord.buttons(aPressed = false)
        sendCurrentGamepadState(controllerId)
        steamMenuChordJob = scope.launch {
            delay(STEAM_MENU_MODIFIER_DELAY_MS)
            steamMenuChordButtons = SteamMenuChord.buttons(aPressed = true)
            sendCurrentGamepadState(controllerId)
            delay(GAMEPAD_GUIDE_AUTO_RELEASE_MS)
            steamMenuChordButtons = SteamMenuChord.buttons(aPressed = false)
            sendCurrentGamepadState(controllerId)
            delay(STEAM_MENU_MODIFIER_DELAY_MS)
            steamMenuChordButtons = 0
            sendCurrentGamepadState(controllerId)
            recordBoosteroidInputDiagnostic("Steam Menu sent Guide+A chord slot=$controllerId")
        }
    }

    override fun setVirtualTrigger(left: Boolean, pressed: Boolean) =
        setVirtualTriggerFromSource(left, "primary-$left", pressed)

    private fun applyVirtualTrigger(left: Boolean, pressed: Boolean) {
        if (left) {
            virtualLeftTrigger = if (pressed) 255 else 0
        } else {
            virtualRightTrigger = if (pressed) 255 else 0
        }
        sendCurrentGamepadState()
    }

    override fun setVirtualLeftStick(x: Float, y: Float) =
        setVirtualLeftStickFromSource("left stick", x, y)

    private fun applyVirtualLeftStick(x: Float, y: Float) {
        val normalized = applyDeadzone(x, y, deadzone = 0.08f)
        virtualLeftStickActive = normalized.first != 0f || normalized.second != 0f
        virtualLeftStickX = normalizeToInt16(normalized.first)
        virtualLeftStickY = normalizeToInt16(-normalized.second)
        sendCurrentGamepadState()
    }

    override fun setVirtualRightStick(x: Float, y: Float) {
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

    private fun startTransport(session: BoosteroidSession, settings: StreamSettings, generation: Int) {
        inputDropLogged = false
        lastIceState = null
        lastStatsSample = null
        gatewayCodec = ""
        remoteIceCandidates.clear()
        emitStats(StreamRuntimeStats())
        audioDeviceModule.setSpeakerMute(audioMuted)
        recordStreamDiagnostic(
            "transport start generation=$generation reconnectAttempts=$reconnectAttempts " +
                "session=${streamDiagnosticId(session.sessionId)} gateway=${session.gatewayHost}",
        )
        emitState(if (reconnectAttempts > 0) "Reconnecting control socket" else "Opening control socket")
        signaling = BoosteroidGatewayClient(
            session = session,
            settings = settings,
            networkType = boosteroidNetworkType(appContext),
            http = gatewayHttp,
        ) { event ->
            scope.launch { handleGatewayEvent(event, generation) }
        }.also { it.connect() }
    }

    private fun closeTransport(clearInputState: Boolean, cancelRecovery: Boolean = true) {
        if (peerConnection != null || signaling != null || boosteroidInputChannel != null) {
            recordStreamDiagnostic("transport close clearInput=$clearInputState cancelRecovery=$cancelRecovery lastIce=${lastIceState?.name ?: "none"}")
        }
        if (cancelRecovery) {
            iceRecoveryJob?.cancel()
            iceRecoveryJob = null
        }
        gamepadKeepaliveJob?.cancel()
        statsJob?.cancel()
        remoteIcePollingJob?.cancel()
        offerTimeoutJob?.cancel()
        negotiationJob?.cancel()
        connectionTimeoutJob?.cancel()
        negotiationJob = null
        connectionTimeoutJob = null
        gamepadKeepaliveJob = null
        statsJob = null
        remoteIcePollingJob = null
        offerTimeoutJob = null
        lastStatsSample = null
        lastIceState = null
        livenessWatchdog.reset()
        releasePressedKeyboard("transport close")
        releaseBoosteroidMouseButtons("transport close")
        (announcedControllerSlots + boosteroidControllerIds.keys).toSet().forEach(::disconnectBoosteroidController)
        signaling?.disconnect()
        signaling = null
        boosteroidInputChannel?.close()
        boosteroidInputChannel?.dispose()
        boosteroidInputChannel = null
        boosteroidKeyboardConnected = false
        boosteroidMouseConnected = false
        boosteroidMouseAbsolute = true
        boosteroidMouseX = 0.5
        boosteroidMouseY = 0.5
        boosteroidCursorName = "default"
        legacyRelativeMouseFallbackLogged = false
        boosteroidCursorResources.clear()
        boosteroidCursorBitmaps.evictAll()
        boosteroidCursorMissedAtMs.clear()
        pressedBoosteroidMouseButtons.clear()
        boosteroidRelativeRemainderX = 0.0
        boosteroidRelativeRemainderY = 0.0
        announcedControllerSlots.clear()
        controllerNamesBySlot.clear()
        boosteroidControllerIds.clear()
        lastBoosteroidControllerStates.clear()
        remoteIceCandidates.clear()
        if (clearInputState) resetInputState()
        emitBoosteroidCursorState()
        if (rendererSinkAttached) {
            videoTrack?.removeSink(renderer)
            rendererSinkAttached = false
        }
        videoTrack = null
        audioTrack = null
        peerConnection?.close()
        peerConnection?.dispose()
        peerConnection = null
    }

    private fun handleGatewayEvent(event: BoosteroidGatewayEvent, generation: Int) {
        if (generation != transportGeneration) return
        when (event) {
            BoosteroidGatewayEvent.Connected -> {
                recordStreamDiagnostic("control socket connected generation=$generation")
                emitState("Control socket connected")
                ensureBoosteroidKeyboardConnected()
                ensureBoosteroidMouseConnected()
                startGamepadKeepalive()
                startOfferTimeout(generation)
            }
            is BoosteroidGatewayEvent.Disconnected -> {
                recordStreamDiagnostic("control socket disconnected ${event.reason}")
                scheduleTransportReconnect("Control socket disconnected: ${event.reason}", SIGNALING_RECONNECT_DELAY_MS, generation)
            }
            is BoosteroidGatewayEvent.Error -> {
                recordStreamDiagnostic("gateway error ${event.message}")
                scheduleTransportReconnect("Boosteroid gateway failed: ${event.message}", SIGNALING_RECONNECT_DELAY_MS, generation)
            }
            is BoosteroidGatewayEvent.Log -> recordStreamDiagnostic(event.message)
            BoosteroidGatewayEvent.WebRtcRequested -> startWebRtcTransport(generation)
            is BoosteroidGatewayEvent.StreamStatus -> {
                gatewayCodec = event.value.string("codec").orEmpty().ifBlank { gatewayCodec }
                recordStreamDiagnostic(
                    "gateway setstatus codec=${gatewayCodec.ifBlank { "unknown" }} " +
                        "fps=${event.value.int("framerate") ?: settings.fps} fsr=${event.value.boolean("fsr")}",
                )
            }
            is BoosteroidGatewayEvent.CursorMessage -> handleBoosteroidCursorMessage(event.value)
            is BoosteroidGatewayEvent.ControllerMessage -> handleBoosteroidControllerMessage(event.value)
        }
    }

    private fun handleBoosteroidCursorMessage(message: JsonObject) {
        message.double("X")?.let { boosteroidMouseX = it.coerceIn(0.0, 1.0) }
        message.double("Y")?.let { boosteroidMouseY = it.coerceIn(0.0, 1.0) }
        message.boolean("isVisible")?.let { requestedAbsolute ->
            val nextAbsolute = requestedAbsolute || Build.VERSION.SDK_INT < 26
            if (nextAbsolute != boosteroidMouseAbsolute) {
                boosteroidRelativeRemainderX = 0.0
                boosteroidRelativeRemainderY = 0.0
            }
            boosteroidMouseAbsolute = nextAbsolute
            if (!requestedAbsolute && Build.VERSION.SDK_INT < 26 && !legacyRelativeMouseFallbackLogged) {
                legacyRelativeMouseFallbackLogged = true
                recordStreamDiagnostic("relative mouse capture unavailable below Android 8; using absolute transport")
            }
        }
        val name = message.string("name")?.takeIf(String::isNotBlank)
        if (name != null) boosteroidCursorName = name
        emitBoosteroidCursorState()
        if (name == null) return
        val resource = message.string("resource")
        if (!resource.isNullOrBlank()) {
            val zipped = message.boolean("zipped") == true
            val generation = transportGeneration
            val targetSizePx = (32f * appContext.resources.displayMetrics.density).roundToInt().coerceIn(24, 256)
            recordStreamDiagnostic("cursor resource received name=$name zipped=$zipped")
            scope.launch {
                val decoded = withContext(Dispatchers.Default) {
                    decodeBoosteroidCursorResource(resource, zipped, targetSizePx)
                }
                if (generation != transportGeneration) return@launch
                if (decoded == null) {
                    recordStreamDiagnostic("cursor resource decode failed name=$name")
                    requestMissingBoosteroidCursor(name)
                } else {
                    boosteroidCursorResources += name
                    boosteroidCursorMissedAtMs.remove(name)
                    boosteroidCursorBitmaps.put(name, decoded)
                    if (boosteroidCursorName == name) emitBoosteroidCursorState()
                }
            }
            return
        }
        if (name == "default" || name == "none" || name in boosteroidCursorResources) return
        requestMissingBoosteroidCursor(name)
    }

    private fun requestMissingBoosteroidCursor(name: String) {
        val now = SystemClock.elapsedRealtime()
        val lastRequest = boosteroidCursorMissedAtMs[name]
        if (lastRequest != null && now - lastRequest < BOOSTEROID_CURSOR_MISS_COOLDOWN_MS) return
        boosteroidCursorMissedAtMs[name] = now
        sendBoosteroidInput(buildJsonObject {
            put("type", "cursor")
            put("action", "missed")
            put("name", name)
        })
    }

    private fun emitBoosteroidCursorState() {
        val absolute = boosteroidMouseAbsolute
        val name = boosteroidCursorName
        val normalizedX = boosteroidMouseX.toFloat()
        val normalizedY = boosteroidMouseY.toFloat()
        scope.launch {
            val decoded = boosteroidCursorBitmaps.get(name)
            onRemoteCursor(
                StreamRemoteCursorState(
                    absolute = absolute,
                    visible = !name.equals("none", ignoreCase = true),
                    normalizedX = normalizedX,
                    normalizedY = normalizedY,
                    name = name,
                    bitmap = decoded?.bitmap,
                    hotSpotX = decoded?.hotSpotX ?: 0f,
                    hotSpotY = decoded?.hotSpotY ?: 0f,
                ),
            )
        }
    }

    private fun startWebRtcTransport(generation: Int) {
        if (generation != transportGeneration || peerConnection != null || negotiationJob?.isActive == true) return
        offerTimeoutJob?.cancel()
        offerTimeoutJob = null
        emitState("Starting WebRTC")
        negotiationJob = scope.launch {
            val gateway = signaling ?: return@launch
            boosteroidResult {
                val iceServers = gateway.fetchIceServers()
                gatewayCodec = gateway.fetchGatewayCodec()
                if (generation != transportGeneration) return@launch
                val pc = ensurePeerConnection(iceServers, generation)
                val codec = if (gateway.activeCodec == VideoCodec.AV1 && !gatewayCodec.equals("H264", true)) {
                    VideoCodec.AV1
                } else {
                    VideoCodec.H264
                }
                applyVideoCodecPreferences(pc, codec)
                pc.createOffer(object : SimpleSdpObserver() {
                    override fun onCreateSuccess(description: SessionDescription?) {
                        scope.launch(Dispatchers.Main) applyOffer@{
                            if (generation != transportGeneration) return@applyOffer
                            if (description == null) {
                                failStream("WebRTC returned an empty offer", generation)
                                return@applyOffer
                            }
                            val sdp = BoosteroidSdpTools.prepareOffer(description.description, codec, gatewayCodec)
                            applyLocalOffer(pc, gateway, sdp, generation)
                        }
                    }

                    override fun onCreateFailure(error: String?) {
                        postPeerFailure(generation, error ?: "Failed to create WebRTC offer")
                    }
                }, MediaConstraints().apply {
                    mandatory.add(MediaConstraints.KeyValuePair("OfferToReceiveAudio", "true"))
                    mandatory.add(MediaConstraints.KeyValuePair("OfferToReceiveVideo", "true"))
                })
                connectionTimeoutJob = scope.launch {
                    delay(CONNECTION_TIMEOUT_MS)
                    if (generation == transportGeneration) restartTransport("Boosteroid media timed out")
                }
            }.onFailure { failStream("Failed to start Boosteroid WebRTC", generation) }
        }
    }

    private fun applyLocalOffer(pc: PeerConnection, gateway: BoosteroidGatewayClient, sdp: String, generation: Int) {
        recordStreamDiagnostic(sdpDiagnosticSummary("created offer", sdp))
        pc.setLocalDescription(object : SimpleSdpObserver() {
            override fun onSetSuccess() {
                scope.launch(Dispatchers.Main) {
                    if (generation != transportGeneration) return@launch
                    boosteroidResult { gateway.sendOffer(sdp) }
                        .onSuccess { answer ->
                            if (generation != transportGeneration) return@onSuccess
                            recordStreamDiagnostic(sdpDiagnosticSummary("received answer", answer.description))
                            pc.setRemoteDescription(object : SimpleSdpObserver() {
                                override fun onSetSuccess() {
                                    scope.launch(Dispatchers.Main) remoteDescription@{
                                        if (generation != transportGeneration) return@remoteDescription
                                        emitState("Connecting stream")
                                        startRemoteIcePolling(generation)
                                    }
                                }

                                override fun onSetFailure(error: String?) {
                                    postPeerFailure(generation, error ?: "Failed to apply Boosteroid answer")
                                }
                            }, answer)
                        }
                        .onFailure { failStream("Boosteroid rejected the WebRTC offer", generation) }
                }
            }

            override fun onSetFailure(error: String?) {
                postPeerFailure(generation, error ?: "Failed to set local WebRTC offer")
            }
        }, SessionDescription(SessionDescription.Type.OFFER, sdp))
    }

    private fun postPeerFailure(generation: Int, message: String) {
        scope.launch(Dispatchers.Main) { failStream(message, generation) }
    }

    private fun attachRemoteTrack(track: MediaStreamTrack?, generation: Int) {
        scope.launch(Dispatchers.Main) {
            if (generation != transportGeneration) return@launch
            when (track) {
                is VideoTrack -> if (videoTrack !== track) attachVideo(track)
                is AudioTrack -> {
                    audioTrack = track
                    track.setEnabled(!audioMuted)
                }
            }
        }
    }

    private fun applyVideoCodecPreferences(pc: PeerConnection, codec: VideoCodec) {
        val preferences = receiverCodecPreferences(codec)
        if (preferences.isEmpty()) return
        val transceiver = pc.transceivers.firstOrNull {
            it.mediaType == MediaStreamTrack.MediaType.MEDIA_TYPE_VIDEO ||
                it.receiver?.track()?.kind() == MediaStreamTrack.VIDEO_TRACK_KIND
        } ?: return
        val result = transceiver.setCodecPreferences(preferences)
        if (result.isSuccess) {
            recordBoosteroidInputDiagnostic("codec preferences applied codec=$codec count=${preferences.size}")
        } else {
            recordBoosteroidInputDiagnostic("codec preferences failed codec=$codec error=${result.error()?.message.orEmpty()}")
        }
    }

    private fun receiverCodecPreferences(codec: VideoCodec): List<RtpCapabilities.CodecCapability> {
        val receiverCaps = runCatching {
            requireNotNull(factory).getRtpReceiverCapabilities(MediaStreamTrack.MediaType.MEDIA_TYPE_VIDEO).codecs
        }.getOrNull().orEmpty()
        val target = codec.webRtcCodecName()
        val preferred = receiverCaps.filter { it.openNowCodecName() == target }
        if (preferred.isEmpty()) return emptyList()
        val auxiliary = receiverCaps.filter { it.openNowCodecName() in WEBRTC_AUXILIARY_VIDEO_CODECS }
        return (preferred + auxiliary).distinctBy { it.preferenceKey() }
    }

    private fun startOfferTimeout(generation: Int) {
        offerTimeoutJob?.cancel()
        offerTimeoutJob = scope.launch {
            delay(OFFER_TIMEOUT_MS)
            if (generation != transportGeneration || peerConnection != null) return@launch
            offerTimeoutJob = null
            recordBoosteroidInputDiagnostic("Boosteroid WebRTC request timeout codec=${settings.codec} resolution=${settings.resolution}")
            restartTransport("Timed out waiting for Boosteroid WebRTC setup")
        }
    }

    private fun ensurePeerConnection(ice: List<PeerConnection.IceServer>, generation: Int): PeerConnection {
        peerConnection?.let { return it }
        val config = PeerConnection.RTCConfiguration(ice).apply {
            sdpSemantics = PeerConnection.SdpSemantics.UNIFIED_PLAN
            continualGatheringPolicy = PeerConnection.ContinualGatheringPolicy.GATHER_CONTINUALLY
            bundlePolicy = PeerConnection.BundlePolicy.MAXBUNDLE
            rtcpMuxPolicy = PeerConnection.RtcpMuxPolicy.REQUIRE
        }
        recordStreamDiagnostic("peer connection create generation=$generation iceServers=${ice.size}")
        val pc = requireNotNull(factory).createPeerConnection(config, object : PeerConnection.Observer {
            override fun onSignalingChange(state: PeerConnection.SignalingState?) {
                recordStreamDiagnostic("webrtc signaling state=${state?.name ?: "null"}")
            }
            override fun onIceConnectionChange(state: PeerConnection.IceConnectionState?) {
                handleIceConnectionChange(state, generation)
            }
            override fun onIceConnectionReceivingChange(receiving: Boolean) {
                recordStreamDiagnostic("ice receiving=$receiving generation=$generation")
            }
            override fun onIceGatheringChange(state: PeerConnection.IceGatheringState?) {
                recordStreamDiagnostic("ice gathering state=${state?.name ?: "null"} generation=$generation")
            }
            override fun onIceCandidate(candidate: IceCandidate?) {
                if (candidate != null) {
                    recordStreamDiagnostic("local ICE candidate gathered ${candidate.diagnosticSummary()}")
                    scope.launch {
                        if (generation == transportGeneration) signaling?.sendIceCandidate(candidate)
                    }
                } else {
                    recordStreamDiagnostic("local ICE candidate gathering complete")
                }
            }
            override fun onIceCandidatesRemoved(candidates: Array<out IceCandidate>?) {
                recordStreamDiagnostic("ice candidates removed count=${candidates?.size ?: 0}")
            }
            override fun onAddStream(stream: MediaStream?) = Unit
            override fun onRemoveStream(stream: MediaStream?) {
                recordStreamDiagnostic("media stream removed video=${stream?.videoTracks?.size ?: 0} audio=${stream?.audioTracks?.size ?: 0}")
            }
            override fun onDataChannel(channel: DataChannel?) {
                if (channel?.label() == BOOSTEROID_DATA_CHANNEL_LABEL) {
                    scope.launch {
                        if (generation == transportGeneration) attachDataChannel(channel)
                    }
                }
            }
            override fun onRenegotiationNeeded() {
                recordStreamDiagnostic("renegotiation needed")
            }
            override fun onAddTrack(receiver: RtpReceiver?, streams: Array<out MediaStream>?) {
                attachRemoteTrack(receiver?.track(), generation)
            }
            override fun onTrack(transceiver: RtpTransceiver?) {
                attachRemoteTrack(transceiver?.receiver?.track(), generation)
            }
        }) ?: error("Failed to create PeerConnection")
        pc.addTransceiver(
            MediaStreamTrack.MediaType.MEDIA_TYPE_AUDIO,
            RtpTransceiver.RtpTransceiverInit(RtpTransceiver.RtpTransceiverDirection.RECV_ONLY),
        )
        pc.addTransceiver(
            MediaStreamTrack.MediaType.MEDIA_TYPE_VIDEO,
            RtpTransceiver.RtpTransceiverInit(RtpTransceiver.RtpTransceiverDirection.RECV_ONLY),
        )
        pc.createDataChannel(
            BOOSTEROID_DATA_CHANNEL_LABEL,
            DataChannel.Init().apply { ordered = true },
        )?.let(::attachDataChannel)
        peerConnection = pc
        recordStreamDiagnostic("peer connection ready generation=$generation")
        return pc
    }

    private fun startRemoteIcePolling(generation: Int) {
        remoteIcePollingJob?.cancel()
        remoteIcePollingJob = scope.launch {
            while (generation == transportGeneration) {
                delay(BOOSTEROID_REMOTE_ICE_POLL_MS)
                val pc = peerConnection ?: continue
                val candidates = signaling?.fetchRemoteIceCandidates().orEmpty()
                candidates.forEach { candidate ->
                    val key = "${candidate.sdpMid}:${candidate.sdpMLineIndex}:${candidate.sdp}"
                    if (!remoteIceCandidates.add(key)) return@forEach
                    val accepted = pc.addIceCandidate(candidate)
                    recordStreamDiagnostic("remote ICE add accepted=$accepted ${candidate.diagnosticSummary()}")
                }
            }
        }
    }

    private fun handleIceConnectionChange(state: PeerConnection.IceConnectionState?, generation: Int) {
        scope.launch {
            if (generation != transportGeneration) return@launch
            val previous = lastIceState
            lastIceState = state
            recordStreamDiagnostic("ice connection ${previous?.name ?: "none"} -> ${state?.name ?: "null"} generation=$generation")
            when (state) {
                PeerConnection.IceConnectionState.CONNECTED,
                PeerConnection.IceConnectionState.COMPLETED,
                -> {
                    iceRecoveryJob?.cancel()
                    iceRecoveryJob = null
                    connectionTimeoutJob?.cancel()
                    connectionTimeoutJob = null
                    reconnectAttempts = 0
                    livenessWatchdog.markConnected(SystemClock.elapsedRealtime())
                    emitState("Streaming")
                    signaling?.sendReady()
                    startGamepadKeepalive()
                    startStatsPolling()
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
        if (generation != transportGeneration || iceRecoveryJob?.isActive == true) {
            recordStreamDiagnostic("reconnect not scheduled reason=$reason generation=$generation activeJob=${iceRecoveryJob?.isActive == true}")
            return
        }
        recordStreamDiagnostic("reconnect scheduled reason=$reason delayMs=$delayMs generation=$generation")
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
            recordStreamDiagnostic("reconnect limit reached reason=$reason attempts=$reconnectAttempts")
            requestSessionRecovery("$reason. Stream reconnect failed after $MAX_TRANSPORT_RECONNECT_ATTEMPTS attempts.")
            return
        }
        reconnectAttempts += 1
        transportGeneration += 1
        val generation = transportGeneration
        recordStreamDiagnostic("transport restart reason=$reason attempt=$reconnectAttempts generation=$generation")
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
        recordStreamDiagnostic("session recovery requested message=$message")
        emitState("Recovering cloud session")
        emitSessionRecoveryRequired(message)
    }

    private fun failStream(message: String, generation: Int? = null) {
        if (generation != null && generation != transportGeneration) return
        transportGeneration += 1
        recordStreamDiagnostic("stream failed message=$message")
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

    private fun attachVideo(track: VideoTrack) {
        if (rendererSinkAttached) {
            videoTrack?.removeSink(renderer)
            rendererSinkAttached = false
        }
        videoTrack = track
        track.setEnabled(true)
        renderer?.let(::attachRendererSinkIfAvailable)
        recordStreamDiagnostic(
            "video track attached id=${track.id()} state=${track.state()?.name ?: "unknown"} " +
                "renderer=${renderer != null} sink=$rendererSinkAttached",
        )
    }

    private fun attachDataChannel(channel: DataChannel) {
        val label = channel.label()
        if (label != BOOSTEROID_DATA_CHANNEL_LABEL) return
        val generation = transportGeneration
        boosteroidInputChannel = channel
        recordBoosteroidInputDiagnostic("Boosteroid data channel attached label=$label state=${channel.state()}")
        channel.registerObserver(object : DataChannel.Observer {
            override fun onBufferedAmountChange(previousAmount: Long) = Unit
            override fun onStateChange() {
                scope.launch {
                    if (generation != transportGeneration || boosteroidInputChannel !== channel) return@launch
                    recordBoosteroidInputDiagnostic("Boosteroid data channel state label=$label state=${channel.state()}")
                    if (channel.state() == DataChannel.State.OPEN) {
                        inputDropLogged = false
                        recordBoosteroidInputDiagnostic("Boosteroid data channel open label=$label")
                        schedulePrimeConnectedGamepadState(reason = "channel open $label")
                    }
                }
            }
            override fun onMessage(buffer: DataChannel.Buffer) {
                val text = buffer.data.duplicate().let { data ->
                    ByteArray(data.remaining()).also(data::get)
                }.toString(Charsets.UTF_8)
                scope.launch {
                    if (generation == transportGeneration && boosteroidInputChannel === channel) {
                        handleInputChannelMessage(text)
                    }
                }
            }
        })
    }

    private fun handleInputChannelMessage(text: String) {
        val message = runCatching { OpenNowJson.parseToJsonElement(text).asObject() }.getOrNull() ?: return
        if (message.string("type") == "controller") handleBoosteroidControllerMessage(message)
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
            scope.launch {
                if (generation != transportGeneration) return@launch
                val snapshot = buildRuntimeStatsSnapshot(report.timestampUs / 1000.0, report.statsMap.values)
                handleMediaLiveness(snapshot)
                sendBoosteroidRuntimeStats(snapshot.stats)
                onStats(snapshot.stats)
            }
        })
    }

    private fun sendBoosteroidRuntimeStats(stats: StreamRuntimeStats) {
        signaling?.sendEvent(buildJsonObject {
            put("type", "stream")
            put("action", "bitrate")
            put("realBitrate", (stats.bitrateKbps ?: 0).coerceAtLeast(0) * 1000)
            put("framerateDecoded", (stats.fps ?: 0).coerceAtLeast(0))
            put("framerateReceived", (stats.fps ?: 0).coerceAtLeast(0))
            put("lossPacket", 0)
            put("time", System.currentTimeMillis())
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
                    attempt = action.attempt,
                )
                emitState("Recovering video")
                recordBoosteroidInputDiagnostic("media stall keyframe requested stalledMs=${action.stalledMs} attempt=${action.attempt}")
            }
            is StreamLivenessAction.RestartTransport -> {
                recordBoosteroidInputDiagnostic(
                    "media stall transport restart stalledMs=${action.stalledMs} preserving=${settings.resolution}/${settings.codec}",
                )
                restartTransport("Media stalled for ${action.stalledMs / 1000}s")
            }
        }
    }

    private fun requestSafeVideoFallback(
        message: String,
        diagnosticReason: String,
        recreateWhenAlreadySafe: Boolean = false,
    ): Boolean {
        val fallback = settings.androidSafeVideoFallback()
        val alreadySafe = settings == fallback
        if (videoSafeFallbackApplied || (alreadySafe && !recreateWhenAlreadySafe)) return false
        videoSafeFallbackApplied = true
        currentSettings = fallback
        recordBoosteroidInputDiagnostic(
            "$diagnosticReason ${if (alreadySafe) "safe profile cloud session recreate" else "safe video fallback"} codec=${fallback.codec} resolution=${fallback.resolution} fps=${fallback.fps} bitrate=${fallback.maxBitrateMbps}",
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
            recordBoosteroidInputDiagnostic("physical gamepad motion source=${event.source} device=${event.deviceId} slot=$controllerId")
        }
        physicalControllerConnected = true
        physicalControllerActive = true
        val rawAxes = event.rawGamepadAxes()
        val axisAvailability = event.axisAvailability()
        val axes = AndroidGamepadAxisMapping.resolve(rawAxes, axisAvailability)
        if (!physicalGamepadAxisLogged) {
            physicalGamepadAxisLogged = true
            recordBoosteroidInputDiagnostic(
                "physical gamepad axes left=${axes.leftSource} right=${axes.rightSource} hatAsLeft=${axes.hatUsedAsLeftStick} " +
                    "x=${rawAxes.x.formatAxis()} y=${rawAxes.y.formatAxis()} z=${rawAxes.z.formatAxis()} rz=${rawAxes.rz.formatAxis()} " +
                    "rx=${rawAxes.rx.formatAxis()} ry=${rawAxes.ry.formatAxis()} hatX=${rawAxes.hatX.formatAxis()} hatY=${rawAxes.hatY.formatAxis()}",
            )
        }
        val lt = max(
            max(event.getAxisValue(MotionEvent.AXIS_LTRIGGER), normalizeTriggerAxis(event.getAxisValue(MotionEvent.AXIS_BRAKE))),
            if (physicalLeftTriggerButtonPressed) 1f else 0f,
        )
        val rt = max(
            max(event.getAxisValue(MotionEvent.AXIS_RTRIGGER), normalizeTriggerAxis(event.getAxisValue(MotionEvent.AXIS_GAS))),
            if (physicalRightTriggerButtonPressed) 1f else 0f,
        )
        val left = applyDeadzone(axes.leftX, axes.leftY)
        val right = applyDeadzone(axes.rightX, axes.rightY)
        physicalHatButtons = if (axes.hatUsedAsLeftStick) 0 else event.hatDpadButtons()
        lastLeftTrigger = normalizeToUint8(lt)
        lastRightTrigger = normalizeToUint8(rt)
        lastLeftStickX = normalizeToInt16(left.first)
        lastLeftStickY = normalizeToInt16(-left.second)
        lastRightStickX = normalizeToInt16(right.first)
        lastRightStickY = normalizeToInt16(-right.second)
        val mouseSent = sendControllerMouseMove(right.first, right.second)
        return sendCurrentGamepadState(controllerId = controllerId) || mouseSent
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
                recordBoosteroidInputDiagnostic("physical gamepad key source=${event.source} device=${event.deviceId} slot=$activeControllerId key=${event.keyCode}")
            }
            physicalControllerConnected = true
            physicalControllerActive = true
            val mouseSent = handleControllerMouseButton(mask, pressed)
            physicalButtons = if (pressed) physicalButtons or mask else physicalButtons and mask.inv()
            val steamOverlayChordActivated = physicalSteamOverlayChord.update(physicalButtons)
            val sent = sendCurrentGamepadState(controllerId = activeControllerId)
            updateGuideAutoRelease(mask, pressed, activeControllerId)
            if (steamOverlayChordActivated) {
                schedulePhysicalSteamOverlayChordRelease(activeControllerId)
            }
            return sent || mouseSent
        }
        when (event.keyCode) {
            KeyEvent.KEYCODE_BUTTON_L2 -> {
                activeControllerId = controllerIdFor(event)
                if (!physicalControllerActive) {
                    recordBoosteroidInputDiagnostic("physical gamepad key source=${event.source} device=${event.deviceId} slot=$activeControllerId key=${event.keyCode}")
                }
                physicalControllerConnected = true
                physicalControllerActive = true
                physicalLeftTriggerButtonPressed = pressed
                lastLeftTrigger = if (pressed) 255 else 0
                val mouseSent = handleControllerMouseTrigger(left = true, pressed = pressed)
                return sendCurrentGamepadState(controllerId = activeControllerId) || mouseSent
            }
            KeyEvent.KEYCODE_BUTTON_R2 -> {
                activeControllerId = controllerIdFor(event)
                if (!physicalControllerActive) {
                    recordBoosteroidInputDiagnostic("physical gamepad key source=${event.source} device=${event.deviceId} slot=$activeControllerId key=${event.keyCode}")
                }
                physicalControllerConnected = true
                physicalControllerActive = true
                physicalRightTriggerButtonPressed = pressed
                lastRightTrigger = if (pressed) 255 else 0
                val mouseSent = handleControllerMouseTrigger(left = false, pressed = pressed)
                return sendCurrentGamepadState(controllerId = activeControllerId) || mouseSent
            }
        }
        return false
    }

    private fun sendControllerMouseMove(stickX: Float, stickY: Float): Boolean {
        if (!controllerMouseAssistActive) return false
        val delta = AndroidControllerMouseAssist.mouseDelta(stickX, stickY) ?: return false
        val sent = sendTouchMouseMove(delta.dx, delta.dy)
        if (sent && !controllerMouseMoveLogged) {
            controllerMouseMoveLogged = true
            recordBoosteroidInputDiagnostic("controller mouse move sent dx=${delta.dx} dy=${delta.dy} auto=$controllerMouseAssistAutoArmed")
        }
        return sent
    }

    private fun handleControllerMouseButton(buttonMask: Int, pressed: Boolean): Boolean {
        if (!controllerMouseAssistActive) return false
        val mouseButton = AndroidControllerMouseAssist.mouseButtonForGamepad(buttonMask) ?: return false
        return setControllerMouseButton(mouseButton, pressed)
    }

    private fun handleControllerMouseTrigger(left: Boolean, pressed: Boolean): Boolean {
        if (!controllerMouseAssistActive) return false
        val mouseButton = AndroidControllerMouseAssist.mouseButtonForTrigger(left) ?: return false
        return setControllerMouseButton(mouseButton, pressed)
    }

    private fun sendCurrentGamepadState(controllerId: Int = activeControllerId): Boolean {
        val slot = controllerId.coerceIn(0, GAMEPAD_MAX_CONTROLLERS - 1)
        if (!hasAnyControllerState()) {
            disconnectBoosteroidController(slot)
            return true
        }
        if (!ensureBoosteroidController(slot)) return signaling != null
        val serverId = boosteroidControllerIds[slot] ?: return false
        val effectiveButtons =
            physicalSteamOverlayChord.effectiveButtons(physicalButtons) or
                physicalHatButtons or
                virtualSteamOverlayChord.effectiveButtons(virtualButtons) or
                steamMenuChordButtons
        val state = BoosteroidControllerState(
            buttons = effectiveButtons,
            hat = boosteroidHat(effectiveButtons),
            leftTrigger = max(lastLeftTrigger, virtualLeftTrigger),
            rightTrigger = max(lastRightTrigger, virtualRightTrigger),
            leftStickX = effectiveLeftStickX(),
            leftStickY = effectiveLeftStickY(),
            rightStickX = effectiveRightStickX(),
            rightStickY = effectiveRightStickY(),
        )
        val previous = lastBoosteroidControllerStates[slot]
        var sent = false
        BOOSTEROID_BUTTON_MAPPINGS.forEach { (mask, button) ->
            val pressed = state.buttons and mask != 0
            val wasPressed = previous?.buttons?.and(mask) != 0
            if (previous == null && !pressed || previous != null && pressed == wasPressed) return@forEach
            sent = sendBoosteroidInput(buildJsonObject {
                put("type", "controller")
                put("action", "button")
                put("id", serverId)
                put("button", button)
                put("value", if (pressed) 1 else 0)
            }) || sent
        }
        if ((previous == null && state.hat != 0) || (previous != null && state.hat != previous.hat)) {
            sent = sendBoosteroidInput(buildJsonObject {
                put("type", "controller")
                put("action", "pad")
                put("id", serverId)
                put("hat", state.hat)
            }) || sent
        }
        val axes = intArrayOf(
            state.leftStickX,
            -state.leftStickY,
            boosteroidTriggerAxis(state.leftTrigger),
            state.rightStickX,
            -state.rightStickY,
            boosteroidTriggerAxis(state.rightTrigger),
        )
        val previousAxes = previous?.let {
            intArrayOf(
                it.leftStickX,
                -it.leftStickY,
                boosteroidTriggerAxis(it.leftTrigger),
                it.rightStickX,
                -it.rightStickY,
                boosteroidTriggerAxis(it.rightTrigger),
            )
        }
        axes.forEachIndexed { index, value ->
            val oldValue = previousAxes?.get(index) ?: 0
            if (previous != null && value == oldValue) {
                return@forEachIndexed
            }
            if (previous == null && value == 0) return@forEachIndexed
            sent = sendBoosteroidInput(buildJsonObject {
                put("type", "controller")
                put("action", "axes")
                put("id", serverId)
                put("axes", index)
                put("value", value)
            }) || sent
        }
        lastBoosteroidControllerStates[slot] = state
        return sent || previous == state
    }

    private fun handleBoosteroidControllerMessage(message: JsonObject) {
        when (message.string("action")) {
            "connected" -> {
                val name = message.string("name") ?: return
                val serverId = message.int("id") ?: return
                val slot = controllerNamesBySlot.entries.firstOrNull { it.value == name }?.key
                    ?: announcedControllerSlots.singleOrNull()
                    ?: return
                boosteroidControllerIds[slot] = serverId
                lastBoosteroidControllerStates.remove(slot)
                recordBoosteroidInputDiagnostic("Boosteroid controller connected slot=$slot serverId=$serverId name=$name")
                sendCurrentGamepadState(slot)
            }
            "rumble" -> {
                val serverId = message.int("id") ?: return
                val slot = boosteroidControllerIds.entries.firstOrNull { it.value == serverId }?.key ?: 0
                val left = message.double("left")?.roundToInt() ?: 0
                val right = message.double("right")?.roundToInt() ?: 0
                applyGamepadRumble(
                    controllerId = slot,
                    weakMagnitude16 = right,
                    strongMagnitude16 = left,
                )
            }
        }
    }

    private fun ensureBoosteroidController(slot: Int): Boolean {
        if (slot in boosteroidControllerIds) return true
        if (slot !in announcedControllerSlots) {
            val deviceId = controllerSlots.entries.firstOrNull { it.value == slot }?.key
            val deviceName = deviceId?.let(InputDevice::getDevice)?.name?.takeIf(String::isNotBlank)
            val name = "${deviceName ?: "OpenNOW Virtual Controller"}$slot"
            controllerNamesBySlot[slot] = name
            announcedControllerSlots += slot
            sendBoosteroidInput(buildJsonObject {
                put("type", "controller")
                put("action", "connected")
                put("name", name)
            })
        }
        return false
    }

    private fun disconnectBoosteroidController(slot: Int) {
        val serverId = boosteroidControllerIds.remove(slot)
        if (serverId != null) {
            sendBoosteroidInput(buildJsonObject {
                put("type", "controller")
                put("action", "disconnected")
                put("id", serverId)
            })
        }
        announcedControllerSlots.remove(slot)
        controllerNamesBySlot.remove(slot)
        lastBoosteroidControllerStates.remove(slot)
    }

    private fun boosteroidHat(buttons: Int): Int {
        val up = buttons and GamepadButtonMapping.DPAD_UP != 0
        val down = buttons and GamepadButtonMapping.DPAD_DOWN != 0
        val left = buttons and GamepadButtonMapping.DPAD_LEFT != 0
        val right = buttons and GamepadButtonMapping.DPAD_RIGHT != 0
        return when {
            up && left -> 9
            up && right -> 3
            down && left -> 12
            down && right -> 6
            up -> 1
            right -> 2
            down -> 4
            left -> 8
            else -> 0
        }
    }

    private fun boosteroidTriggerAxis(value: Int): Int =
        ((value.coerceIn(0, 255) / 255.0) * 65_534.0 - 32_767.0).roundToInt()

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
            recordBoosteroidInputDiagnostic("physical gamepad guide auto-release slot=$controllerId")
        }
    }

    private fun schedulePhysicalSteamOverlayChordRelease(controllerId: Int) {
        physicalSteamOverlayChordReleaseJob?.cancel()
        physicalSteamOverlayChordReleaseJob = scope.launch {
            delay(GAMEPAD_GUIDE_AUTO_RELEASE_MS)
            if (!physicalSteamOverlayChord.releaseChord()) return@launch
            sendCurrentGamepadState(controllerId = controllerId)
            recordBoosteroidInputDiagnostic("physical View+Start sent Steam Menu Home+A chord slot=$controllerId")
        }
    }

    private fun scheduleVirtualSteamOverlayChordRelease() {
        virtualSteamOverlayChordReleaseJob?.cancel()
        virtualSteamOverlayChordReleaseJob = scope.launch {
            delay(GAMEPAD_GUIDE_AUTO_RELEASE_MS)
            if (!virtualSteamOverlayChord.releaseChord()) return@launch
            sendCurrentGamepadState()
            recordBoosteroidInputDiagnostic("touch View+Start sent Steam Menu Home+A chord")
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
            steamMenuChordButtons != 0 ||
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

    private fun sendBoosteroidRttInput(value: JsonObject): Boolean {
        boosteroidInputEventCount += 1
        val timed = if (boosteroidInputEventCount > 29) {
            boosteroidInputEventCount = 0
            buildJsonObject {
                value.forEach { (key, item) -> put(key, item) }
                put("time", System.currentTimeMillis())
            }
        } else {
            value
        }
        return sendBoosteroidInput(timed)
    }

    private fun sendBoosteroidInput(value: JsonObject): Boolean {
        val type = value.string("type").orEmpty()
        val externalDevice = type == "keyboard" || type == "mouse" || type == "controller" || type == "finger"
        val commandId = if (externalDevice) boosteroidInputCommandId++ else null
        val controlPayload = if (externalDevice) {
            buildJsonObject {
                value.forEach { (key, item) -> put(key, item) }
                put("id_cmd", commandId ?: 0L)
                put("from_udp", false)
            }
        } else {
            value
        }
        val controlSent = signaling?.sendEvent(controlPayload) == true
        val channel = boosteroidInputChannel
        val dataSent = if (externalDevice && channel?.state() == DataChannel.State.OPEN) {
            val dataPayload = buildJsonObject {
                value.forEach { (key, item) -> put(key, item) }
                put("id_cmd", commandId ?: 0L)
                put("from_udp", true)
            }.toString().toByteArray(Charsets.UTF_8)
            channel.send(DataChannel.Buffer(ByteBuffer.wrap(dataPayload), false))
        } else {
            false
        }
        if (!controlSent && !dataSent && !inputDropLogged) {
            inputDropLogged = true
            recordBoosteroidInputDiagnostic("Boosteroid input dropped type=$type control=${signaling != null} data=${channel?.state()}")
        } else if (controlSent || dataSent) {
            inputDropLogged = false
        }
        return controlSent || dataSent
    }

    private fun refreshConnectedPhysicalControllers() {
        val connectedDevices = mutableListOf<InputDevice>()
        InputDevice.getDeviceIds().forEach { deviceId ->
            val device = InputDevice.getDevice(deviceId) ?: return@forEach
            if (AndroidControllerInput.isControllerDevice(device)) {
                connectedDevices += device
            }
        }
        val connectedDeviceIds = connectedDevices.mapTo(mutableSetOf()) { it.id }
        controllerSlots.keys.filter { it !in connectedDeviceIds }.toList().forEach { deviceId ->
            val slot = controllerSlots.remove(deviceId) ?: return@forEach
            disconnectBoosteroidController(slot)
            if (activeControllerId == slot) {
                physicalControllerActive = false
                physicalButtons = 0
                physicalHatButtons = 0
                physicalSteamOverlayChord.reset()
                physicalSteamOverlayChordReleaseJob?.cancel()
                physicalSteamOverlayChordReleaseJob = null
                lastLeftTrigger = 0
                lastRightTrigger = 0
                lastLeftStickX = 0
                lastLeftStickY = 0
                lastRightStickX = 0
                lastRightStickY = 0
            }
            recordBoosteroidInputDiagnostic("Boosteroid controller disconnected slot=$slot device=$deviceId")
        }
        val connected = connectedDevices.isNotEmpty()
        val connectionChanged = connected != physicalControllerConnected
        if (connectionChanged) {
            recordBoosteroidInputDiagnostic(
                "physical gamepad connected=$connected devices=${connectedDevices.joinToString { "${it.id}:${it.name}" }}",
            )
        }
        physicalControllerConnected = connected
        if (connected && activeControllerId !in controllerSlots.values) {
            activeControllerId = controllerSlots.values.firstOrNull() ?: controllerIdFor(connectedDevices.first().id)
        }
        if (connected && !physicalControllerActive && controllerSlots.isEmpty()) {
            activeControllerId = controllerIdFor(connectedDevices.first().id)
        }
        if (!connected && physicalControllerActive) {
            physicalControllerActive = false
            physicalButtons = 0
            physicalHatButtons = 0
            physicalSteamOverlayChord.reset()
            physicalSteamOverlayChordReleaseJob?.cancel()
            physicalSteamOverlayChordReleaseJob = null
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

    private fun schedulePrimeConnectedGamepadState(reason: String) {
        scope.launch {
            if (reason.startsWith("channel open")) {
                val pendingSlots = announcedControllerSlots.filterNot(boosteroidControllerIds::containsKey)
                pendingSlots.forEach { slot ->
                    announcedControllerSlots.remove(slot)
                    controllerNamesBySlot.remove(slot)
                }
                if (pendingSlots.isNotEmpty()) {
                    recordBoosteroidInputDiagnostic(
                        "gamepad handshake retry on data channel open slots=${pendingSlots.joinToString()}",
                    )
                }
            }
            primeConnectedGamepadState(reason)
        }
    }

    private fun primeConnectedGamepadState(reason: String) {
        refreshConnectedPhysicalControllers()
        if (!hasAnyControllerState()) return
        val sent = sendCurrentGamepadState()
        recordBoosteroidInputDiagnostic(
            "gamepad state prime reason=$reason sent=$sent connected=$physicalControllerConnected active=$physicalControllerActive " +
                "slot=$activeControllerId control=${signaling != null} data=${boosteroidInputChannel?.state()}",
        )
    }

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
            logHapticsWarning("input haptics no vibrator controller=$controllerId")
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
            recordBoosteroidInputDiagnostic("gamepad haptics available controller=$slot device=${device.id}:${device.name}")
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
        recordBoosteroidInputDiagnostic(message)
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
            recordBoosteroidInputDiagnostic("gamepad haptics using phone fallback")
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
            recordBoosteroidInputDiagnostic("gamepad haptics using phone fallback")
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
            actionButton != 0 -> actionButton.toBoosteroidMouseButton()
            buttonState != 0 -> buttonState.toBoosteroidMouseButton()
            else -> 0
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

    private fun MotionEvent.rawGamepadAxes(): AndroidGamepadRawAxes =
        AndroidGamepadRawAxes(
            x = getAxisValue(MotionEvent.AXIS_X),
            y = getAxisValue(MotionEvent.AXIS_Y),
            z = getAxisValue(MotionEvent.AXIS_Z),
            rz = getAxisValue(MotionEvent.AXIS_RZ),
            rx = getAxisValue(MotionEvent.AXIS_RX),
            ry = getAxisValue(MotionEvent.AXIS_RY),
            hatX = getAxisValue(MotionEvent.AXIS_HAT_X),
            hatY = getAxisValue(MotionEvent.AXIS_HAT_Y),
        )

    private fun MotionEvent.axisAvailability(): AndroidGamepadAxisAvailability =
        AndroidGamepadAxisAvailability(
            x = hasMotionAxis(MotionEvent.AXIS_X),
            y = hasMotionAxis(MotionEvent.AXIS_Y),
            z = hasMotionAxis(MotionEvent.AXIS_Z),
            rz = hasMotionAxis(MotionEvent.AXIS_RZ),
            rx = hasMotionAxis(MotionEvent.AXIS_RX),
            ry = hasMotionAxis(MotionEvent.AXIS_RY),
            hatX = hasMotionAxis(MotionEvent.AXIS_HAT_X),
            hatY = hasMotionAxis(MotionEvent.AXIS_HAT_Y),
        )

    private fun MotionEvent.hasMotionAxis(axis: Int): Boolean {
        val inputDevice = device ?: return false
        return inputDevice.getMotionRange(axis, source) != null ||
            inputDevice.getMotionRange(axis) != null
    }

    private fun KeyEvent.isGamepadEvent(): Boolean {
        val controllerInputDevice = isControllerInputDevice()
        return (controllerInputDevice &&
            (GamepadButtonMapping.maskForKeyCode(keyCode, controllerActivation = true) != null ||
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

    private fun Int.toBoosteroidMouseButton(): Int = when {
        this and MotionEvent.BUTTON_PRIMARY != 0 -> 0
        this and MotionEvent.BUTTON_TERTIARY != 0 -> 1
        this and MotionEvent.BUTTON_SECONDARY != 0 -> 2
        this and MotionEvent.BUTTON_BACK != 0 -> 3
        this and MotionEvent.BUTTON_FORWARD != 0 -> 4
        else -> 0
    }

    private companion object {
        private const val EXTERNAL_MOUSE_ABSOLUTE_DELTA_LIMIT_PX = 240f
        private const val GAMEPAD_MAX_CONTROLLERS = 4
        private const val BOOSTEROID_DATA_CHANNEL_LABEL = "ClientDataChannel"
        private const val BOOSTEROID_REMOTE_ICE_POLL_MS = 500L
        private const val BOOSTEROID_CURSOR_MISS_COOLDOWN_MS = 5000L
        private const val RUMBLE_EFFECT_MS = 400L
        private const val RUMBLE_THROTTLE_MS = 35L
        private const val HAPTICS_LOG_INTERVAL_MS = 5000L
        private val BOOSTEROID_BUTTON_MAPPINGS = listOf(
            GamepadButtonMapping.A to 0,
            GamepadButtonMapping.B to 1,
            GamepadButtonMapping.X to 2,
            GamepadButtonMapping.Y to 3,
            GamepadButtonMapping.LEFT_SHOULDER to 4,
            GamepadButtonMapping.RIGHT_SHOULDER to 5,
            GamepadButtonMapping.BACK to 6,
            GamepadButtonMapping.START to 7,
            GamepadButtonMapping.LEFT_THUMB to 8,
            GamepadButtonMapping.RIGHT_THUMB to 9,
            GamepadButtonMapping.GUIDE to 16,
        )
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
    private fun Float.formatAxis(): String = String.format(Locale.US, "%.3f", this)
}

private const val ICE_DISCONNECTED_GRACE_MS = 3500L
private const val ICE_FAILED_RECONNECT_DELAY_MS = 250L
private const val SIGNALING_RECONNECT_DELAY_MS = 1000L
private const val MAX_TRANSPORT_RECONNECT_ATTEMPTS = 3
private const val OFFER_TIMEOUT_MS = 30_000L
private const val CONNECTION_TIMEOUT_MS = 45_000L
private const val GAMEPAD_MAX_CONTROLLERS = 4
private const val GAMEPAD_GUIDE_AUTO_RELEASE_MS = 160L
private const val STEAM_MENU_MODIFIER_DELAY_MS = 40L
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

private fun recordBoosteroidInputDiagnostic(message: String) {
    if (BuildConfig.DEBUG) android.util.Log.d("OpenNOW Boosteroid", message.substringBefore('?').take(240))
}

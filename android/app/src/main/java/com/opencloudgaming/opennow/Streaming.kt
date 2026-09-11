package com.opencloudgaming.opennow

import android.Manifest
import android.app.ActivityManager
import android.content.Context
import android.content.pm.PackageManager
import android.content.res.Configuration
import android.media.AudioAttributes
import android.media.MediaCodecInfo
import android.media.MediaCodecList
import android.media.MediaRecorder
import android.opengl.GLES11Ext
import android.opengl.GLES20
import android.os.Build
import android.os.CombinedVibration
import android.os.SystemClock
import android.os.VibrationEffect
import android.os.Vibrator
import android.os.VibratorManager
import android.view.InputDevice
import android.view.KeyEvent
import android.view.MotionEvent
import android.view.SurfaceHolder
import android.view.View
import androidx.annotation.RequiresApi
import androidx.core.content.ContextCompat
import kotlinx.coroutines.CoroutineScope
import kotlinx.coroutines.Dispatchers
import kotlinx.coroutines.Job
import kotlinx.coroutines.SupervisorJob
import kotlinx.coroutines.asCoroutineDispatcher
import kotlinx.coroutines.cancel
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
import okhttp3.ConnectionSpec
import okhttp3.OkHttpClient
import okhttp3.Request
import okhttp3.Response
import okhttp3.TlsVersion
import okhttp3.WebSocket
import okhttp3.WebSocketListener
import okio.ByteString
import org.webrtc.AudioSource
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
import org.webrtc.RtpSender
import org.webrtc.RtpTransceiver
import org.webrtc.SdpObserver
import org.webrtc.SessionDescription
import org.webrtc.VideoCodecInfo
import org.webrtc.VideoDecoder
import org.webrtc.VideoDecoderFactory
import org.webrtc.VideoTrack
import org.webrtc.audio.AudioDeviceModule
import org.webrtc.audio.JavaAudioDeviceModule
import java.net.InetAddress
import java.nio.ByteBuffer
import java.nio.ByteOrder
import java.nio.FloatBuffer
import java.security.SecureRandom
import java.util.Locale
import java.util.concurrent.TimeUnit
import java.util.concurrent.atomic.AtomicBoolean
import java.util.concurrent.atomic.AtomicInteger
import kotlin.math.abs
import kotlin.math.max
import kotlin.math.min
import kotlin.math.roundToInt
import kotlin.math.sqrt

/** Prefer Android's explicit relative axes whenever either axis carries a captured-mouse delta. */
internal fun shouldUseAndroidRelativeMouseAxes(relativeDx: Float, relativeDy: Float): Boolean =
    relativeDx != 0f || relativeDy != 0f

/**
 * Android pointer capture defines X/Y as relative movement. Some device builds also expose the
 * explicit RELATIVE_X/Y axes. Keep the explicit axis when both representations agree, but trust the
 * pointer-capture contract when an OEM reports the two with opposing signs.
 */
internal fun resolveAndroidCapturedMouseAxis(relativeAxis: Float, capturedAxis: Float): Float = when {
    !relativeAxis.isFinite() -> capturedAxis.takeIf(Float::isFinite) ?: 0f
    !capturedAxis.isFinite() -> relativeAxis
    relativeAxis == 0f -> capturedAxis
    capturedAxis == 0f -> relativeAxis
    (relativeAxis > 0f) == (capturedAxis > 0f) -> relativeAxis
    else -> capturedAxis
}

internal fun androidCapturedMouseAxesConflict(
    relativeDx: Float,
    relativeDy: Float,
    capturedDx: Float,
    capturedDy: Float,
): Boolean =
    (
        relativeDx.isFinite() && capturedDx.isFinite() &&
            relativeDx != 0f && capturedDx != 0f && (relativeDx > 0f) != (capturedDx > 0f)
    ) || (
        relativeDy.isFinite() && capturedDy.isFinite() &&
            relativeDy != 0f && capturedDy != 0f && (relativeDy > 0f) != (capturedDy > 0f)
    )

/**
 * Captured pointers and explicit relative axes must stay relative on the host. Converting those
 * deltas to a bounded absolute cursor prevents games from rotating the camera past a stream edge.
 */
internal fun shouldSendExternalMouseAsRelative(
    capturedPointer: Boolean,
    hasRelativeAxisMotion: Boolean,
): Boolean = capturedPointer || hasRelativeAxisMotion

internal fun shouldUseFixedSizeStreamSurface(
    videoWidth: Int,
    videoHeight: Int,
    rotation: Int,
    viewWidth: Int,
    viewHeight: Int,
): Boolean {
    if (videoWidth <= 0 || videoHeight <= 0 || viewWidth <= 0 || viewHeight <= 0) return false
    val quarterTurns = ((rotation % 360) + 360) % 360
    val rotatedWidth = if (quarterTurns == 90 || quarterTurns == 270) videoHeight else videoWidth
    val rotatedHeight = if (quarterTurns == 90 || quarterTurns == 270) videoWidth else videoHeight
    val frameLongEdge = max(rotatedWidth, rotatedHeight)
    val frameShortEdge = min(rotatedWidth, rotatedHeight)
    val viewLongEdge = max(viewWidth, viewHeight)
    val viewShortEdge = min(viewWidth, viewHeight)

    // A fixed-size Surface is useful when SurfaceFlinger can upscale a smaller decoded buffer.
    // At or above either viewport edge, some OEM compositors crop the fixed buffer instead of
    // fitting it. Let the EGL renderer draw into the layout-sized Surface for those modes.
    return frameLongEdge < viewLongEdge && frameShortEdge < viewShortEdge
}

internal fun shouldSuppressHardwareKeyboardRepeat(
    hardwareKeyboard: Boolean,
    action: Int,
    repeatCount: Int,
): Boolean =
    hardwareKeyboard && action == KeyEvent.ACTION_DOWN && repeatCount > 0

internal fun isCurrentPeerOperation(
    operationGeneration: Int,
    currentGeneration: Int,
    expectedPeer: Any?,
    activePeer: Any?,
): Boolean =
    activePeer != null &&
        operationGeneration == currentGeneration &&
        (expectedPeer == null || expectedPeer === activePeer)

internal fun normalizedLiveBitrateKbps(maxBitrateKbps: Int): Int =
    maxBitrateKbps.coerceAtLeast(1_000).let { value ->
        ((value + 500) / 1_000) * 1_000
    }

internal enum class HapticsOutputTarget {
    Controller,
    Device,
    None,
}

/**
 * A forced [HapticsOutputPreference] is a hard selection, not a hint: picking Controller or Device
 * and then silently falling back to the other would defeat the point of the setting, which exists
 * precisely because one of the two is lying about its capability.
 */
internal fun selectHapticsOutputTarget(
    vibrationEnabled: Boolean,
    controllerRumbleAvailable: Boolean,
    deviceHapticsAvailable: Boolean,
    preference: HapticsOutputPreference = HapticsOutputPreference.Auto,
): HapticsOutputTarget = when {
    !vibrationEnabled -> HapticsOutputTarget.None
    preference == HapticsOutputPreference.Controller ->
        if (controllerRumbleAvailable) HapticsOutputTarget.Controller else HapticsOutputTarget.None
    preference == HapticsOutputPreference.Device ->
        if (deviceHapticsAvailable) HapticsOutputTarget.Device else HapticsOutputTarget.None
    controllerRumbleAvailable -> HapticsOutputTarget.Controller
    deviceHapticsAvailable -> HapticsOutputTarget.Device
    else -> HapticsOutputTarget.None
}

/**
 * GFN does not return ordinary game rumble for its native PlayStation controller identity. An
 * explicitly forced Controller output therefore opts the pad into the XInput-compatible identity
 * that carries the host's two-motor rumble stream. Auto deliberately preserves PlayStation button
 * prompts; this compatibility tradeoff must never happen silently merely because vibration is on.
 */
internal fun usesPlayStationRumbleCompatibility(
    vibrationEnabled: Boolean,
    preference: HapticsOutputPreference,
): Boolean = vibrationEnabled && preference == HapticsOutputPreference.Controller

class NativeStreamClient(
    context: Context,
    private val onState: (String) -> Unit,
    private val onError: (String) -> Unit,
    private val onSessionRecoveryRequired: (String) -> Unit = {},
    private val onFirstVideoFrameRendered: () -> Unit = {},
    private val onStats: (StreamRuntimeStats) -> Unit = {},
    private val onControllerMouseAssistChanged: (Boolean) -> Unit = {},
) {
    private val appContext = context.applicationContext
    private val initialAndroidTvProfile = isAndroidTvProfile(appContext)
    private val eglBase: EglBase = EglBase.create()
    private val scope = CoroutineScope(SupervisorJob() + Dispatchers.Main.immediate)
    private val inputExecutor = java.util.concurrent.Executors.newSingleThreadExecutor { runnable ->
        Thread(runnable, "opennow-input-sender").apply {
            priority = Thread.MAX_PRIORITY
        }
    }
    private val nativeLifecycleExecutor = java.util.concurrent.Executors.newSingleThreadExecutor { runnable ->
        Thread(runnable, "opennow-native-lifecycle").apply {
            priority = Thread.NORM_PRIORITY
        }
    }
    private val inputScope = CoroutineScope(SupervisorJob() + inputExecutor.asCoroutineDispatcher())
    private val inputEncoder = InputEncoder()
    private val audioDeviceModule: AudioDeviceModule =
        JavaAudioDeviceModule.builder(appContext)
            .setUseLowLatency(shouldUseLowLatencyStreamAudio(initialAndroidTvProfile))
            .setAudioSource(MediaRecorder.AudioSource.VOICE_COMMUNICATION)
            .setUseStereoInput(false)
            .setUseStereoOutput(true)
            .setUseHardwareAcousticEchoCanceler(true)
            .setUseHardwareNoiseSuppressor(true)
            .setAudioRecordErrorCallback(
                object : JavaAudioDeviceModule.AudioRecordErrorCallback {
                    override fun onWebRtcAudioRecordInitError(errorMessage: String?) {
                        recordStreamDiagnostic("microphone capture init failed error=${errorMessage.orEmpty()}")
                    }

                    override fun onWebRtcAudioRecordStartError(
                        errorCode: JavaAudioDeviceModule.AudioRecordStartErrorCode?,
                        errorMessage: String?,
                    ) {
                        recordStreamDiagnostic(
                            "microphone capture start failed code=${errorCode?.name.orEmpty()} error=${errorMessage.orEmpty()}",
                        )
                    }

                    override fun onWebRtcAudioRecordError(errorMessage: String?) {
                        recordStreamDiagnostic("microphone capture runtime failed error=${errorMessage.orEmpty()}")
                    }
                },
            )
            .setAudioRecordStateCallback(
                object : JavaAudioDeviceModule.AudioRecordStateCallback {
                    override fun onWebRtcAudioRecordStart() {
                        recordStreamDiagnostic("microphone capture started")
                    }

                    override fun onWebRtcAudioRecordStop() {
                        recordStreamDiagnostic("microphone capture stopped")
                    }
                },
            )
            .setAudioAttributes(
                AudioAttributes.Builder()
                    .setUsage(AudioAttributes.USAGE_GAME)
                    .setContentType(AudioAttributes.CONTENT_TYPE_MUSIC)
                    .build(),
            )
            .createAudioDeviceModule()
    private var factory: PeerConnectionFactory? = null
    /**
     * Owned by [nativeLifecycleExecutor]. Volatile reads are used only for cheap readiness checks;
     * every JNI call and the final close/dispose run on that same executor so a native handle
     * cannot be freed while another thread is entering libwebrtc.
     */
    @Volatile
    private var peerConnection: PeerConnection? = null
    private val remoteIceCandidates = RemoteIceCandidateBuffer()
    private var signaling: GfnSignalingClient? = null
    @Volatile private var nvstTransport: NvstTransport? = null
    private var lastNvstError: String? = null
    @Volatile private var reliableInput: DataChannel? = null
    @Volatile
    private var partiallyReliableInput: DataChannel? = null
    @Volatile
    private var reliableInputState: DataChannel.State? = null
    @Volatile
    private var partiallyReliableInputState: DataChannel.State? = null
    private val pendingInputSends = AtomicInteger(0)
    private val synchronousInputFallback = AtomicBoolean(false)
    private val workerInputSendConfirmed = AtomicBoolean(false)
    private val directInputSendConfirmed = AtomicBoolean(false)
    private var statsChannel: DataChannel? = null
    private var lastParsedGameFps: Int? = null
    // Informational only. Gamepad snapshots stay ordered and reliable because a late, older
    // snapshot on the loss-tolerant channel can undo a newer button or stick state.
    private var partiallyReliableGamepadMask = 0
    private var hidDeviceMask = 0
    private var partiallyReliableHidMask = 0
    @Volatile
    private var inputHandshakeReady = false
    private var hapticsAdvertised: Boolean? = null
    private var lastHapticsAdvertisementAtMs = 0L
    private var videoTrack: VideoTrack? = null
    private var audioTrack: AudioTrack? = null
    private var microphoneSource: AudioSource? = null
    private var microphoneTrack: AudioTrack? = null
    private var microphoneSender: RtpSender? = null
    @Volatile private var renderer: StreamVideoSurface? = null
    private var rendererSharpnessDrawer: StreamSharpnessGlDrawer? = null
    private var rendererSurfaceCallback: SurfaceHolder.Callback? = null
    private val rendererSinkLifecycle = RendererSinkLifecycle()
    private var heartbeatJob: Job? = null
    private var gamepadKeepaliveJob: Job? = null
    private var statsJob: Job? = null
    private var iceRecoveryJob: Job? = null
    private var offerTimeoutJob: Job? = null
    private var bitrateUpdateJob: Job? = null
    internal var settings: StreamSettings = StreamSettings()
    private var session: SessionInfo? = null
    @Volatile
    private var transportGeneration = 0
    private var reconnectAttempts = 0
    private var transientSignalingFailures = 0
    /**
     * Bitrate ceiling (kbps) requested for the live transport, exposed for the overlay indicator.
     * A change is queued for the next legitimate offer. Replacing a healthy signaling/ICE
     * transport just to change bitrate can strand the allocated cloud session on a stale endpoint.
     */
    @Volatile
    var liveBitrateLimitKbps: Int? = null
    private var selectedProfileRetryApplied = false
    private var stableMediaStallRestarts = 0
    private var transportHasStableMedia = false
    private var consecutiveTransportProgressSamples = 0
    private var sessionRecoveryRequested = false
    private var lastIceState: PeerConnection.IceConnectionState? = null
    private var audioMuted = false
    private var microphoneMuted = false
    private var virtualButtons = 0
    private val virtualButtonPressSources = mutableMapOf<Int, MutableSet<String>>()
    private var virtualLeftTrigger = 0
    private var virtualRightTrigger = 0
    private val virtualLeftTriggerPressSources = mutableSetOf<String>()
    private val virtualRightTriggerPressSources = mutableSetOf<String>()
    private var virtualLeftStickActive = false
    private var virtualLeftStickX = 0
    private var virtualLeftStickY = 0
    private var virtualRightStickActive = false
    private var virtualRightStickX = 0
    private var virtualRightStickY = 0
    private var virtualControllerVisible = false
    @Volatile
    private var touchMouseEnabled = false
    private var physicalControllerConnected = false
    private var physicalControllerActive = false
    private var activeControllerId = 0
    private val controllerSlots = linkedMapOf<Int, Int>()
    private val controllerFamiliesBySlot = mutableMapOf<Int, AndroidControllerFamily>()
    private val controllerAxisAvailability = mutableMapOf<Int, AndroidGamepadAxisAvailability>()
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
    private var controllerMouseAutoArmOnStart = false
    private var controllerMouseAssistActive = false
    private var controllerMouseAssistAutoArmed = false
    private var controllerMouseEmulationActive = false
    private var controllerMouseMoveLogged = false
    private var controllerMouseLeftButtonDown = false
    private var controllerMouseRightButtonDown = false
    private var mouseLastDeviceId = Int.MIN_VALUE
    private var mouseLastSource = 0
    private var mouseLastX = 0f
    private var mouseLastY = 0f
    private var mousePositionValid = false
    private var mouseSuppressNextAbsoluteDelta = false
    private val mouseMoveBurstLock = Any()
    private val mouseMoveBurstLimiter = MouseMoveBurstLimiter(MOUSE_MOVE_MIN_SEND_INTERVAL_MS)
    private var mouseMoveBurstFlushJob: Job? = null
    private val externalMouseMoveBurstLock = Any()
    private val externalMouseMoveBurstLimiter = MouseMoveBurstLimiter(MOUSE_MOVE_MIN_SEND_INTERVAL_MS)
    private var externalMouseMoveBurstFlushJob: Job? = null
    private val gamepadStateBurstLock = Any()
    private val gamepadStateBurstLimiter = GamepadStateBurstLimiter(GAMEPAD_STATE_MIN_SEND_INTERVAL_MS)
    private var gamepadStateBurstFlushJob: Job? = null
    private val externalMouseMotionAccumulator = MouseMotionAccumulator(minimumSendIntervalMs = 0L)
    private val externalMouseAbsolutePosition = ExternalMouseAbsolutePosition()
    private val gyroscopeMouseMotionAccumulator = MouseMotionAccumulator()
    private val forwardedPhysicalInput = ForwardedPhysicalInputState()
    private var externalMouseMotionDeviceId = Int.MIN_VALUE
    private var externalMouseMotionSource = 0
    private var inputDropLogged = false
    private var externalMouseEventLogged = false
    private var externalMouseMoveSentLogged = false
    private var externalMouseCapturedMoveSentLogged = false
    private var externalMouseAxisConflictLogged = false
    private var externalMouseAbsoluteJumpLogged = false
    private var hardwareKeyboardEventLogged = false
    private var physicalGamepadAxisLogged = false
    private var lastStatsSample: StreamStatsSample? = null
    private val processCpuSampler = ProcessCpuSampler()
    private val packetLossWindow = StreamPacketLossWindow()
    private val packetLossRecoveryGate = StreamPacketLossRecoveryGate()
    private val decoderRecoveryGate = StreamDecoderRecoveryGate()
    private var androidTvProfile = initialAndroidTvProfile
    private var livenessWatchdog = newStreamLivenessWatchdog(androidTvProfile)
    private var firstVideoFrameWatchdog = FirstVideoFrameWatchdog(
        timeoutMs = firstVideoFrameRecoveryTimeoutMs(androidTvProfile),
    )
    private val textSendMutex = Mutex()
    private var guideAutoReleaseJob: Job? = null
    private var steamMenuChordJob: Job? = null
    private var physicalSteamOverlayChordReleaseJob: Job? = null
    private var virtualSteamOverlayChordReleaseJob: Job? = null
    private val lastRumbleEffectAtMs = LongArray(GAMEPAD_MAX_CONTROLLERS)
    private val hapticsSupportLogged = BooleanArray(GAMEPAD_MAX_CONTROLLERS)
    private var lastHapticsWarningAtMs = 0L
    private var vibrationEnabled = true
    private var hapticsOutputPreference = HapticsOutputPreference.Auto
    private var deviceHapticsSupportLogged = false
    private var released = false
    private var controllerMouseLoopJob: Job? = null
    private var physicalLeftStickX = 0f
    private var physicalLeftStickY = 0f
    private var physicalRightStickX = 0f
    private var physicalRightStickY = 0f
    private var controllerScrollAccumulator = 0f

    private data class RumbleEffectProfile(
        val weakAmplitude: Int,
        val strongAmplitude: Int,
        val combinedAmplitude: Int,
    ) {
        val isStop: Boolean
            get() = weakAmplitude <= 0 && strongAmplitude <= 0 && combinedAmplitude <= 0
    }

    private data class StreamStatsSample(
        val inboundRtpId: String,
        val atMs: Double,
        val bytesReceived: Long,
        val framesReceived: Long,
        val framesDecoded: Long,
        val totalDecodeTime: Double,
        val packetsLost: Long,
        val packetsReceived: Long,
    )

    private data class RuntimeStatsSnapshot(
        val stats: StreamRuntimeStats,
        val bytesReceived: Long?,
        val framesDecoded: Long?,
    )

    private data class MicrophoneResources(
        val sender: RtpSender?,
        val track: AudioTrack?,
        val source: AudioSource?,
    )

    private fun recordStreamDiagnostic(message: String) {
        NativeInputDiagnostics.add("stream $message")
    }

    private fun enqueueNativeLifecycleOperation(label: String, command: () -> Unit) {
        runCatching {
            nativeLifecycleExecutor.execute {
                runCatching(command).onFailure { error ->
                    recordStreamDiagnostic("native lifecycle failed step=$label error=${error.message.orEmpty()}")
                }
            }
        }.onFailure { error ->
            recordStreamDiagnostic("native lifecycle rejected step=$label error=${error.message.orEmpty()}")
        }
    }

    /** Must be called from [nativeLifecycleExecutor] before entering a PeerConnection JNI method. */
    private fun activePeerConnection(generation: Int, expected: PeerConnection? = null): PeerConnection? {
        val active = peerConnection
        return active.takeIf {
            isCurrentPeerOperation(
                operationGeneration = generation,
                currentGeneration = transportGeneration,
                expectedPeer = expected,
                activePeer = active,
            )
        }
    }

    private fun dispatchPeerFailure(
        diagnostic: String,
        message: String,
        generation: Int,
        expected: PeerConnection,
    ) {
        scope.launch {
            if (!isCurrentPeerOperation(generation, transportGeneration, expected, peerConnection)) return@launch
            recordStreamDiagnostic(diagnostic)
            failStream(message, generation)
        }
    }

    init {
        WebRtcRuntime.ensureInitialized(appContext)
        val lowLatencyEnabled = SettingsStore(appContext).settings.value.nativeLowLatencyDecoder
        factory = PeerConnectionFactory.builder()
            .setOptions(PeerConnectionFactory.Options())
            .setAudioDeviceModule(audioDeviceModule)
            .setVideoDecoderFactory(
                OpenNowVideoDecoderFactory(
                    sharedContext = eglBase.eglBaseContext,
                    nativeLowLatencyDecoderEnabled = lowLatencyEnabled,
                    requestedFps = { settings.fps },
                    hdrEnabled = { settings.hdrEnabled },
                    hdrSurface = { renderer?.hdrTarget },
                ),
            )
            .setVideoEncoderFactory(DefaultVideoEncoderFactory(eglBase.eglBaseContext, true, true))
            .createPeerConnectionFactory()
    }

    fun createRenderer(context: Context, settings: StreamSettings): StreamVideoSurface =
        StreamVideoSurface(context, settings.hdrEnabled).also { rendererView ->
            renderer?.let { oldRenderer ->
                releaseRendererInternal(oldRenderer)
            }
            firstVideoFrameWatchdog.reset()
            val requestedResolution = streamResolutionPixels(settings)
            val displayMetrics = context.resources.displayMetrics
            var fixedSizeSurface = shouldUseFixedSizeStreamSurface(
                videoWidth = requestedResolution.first,
                videoHeight = requestedResolution.second,
                rotation = 0,
                viewWidth = displayMetrics.widthPixels,
                viewHeight = displayMetrics.heightPixels,
            )
            val decodedResolutionTracker = DecodedResolutionTracker()
            val rendererEvents = object : RendererCommon.RendererEvents {
                override fun onFirstFrameRendered() {
                    firstVideoFrameWatchdog.markRendered()
                    NativeInputDiagnostics.add("video renderer first frame codec=${this@NativeStreamClient.settings.codec}")
                    scope.launch { onFirstVideoFrameRendered() }
                }

                override fun onFrameResolutionChanged(videoWidth: Int, videoHeight: Int, rotation: Int) {
                    val transition = decodedResolutionTracker.observe(videoWidth, videoHeight)
                    val resolutionMessage = when {
                        transition == null ->
                            "video renderer resolution=${videoWidth}x$videoHeight rotation=$rotation"
                        transition.isInitial ->
                            "video renderer initial resolution=${videoWidth}x$videoHeight rotation=$rotation"
                        else ->
                            "video renderer resolution changed=${transition.previousWidth}x${transition.previousHeight}" +
                                "->${videoWidth}x$videoHeight rotation=$rotation keeping transport"
                    }
                    NativeInputDiagnostics.add(resolutionMessage)
                    if (videoWidth > 0 && videoHeight > 0) {
                        NativeStreamInputRouter.setDecodedStreamResolution(videoWidth, videoHeight)
                    }
                    if (transition?.isInitial == false) {
                        scope.launch {
                            if (renderer !== rendererView) return@launch
                            // Uplay/provider mode switches can briefly pause decoded-frame stats.
                            // Accept the decoder's new size and give the existing transport a fresh
                            // liveness window instead of interpreting the change as a stream crash.
                            livenessWatchdog.markConnected(SystemClock.elapsedRealtime())
                            packetLossRecoveryGate.reset()
                            firstVideoFrameWatchdog.markRendered()
                            recordStreamDiagnostic(
                                "decoded resolution change accepted from=${transition.previousWidth}x${transition.previousHeight} " +
                                    "to=${transition.width}x${transition.height}; cloud session and transport preserved",
                            )
                        }
                    }
                    rendererView.post {
                        if (
                            renderer !== rendererView ||
                            rendererView.width <= 0 ||
                            rendererView.height <= 0
                        ) {
                            return@post
                        }
                        val nextFixedSizeSurface = shouldUseFixedSizeStreamSurface(
                            videoWidth = videoWidth,
                            videoHeight = videoHeight,
                            rotation = rotation,
                            viewWidth = rendererView.width,
                            viewHeight = rendererView.height,
                        )
                        if (nextFixedSizeSurface != fixedSizeSurface) {
                            fixedSizeSurface = nextFixedSizeSurface
                            rendererView.setEnableHardwareScaler(nextFixedSizeSurface)
                            NativeInputDiagnostics.add(
                                "video renderer surface fixed=$nextFixedSizeSurface " +
                                    "decoded=${videoWidth}x$videoHeight rotation=$rotation " +
                                    "view=${rendererView.width}x${rendererView.height}",
                            )
                        }
                    }
                }
            }
            // Always attach the sharpness drawer so mid-session toggles take effect live: its
            // fragment shader passes pixels through untouched while the amount is 0, and
            // updateRendererSettings() adjusts the amount on the fly. Attaching it conditionally
            // here used to make the overlay toggle dead whenever the session started with
            // sharpening off (the drawer was never created to receive the new amount).
            val sharpnessDrawer = StreamSharpnessGlDrawer().also { drawer ->
                drawer.amount = streamSharpnessShaderStrength(
                    settings.streamSharpeningEnabled,
                    settings.streamSharpeningAmount,
                )
            }
            rendererSharpnessDrawer = if (settings.hdrEnabled) null else sharpnessDrawer
            rendererView.init(eglBase.eglBaseContext, rendererEvents, EglBase.CONFIG_PLAIN, sharpnessDrawer)
            rendererView.setEnableHardwareScaler(fixedSizeSurface)
            NativeInputDiagnostics.add(
                "video renderer surface fixed=$fixedSizeSurface requested=${settings.resolution} " +
                    "display=${displayMetrics.widthPixels}x${displayMetrics.heightPixels}",
            )
            rendererView.setMirror(false)
            // Do not give StreamVideoSurface an opaque View background. Its decoded
            // frames are presented by a separate Surface layer, so a normal View
            // background can cover every rendered frame on physical devices. The
            // Compose stream container already supplies the black pre-frame backdrop.
            rendererView.setStreamScaling()
            renderer = rendererView
            rendererSurfaceCallback = object : SurfaceHolder.Callback {
                override fun surfaceCreated(holder: SurfaceHolder) {
                    attachRendererSinkIfAvailable(rendererView)
                }

                override fun surfaceChanged(holder: SurfaceHolder, format: Int, width: Int, height: Int) {
                    NativeInputDiagnostics.add(
                        "video renderer surface changed=${width}x$height " +
                            "view=${rendererView.width}x${rendererView.height} fixed=$fixedSizeSurface",
                    )
                    attachRendererSinkIfAvailable(rendererView)
                }

                override fun surfaceDestroyed(holder: SurfaceHolder) {
                    detachRendererSink(rendererView)
                }
            }.also(rendererView.holder::addCallback)
            attachRendererSinkIfAvailable(rendererView)
        }

    fun releaseRenderer(candidate: StreamVideoSurface) {
        if (renderer !== candidate) return
        releaseRendererInternal(candidate)
        renderer = null
        rendererSharpnessDrawer = null
    }

    private fun attachRendererSinkIfAvailable(candidate: StreamVideoSurface) {
        if (renderer !== candidate || candidate.holder.surface?.isValid != true) return
        val track = videoTrack ?: return
        val generation = transportGeneration
        enqueueNativeLifecycleOperation("renderer-sink-attach") {
            if (
                activePeerConnection(generation) == null ||
                videoTrack !== track ||
                renderer !== candidate
            ) {
                return@enqueueNativeLifecycleOperation
            }
            if (!rendererSinkLifecycle.requestAttach()) return@enqueueNativeLifecycleOperation
            firstVideoFrameWatchdog.reset()
            track.addSink(candidate)
            recordStreamDiagnostic("video renderer sink attached")
        }
    }

    private fun detachRendererSink(candidate: StreamVideoSurface) {
        if (renderer !== candidate || !rendererSinkLifecycle.requestDetach()) return
        val attachedTrack = videoTrack
        val generation = transportGeneration
        val surfaceValid = candidate.holder.surface?.isValid == true
        enqueueNativeLifecycleOperation("renderer-sink-detach") {
            if (activePeerConnection(generation) == null) return@enqueueNativeLifecycleOperation
            attachedTrack?.removeSink(candidate)
            recordStreamDiagnostic("video renderer sink detached surface=$surfaceValid")
        }
    }

    private fun releaseRendererInternal(candidate: StreamVideoSurface) {
        prepareRendererForRelease(candidate)
        enqueueNativeLifecycleOperation("renderer-release") {
            candidate.release()
        }
    }

    private fun prepareRendererForRelease(candidate: StreamVideoSurface) {
        if (renderer === candidate) {
            detachRendererSink(candidate)
        }
        rendererSurfaceCallback?.let(candidate.holder::removeCallback)
        rendererSurfaceCallback = null
        candidate.hideSurfaceBeforeRelease()
    }

    private fun StreamVideoSurface.hideSurfaceBeforeRelease() {
        // SurfaceView frames are composited in a separate native layer. Hide that
        // layer before tearing down WebRTC so a stale/pre-frame buffer cannot remain
        // above the next Compose screen while SurfaceFlinger processes the detach.
        alpha = 0f
        visibility = View.GONE
    }

    fun updateRendererSettings(settings: StreamSettings) {
        val updatedSettings = this.settings.copy(
            mouseSensitivity = settings.mouseSensitivity,
            mouseAcceleration = settings.mouseAcceleration,
            streamSharpeningEnabled = settings.streamSharpeningEnabled,
            streamSharpeningAmount = settings.streamSharpeningAmount,
            mouseScrollSensitivity = settings.mouseScrollSensitivity,
        )
        if (updatedSettings == this.settings) return
        this.settings = updatedSettings
        rendererSharpnessDrawer?.amount = streamSharpnessShaderStrength(settings.streamSharpeningEnabled, settings.streamSharpeningAmount)
    }

    private fun StreamVideoSurface.setStreamScaling() {
        // Keep the complete decoded frame inside the SurfaceView. Phone edge-to-edge
        // presentation is applied by scaling the View itself, never by cropping video.
        setScalingType(RendererCommon.ScalingType.SCALE_ASPECT_FIT)
    }

    fun updateHapticsSettings(enabled: Boolean, preference: HapticsOutputPreference) {
        if (vibrationEnabled == enabled && hapticsOutputPreference == preference) return
        val outputChanged = hapticsOutputPreference != preference
        vibrationEnabled = enabled
        hapticsOutputPreference = preference
        // Switching output mid-rumble has to silence the old one: the stop that eventually arrives
        // from the host is routed to the *new* target and would leave the old motor running.
        if (!enabled || outputChanged) {
            stopAllGamepadRumble()
        }
        if (outputChanged && hasAnyControllerState()) {
            // The compatibility bit is carried in every gamepad state packet, not in the separate
            // haptics advertisement. Publish the new identity immediately when the setting changes.
            sendCurrentGamepadState()
        }
        updateHapticsAdvertisement(force = true)
    }

    /**
     * Applies every mid-session-adjustable setting in one call so overlay and settings-screen
     * changes reach the renderer, the haptics advertisement, and the input router together. All
     * three setters are idempotent (guarded on value change), so this is safe to invoke from any
     * LaunchedEffect keyed on the relevant fields — including on every AndroidView update.
     */
    fun applyLiveSettings(
        rendererSettings: StreamSettings,
        vibrationEnabled: Boolean,
        hapticsOutput: HapticsOutputPreference,
        stretchToFit: Boolean,
    ) {
        updateRendererSettings(rendererSettings)
        updateHapticsSettings(vibrationEnabled, hapticsOutput)
        NativeStreamInputRouter.setStretchToFit(stretchToFit)
    }

    fun updateControllerMouseAssistAutoArm(enabled: Boolean) {
        controllerMouseAutoArmOnStart = enabled
        if (!enabled) {
            setControllerMouseAssistActive(false)
        }
    }

    fun updateAndroidTvProfile(enabled: Boolean) {
        if (androidTvProfile == enabled) return
        androidTvProfile = enabled
        livenessWatchdog = newStreamLivenessWatchdog(enabled)
        firstVideoFrameWatchdog = FirstVideoFrameWatchdog(
            timeoutMs = firstVideoFrameRecoveryTimeoutMs(enabled),
        )
        recordStreamDiagnostic("recovery profile=${if (enabled) "android-tv" else "mobile"}")
    }

    fun setControllerMouseAssistEnabled(enabled: Boolean) {
        setControllerMouseAssistActive(enabled)
    }

    fun setControllerMouseEmulationActive(enabled: Boolean) {
        if (controllerMouseEmulationActive == enabled) return
        if (!enabled) {
            // Release any held mouse buttons so state stays clean.
            releaseControllerMouseButtons()
            // Zero both physical and virtual left-stick memory so neither controller path
            // delivers stale deflection to the game after mode is disabled.
            lastLeftStickX = 0
            lastLeftStickY = 0
            virtualLeftStickActive = false
            virtualLeftStickX = 0
            virtualLeftStickY = 0
            physicalLeftStickX = 0f
            physicalLeftStickY = 0f
            physicalRightStickX = 0f
            physicalRightStickY = 0f
        }
        controllerMouseEmulationActive = enabled
        updateControllerMouseLoop()
        // Push a fresh gamepad state immediately so the zeroed stick is sent before any next frame.
        sendCurrentGamepadState()
        NativeInputDiagnostics.add("controller mouse emulation ${if (enabled) "enabled" else "disabled"}")
    }

    private fun startControllerMouseLoop() {
        if (controllerMouseLoopJob?.isActive == true) return
        controllerMouseLoopJob = scope.launch {
            val currentJob = coroutineContext[Job]
            while (currentJob?.isActive == true) {
                // Poll/send mouse updates at 60Hz (approx 16ms delay; physical caches are cleared on disconnect/reset)
                delay(16L)
                if (controllerMouseEmulationActive) {
                    sendControllerMouseMove(physicalLeftStickX, physicalLeftStickY)
                    sendControllerMouseScroll(physicalRightStickY)
                }
                if (controllerMouseAssistActive) {
                    sendControllerMouseMove(physicalRightStickX, physicalRightStickY)
                }
            }
        }
    }

    private fun updateControllerMouseLoop() {
        if (shouldRunControllerMouseLoop(controllerMouseAssistActive, controllerMouseEmulationActive)) {
            startControllerMouseLoop()
        } else {
            stopControllerMouseLoop()
        }
    }

    private fun stopControllerMouseLoop() {
        controllerMouseLoopJob?.cancel()
        controllerMouseLoopJob = null
        physicalLeftStickX = 0f
        physicalLeftStickY = 0f
        physicalRightStickX = 0f
        physicalRightStickY = 0f
    }

    fun start(session: SessionInfo, settings: StreamSettings) {
        if (released) return
        this.session = session
        this.settings = settings
        transportGeneration += 1
        reconnectAttempts = 0
        transientSignalingFailures = 0
        selectedProfileRetryApplied = false
        stableMediaStallRestarts = 0
        sessionRecoveryRequested = false
        bitrateUpdateJob?.cancel()
        bitrateUpdateJob = null
        liveBitrateLimitKbps = null
        lastStatsSample = null
        processCpuSampler.reset()
        ProcessCpuDiagnostics.beginStream()
        packetLossWindow.reset()
        packetLossRecoveryGate.reset()
        decoderRecoveryGate.reset()
        livenessWatchdog.reset()
        firstVideoFrameWatchdog.reset()
        onStats(StreamRuntimeStats())
        audioDeviceModule.setSpeakerMute(audioMuted)
        audioDeviceModule.setMicrophoneMute(
            settings.microphoneMode == MicrophoneMode.Disabled || microphoneMuted,
        )
        closeTransport(clearInputState = false)
        armControllerMouseAssistForSession()
        recordStreamDiagnostic(
            "start session=${streamDiagnosticId(session.sessionId)} status=${session.status} server=${session.serverIp.take(96)} signaling=${signalingUrlForDiagnostics(session.signalingUrl, session.sessionId)} settings=${settings.resolution}/${settings.fps}/${settings.codec} bitrate=${settings.maxBitrateMbps} microphone=${settings.microphoneMode.name}",
        )
        startTransport(session, settings, transportGeneration)
        updateControllerMouseLoop()
    }

    fun stop() {
        stopControllerMouseLoop()
        transportGeneration += 1
        reconnectAttempts = 0
        transientSignalingFailures = 0
        stableMediaStallRestarts = 0
        sessionRecoveryRequested = false
        bitrateUpdateJob?.cancel()
        bitrateUpdateJob = null
        liveBitrateLimitKbps = null
        packetLossRecoveryGate.reset()
        decoderRecoveryGate.reset()
        livenessWatchdog.reset()
        firstVideoFrameWatchdog.reset()
        closeTransport(clearInputState = true)
        emitState("Stopped")
    }

    fun release() {
        if (released) return
        released = true
        if (androidTvProfile) {
            val activeRenderer = renderer
            activeRenderer?.let(::prepareRendererForRelease)
            renderer = null
            rendererSharpnessDrawer = null
            stop()
            scope.launch {
                delay(ANDROID_TV_CODEC_RELEASE_SETTLE_MS)
                finishRelease(activeRenderer)
            }
            return
        }
        stop()
        renderer?.let { activeRenderer ->
            releaseRendererInternal(activeRenderer)
        }
        renderer = null
        rendererSharpnessDrawer = null
        finishRelease()
    }

    private fun finishRelease(preparedRenderer: StreamVideoSurface? = null) {
        inputScope.cancel()
        inputExecutor.shutdown()
        val activeFactory = factory
        factory = null
        enqueueNativeLifecycleOperation("runtime-release") {
            preparedRenderer?.let { renderer ->
                runCatching { renderer.release() }
                    .onFailure { error -> recordStreamDiagnostic("renderer release failed error=${error.message.orEmpty()}") }
            }
            runCatching { activeFactory?.dispose() }
                .onFailure { error -> recordStreamDiagnostic("peer factory release failed error=${error.message.orEmpty()}") }
            runCatching { audioDeviceModule.release() }
                .onFailure { error -> recordStreamDiagnostic("audio module release failed error=${error.message.orEmpty()}") }
            runCatching { eglBase.release() }
                .onFailure { error -> recordStreamDiagnostic("EGL release failed error=${error.message.orEmpty()}") }
        }
        nativeLifecycleExecutor.shutdown()
        scope.cancel()
    }

    private fun resetInputState() {
        virtualButtons = 0
        virtualButtonPressSources.clear()
        virtualLeftTrigger = 0
        virtualRightTrigger = 0
        virtualLeftTriggerPressSources.clear()
        virtualRightTriggerPressSources.clear()
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
        physicalLeftStickX = 0f
        physicalLeftStickY = 0f
        physicalRightStickX = 0f
        physicalRightStickY = 0f
        controllerScrollAccumulator = 0f
        controllerMouseAssistActive = false
        controllerMouseAssistAutoArmed = false
        controllerMouseEmulationActive = false
        controllerMouseMoveLogged = false
        controllerMouseLeftButtonDown = false
        controllerMouseRightButtonDown = false
        activeControllerId = 0
        controllerSlots.clear()
        controllerFamiliesBySlot.clear()
        controllerAxisAvailability.clear()
        mousePositionValid = false
        mouseSuppressNextAbsoluteDelta = false
        externalMouseMotionAccumulator.reset()
        externalMouseAbsolutePosition.reset()
        gyroscopeMouseMotionAccumulator.reset()
        externalMouseMotionDeviceId = Int.MIN_VALUE
        externalMouseMotionSource = 0
        forwardedPhysicalInput.reset()
        textFallbackKeys.clear()
        inputDropLogged = false
        externalMouseEventLogged = false
        externalMouseMoveSentLogged = false
        externalMouseCapturedMoveSentLogged = false
        externalMouseAbsoluteJumpLogged = false
        hardwareKeyboardEventLogged = false
        physicalGamepadAxisLogged = false
        resetGamepadStateBurstLimiter()
        inputEncoder.resetGamepadSequences()
        inputHandshakeReady = false
        emitControllerMouseAssistChanged(false)
    }

    private val textFallbackKeys = java.util.concurrent.ConcurrentHashMap.newKeySet<Triple<Int, Int, Int>>()

    fun dispatchKey(event: KeyEvent): Boolean {
        // Before the WebRTC input channel opens, leave keyboard events available to Android and
        // Compose. Consuming them here makes queue/desktop UI look completely unresponsive.
        if (!hasReadyInputChannel()) return false
        if (event.isGamepadEvent()) {
            val receivedAtMs = SystemClock.uptimeMillis()
            val handled = dispatchGamepadKey(event)
            if (event.repeatCount == 0) {
                NativeInputDiagnostics.retainCounted(
                    key = "controller.key.${event.action}",
                ) {
                    "physical gamepad key device=${event.deviceId} key=${event.keyCode} action=${event.action} " +
                        "deliveryDelayMs=${(receivedAtMs - event.eventTime).coerceAtLeast(0L)} " +
                        "dispatchMs=${SystemClock.uptimeMillis() - receivedAtMs} handled=$handled"
                }
            }
            if (handled) return true
        }
        val key = InputEncoder.mapKeyEvent(event)
        val hardwareKeyboard = event.isHardwareKeyboardSource()
        if (hardwareKeyboard && !hardwareKeyboardEventLogged) {
            hardwareKeyboardEventLogged = true
            NativeInputDiagnostics.add(
                "hardware keyboard event action=${event.action} key=${event.keyCode} " +
                    "scan=${event.scanCode} repeat=${event.repeatCount} source=${event.source} " +
                    "device=${event.deviceId} mapped=${key != null}",
            )
        }
        if (shouldSuppressHardwareKeyboardRepeat(hardwareKeyboard, event.action, event.repeatCount)) {
            return true
        }
        val textKey = Triple(event.deviceId, event.keyCode, event.scanCode)
        if (event.action == KeyEvent.ACTION_UP && textFallbackKeys.remove(textKey)) return true
        val textFallback = InputEncoder.keyboardTextFallbackChar(
            unicodeChar = event.unicodeChar,
            baseUnicodeChar = event.getUnicodeChar(0),
            mapped = key != null,
            altGraph = event.isAltPressed && event.unicodeChar != event.getUnicodeChar(0),
            shortcut = event.isMetaPressed || (event.isCtrlPressed && !event.isAltPressed),
            keyCode = event.keyCode,
        )
        if (textFallback != null) {
            // Send once, on the press. The release carries the same character and would double it.
            if (event.action == KeyEvent.ACTION_DOWN) {
                textFallbackKeys.add(textKey)
                sendKeyboardTextFallback(textFallback)
            }
            return true
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
        if (hardwareKeyboard && (event.action == KeyEvent.ACTION_DOWN || event.action == KeyEvent.ACTION_UP)) {
            forwardedPhysicalInput.recordKey(
                deviceId = event.deviceId,
                keyCode = event.keyCode,
                scanCode = event.scanCode,
                payload = key,
                pressed = event.action == KeyEvent.ACTION_DOWN,
                sent = sent,
            )
        }
        if (hardwareKeyboard && !sent) {
            NativeInputDiagnostics.add("hardware keyboard consumed without send key=${event.keyCode} ${inputChannelStateSummary()}")
        }
        return sent || hardwareKeyboard
    }

    fun dispatchMotion(event: MotionEvent): Boolean {
        // Mouse/controller events must fall through to native UI until cloud input is ready.
        if (!hasReadyInputChannel()) return false
        if (event.isGamepadMotionEvent()) {
            return dispatchJoystick(event)
        }
        if (event.isMouseLikePointer()) {
            return dispatchMouseLikePointer(event)
        }
        return false
    }

    /**
     * Mouse packets are relative deltas, so every packet must arrive in order. Keep Android mouse
     * motion on the reliable channel: losing a delta leaves the VM cursor permanently displaced,
     * and the Android partially-reliable data channel has failed to move the live host cursor.
     */
    fun sendRawMouseMove(dx: Int, dy: Int): Boolean {
        return sendBurstLimitedMouseMove(dx, dy, partiallyReliable = false)
    }

    /** Sends one batch of finger updates. Reliable: a dropped lift leaves a finger stuck down. */
    internal fun sendNativeTouch(touches: List<TouchRecord>): Boolean {
        val packet = inputEncoder.encodeTouchBatch(touches) ?: return false
        return sendReliableInput(packet)
    }

    fun sendTouchMouseMove(dx: Int, dy: Int): Boolean {
        var adjustedDx = dx * settings.mouseSensitivity
        var adjustedDy = dy * settings.mouseSensitivity
        if (settings.mouseAcceleration > 1) {
            val speed = sqrt(adjustedDx * adjustedDx + adjustedDy * adjustedDy)
            val strength = (settings.mouseAcceleration - 1f) / 149f
            val accelFactor = 1f + min(0.6f * strength, (speed / 50f) * strength)
            adjustedDx *= accelFactor
            adjustedDy *= accelFactor
        }
        return sendBurstLimitedMouseMove(
            dx = adjustedDx.roundToInt(),
            dy = adjustedDy.roundToInt(),
            partiallyReliable = false,
        )
    }

    /**
     * Sends the leading movement immediately, then combines only the excess events inside the
     * next short interval. This retains responsive mouse/controller movement while keeping a
     * 500 Hz device from creating 500 SCTP packets and sender coroutines per second.
     */
    private fun sendBurstLimitedMouseMove(dx: Int, dy: Int, partiallyReliable: Boolean): Boolean {
        if (!canSendInput(partiallyReliable, fallbackToReliable = true)) return false
        if (dx == 0 && dy == 0) return true
        return synchronized(mouseMoveBurstLock) {
            val batch = mouseMoveBurstLimiter.offer(
                dx = dx,
                dy = dy,
                partiallyReliable = partiallyReliable,
                nowMs = SystemClock.elapsedRealtime(),
            )
            if (batch == null && mouseMoveBurstFlushJob?.isActive != true) {
                scheduleMouseMoveBurstFlushLocked()
            }
            batch?.let(::sendMouseMoveBatch) ?: true
        }
    }

    /** Must be called with [mouseMoveBurstLock] held. */
    private fun scheduleMouseMoveBurstFlushLocked() {
        mouseMoveBurstFlushJob = inputScope.launch {
            while (true) {
                val waitMs = synchronized(mouseMoveBurstLock) {
                    mouseMoveBurstLimiter.delayUntilFlushMs(SystemClock.elapsedRealtime()).also {
                        if (it == null) mouseMoveBurstFlushJob = null
                    }
                } ?: return@launch
                if (waitMs > 0L) delay(waitMs)

                val flushed = synchronized(mouseMoveBurstLock) {
                    val nowMs = SystemClock.elapsedRealtime()
                    if ((mouseMoveBurstLimiter.delayUntilFlushMs(nowMs) ?: 0L) > 0L) {
                        false
                    } else {
                        mouseMoveBurstLimiter.flush(nowMs)?.let(::sendMouseMoveBatch)
                        mouseMoveBurstFlushJob = null
                        true
                    }
                }
                if (flushed) return@launch
            }
        }
    }

    private fun flushPendingMouseMove() {
        synchronized(mouseMoveBurstLock) {
            mouseMoveBurstFlushJob?.cancel()
            mouseMoveBurstFlushJob = null
            mouseMoveBurstLimiter.flush(SystemClock.elapsedRealtime())?.let(::sendMouseMoveBatch)
        }
        synchronized(externalMouseMoveBurstLock) {
            externalMouseMoveBurstFlushJob?.cancel()
            externalMouseMoveBurstFlushJob = null
            externalMouseMoveBurstLimiter.flush(SystemClock.elapsedRealtime())
                ?.let(::sendExternalMouseAbsoluteBatch)
        }
    }

    private fun resetMouseMoveBurstLimiter() {
        synchronized(mouseMoveBurstLock) {
            mouseMoveBurstFlushJob?.cancel()
            mouseMoveBurstFlushJob = null
            mouseMoveBurstLimiter.reset()
        }
        resetExternalMouseMoveBurstLimiter()
    }

    private fun resetExternalMouseMoveBurstLimiter() {
        synchronized(externalMouseMoveBurstLock) {
            externalMouseMoveBurstFlushJob?.cancel()
            externalMouseMoveBurstFlushJob = null
            externalMouseMoveBurstLimiter.reset()
        }
    }

    private fun resetGamepadStateBurstLimiter() {
        synchronized(gamepadStateBurstLock) {
            gamepadStateBurstFlushJob?.cancel()
            gamepadStateBurstFlushJob = null
            gamepadStateBurstLimiter.reset()
        }
    }

    private fun sendMouseMoveBatch(batch: MouseMoveBatch): Boolean {
        for (packet in inputEncoder.encodeMouseMoves(batch.dx, batch.dy)) {
            if (!sendInput(packet, partiallyReliable = batch.partiallyReliable,
                    fallbackToReliable = true, resultDiagnosticKey = "mouse.move")) return false
        }
        return true
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
                val capturedPointer = event.isRelativeMousePointer()
                val hasRelativeAxisMotion = event.hasRelativeAxisMotion()
                if (shouldSendExternalMouseAsRelative(capturedPointer, hasRelativeAxisMotion)) {
                    // Captured-pointer implementations disagree about where relative motion lives.
                    // Pixel/Logitech exposes RELATIVE_X/Y, while other devices only populate X/Y.
                    // Resolve every coalesced sample independently so neither representation is
                    // discarded when Android changes it during capture or device hand-off.
                    val sent = sendExternalMouseMotionSamples(event)
                    if (capturedPointer && sent && !externalMouseCapturedMoveSentLogged) {
                        externalMouseCapturedMoveSentLogged = true
                        NativeInputDiagnostics.add(
                            "external mouse captured move sent source=${event.source} device=${event.deviceId} " +
                                "x=${event.x} y=${event.y} " +
                                "relativeX=${event.getAxisValue(MotionEvent.AXIS_RELATIVE_X)} " +
                                "relativeY=${event.getAxisValue(MotionEvent.AXIS_RELATIVE_Y)}",
                        )
                    }
                    if (sent && !externalMouseMoveSentLogged) {
                        externalMouseMoveSentLogged = true
                        val mode = if (capturedPointer) "capturedRelative" else "relative"
                        NativeInputDiagnostics.add(
                            "external mouse move sent source=${event.source} device=${event.deviceId} mode=$mode packet=relative",
                        )
                    }
                    mousePositionValid = false
                } else if (mousePositionValid && mouseLastDeviceId == event.deviceId && mouseLastSource == event.source) {
                    val dx = event.x - mouseLastX
                    val dy = event.y - mouseLastY
                    if (dx != 0f || dy != 0f) {
                        val discontinuous = mouseSuppressNextAbsoluteDelta ||
                            abs(dx) > EXTERNAL_MOUSE_ABSOLUTE_DELTA_LIMIT_PX ||
                            abs(dy) > EXTERNAL_MOUSE_ABSOLUTE_DELTA_LIMIT_PX
                        if (discontinuous) {
                            externalMouseMotionAccumulator.reset()
                            if (!externalMouseAbsoluteJumpLogged) {
                                externalMouseAbsoluteJumpLogged = true
                                NativeInputDiagnostics.add("external mouse absolute delta rebased source=${event.source} device=${event.deviceId} dx=${dx.roundToInt()} dy=${dy.roundToInt()}")
                            }
                        } else {
                            val sent = sendExternalMouseMotion(event, dx, dy)
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
                flushPendingMouseMove()
                sendExternalMouseButton(event.primaryMouseButton(), pressed = true)
            }
            MotionEvent.ACTION_UP,
            MotionEvent.ACTION_CANCEL,
            -> {
                mousePositionValid = false
                mouseSuppressNextAbsoluteDelta = true
                flushPendingMouseMove()
                sendExternalMouseButton(event.primaryMouseButton(), pressed = false)
            }
            MotionEvent.ACTION_BUTTON_PRESS -> {
                mouseSuppressNextAbsoluteDelta = true
                rememberMousePosition(event)
                flushPendingMouseMove()
                val handled = sendExternalMouseButton(event.actionButton.toGfnMouseButton(), pressed = true)
                if (!handled) {
                    NativeInputDiagnostics.add("external mouse button consumed without send action=press button=${event.actionButton} ${inputChannelStateSummary()}")
                }
                return true
            }
            MotionEvent.ACTION_BUTTON_RELEASE -> {
                mousePositionValid = false
                mouseSuppressNextAbsoluteDelta = true
                flushPendingMouseMove()
                val handled = sendExternalMouseButton(event.actionButton.toGfnMouseButton(), pressed = false)
                if (!handled) {
                    NativeInputDiagnostics.add("external mouse button consumed without send action=release button=${event.actionButton} ${inputChannelStateSummary()}")
                }
                return true
            }
            MotionEvent.ACTION_SCROLL -> {
                val vertical = event.getAxisValue(MotionEvent.AXIS_VSCROLL)
                if (abs(vertical) >= 0.01f) {
                    flushPendingMouseMove()
                    sendReliableInput(inputEncoder.encodeMouseWheel((vertical * 120).roundToInt()))
                }
            }
        }
        return true
    }

    private fun sendExternalMouseButton(button: Int, pressed: Boolean): Boolean {
        val sent = sendReliableInput(
            inputEncoder.encodeMouseButton(
                if (pressed) InputEncoder.INPUT_MOUSE_BUTTON_DOWN else InputEncoder.INPUT_MOUSE_BUTTON_UP,
                button,
            ),
        )
        forwardedPhysicalInput.recordMouseButton(button = button, pressed = pressed, sent = sent)
        return sent
    }

    /** Releases input whose platform UP event can be lost when a desktop window loses focus. */
    fun releasePhysicalInputForLifecycle(reason: String) {
        textFallbackKeys.clear()
        val pressed = forwardedPhysicalInput.takeReleaseSnapshot()
        if (pressed.isEmpty) return
        flushPendingMouseMove()
        var queued = 0
        pressed.keys.forEach { payload ->
            if (
                sendReliableInput(
                    inputEncoder.encodeKeyUp(payload.copy(modifiers = 0, timestampUs = timestampUs())),
                )
            ) {
                queued += 1
            }
        }
        pressed.mouseButtons.forEach { button ->
            if (sendReliableInput(inputEncoder.encodeMouseButton(InputEncoder.INPUT_MOUSE_BUTTON_UP, button))) {
                queued += 1
            }
        }
        NativeInputDiagnostics.add(
            "physical input lifecycle release reason=$reason keys=${pressed.keys.size} " +
                "mouseButtons=${pressed.mouseButtons.size} queued=$queued ${inputChannelStateSummary()}",
        )
    }

    private fun MotionEvent.hasRelativeAxisMotion(): Boolean {
        if (Build.VERSION.SDK_INT < 26) return false
        for (historyIndex in 0 until historySize) {
            if (
                getHistoricalAxisValue(MotionEvent.AXIS_RELATIVE_X, historyIndex) != 0f ||
                getHistoricalAxisValue(MotionEvent.AXIS_RELATIVE_Y, historyIndex) != 0f
            ) {
                return true
            }
        }
        return getAxisValue(MotionEvent.AXIS_RELATIVE_X) != 0f ||
            getAxisValue(MotionEvent.AXIS_RELATIVE_Y) != 0f
    }

    private fun sendExternalMouseMotionSamples(event: MotionEvent): Boolean {
        prepareExternalMouseMotion(event)
        var sendDx = 0
        var sendDy = 0
        // Pointer capture may coalesce several raw samples into one MotionEvent. Process every
        // sample before one packet is sent so neither slow motion nor high-polling-rate input is lost.
        for (historyIndex in 0 until event.historySize) {
            val relativeDx = event.getHistoricalAxisValue(MotionEvent.AXIS_RELATIVE_X, historyIndex)
            val relativeDy = event.getHistoricalAxisValue(MotionEvent.AXIS_RELATIVE_Y, historyIndex)
            val capturedDx = event.getHistoricalX(historyIndex)
            val capturedDy = event.getHistoricalY(historyIndex)
            val capturedPointer = event.isRelativeMousePointer()
            val useRelativeAxes = shouldUseAndroidRelativeMouseAxes(relativeDx, relativeDy)
            val dx = when {
                capturedPointer -> resolveAndroidCapturedMouseAxis(relativeDx, capturedDx)
                useRelativeAxes -> relativeDx
                else -> capturedDx
            }
            val dy = when {
                capturedPointer -> resolveAndroidCapturedMouseAxis(relativeDy, capturedDy)
                useRelativeAxes -> relativeDy
                else -> capturedDy
            }
            externalMouseMotionAccumulator.add(
                dx = dx,
                dy = dy,
                eventTimeMs = event.getHistoricalEventTime(historyIndex),
                sensitivity = settings.mouseSensitivity,
                acceleration = settings.mouseAcceleration,
            )?.let { delta ->
                sendDx += delta.dx
                sendDy += delta.dy
            }
        }
        val relativeDx = event.getAxisValue(MotionEvent.AXIS_RELATIVE_X)
        val relativeDy = event.getAxisValue(MotionEvent.AXIS_RELATIVE_Y)
        val capturedPointer = event.isRelativeMousePointer()
        val useRelativeAxes = shouldUseAndroidRelativeMouseAxes(relativeDx, relativeDy)
        val dx = when {
            capturedPointer -> resolveAndroidCapturedMouseAxis(relativeDx, event.x)
            useRelativeAxes -> relativeDx
            else -> event.x
        }
        val dy = when {
            capturedPointer -> resolveAndroidCapturedMouseAxis(relativeDy, event.y)
            useRelativeAxes -> relativeDy
            else -> event.y
        }
        if (
            capturedPointer &&
            !externalMouseAxisConflictLogged &&
            androidCapturedMouseAxesConflict(relativeDx, relativeDy, event.x, event.y)
        ) {
            externalMouseAxisConflictLogged = true
            NativeInputDiagnostics.add(
                "external mouse axis conflict resolved sdk=${Build.VERSION.SDK_INT} source=${event.source} " +
                    "relativeX=$relativeDx relativeY=$relativeDy capturedX=${event.x} capturedY=${event.y}",
            )
        }
        externalMouseMotionAccumulator.add(
            dx = dx,
            dy = dy,
            eventTimeMs = event.eventTime,
            sensitivity = settings.mouseSensitivity,
            acceleration = settings.mouseAcceleration,
        )?.let { delta ->
            sendDx += delta.dx
            sendDy += delta.dy
        }
        // Relative motion is unbounded by design. Camera-look must not inherit the clamped
        // absolute cursor used by the uncaptured desktop-pointer path below.
        return (sendDx != 0 || sendDy != 0) && sendRawMouseMove(sendDx, sendDy)
    }

    private fun sendExternalMouseMotion(event: MotionEvent, dx: Float, dy: Float): Boolean {
        prepareExternalMouseMotion(event)
        val delta = externalMouseMotionAccumulator.add(
            dx = dx,
            dy = dy,
            eventTimeMs = event.eventTime,
            sensitivity = settings.mouseSensitivity,
            acceleration = settings.mouseAcceleration,
        ) ?: return false
        return sendExternalMouseAbsoluteMove(delta.dx, delta.dy)
    }

    private fun sendExternalMouseAbsoluteMove(dx: Int, dy: Int): Boolean {
        val usePartiallyReliable =
            partiallyReliableInputState == DataChannel.State.OPEN &&
                SdpTools.supportsPartiallyReliableHidInput(
                    hidDeviceMask = hidDeviceMask,
                    partiallyReliableHidMask = partiallyReliableHidMask,
                    inputType = InputEncoder.INPUT_MOUSE_ABS,
                )
        if (!canSendInput(usePartiallyReliable, fallbackToReliable = true)) return false
        if (dx == 0 && dy == 0) return true
        return synchronized(externalMouseMoveBurstLock) {
            val batch = externalMouseMoveBurstLimiter.offer(
                dx = dx,
                dy = dy,
                partiallyReliable = usePartiallyReliable,
                nowMs = SystemClock.elapsedRealtime(),
            )
            if (batch == null && externalMouseMoveBurstFlushJob?.isActive != true) {
                scheduleExternalMouseMoveBurstFlushLocked()
            }
            batch?.let(::sendExternalMouseAbsoluteBatch) ?: true
        }
    }

    /** Must be called with [externalMouseMoveBurstLock] held. */
    private fun scheduleExternalMouseMoveBurstFlushLocked() {
        externalMouseMoveBurstFlushJob = inputScope.launch {
            while (true) {
                val waitMs = synchronized(externalMouseMoveBurstLock) {
                    externalMouseMoveBurstLimiter.delayUntilFlushMs(SystemClock.elapsedRealtime()).also {
                        if (it == null) externalMouseMoveBurstFlushJob = null
                    }
                } ?: return@launch
                if (waitMs > 0L) delay(waitMs)

                val flushed = synchronized(externalMouseMoveBurstLock) {
                    val nowMs = SystemClock.elapsedRealtime()
                    if ((externalMouseMoveBurstLimiter.delayUntilFlushMs(nowMs) ?: 0L) > 0L) {
                        false
                    } else {
                        externalMouseMoveBurstLimiter.flush(nowMs)?.let(::sendExternalMouseAbsoluteBatch)
                        externalMouseMoveBurstFlushJob = null
                        true
                    }
                }
                if (flushed) return@launch
            }
        }
    }

    private fun sendExternalMouseAbsoluteBatch(batch: MouseMoveBatch): Boolean {
        val (width, height) = streamResolutionPixels(settings)
        val position = externalMouseAbsolutePosition.moveBy(batch.dx, batch.dy, width, height)
        return sendInput(
            inputEncoder.encodeMouseAbsolute(position.x, position.y, position.width, position.height),
            partiallyReliable = batch.partiallyReliable,
            fallbackToReliable = true,
            resultDiagnosticKey = "mouse.absolute",
        )
    }

    private fun prepareExternalMouseMotion(event: MotionEvent) {
        if (externalMouseMotionDeviceId == event.deviceId && externalMouseMotionSource == event.source) return
        flushPendingMouseMove()
        externalMouseMotionAccumulator.reset()
        externalMouseAbsolutePosition.reset()
        resetExternalMouseMoveBurstLimiter()
        externalMouseMotionDeviceId = event.deviceId
        externalMouseMotionSource = event.source
    }

    private fun rememberMousePosition(event: MotionEvent) {
        mouseLastDeviceId = event.deviceId
        mouseLastSource = event.source
        mouseLastX = event.x
        mouseLastY = event.y
        mousePositionValid = true
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

    fun sendTouchMouseRightClick() {
        scope.launch {
            if (!sendMouseButton(button = 3, pressed = true, source = "touch mouse right click")) return@launch
            delay(160L)
            sendMouseButton(button = 3, pressed = false, source = "touch mouse right click")
        }
    }

    fun sendTouchMouseWheel(delta: Int) {
        flushPendingMouseMove()
        sendReliableInput(inputEncoder.encodeMouseWheel(delta))
    }

    fun sendKeyCode(keyCode: Int) {
        val down = KeyEvent(SystemClock.uptimeMillis(), SystemClock.uptimeMillis(), KeyEvent.ACTION_DOWN, keyCode, 0)
        val up = KeyEvent(SystemClock.uptimeMillis(), SystemClock.uptimeMillis(), KeyEvent.ACTION_UP, keyCode, 0)
        val mapped = InputEncoder.mapKeyEvent(down)
        val downQueued = dispatchKey(down)
        val upQueued = dispatchKey(up)
        NativeInputDiagnostics.add(
            "overlay keyboard key=$keyCode mapped=${mapped != null} " +
                "vk=${mapped?.keycode} scan=${mapped?.scancode} " +
                "downQueued=$downQueued upQueued=$upQueued " +
                inputChannelStateSummary(),
        )
    }

    /** Applies each Android IME edit immediately while preserving input packet order. */
    fun syncText(syncedText: String?, draft: String) {
        val edit = streamKeyboardEdit(syncedText, draft)
        if (edit == StreamKeyboardEdit.None) return
        scope.launch {
            textSendMutex.withLock {
                when (edit) {
                    StreamKeyboardEdit.None -> Unit
                    is StreamKeyboardEdit.Append -> sendTextLocked(edit.text.take(STREAM_TEXT_SEND_MAX_CHARS))
                    is StreamKeyboardEdit.Backspace -> repeat(edit.count.coerceAtMost(STREAM_TEXT_SEND_MAX_CHARS)) {
                        if (!sendTextKeyStroke(KeyEvent.KEYCODE_DEL)) return@withLock
                    }
                    is StreamKeyboardEdit.ReplaceSuffix -> {
                        repeat(edit.backspaces.coerceAtMost(STREAM_TEXT_SEND_MAX_CHARS)) {
                            if (!sendTextKeyStroke(KeyEvent.KEYCODE_DEL)) return@withLock
                        }
                        sendTextLocked(edit.text.take(STREAM_TEXT_SEND_MAX_CHARS))
                    }
                }
            }
        }
    }

    /** Clears the focused remote field without dismissing the Android stream keyboard. */
    fun clearText() {
        scope.launch {
            textSendMutex.withLock {
                selectAllAndDeleteRemoteText()
            }
        }
    }

    /** Queues editor control keys behind any text currently being replayed to the host. */
    fun sendTextControlKey(keyCode: Int) {
        scope.launch {
            textSendMutex.withLock {
                sendTextKeyStroke(keyCode)
            }
        }
    }

    // Enqueue physical text synchronously, just like physical keys. Launching a separate
    // coroutine here lets the following letter or Enter overtake the symbol.
    private fun sendKeyboardTextFallback(char: Char) {
        inputEncoder.encodeTextInput(char.toString()).forEach(::sendReliableInput)
    }

    private suspend fun sendTextLocked(text: String) {
        inputEncoder.encodeTextInput(text).forEach { packet ->
            if (!sendTextPacketWithRetry(packet)) return
        }
    }

    private suspend fun selectAllAndDeleteRemoteText(): Boolean {
        val ctrl = InputEncoder.mapKeyboardPayload(KeyEvent.KEYCODE_CTRL_LEFT, unicode = 0, scanCode = 0)
            ?: return false
        val selectAll = InputEncoder.mapKeyboardPayload(
            keyCode = KeyEvent.KEYCODE_A,
            unicode = 0,
            scanCode = 0,
            ctrl = true,
        ) ?: return false
        val ctrlPressed = sendKeyboardPayloadWithRetry(ctrl.copy(modifiers = 0x02), isDown = true)
        if (!ctrlPressed) return false
        val selected = sendKeyboardPayloadWithRetry(selectAll, isDown = true) &&
            sendKeyboardPayloadWithRetry(selectAll, isDown = false)
        val ctrlReleased = sendKeyboardPayloadWithRetry(ctrl.copy(modifiers = 0), isDown = false)
        if (!selected || !ctrlReleased) return false
        return sendTextKeyStroke(KeyEvent.KEYCODE_DEL)
    }

    private suspend fun sendTextKeyStroke(keyCode: Int): Boolean {
        val payload = InputEncoder.mapKeyboardPayload(keyCode, unicode = 0, scanCode = 0) ?: return false
        return sendKeyboardPayloadWithRetry(payload, isDown = true) &&
            sendKeyboardPayloadWithRetry(payload, isDown = false)
    }

    private fun sendKeyboardPayload(payload: InputEncoder.KeyboardPayload, isDown: Boolean): Boolean =
        sendReliableInput(if (isDown) inputEncoder.encodeKeyDown(payload) else inputEncoder.encodeKeyUp(payload))

    private suspend fun sendTextPacketWithRetry(packet: ByteArray): Boolean {
        repeat(STREAM_TEXT_SEND_ATTEMPTS) { attempt ->
            if (sendReliableInput(packet)) {
                delay(STREAM_TEXT_PACKET_DELAY_MS)
                return true
            }
            if (attempt < STREAM_TEXT_SEND_ATTEMPTS - 1) delay(STREAM_TEXT_RETRY_DELAY_MS)
        }
        NativeInputDiagnostics.add(
            "overlay keyboard dropped unicode bytes=${packet.size} ${inputChannelStateSummary()}",
        )
        return false
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
            "overlay keyboard dropped key=${payload.keycode} action=${if (isDown) "down" else "up"} ${inputChannelStateSummary()}",
        )
        return false
    }

    fun setAudioMuted(muted: Boolean) {
        audioMuted = muted
        nvstTransport?.muted = muted
        audioDeviceModule.setSpeakerMute(muted)
        val generation = transportGeneration
        enqueueNativeLifecycleOperation("audio-track-mute") {
            if (activePeerConnection(generation) == null) return@enqueueNativeLifecycleOperation
            audioTrack?.setEnabled(!muted)
        }
    }

    fun setMicrophoneEnabled(enabled: Boolean) {
        microphoneMuted = !enabled
        nvstTransport?.setMicrophoneEnabled(enabled)
        audioDeviceModule.setMicrophoneMute(!enabled)
        val generation = transportGeneration
        enqueueNativeLifecycleOperation("microphone-track-mute") {
            if (activePeerConnection(generation) == null) return@enqueueNativeLifecycleOperation
            microphoneTrack?.setEnabled(enabled)
        }
        recordStreamDiagnostic("microphone ${if (enabled) "enabled" else "muted"}")
    }

    /** Queues a receive bitrate ceiling for the next offer without replacing the active transport. */
    fun updateBitrateLimit(maxBitrateKbps: Int) {
        val normalizedKbps = normalizedLiveBitrateKbps(maxBitrateKbps)
        if (liveBitrateLimitKbps == normalizedKbps && bitrateUpdateJob?.isActive != true) return
        liveBitrateLimitKbps = normalizedKbps
        bitrateUpdateJob?.cancel()
        bitrateUpdateJob = scope.launch {
            delay(LIVE_BITRATE_UPDATE_DEBOUNCE_MS)
            bitrateUpdateJob = null
            val updatedSettings = settings.copy(maxBitrateMbps = normalizedKbps / 1_000)
            if (updatedSettings == settings) return@launch
            settings = updatedSettings
            recordStreamDiagnostic("live bitrate limit queued for next offer $normalizedKbps kbps")
        }
    }

    fun setTouchMouseButton(pressed: Boolean): Boolean {
        return sendMouseButton(button = 1, pressed = pressed, source = "touch mouse")
    }

    private fun sendMouseButton(button: Int, pressed: Boolean, source: String): Boolean {
        flushPendingMouseMove()
        val packet = inputEncoder.encodeMouseButton(
            if (pressed) InputEncoder.INPUT_MOUSE_BUTTON_DOWN else InputEncoder.INPUT_MOUSE_BUTTON_UP,
            button,
        )
        val reliableSent = sendInput(packet, partiallyReliable = false)
        // NVST maps mouse buttons to its one reliable control stream. Sending the WebRTC
        // compatibility copy there would deliver the same button edge twice.
        val partialSent = nvstTransport == null && sendInput(packet, partiallyReliable = true)
        NativeInputDiagnostics.add(
            "$source button=$button ${if (pressed) "down" else "up"} reliableSent=$reliableSent partialSent=$partialSent ${inputChannelStateSummary()}",
        )
        return reliableSent || partialSent
    }

    private fun armControllerMouseAssistForSession() {
        if (!controllerMouseAutoArmOnStart) return
        controllerMouseAssistActive = true
        controllerMouseAssistAutoArmed = true
        controllerMouseMoveLogged = false
        emitControllerMouseAssistChanged(true)
        NativeInputDiagnostics.add("controller mouse assist auto-armed for Android TV")
    }

    private fun setControllerMouseAssistActive(active: Boolean, autoArmed: Boolean = false) {
        if (controllerMouseAssistActive == active && controllerMouseAssistAutoArmed == (autoArmed && active)) return
        if (!active) releaseControllerMouseButtons()
        controllerMouseAssistActive = active
        controllerMouseAssistAutoArmed = autoArmed && active
        updateControllerMouseLoop()
        controllerMouseMoveLogged = false
        sendCurrentGamepadState()
        emitControllerMouseAssistChanged(active)
        NativeInputDiagnostics.add("controller mouse assist ${if (active) "enabled" else "disabled"} auto=$controllerMouseAssistAutoArmed")
    }

    private fun emitControllerMouseAssistChanged(active: Boolean) {
        scope.launch { onControllerMouseAssistChanged(active) }
    }

    private fun releaseControllerMouseButtons() {
        if (controllerMouseLeftButtonDown) {
            controllerMouseLeftButtonDown = false
            sendMouseButton(button = 1, pressed = false, source = "controller mouse")
        }
        if (controllerMouseRightButtonDown) {
            controllerMouseRightButtonDown = false
            sendMouseButton(button = 3, pressed = false, source = "controller mouse")
        }
    }

    private fun setControllerMouseButton(button: Int, pressed: Boolean): Boolean {
        when (button) {
            1 -> {
                if (controllerMouseLeftButtonDown == pressed) return true
                controllerMouseLeftButtonDown = pressed
            }
            3 -> {
                if (controllerMouseRightButtonDown == pressed) return true
                controllerMouseRightButtonDown = pressed
            }
            else -> return false
        }
        val sent = sendMouseButton(button = button, pressed = pressed, source = "controller mouse")
        if (!pressed && controllerMouseAssistAutoArmed && button == 1) {
            setControllerMouseAssistActive(false)
        }
        return sent
    }

    fun setVirtualButton(buttonMask: Int, pressed: Boolean) {
        setVirtualButtonFromSource(buttonMask, "primary-$buttonMask", pressed)
    }

    /** Keeps duplicate movable buttons from releasing an action another finger still holds. */
    fun setVirtualButtonFromSource(buttonMask: Int, sourceId: String, pressed: Boolean) {
        val sources = virtualButtonPressSources.getOrPut(buttonMask) { mutableSetOf() }
        val wasEffectivelyPressed = sources.isNotEmpty()
        val changed = if (pressed) sources.add(sourceId) else sources.remove(sourceId)
        if (!pressed && sources.isEmpty()) virtualButtonPressSources.remove(buttonMask)
        if (!changed) return
        val effectivelyPressed = sources.isNotEmpty()
        if (wasEffectivelyPressed == effectivelyPressed) return
        // When left-stick mouse emulation is active, intercept A (left click) and B (right click).
        if (controllerMouseEmulationActive) {
            val mouseButton = AndroidControllerMouseAssist.mouseButtonForGamepad(buttonMask)
            if (mouseButton != null) {
                val sent = setControllerMouseButton(mouseButton, effectivelyPressed)
                recordVirtualButtonDiagnostic(
                    buttonMask = buttonMask,
                    pressed = effectivelyPressed,
                    route = "mouse-$mouseButton",
                    sent = sent,
                )
                return
            }
        }
        virtualButtons = if (effectivelyPressed) virtualButtons or buttonMask else virtualButtons and buttonMask.inv()
        val steamOverlayChordActivated = virtualSteamOverlayChord.update(virtualButtons)
        val sent = sendCurrentGamepadState()
        recordVirtualButtonDiagnostic(
            buttonMask = buttonMask,
            pressed = effectivelyPressed,
            route = "gamepad",
            sent = sent,
        )
        if (steamOverlayChordActivated) {
            scheduleVirtualSteamOverlayChordRelease()
        }
    }

    private fun recordVirtualButtonDiagnostic(
        buttonMask: Int,
        pressed: Boolean,
        route: String,
        sent: Boolean,
    ) {
        val action = if (pressed) "down" else "up"
        val maskHex = buttonMask.toString(16).padStart(4, '0')
        NativeInputDiagnostics.retain(
            key = "controller.virtual-button.$maskHex.$action",
            message = "virtual gamepad button mask=0x$maskHex action=$action route=$route sent=$sent " +
                "buttons=$virtualButtons ${inputChannelStateSummary()}",
        )
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
            NativeInputDiagnostics.add("Steam Menu sent Guide+A chord slot=$controllerId")
        }
    }

    fun setVirtualTrigger(left: Boolean, pressed: Boolean) {
        setVirtualTriggerFromSource(left, if (left) "primary-left" else "primary-right", pressed)
    }

    /** Trigger equivalent of [setVirtualButtonFromSource]. */
    fun setVirtualTriggerFromSource(left: Boolean, sourceId: String, pressed: Boolean) {
        val sources = if (left) virtualLeftTriggerPressSources else virtualRightTriggerPressSources
        val wasEffectivelyPressed = sources.isNotEmpty()
        val changed = if (pressed) sources.add(sourceId) else sources.remove(sourceId)
        if (!changed) return
        val effectivelyPressed = sources.isNotEmpty()
        if (wasEffectivelyPressed == effectivelyPressed) return
        if (left) {
            virtualLeftTrigger = if (effectivelyPressed) 255 else 0
        } else {
            virtualRightTrigger = if (effectivelyPressed) 255 else 0
        }
        val sent = sendCurrentGamepadState()
        val side = if (left) "LT" else "RT"
        val action = if (effectivelyPressed) "down" else "up"
        NativeInputDiagnostics.retain(
            key = "controller.virtual-trigger.$side.$action",
            message = "virtual gamepad trigger=$side action=$action sent=$sent " +
                "triggers=$virtualLeftTrigger,$virtualRightTrigger ${inputChannelStateSummary()}",
        )
    }

    fun setVirtualLeftStick(x: Float, y: Float) {
        val scale = radialDeadzoneScale(x, y, deadzone = 0.08f)
        val normalizedX = x * scale
        val normalizedY = y * scale
        if (controllerMouseEmulationActive) {
            // Redirect left-stick input to mouse movement; keep virtual stick zeroed so the game
            // receives no stick deflection from the touch controller either.
            physicalLeftStickX = normalizedX
            physicalLeftStickY = normalizedY
            virtualLeftStickActive = false
            virtualLeftStickX = 0
            virtualLeftStickY = 0
            sendBurstLimitedGamepadState()
            return
        }
        virtualLeftStickActive = normalizedX != 0f || normalizedY != 0f
        virtualLeftStickX = normalizeToInt16(normalizedX)
        virtualLeftStickY = normalizeToInt16(-normalizedY)
        sendBurstLimitedGamepadState()
    }

    fun setVirtualRightStick(x: Float, y: Float) {
        val scale = radialDeadzoneScale(x, y, deadzone = 0.08f)
        val normalizedX = x * scale
        val normalizedY = y * scale
        if (controllerMouseEmulationActive) {
            // Redirect right-stick input to scrolling; keep virtual stick zeroed.
            physicalRightStickX = normalizedX
            physicalRightStickY = normalizedY
            virtualRightStickActive = false
            virtualRightStickX = 0
            virtualRightStickY = 0
            sendBurstLimitedGamepadState()
            return
        }
        virtualRightStickActive = normalizedX != 0f || normalizedY != 0f
        virtualRightStickX = normalizeToInt16(normalizedX)
        virtualRightStickY = normalizeToInt16(-normalizedY)
        sendBurstLimitedGamepadState()
    }

    fun beginGyroscopeMouseAim() = gyroscopeMouseMotionAccumulator.reset()

    fun sendGyroscopeMouseMove(dx: Float, dy: Float, eventTimeMs: Long): Boolean =
        sendAccumulatedMouseMove(
            accumulator = gyroscopeMouseMotionAccumulator,
            dx = dx,
            dy = dy,
            eventTimeMs = eventTimeMs,
            sensitivity = 1f,
            acceleration = 1,
        )

    fun endGyroscopeMouseAim(eventTimeMs: Long) {
        flushAccumulatedMouseMove(gyroscopeMouseMotionAccumulator, eventTimeMs)
    }

    private fun sendAccumulatedMouseMove(
        accumulator: MouseMotionAccumulator,
        dx: Float,
        dy: Float,
        eventTimeMs: Long,
        sensitivity: Float,
        acceleration: Int,
    ): Boolean {
        val delta = accumulator.add(
            dx = dx,
            dy = dy,
            eventTimeMs = eventTimeMs,
            sensitivity = sensitivity,
            acceleration = acceleration,
        ) ?: return true
        return sendRawMouseMove(delta.dx, delta.dy)
    }

    private fun flushAccumulatedMouseMove(accumulator: MouseMotionAccumulator, eventTimeMs: Long) {
        accumulator.add(
            dx = 0f,
            dy = 0f,
            eventTimeMs = eventTimeMs,
            sensitivity = 1f,
            acceleration = 1,
            force = true,
        )?.let { delta -> sendRawMouseMove(delta.dx, delta.dy) }
        accumulator.reset()
    }

    fun setVirtualControllerVisible(visible: Boolean) {
        if (virtualControllerVisible == visible) return
        virtualControllerVisible = visible
        sendCurrentGamepadState()
    }

    fun setTouchMouseEnabled(enabled: Boolean) {
        touchMouseEnabled = enabled
    }

    private fun startTransport(session: SessionInfo, settings: StreamSettings, generation: Int) {
        inputDropLogged = false
        lastIceState = null
        lastStatsSample = null
        packetLossWindow.reset()
        packetLossRecoveryGate.reset()
        transportHasStableMedia = false
        consecutiveTransportProgressSamples = 0
        firstVideoFrameWatchdog.reset()
        emitStats(StreamRuntimeStats())
        audioDeviceModule.setSpeakerMute(audioMuted)
        audioDeviceModule.setMicrophoneMute(
            settings.microphoneMode == MicrophoneMode.Disabled || microphoneMuted,
        )
        recordStreamDiagnostic(
            "transport start generation=$generation reconnectAttempts=$reconnectAttempts session=${streamDiagnosticId(session.sessionId)} iceServers=${session.iceServers.size} media=${session.mediaConnectionInfo?.let { "${it.ip}:${it.port}" } ?: "unknown"}",
        )
        if (settings.experimentalNvst) {
            if (session.rtspsEndpoints.isEmpty()) {
                failStream("The session has no NVST endpoint. Disable experimental NVST to use WebRTC, or launch a new session. The cloud session is retained.", generation)
                return
            }
            emitState("Connecting experimental NVST")
            runCatching {
                NvstTransport(appContext, session, settings,
                    OpenNowVideoDecoderFactory(eglBase.eglBaseContext,
                        nativeLowLatencyDecoderEnabled = SettingsStore(appContext).settings.value.nativeLowLatencyDecoder,
                        requestedFps = { settings.fps }, hdrEnabled = { settings.hdrEnabled },
                        hdrSurface = { renderer?.hdrTarget }),
                    sink = { renderer },
                    event = { kind, detail -> scope.launch {
                        if (generation == transportGeneration) when (kind) {
                            "connected" -> emitState("Waiting for NVST video")
                            "streaming" -> {
                                transportHasStableMedia = true
                                emitState("Streaming")
                            }
                            "input-ready" -> {
                                inputEncoder.setProtocolVersion(detail.toIntOrNull() ?: DEFAULT_INPUT_PROTOCOL_VERSION)
                                inputEncoder.resetGamepadSequences()
                                startInputSessionClock()
                                inputHandshakeReady = true
                                updateHapticsAdvertisement(force = true)
                                startGamepadKeepalive()
                            }
                            "input-unavailable" -> inputHandshakeReady = false
                            "microphone-error" -> recordStreamDiagnostic("NVST microphone failed: $detail")
                            "error" -> {
                                lastNvstError = detail
                                recordStreamDiagnostic("NVST attachment failed: $detail")
                                scheduleTransportReconnect("NVST attachment failed", SIGNALING_RECONNECT_DELAY_MS, generation)
                            }
                        }
                    } },
                    stats = { value -> scope.launch { if (generation == transportGeneration) emitStats(value) } },
                    rumble = { command -> scope.launch {
                        if (generation == transportGeneration && !released) {
                            applyGamepadRumble(command.controllerId, command.weakMagnitude, command.strongMagnitude)
                        }
                    } },
                ).also { transport ->
                    transport.muted = audioMuted
                    transport.setMicrophoneEnabled(!microphoneMuted)
                    nvstTransport = transport
                    enqueueNativeLifecycleOperation("nvst-start") { transport.start() }
                }
            }.onFailure { failStream("NVST startup failed: ${it.javaClass.simpleName}", generation) }
            return
        }
        emitState(if (reconnectAttempts > 0) "Reconnecting signaling" else "Connecting signaling")
        signaling = GfnSignalingClient(session, settings = settings) { event ->
            // OkHttp invokes WebSocket callbacks on its own threads. Keep transport state on the
            // existing main-owner scope; offer parsing and all PeerConnection JNI work are handed
            // to nativeLifecycleExecutor below so this does not add UI-thread SDP work.
            scope.launch { handleSignaling(event, generation) }
        }.also { it.connect() }
    }

    private fun closeTransport(clearInputState: Boolean, cancelRecovery: Boolean = true) {
        val closingNvst = nvstTransport
        closingNvst?.stop()
        nvstTransport = null
        if (peerConnection != null || signaling != null || reliableInput != null || partiallyReliableInput != null) {
            recordStreamDiagnostic("transport close clearInput=$clearInputState cancelRecovery=$cancelRecovery lastIce=${lastIceState?.name ?: "none"}")
        }
        if (cancelRecovery) {
            iceRecoveryJob?.cancel()
            iceRecoveryJob = null
        }
        if (heartbeatJob != null) {
            NativeInputDiagnostics.retain(
                "heartbeat.input.lifecycle",
                "input heartbeat stopped generation=$transportGeneration",
            )
            heartbeatJob?.cancel()
        }
        gamepadKeepaliveJob?.cancel()
        statsJob?.cancel()
        offerTimeoutJob?.cancel()
        heartbeatJob = null
        gamepadKeepaliveJob = null
        statsJob = null
        offerTimeoutJob = null
        resetMouseMoveBurstLimiter()
        resetGamepadStateBurstLimiter()
        lastStatsSample = null
        packetLossWindow.reset()
        packetLossRecoveryGate.reset()
        decoderRecoveryGate.reset()
        lastIceState = null
        livenessWatchdog.reset()
        val closingSignaling = signaling
        val closingRenderer = renderer
        val closingRendererSinkAttached = rendererSinkLifecycle.requestDetach()
        signaling = null
        reliableInput = null
        partiallyReliableInput = null
        reliableInputState = null
        partiallyReliableInputState = null
        statsChannel = null
        lastParsedGameFps = null
        partiallyReliableGamepadMask = 0
        hidDeviceMask = 0
        partiallyReliableHidMask = 0
        inputHandshakeReady = false
        hapticsAdvertised = null
        lastHapticsAdvertisementAtMs = 0L
        if (clearInputState) resetInputState()
        enqueueNativeLifecycleOperation("transport-close") {
            closingNvst?.awaitStopped()
            remoteIceCandidates.clear()
            // Peer creation, SDP/ICE/stats JNI calls, and this detach/dispose step share this one
            // executor. Capturing the peer here (instead of on the caller thread) closes the race
            // where a stale offer could create a peer after teardown had already captured null.
            val closingMicrophone = takeMicrophoneResources()
            val closingPeerConnection = peerConnection
            val closingVideoTrack = videoTrack
            peerConnection = null
            videoTrack = null
            audioTrack = null
            // Repeat the fast caller-thread detach after all earlier native operations. A callback
            // already running on this executor may have attached a channel just before close was
            // queued; clearing again prevents that stale wrapper from escaping this generation.
            reliableInput = null
            partiallyReliableInput = null
            reliableInputState = null
            partiallyReliableInputState = null
            statsChannel = null
            lastParsedGameFps = null
            partiallyReliableGamepadMask = 0
            hidDeviceMask = 0
            partiallyReliableHidMask = 0
            inputHandshakeReady = false
            runCatching { closingSignaling?.disconnect() }
                .onFailure { error -> recordStreamDiagnostic("signaling disconnect failed error=${error.message.orEmpty()}") }
            if (closingRendererSinkAttached && closingRenderer != null) {
                runCatching { closingVideoTrack?.removeSink(closingRenderer) }
                    .onFailure { error -> recordStreamDiagnostic("video sink detach failed error=${error.message.orEmpty()}") }
            }
            runCatching { disposeMicrophoneResources(closingMicrophone) }
                .onFailure { error -> recordStreamDiagnostic("microphone release failed error=${error.message.orEmpty()}") }
            runCatching { closingPeerConnection?.close() }
                .onFailure { error -> recordStreamDiagnostic("peer close failed error=${error.message.orEmpty()}") }
            runCatching { closingPeerConnection?.dispose() }
                .onFailure { error -> recordStreamDiagnostic("peer dispose failed error=${error.message.orEmpty()}") }
        }
    }

    private fun handleSignaling(event: SignalingEvent, generation: Int) {
        if (generation != transportGeneration) return
        when (event) {
            SignalingEvent.Connected -> {
                transientSignalingFailures = 0
                recordStreamDiagnostic("signaling connected generation=$generation")
                emitState("Waiting for offer")
                startOfferTimeout(generation)
            }
            is SignalingEvent.Disconnected -> {
                recordStreamDiagnostic("signaling disconnected ${event.reason}")
                // A clean WebSocket close is never proof that the allocated cloud session ended.
                // With packet loss or high RTT the signaling socket can close while media remains
                // healthy or independently reconnectable. Even an explicit 410 is probed through
                // session recovery before the local client leaves the stream.
                val disposition = signalingFailureDisposition(event.reason)
                if (shouldPreserveMediaAfterSignalingFailure(disposition, lastIceState)) {
                    recordStreamDiagnostic(
                        "signaling disconnected while ICE=${lastIceState?.name}; preserving active media transport",
                    )
                    return
                }
                when (disposition) {
                    SignalingFailureDisposition.SessionEnded -> {
                        recordStreamDiagnostic("Signaling reported a terminal session response; verifying allocated session before local exit.")
                        requestSessionRecovery("The provider reported that the signaling session ended.")
                    }
                    SignalingFailureDisposition.RecoverSession -> {
                        recordStreamDiagnostic("Signaling endpoint is stale. Recovering cloud session.")
                        requestSessionRecovery("Signaling endpoint became unavailable while connecting to the cloud session.")
                    }
                    SignalingFailureDisposition.RetrySignaling ->
                        scheduleTransientSignalingRetry(event.reason, generation)
                    SignalingFailureDisposition.RetryTransport ->
                        scheduleTransportReconnect("Signaling disconnected: ${event.reason}", SIGNALING_RECONNECT_DELAY_MS, generation)
                }
            }
            is SignalingEvent.Error -> {
                recordStreamDiagnostic("signaling error ${event.message}")
                val disposition = signalingFailureDisposition(event.message)
                if (shouldPreserveMediaAfterSignalingFailure(disposition, lastIceState)) {
                    recordStreamDiagnostic(
                        "signaling error while ICE=${lastIceState?.name}; preserving active media transport",
                    )
                    return
                }
                when (disposition) {
                    SignalingFailureDisposition.SessionEnded -> {
                        recordStreamDiagnostic("Signaling error reported a terminal session response; verifying allocated session before local exit.")
                        requestSessionRecovery("The provider reported that the signaling session ended.")
                    }
                    SignalingFailureDisposition.RecoverSession -> {
                        recordStreamDiagnostic("Signaling endpoint is stale. Recovering cloud session.")
                        requestSessionRecovery("Signaling endpoint became unavailable while connecting to the cloud session.")
                    }
                    SignalingFailureDisposition.RetrySignaling ->
                        scheduleTransientSignalingRetry(event.message, generation)
                    SignalingFailureDisposition.RetryTransport ->
                        scheduleTransportReconnect("Signaling failed: ${event.message}", SIGNALING_RECONNECT_DELAY_MS, generation)
                }
            }
            is SignalingEvent.Log -> recordStreamDiagnostic(event.message)
            is SignalingEvent.RemoteIce -> {
                enqueueNativeLifecycleOperation("remote-ice") remoteIce@ {
                    if (generation != transportGeneration) return@remoteIce
                    remoteIceCandidates.receive(event.candidate) { candidate ->
                        addRemoteIceCandidate(candidate, generation)
                    }
                }
            }
            is SignalingEvent.Offer -> {
                offerTimeoutJob?.cancel()
                offerTimeoutJob = null
                enqueueNativeLifecycleOperation("remote-offer") {
                    handleOffer(event.sdp, generation)
                }
            }
        }
    }

    /** Runs on [nativeLifecycleExecutor], preserving native handle ownership through SDP setup. */
    private fun addRemoteIceCandidate(candidate: IceCandidate, generation: Int) {
        val pc = activePeerConnection(generation) ?: return
        val added = pc.addIceCandidate(candidate)
        recordStreamDiagnostic(
            "remote ICE add requested accepted=$added pcReady=true ${candidate.diagnosticSummary()}",
        )
    }

    /** Runs on [nativeLifecycleExecutor], preserving native handle ownership through SDP setup. */
    private fun handleOffer(rawOffer: String, generation: Int) {
        if (generation != transportGeneration) return
        val currentSession = session ?: return
        recordStreamDiagnostic(sdpDiagnosticSummary("raw offer", rawOffer))
        val fixed = prepareRemoteOffer(rawOffer, currentSession)
        val preferred = SdpTools.preferCodec(fixed, settings)
        if (fixed != rawOffer) {
            recordStreamDiagnostic(sdpDiagnosticSummary("fixed offer", fixed))
        }
        if (preferred != fixed) {
            recordStreamDiagnostic(sdpDiagnosticSummary("preferred offer", preferred))
        }
        val pc = ensurePeerConnection(currentSession, generation)
        if (activePeerConnection(generation, pc) == null) return
        ensureInputDataChannels(pc, preferred)
        inputEncoder.setProtocolVersion(SdpTools.parseInputProtocolVersion(preferred))
        partiallyReliableGamepadMask = SdpTools.parsePartiallyReliableGamepadMask(preferred)
        hidDeviceMask = SdpTools.parseHidDeviceMask(preferred)
        partiallyReliableHidMask = SdpTools.parsePartiallyReliableHidMask(preferred)
        recordStreamDiagnostic(
            "offer input protocol=${SdpTools.parseInputProtocolVersion(preferred)} " +
                "partialGamepadMask=$partiallyReliableGamepadMask " +
                "hidMask=${hidDeviceMask.toUInt()} partialHidMask=${partiallyReliableHidMask.toUInt()} " +
                "mouseMoveTransport=relative-captured-absolute-uncaptured",
        )
        pc.setRemoteDescription(
            object : SimpleSdpObserver() {
                override fun onSetSuccess() {
                    enqueueNativeLifecycleOperation("remote-description-set") remoteDescription@ {
                        val active = activePeerConnection(generation, pc) ?: return@remoteDescription
                        recordStreamDiagnostic("remote description set")
                        remoteIceCandidates.onRemoteDescriptionSet { candidate ->
                            addRemoteIceCandidate(candidate, generation)
                        }
                        // WebRTC disposes previously returned transceiver wrappers whenever
                        // getTransceivers() refreshes its cache. Share one snapshot so the
                        // microphone sender remains valid through transport teardown.
                        val transceivers = active.transceivers
                        applyVideoCodecPreferences(transceivers)
                        attachMicrophoneTrack(active, transceivers)
                        active.createAnswer(
                            object : SimpleSdpObserver() {
                                override fun onCreateSuccess(description: SessionDescription?) {
                                    enqueueNativeLifecycleOperation("answer-created") answerCreated@ {
                                        val answerPeer = activePeerConnection(generation, pc)
                                            ?: return@answerCreated
                                        val rawDescription = description ?: run {
                                            dispatchPeerFailure(
                                                diagnostic = "answer create returned empty description",
                                                message = "WebRTC returned an empty answer",
                                                generation = generation,
                                                expected = pc,
                                            )
                                            return@answerCreated
                                        }
                                        val munged = SdpTools.mungeAnswerSdp(
                                            rawDescription.description,
                                            settings.maxBitrateMbps * 1000,
                                        )
                                        recordStreamDiagnostic(sdpDiagnosticSummary("created answer", munged))
                                        if (
                                            settings.codec != VideoCodec.H264 &&
                                            !SdpTools.negotiatesCodec(munged, settings.codec)
                                        ) {
                                            scope.launch {
                                                if (!isCurrentPeerOperation(generation, transportGeneration, pc, peerConnection)) {
                                                    return@launch
                                                }
                                                NativeInputDiagnostics.add(
                                                    "local answer did not negotiate requested codec=${settings.codec}; retrying selected profile",
                                                )
                                                if (
                                                    requestSelectedVideoProfileRetry(
                                                        message = "${settings.codec} was requested but WebRTC did not negotiate it; retrying the selected profile",
                                                        diagnosticReason = "codec negotiation",
                                                    )
                                                ) {
                                                    return@launch
                                                }
                                                failStream(
                                                    "${settings.codec} requested but not negotiated in local SDP",
                                                    generation,
                                                )
                                            }
                                            return@answerCreated
                                        }
                                        val answer = SessionDescription(SessionDescription.Type.ANSWER, munged)
                                        answerPeer.setLocalDescription(
                                            object : SimpleSdpObserver() {
                                                override fun onSetSuccess() {
                                                    enqueueNativeLifecycleOperation("local-description-set") localDescription@ {
                                                        if (activePeerConnection(generation, pc) == null) {
                                                            return@localDescription
                                                        }
                                                        val nvst = SdpTools.buildNvstSdp(
                                                            offerSdp = preferred,
                                                            settings = settings,
                                                            localAnswer = munged,
                                                        )
                                                        scope.launch {
                                                            if (
                                                                !isCurrentPeerOperation(
                                                                    generation,
                                                                    transportGeneration,
                                                                    pc,
                                                                    peerConnection,
                                                                )
                                                            ) {
                                                                return@launch
                                                            }
                                                            signaling?.sendAnswer(munged, nvst)
                                                            recordStreamDiagnostic("local description set and answer sent")
                                                            emitState("Streaming")
                                                            startHeartbeat()
                                                            startGamepadKeepalive()
                                                            startStatsPolling()
                                                        }
                                                    }
                                                }

                                                override fun onSetFailure(error: String?) {
                                                    dispatchPeerFailure(
                                                        diagnostic = "local description failed error=${error.orEmpty()}",
                                                        message = error ?: "Failed to set local description",
                                                        generation = generation,
                                                        expected = pc,
                                                    )
                                                }
                                            },
                                            answer,
                                        )
                                    }
                                }

                                override fun onCreateFailure(error: String?) {
                                    dispatchPeerFailure(
                                        diagnostic = "answer create failed error=${error.orEmpty()}",
                                        message = error ?: "Failed to create WebRTC answer",
                                        generation = generation,
                                        expected = pc,
                                    )
                                }
                            },
                            MediaConstraints(),
                        )
                    }
                }

                override fun onSetFailure(error: String?) {
                    dispatchPeerFailure(
                        diagnostic = "remote description failed error=${error.orEmpty()}",
                        message = error ?: "Failed to apply server offer",
                        generation = generation,
                        expected = pc,
                    )
                }
            },
            SessionDescription(SessionDescription.Type.OFFER, preferred),
        )
    }

    private fun prepareRemoteOffer(rawOffer: String, session: SessionInfo): String {
        var prepared = SdpTools.fixServerEndpoint(rawOffer, session.serverIp, session.mediaConnectionInfo)
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

    private fun applyVideoCodecPreferences(transceivers: List<RtpTransceiver>) {
        val preferences = receiverCodecPreferences(settings.codec)
        if (preferences.isEmpty()) return
        val transceiver = transceivers.firstOrNull {
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

    @Synchronized
    private fun attachMicrophoneTrack(
        pc: PeerConnection,
        transceivers: List<RtpTransceiver>,
    ) {
        val permissionGranted = ContextCompat.checkSelfPermission(
            appContext,
            Manifest.permission.RECORD_AUDIO,
        ) == PackageManager.PERMISSION_GRANTED
        if (!shouldCaptureMicrophone(settings.microphoneMode, permissionGranted)) {
            recordStreamDiagnostic(
                "microphone not attached mode=${settings.microphoneMode.name} permission=$permissionGranted",
            )
            return
        }

        releaseMicrophoneTrack()
        val audioConstraints = MediaConstraints().apply {
            optional.add(MediaConstraints.KeyValuePair("googEchoCancellation", "true"))
            optional.add(MediaConstraints.KeyValuePair("googAutoGainControl", "true"))
            optional.add(MediaConstraints.KeyValuePair("googNoiseSuppression", "true"))
            optional.add(MediaConstraints.KeyValuePair("googHighpassFilter", "true"))
        }
        val source = requireNotNull(factory).createAudioSource(audioConstraints)
        val track = requireNotNull(factory).createAudioTrack(MICROPHONE_TRACK_ID, source)
        track.setEnabled(!microphoneMuted)

        val audioTransceivers = transceivers.filter {
            it.mediaType == MediaStreamTrack.MediaType.MEDIA_TYPE_AUDIO ||
                it.receiver?.track()?.kind() == MediaStreamTrack.AUDIO_TRACK_KIND
        }
        val transceiver = audioTransceivers.firstOrNull { it.mid == GFN_MICROPHONE_MID }
            ?: audioTransceivers.firstOrNull {
                it.direction == RtpTransceiver.RtpTransceiverDirection.SEND_ONLY &&
                    it.sender?.track() == null
            }
            ?: audioTransceivers.firstOrNull { it.sender?.track() == null }

        val sender = if (transceiver != null) {
            when (transceiver.direction) {
                RtpTransceiver.RtpTransceiverDirection.RECV_ONLY ->
                    transceiver.setDirection(RtpTransceiver.RtpTransceiverDirection.SEND_RECV)
                RtpTransceiver.RtpTransceiverDirection.INACTIVE ->
                    transceiver.setDirection(RtpTransceiver.RtpTransceiverDirection.SEND_ONLY)
                else -> Unit
            }
            transceiver.sender.apply {
                setStreams(listOf(MICROPHONE_STREAM_ID))
            }.takeIf { sender ->
                runCatching { sender.setTrack(track, false) }
                    .onFailure { error ->
                        if (error is IllegalStateException && isDisposedRtpSenderFailure(error)) {
                            recordStreamDiagnostic("microphone sender was disposed during setTrack attachment")
                        } else throw error
                    }
                    .getOrDefault(false)
            }
        } else {
            pc.addTrack(track, listOf(MICROPHONE_STREAM_ID))
        }

        if (sender == null) {
            track.dispose()
            source.dispose()
            recordStreamDiagnostic("microphone sender attachment failed")
            return
        }
        microphoneSource = source
        microphoneTrack = track
        microphoneSender = sender
        audioDeviceModule.setMicrophoneMute(microphoneMuted)
        recordStreamDiagnostic(
            "microphone track attached mid=${transceiver?.mid ?: "new"} direction=${transceiver?.direction?.name ?: "new"} muted=$microphoneMuted",
        )
    }

    @Synchronized
    private fun releaseMicrophoneTrack() {
        disposeMicrophoneResources(takeMicrophoneResources())
    }

    @Synchronized
    private fun takeMicrophoneResources(): MicrophoneResources {
        val resources = MicrophoneResources(
            sender = microphoneSender,
            track = microphoneTrack,
            source = microphoneSource,
        )
        microphoneSender = null
        microphoneTrack = null
        microphoneSource = null
        return resources
    }

    private fun disposeMicrophoneResources(resources: MicrophoneResources) {
        try {
            resources.sender?.setTrack(null, false)
        } catch (error: IllegalStateException) {
            if (!isDisposedRtpSenderFailure(error)) throw error
            recordStreamDiagnostic("microphone sender was already disposed during transport close")
        } finally {
            if (resources.track?.isDisposed == false) resources.track.dispose()
            resources.source?.dispose()
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
                requestSelectedVideoProfileRetry(
                    message = "Timed out waiting for video offer; retrying the selected profile",
                    diagnosticReason = "offer timeout",
                )
            ) {
                return@launch
            }
            restartTransport("Timed out waiting for video offer", videoFailure = true)
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
            tcpCandidatePolicy = PeerConnection.TcpCandidatePolicy.ENABLED
            bundlePolicy = PeerConnection.BundlePolicy.MAXBUNDLE
            rtcpMuxPolicy = PeerConnection.RtcpMuxPolicy.REQUIRE
        }
        recordStreamDiagnostic(
            "peer connection create generation=$generation iceServers=${ice.size} iceUrls=${session.iceServers.flatMap { it.urls }.joinToString(limit = 8) { url -> url.substringBefore('?').take(120) }}",
        )
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
                scope.launch {
                    if (generation != transportGeneration) return@launch
                    if (candidate != null) {
                        recordStreamDiagnostic("local ICE candidate gathered ${candidate.diagnosticSummary()}")
                        signaling?.sendIceCandidate(candidate)
                    } else {
                        recordStreamDiagnostic("local ICE candidate gathering complete")
                    }
                }
            }
            override fun onIceCandidatesRemoved(candidates: Array<out IceCandidate>?) {
                recordStreamDiagnostic("ice candidates removed count=${candidates?.size ?: 0}")
            }
            override fun onAddStream(stream: MediaStream?) {
                enqueueNativeLifecycleOperation("media-stream-added") {
                    if (activePeerConnection(generation) == null) return@enqueueNativeLifecycleOperation
                    recordStreamDiagnostic("media stream added video=${stream?.videoTracks?.size ?: 0} audio=${stream?.audioTracks?.size ?: 0}")
                    stream?.videoTracks?.firstOrNull()?.let(::attachVideo)
                    stream?.audioTracks?.firstOrNull()?.let {
                        audioTrack = it
                        it.setEnabled(!audioMuted)
                    }
                }
            }
            override fun onRemoveStream(stream: MediaStream?) {
                recordStreamDiagnostic("media stream removed video=${stream?.videoTracks?.size ?: 0} audio=${stream?.audioTracks?.size ?: 0}")
            }
            override fun onDataChannel(channel: DataChannel?) {
                enqueueNativeLifecycleOperation("data-channel-added") {
                    if (activePeerConnection(generation) == null) return@enqueueNativeLifecycleOperation
                    if (channel != null) attachDataChannel(channel)
                }
            }
            override fun onRenegotiationNeeded() {
                recordStreamDiagnostic("renegotiation needed")
            }
            override fun onAddTrack(receiver: RtpReceiver?, streams: Array<out MediaStream>?) {
                enqueueNativeLifecycleOperation("receiver-track-added") {
                    if (activePeerConnection(generation) == null) return@enqueueNativeLifecycleOperation
                    val track = receiver?.track()
                    recordStreamDiagnostic("track added kind=${track?.kind().orEmpty()} streams=${streams?.size ?: 0}")
                    if (track is VideoTrack) attachVideo(track)
                    if (track is AudioTrack) {
                        audioTrack = track
                        track.setEnabled(!audioMuted)
                    }
                }
            }
            override fun onTrack(transceiver: RtpTransceiver?) {
                enqueueNativeLifecycleOperation("transceiver-track-added") {
                    if (activePeerConnection(generation) == null) return@enqueueNativeLifecycleOperation
                    val track = transceiver?.receiver?.track()
                    recordStreamDiagnostic("transceiver track kind=${track?.kind().orEmpty()} media=${transceiver?.mediaType?.name ?: "unknown"}")
                    if (track is VideoTrack) attachVideo(track)
                    if (track is AudioTrack) {
                        audioTrack = track
                        track.setEnabled(!audioMuted)
                    }
                }
            }
        }) ?: error("Failed to create PeerConnection")
        peerConnection = pc
        recordStreamDiagnostic("peer connection ready generation=$generation")
        return pc
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
                    livenessWatchdog.markConnected(SystemClock.elapsedRealtime())
                    if (reconnectAttempts > 0) {
                        signaling?.requestKeyframe(
                            reason = "transport_reconnect",
                            backlogFrames = 0,
                            attempt = reconnectAttempts,
                        )
                        recordStreamDiagnostic("reconnect keyframe requested attempt=$reconnectAttempts generation=$generation")
                    }
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

    private fun scheduleTransientSignalingRetry(message: String, generation: Int) {
        if (generation != transportGeneration || iceRecoveryJob?.isActive == true) {
            recordStreamDiagnostic(
                "signaling service retry not scheduled generation=$generation activeJob=${iceRecoveryJob?.isActive == true}",
            )
            return
        }
        transientSignalingFailures += 1
        val failureCount = transientSignalingFailures
        val delayMs = transientSignalingRetryDelayMs(failureCount)
        if (delayMs == null) {
            recordStreamDiagnostic("signaling service retry limit reached failures=$failureCount")
            requestSessionRecovery(
                "The signaling service stayed unavailable after ${failureCount - 1} retries.",
            )
            return
        }
        recordStreamDiagnostic(
            "signaling service retry scheduled failure=$failureCount/$MAX_TRANSIENT_SIGNALING_RETRIES " +
                "delayMs=$delayMs generation=$generation",
        )
        iceRecoveryJob = scope.launch {
            delay(delayMs)
            if (generation != transportGeneration) return@launch
            restartTransport(
                reason = "Signaling service unavailable: $message",
                consumeReconnectAttempt = false,
            )
        }
    }

    private fun restartTransport(
        reason: String,
        videoFailure: Boolean = false,
        consumeReconnectAttempt: Boolean = true,
    ) {
        val currentSession = session ?: return
        val currentSettings = settings
        val hadStableMedia = transportHasStableMedia
        if (
            transportRestartShouldRetrySelectedProfile(
                videoFailure = videoFailure,
                reconnectAttempts = reconnectAttempts,
                transportHasStableMedia = transportHasStableMedia,
            ) &&
            requestSelectedVideoProfileRetry(
                message = "$reason. Retrying the selected local transport profile.",
                diagnosticReason = "transport reconnect",
            )
        ) {
            return
        }
        if (consumeReconnectAttempt && reconnectAttempts >= MAX_TRANSPORT_RECONNECT_ATTEMPTS) {
            recordStreamDiagnostic("reconnect limit reached reason=$reason attempts=$reconnectAttempts")
            if (currentSettings.experimentalNvst) {
                // A successful CloudMatch ready poll does not prove media recovered. Reclaiming
                // here recreates this client and resets the retry budget indefinitely.
                failStream("${lastNvstError ?: reason}. NVST could not reconnect after $MAX_TRANSPORT_RECONNECT_ATTEMPTS retries. Your cloud session is still running; retry when ready.")
            } else {
                requestSessionRecovery("$reason. Stream reconnect failed after $MAX_TRANSPORT_RECONNECT_ATTEMPTS attempts.")
            }
            return
        }
        if (consumeReconnectAttempt) reconnectAttempts += 1
        transportGeneration += 1
        val generation = transportGeneration
        recordStreamDiagnostic(
            "transport restart reason=$reason attempt=$reconnectAttempts " +
                "signalingFailures=$transientSignalingFailures generation=$generation",
        )
        if (consumeReconnectAttempt) {
            emitState("Reconnecting stream ($reconnectAttempts/$MAX_TRANSPORT_RECONNECT_ATTEMPTS)")
        } else {
            emitState("Reconnecting signaling ($transientSignalingFailures/$MAX_TRANSIENT_SIGNALING_RETRIES)")
        }
        closeTransport(clearInputState = false, cancelRecovery = false)
        val codecSettleDelayMs = advancedCodecRestartSettleDelayMs(
            codec = currentSettings.codec,
            hadStableMedia = hadStableMedia,
        )
        if (codecSettleDelayMs == 0L) {
            iceRecoveryJob = null
            startTransport(currentSession, currentSettings, generation)
            return
        }
        recordStreamDiagnostic(
            "waiting ${codecSettleDelayMs}ms for ${currentSettings.codec} decoder release before transport restart generation=$generation",
        )
        iceRecoveryJob = scope.launch {
            delay(codecSettleDelayMs)
            if (generation != transportGeneration || session?.sessionId != currentSession.sessionId) return@launch
            iceRecoveryJob = null
            startTransport(currentSession, currentSettings, generation)
        }
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
        if (statsChannel == null) {
            val statsInit = DataChannel.Init().apply {
                ordered = false
                maxRetransmits = 0
            }
            pc.createDataChannel("stats_channel", statsInit)?.let(::attachDataChannel)
        }
    }

    private fun attachVideo(track: VideoTrack) {
        val currentTrack = videoTrack
        if (currentTrack != null && currentTrack.id() == track.id() && currentTrack.state() != MediaStreamTrack.State.ENDED) {
            currentTrack.setEnabled(true)
            renderer?.let(::attachRendererSinkIfAvailable)
            return
        }
        renderer?.let(::detachRendererSink)
        videoTrack = track
        track.setEnabled(true)
        renderer?.let(::attachRendererSinkIfAvailable)
        recordStreamDiagnostic(
            "video track attached id=${track.id()} state=${track.state()?.name ?: "unknown"} " +
                "renderer=${renderer != null} sink=${rendererSinkLifecycle.isAttachRequested()}",
        )
    }

    private fun attachDataChannel(channel: DataChannel) {
        val label = channel.label()
        val normalizedLabel = label.lowercase(Locale.US)
        val role = InputDataChannelLabels.classify(label)
        val initialState = channel.state()
        NativeInputDiagnostics.addRetained(
            key = "channel.$normalizedLabel",
            message = "data channel attached label=$normalizedLabel role=$role state=$initialState",
        )
        if (normalizedLabel == "stats_channel") {
            statsChannel = channel
            channel.registerObserver(object : DataChannel.Observer {
                override fun onBufferedAmountChange(previousAmount: Long) = Unit
                override fun onStateChange() {
                    NativeInputDiagnostics.add("stats channel state label=$normalizedLabel state=${channel.state()}")
                }
                override fun onMessage(buffer: DataChannel.Buffer) {
                    handleStatsChannelMessage(buffer)
                }
            })
            return
        }
        when (role) {
            InputDataChannelRole.Reliable -> {
                reliableInput = channel
                reliableInputState = initialState
            }
            InputDataChannelRole.PartiallyReliable -> {
                partiallyReliableInput = channel
                partiallyReliableInputState = initialState
            }
            InputDataChannelRole.Other -> return
        }
        channel.registerObserver(object : DataChannel.Observer {
            override fun onBufferedAmountChange(previousAmount: Long) = Unit
            override fun onStateChange() {
                val state = channel.state()
                when (role) {
                    InputDataChannelRole.Reliable -> if (reliableInput === channel) reliableInputState = state
                    InputDataChannelRole.PartiallyReliable -> if (partiallyReliableInput === channel) partiallyReliableInputState = state
                    InputDataChannelRole.Other -> Unit
                }
                NativeInputDiagnostics.addRetained(
                    key = "channel.$normalizedLabel",
                    message = "input channel state label=$normalizedLabel role=$role state=$state",
                )
                if (state == DataChannel.State.OPEN) {
                    inputDropLogged = false
                    NativeInputDiagnostics.add("input channel open label=$normalizedLabel")
                }
            }
            override fun onMessage(buffer: DataChannel.Buffer) {
                handleInputChannelMessage(buffer)
            }
        })
    }

    private fun handleStatsChannelMessage(buffer: DataChannel.Buffer) {
        val data = buffer.data.duplicate()
        val size = data.remaining()
        if (size <= 0) return

        val firstByte = data.get(data.position()).toInt() and 0xff
        val statsBuffer = when (firstByte) {
            3 -> {
                if (size < 2) return
                data.position(data.position() + 1)
                data.slice().order(ByteOrder.LITTLE_ENDIAN)
            }
            4 -> {
                data.order(ByteOrder.LITTLE_ENDIAN)
            }
            else -> return
        }

        val statsSize = statsBuffer.remaining()
        if (statsSize < 33) return

        val version = statsBuffer.get(0).toInt() and 0xff
        if (version >= 4) {
            val avgGameFps = statsBuffer.getDouble(25)
            if (avgGameFps > 0.0 && avgGameFps <= 360.0) {
                lastParsedGameFps = kotlin.math.round(avgGameFps).toInt()
            }
        }
    }

    private fun handleInputChannelMessage(buffer: DataChannel.Buffer) {
        val bytes = buffer.data.duplicate().let { data ->
            ByteArray(data.remaining()).also(data::get)
        }
        if (bytes.isEmpty()) return
        if (handleInputHandshakeMessage(ByteBuffer.wrap(bytes).order(ByteOrder.LITTLE_ENDIAN))) return
        HapticsPacketParser.parse(bytes)?.let { command ->
            applyGamepadRumble(command.controllerId, command.weakMagnitude, command.strongMagnitude)
        }
    }

    private fun handleInputHandshakeMessage(data: ByteBuffer): Boolean {
        val size = data.remaining()
        val firstWord = if (size >= 2) {
            data.getShort(0).toInt() and 0xffff
        } else {
            data.get(0).toInt() and 0xff
        }
        val version = when {
            firstWord == INPUT_HANDSHAKE_MAGIC_WORD -> {
                if (size >= 4) {
                    data.getShort(2).toInt() and 0xffff
                } else {
                    DEFAULT_INPUT_PROTOCOL_VERSION
                }
            }
            (data.get(0).toInt() and 0xff) == INPUT_HANDSHAKE_MARKER -> firstWord
            else -> return false
        }.coerceAtLeast(1)

        inputEncoder.setProtocolVersion(version)
        inputEncoder.resetGamepadSequences()
        startInputSessionClock()
        inputHandshakeReady = true
        // Pre-handshake sends are intentionally blocked now. Re-arm the one-shot diagnostics so
        // the first protocol-ready mouse packet records its v3 size and negotiated transport.
        externalMouseMoveSentLogged = false
        externalMouseCapturedMoveSentLogged = false
        workerInputSendConfirmed.set(false)
        directInputSendConfirmed.set(false)
        NativeInputDiagnostics.addRetained(
            key = "protocol",
            message = "input handshake protocol=$version bytes=$size",
        )
        updateHapticsAdvertisement(force = true)
        schedulePrimeConnectedGamepadState(reason = "input handshake")
        return true
    }

    private fun startHeartbeat() {
        heartbeatJob?.cancel()
        NativeInputDiagnostics.retain(
            "heartbeat.input.lifecycle",
            "input heartbeat active intervalMs=1000 generation=$transportGeneration",
        )
        heartbeatJob = scope.launch {
            while (true) {
                delay(1000)
                if (!inputHandshakeReady) continue
                val usePartialFallback =
                    reliableInputState != DataChannel.State.OPEN &&
                        partiallyReliableInputState == DataChannel.State.OPEN
                sendInput(
                    bytes = inputEncoder.encodeHeartbeat(),
                    partiallyReliable = usePartialFallback,
                    fallbackToReliable = !usePartialFallback,
                    resultDiagnosticKey = "heartbeat.input",
                )
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
                if (
                    shouldSendGamepadKeepalive(
                        hasControllerState = hasAnyControllerState(),
                        hasActiveControllerInput = hasActiveControllerInput(),
                        touchMouseEnabled = touchMouseEnabled,
                    )
                ) {
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
        val generation = transportGeneration
        enqueueNativeLifecycleOperation("runtime-stats") {
            val pc = activePeerConnection(generation) ?: return@enqueueNativeLifecycleOperation
            pc.getStats(RTCStatsCollectorCallback { report ->
                if (generation != transportGeneration) return@RTCStatsCollectorCallback
                val cpuSample = processCpuSampler.sample()
                cpuSample?.let(ProcessCpuDiagnostics::record)
                val snapshot = buildRuntimeStatsSnapshot(
                    timestampMs = report.timestampUs / 1000.0,
                    stats = report.statsMap.values,
                    cpuSample = cpuSample,
                ) ?: return@RTCStatsCollectorCallback
                scope.launch {
                    if (generation != transportGeneration) return@launch
                    handleMediaLiveness(snapshot)
                    onStats(snapshot.stats)
                }
            })
        }
    }

    @Synchronized
    private fun buildRuntimeStatsSnapshot(
        timestampMs: Double,
        stats: Collection<RTCStats>,
        cpuSample: ProcessCpuUsageSample?,
    ): RuntimeStatsSnapshot? {
        if (!isNewerStreamStatsSample(timestampMs, lastStatsSample?.atMs)) {
            recordStreamDiagnostic("stale runtime stats ignored timestampMs=$timestampMs previousMs=${lastStatsSample?.atMs}")
            return null
        }
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
        val framesReceived = members["framesReceived"].statsLong()
        val framesDecoded = members["framesDecoded"].statsLong()
        val explicitFps = members["framesPerSecond"].statsDouble()
        val width = members["frameWidth"].statsLong()
        val height = members["frameHeight"].statsLong()
        val totalDecodeTime = members["totalDecodeTime"].statsDouble() ?: 0.0
        val jitterMs = members["jitter"].statsDouble()?.let { (it * 1000.0).coerceAtLeast(0.0) }
        val packetsLost = members["packetsLost"].statsLong() ?: 0L
        val packetsReceived = members["packetsReceived"].statsLong() ?: 0L

        val previous = lastStatsSample?.takeIf { it.inboundRtpId == inboundVideo?.id }
        if (lastStatsSample != null && previous == null) {
            packetLossWindow.reset()
        }
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
        val receivedFps = if (previous != null && framesReceived != null && elapsedSeconds != null) {
            ((framesReceived - previous.framesReceived).coerceAtLeast(0) / elapsedSeconds).roundToInt()
        } else {
            null
        }

        val decodeMs = if (previous != null && framesDecoded != null && framesDecoded > previous.framesDecoded) {
            val deltaDecodeTime = totalDecodeTime - previous.totalDecodeTime
            val deltaFrames = framesDecoded - previous.framesDecoded
            if (deltaFrames > 0) {
                (deltaDecodeTime / deltaFrames * 1000.0).coerceIn(0.1, 50.0)
            } else {
                null
            }
        } else {
            null
        }

        val packetDelta = previous?.let {
            streamPacketDelta(
                currentLost = packetsLost,
                currentReceived = packetsReceived,
                previousLost = it.packetsLost,
                previousReceived = it.packetsReceived,
            )
        }
        if (previous != null && packetDelta == null) {
            packetLossWindow.reset()
        }
        val packetLossPct = packetDelta?.let(packetLossWindow::add)
        val packetsLostDelta = packetDelta?.lost
        val packetsReceivedDelta = packetDelta?.received

        if (inboundVideo != null && (bytesReceived != null || framesDecoded != null)) {
            lastStatsSample = StreamStatsSample(
                inboundRtpId = inboundVideo.id,
                atMs = timestampMs,
                bytesReceived = bytesReceived ?: previous?.bytesReceived ?: 0L,
                framesReceived = framesReceived ?: previous?.framesReceived ?: 0L,
                framesDecoded = framesDecoded ?: previous?.framesDecoded ?: 0L,
                totalDecodeTime = totalDecodeTime,
                packetsLost = packetsLost,
                packetsReceived = packetsReceived,
            )
        }

        val pingMs = activePair?.members?.get("currentRoundTripTime")
            .statsDouble()
            ?.let { (it * 1000.0).roundToInt().coerceAtLeast(0) }
        val availableIncomingBitrateKbps = activePair?.members?.get("availableIncomingBitrate")
            .statsDouble()
            ?.takeIf { it >= 0.0 }
            ?.let { (it / 1000.0).roundToInt().coerceAtLeast(0) }
        val resolution = if (width != null && height != null && width > 0 && height > 0) {
            "${width}x$height"
        } else {
            null
        }

        return RuntimeStatsSnapshot(
            stats = StreamRuntimeStats(
                bitrateKbps = bitrateKbps,
                availableIncomingBitrateKbps = availableIncomingBitrateKbps,
                pingMs = pingMs,
                fps = explicitFps?.roundToInt()?.takeIf { it > 0 } ?: derivedFps?.takeIf { it > 0 },
                gameFps = lastParsedGameFps,
                receivedFps = receivedFps?.takeIf { it > 0 },
                decodedFps = derivedFps?.takeIf { it > 0 },
                resolution = resolution,
                codec = codec,
                decodeMs = decodeMs,
                jitterMs = jitterMs,
                packetLossPct = packetLossPct,
                packetsLostDelta = packetsLostDelta,
                packetsReceivedDelta = packetsReceivedDelta,
                processCpuPercent = cpuSample?.processCpuPercent,
                deviceCpuCapacityPercent = cpuSample?.deviceCpuCapacityPercent,
                cpuLogicalCoreCount = cpuSample?.logicalCoreCount,
            ),
            bytesReceived = bytesReceived,
            framesDecoded = framesDecoded,
        )
    }

    private fun handleMediaLiveness(snapshot: RuntimeStatsSnapshot) {
        val connected = lastIceState == PeerConnection.IceConnectionState.CONNECTED ||
            lastIceState == PeerConnection.IceConnectionState.COMPLETED
        val action = livenessWatchdog.observe(
            SystemClock.elapsedRealtime(),
            snapshot.bytesReceived,
            snapshot.framesDecoded,
            connected,
        )
        updateTransportRecoveryProgress(livenessWatchdog.latestObservationProgressed)
        val requestPostLossKeyframe = packetLossRecoveryGate.observe(
            stats = snapshot.stats,
            recoveryEligible = connected &&
                transportHasStableMedia &&
                iceRecoveryJob?.isActive != true &&
                !sessionRecoveryRequested,
        )
        if (requestPostLossKeyframe && action == StreamLivenessAction.None) {
            signaling?.requestKeyframe(
                reason = "packet_loss_recovered",
                backlogFrames = 0,
                attempt = 1,
            )
            NativeInputDiagnostics.add(
                "post-loss keyframe requested rawLost=${snapshot.stats.packetsLostDelta} " +
                    "rawReceived=${snapshot.stats.packetsReceivedDelta} displayedLoss=${snapshot.stats.packetLossPct}",
            )
        }
        val decoderOverloaded = decoderRecoveryGate.observe(
            stats = snapshot.stats,
            requestedFps = settings.fps,
            advancedCodecActive = settings.codec != VideoCodec.H264,
            recoveryEligible = connected &&
                transportHasStableMedia &&
                rendererSinkLifecycle.isAttachRequested() &&
                iceRecoveryJob?.isActive != true &&
                !sessionRecoveryRequested,
        )
        if (
            decoderOverloaded &&
            requestSelectedVideoProfileRetry(
                message = "The decoder could not sustain ${settings.fps} FPS; retrying the selected profile without changing it",
                diagnosticReason = "sustained decoder overload receivedFps=${snapshot.stats.receivedFps} " +
                    "decodedFps=${snapshot.stats.decodedFps} decodeMs=${snapshot.stats.decodeMs}",
            )
        ) {
            return
        }
        if (
            rendererSinkLifecycle.isAttachRequested() &&
            firstVideoFrameWatchdog.shouldRecover(SystemClock.elapsedRealtime(), snapshot.bytesReceived, connected)
        ) {
            when (firstFrameRecoveryStep(transportHasStableMedia, reconnectAttempts, selectedProfileRetryApplied)) {
                FirstFrameRecoveryStep.RetryRequestedProfile -> {
                    NativeInputDiagnostics.add(
                        "first frame timeout requested profile retry codec=${settings.codec} resolution=${settings.resolution}",
                    )
                    restartTransport("First video frame timed out", videoFailure = true)
                    return
                }
                FirstFrameRecoveryStep.RetrySelectedProfile -> {
                    if (
                        requestSelectedVideoProfileRetry(
                            message = "Video packets arrived but no frame rendered; retrying the selected profile",
                            diagnosticReason = "first frame timeout",
                        )
                    ) {
                        return
                    }
                }
                FirstFrameRecoveryStep.ContinueBoundedTransportRecovery -> Unit
            }
        }
        when (action) {
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
                if (transportHasStableMedia) {
                    stableMediaStallRestarts += 1
                    NativeInputDiagnostics.add(
                        "stable media stall count=$stableMediaStallRestarts codec=${settings.codec} androidTv=$androidTvProfile",
                    )
                }
                if (
                    repeatedStableMediaStallShouldRetrySelectedProfile(
                        androidTvProfile = androidTvProfile,
                        transportCodec = settings.codec,
                        completedStableMediaStallRestarts = stableMediaStallRestarts,
                        selectedProfileRetryApplied = selectedProfileRetryApplied,
                    ) &&
                    requestSelectedVideoProfileRetry(
                        message = "Decoder repeatedly stalled after stable playback; retrying the selected profile",
                        diagnosticReason = "repeated stable media stall",
                    )
                ) {
                    return
                }
                if (
                    !transportHasStableMedia &&
                    requestSelectedVideoProfileRetry(
                        message = "Decoder stalled; retrying the selected profile",
                        diagnosticReason = "media stall",
                    )
                ) {
                    return
                }
                NativeInputDiagnostics.add("media stall transport restart stalledMs=${action.stalledMs}")
                restartTransport("Media stalled for ${action.stalledMs / 1000}s", videoFailure = true)
            }
        }
    }

    private fun updateTransportRecoveryProgress(progressed: Boolean) {
        if (!progressed) {
            consecutiveTransportProgressSamples = 0
            return
        }
        firstVideoFrameWatchdog.markRendered()
        consecutiveTransportProgressSamples += 1
        if (consecutiveTransportProgressSamples < STABLE_TRANSPORT_PROGRESS_SAMPLES) return

        if (!transportHasStableMedia && reconnectAttempts > 0) {
            recordStreamDiagnostic(
                "transport media stable; reconnect budget reset attempts=$reconnectAttempts generation=$transportGeneration",
            )
        }
        transportHasStableMedia = true
        reconnectAttempts = 0
    }

    private fun requestSelectedVideoProfileRetry(
        message: String,
        diagnosticReason: String,
    ): Boolean {
        val currentSession = session ?: return false
        val selectedSettings = settings
        val hadStableMedia = transportHasStableMedia
        if (selectedProfileRetryApplied) return false
        selectedProfileRetryApplied = true
        reconnectAttempts = (reconnectAttempts + 1).coerceAtMost(MAX_TRANSPORT_RECONNECT_ATTEMPTS)
        NativeInputDiagnostics.add(
            "$diagnosticReason selected profile retry codec=${selectedSettings.codec} " +
                "resolution=${selectedSettings.resolution} fps=${selectedSettings.fps} bitrate=${selectedSettings.maxBitrateMbps}",
        )
        transportGeneration += 1
        val generation = transportGeneration
        closeTransport(clearInputState = false)
        firstVideoFrameWatchdog.reset()
        recordStreamDiagnostic(
            "selected profile transport retry generation=$generation " +
                "session=${streamDiagnosticId(currentSession.sessionId)} settings=${selectedSettings.resolution}/" +
                "${selectedSettings.fps}/${selectedSettings.codec}/${selectedSettings.maxBitrateMbps} reason=$diagnosticReason",
        )
        emitState(message)
        val settleDelayMs = advancedCodecRestartSettleDelayMs(selectedSettings.codec, hadStableMedia)
        if (settleDelayMs == 0L) {
            startTransport(currentSession, selectedSettings, generation)
        } else {
            recordStreamDiagnostic(
                "waiting ${settleDelayMs}ms for ${selectedSettings.codec} decoder release before selected profile retry generation=$generation",
            )
            iceRecoveryJob = scope.launch {
                delay(settleDelayMs)
                if (generation != transportGeneration || session?.sessionId != currentSession.sessionId) return@launch
                iceRecoveryJob = null
                startTransport(currentSession, selectedSettings, generation)
            }
        }
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
            NativeInputDiagnostics.addRetained(
                key = "controller.device.$controllerId",
                message = "physical gamepad motion source=${event.source} device=${event.deviceId}:${event.device?.name.orEmpty()} slot=$controllerId",
            )
        }
        physicalControllerConnected = true
        physicalControllerActive = true
        val raw = event.rawGamepadAxes()
        val axes = AndroidGamepadAxisMapping.resolve(raw, event.axisAvailability())
        if (!physicalGamepadAxisLogged) {
            physicalGamepadAxisLogged = true
            NativeInputDiagnostics.addRetained(
                key = "controller.axes.$controllerId",
                message = "physical gamepad axes left=${axes.leftSource} right=${axes.rightSource} hatAsLeft=${axes.hatUsedAsLeftStick} " +
                    "x=${raw.x.formatAxis()} y=${raw.y.formatAxis()} z=${raw.z.formatAxis()} rz=${raw.rz.formatAxis()} " +
                    "rx=${raw.rx.formatAxis()} ry=${raw.ry.formatAxis()} hatX=${raw.hatX.formatAxis()} hatY=${raw.hatY.formatAxis()}",
            )
        }
        val hasAnalogL = event.device?.getMotionRange(MotionEvent.AXIS_LTRIGGER) != null ||
                         event.device?.getMotionRange(MotionEvent.AXIS_BRAKE) != null
        val lt = if (hasAnalogL) {
            max(event.getAxisValue(MotionEvent.AXIS_LTRIGGER), normalizeTriggerAxis(event.getAxisValue(MotionEvent.AXIS_BRAKE)))
        } else {
            if (physicalLeftTriggerButtonPressed) 1f else 0f
        }

        val hasAnalogR = event.device?.getMotionRange(MotionEvent.AXIS_RTRIGGER) != null ||
                         event.device?.getMotionRange(MotionEvent.AXIS_GAS) != null
        val rt = if (hasAnalogR) {
            max(event.getAxisValue(MotionEvent.AXIS_RTRIGGER), normalizeTriggerAxis(event.getAxisValue(MotionEvent.AXIS_GAS)))
        } else {
            if (physicalRightTriggerButtonPressed) 1f else 0f
        }
        val leftScale = radialDeadzoneScale(axes.leftX, axes.leftY)
        val rightScale = radialDeadzoneScale(axes.rightX, axes.rightY)
        val leftX = axes.leftX * leftScale
        val leftY = axes.leftY * leftScale
        val rightX = axes.rightX * rightScale
        val rightY = axes.rightY * rightScale
        physicalHatButtons = if (axes.hatUsedAsLeftStick) 0 else event.hatDpadButtons()
        lastLeftTrigger = normalizeToUint8(lt)
        lastRightTrigger = normalizeToUint8(rt)
        // When left-stick mouse emulation is active, keep both sticks' deflection = 0 so the game
        // receives no stick deflection. The actual motions are forwarded as mouse delta and mouse scroll instead.
        if (controllerMouseEmulationActive) {
            lastLeftStickX = 0
            lastLeftStickY = 0
            lastRightStickX = 0
            lastRightStickY = 0
        } else {
            lastLeftStickX = normalizeToInt16(leftX)
            lastLeftStickY = normalizeToInt16(-leftY)
            lastRightStickX = normalizeToInt16(rightX)
            lastRightStickY = normalizeToInt16(-rightY)
        }
        physicalLeftStickX = leftX
        physicalLeftStickY = leftY
        physicalRightStickX = rightX
        physicalRightStickY = rightY
        val sent = sendBurstLimitedGamepadState(controllerId = controllerId)
        if (
            abs(leftX) > ANALOG_ACTIVITY_THRESHOLD ||
            abs(leftY) > ANALOG_ACTIVITY_THRESHOLD ||
            abs(rightX) > ANALOG_ACTIVITY_THRESHOLD ||
            abs(rightY) > ANALOG_ACTIVITY_THRESHOLD ||
            lt > ANALOG_ACTIVITY_THRESHOLD ||
            rt > ANALOG_ACTIVITY_THRESHOLD
        ) {
            NativeInputDiagnostics.retainThrottled(
                key = "controller.last-analog.$controllerId",
                minimumIntervalMs = ANALOG_DIAGNOSTIC_INTERVAL_MS,
            ) {
                "physical gamepad analog device=${event.deviceId}:${event.device?.name.orEmpty()} slot=$controllerId " +
                    "left=${leftX.formatAxis()},${leftY.formatAxis()} right=${rightX.formatAxis()},${rightY.formatAxis()} " +
                    "triggers=${lt.formatAxis()},${rt.formatAxis()} sources=${axes.leftSource}/${axes.rightSource} " +
                    "sent=$sent ${inputChannelStateSummary()}"
            }
        }
        return sent
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
            if (handleControllerMouseEmulationButton(mask, pressed)) {
                return true
            }
            if (handleControllerMouseButton(mask, pressed)) {
                return true
            }
            physicalButtons = if (pressed) physicalButtons or mask else physicalButtons and mask.inv()
            val steamOverlayChordActivated = physicalSteamOverlayChord.update(physicalButtons)
            val sent = sendCurrentGamepadState(controllerId = activeControllerId)
            updateGuideAutoRelease(mask, pressed, activeControllerId)
            if (steamOverlayChordActivated) {
                schedulePhysicalSteamOverlayChordRelease(activeControllerId)
            }
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
                val hasAnalogTrigger = event.device?.getMotionRange(MotionEvent.AXIS_LTRIGGER) != null ||
                                       event.device?.getMotionRange(MotionEvent.AXIS_BRAKE) != null
                if (!hasAnalogTrigger) {
                    lastLeftTrigger = if (pressed) 255 else 0
                }
                val mouseSent = handleControllerMouseTrigger(left = true, pressed = pressed)
                if (mouseSent) {
                    return true
                }
                return if (!hasAnalogTrigger) {
                    sendCurrentGamepadState(controllerId = activeControllerId)
                } else {
                    true
                }
            }
            KeyEvent.KEYCODE_BUTTON_R2 -> {
                activeControllerId = controllerIdFor(event)
                if (!physicalControllerActive) {
                    NativeInputDiagnostics.add("physical gamepad key source=${event.source} device=${event.deviceId} slot=$activeControllerId key=${event.keyCode}")
                }
                physicalControllerConnected = true
                physicalControllerActive = true
                physicalRightTriggerButtonPressed = pressed
                val hasAnalogTrigger = event.device?.getMotionRange(MotionEvent.AXIS_RTRIGGER) != null ||
                                       event.device?.getMotionRange(MotionEvent.AXIS_GAS) != null
                if (!hasAnalogTrigger) {
                    lastRightTrigger = if (pressed) 255 else 0
                }
                val mouseSent = handleControllerMouseTrigger(left = false, pressed = pressed)
                if (mouseSent) {
                    return true
                }
                return if (!hasAnalogTrigger) {
                    sendCurrentGamepadState(controllerId = activeControllerId)
                } else {
                    true
                }
            }
        }
        return false
    }

    private fun sendControllerMouseMove(stickX: Float, stickY: Float): Boolean {
        if (!controllerMouseAssistActive && !controllerMouseEmulationActive) return false
        val delta = AndroidControllerMouseAssist.mouseDelta(stickX, stickY) ?: return false
        val sent = sendTouchMouseMove(delta.dx, delta.dy)
        if (sent && !controllerMouseMoveLogged) {
            controllerMouseMoveLogged = true
            NativeInputDiagnostics.add("controller mouse move sent dx=${delta.dx} dy=${delta.dy} auto=$controllerMouseAssistAutoArmed emulation=$controllerMouseEmulationActive")
        }
        return sent
    }

    private fun sendControllerMouseScroll(stickY: Float) {
        if (!controllerMouseEmulationActive) return
        val (notches, nextAccumulator) = AndroidControllerMouseAssist.scrollNotches(
            stickY = stickY,
            scrollSensitivity = settings.mouseScrollSensitivity,
            accumulator = controllerScrollAccumulator
        )
        controllerScrollAccumulator = nextAccumulator
        if (notches != 0) {
            sendTouchMouseWheel(notches * 120)
        }
    }

    private fun handleControllerMouseButton(buttonMask: Int, pressed: Boolean): Boolean {
        if (!controllerMouseAssistActive && !controllerMouseEmulationActive) return false
        val mouseButton = AndroidControllerMouseAssist.mouseButtonForGamepad(buttonMask) ?: return false
        setControllerMouseButton(mouseButton, pressed)
        return true
    }

    /** When emulation mode is on, intercept Gamepad A as a left mouse click (button 1). */
    private fun handleControllerMouseEmulationButton(buttonMask: Int, pressed: Boolean): Boolean {
        if (!controllerMouseEmulationActive) return false
        if (buttonMask != GamepadButtonMapping.A) return false
        setControllerMouseButton(1, pressed)
        return true
    }

    private fun handleControllerMouseTrigger(left: Boolean, pressed: Boolean): Boolean {
        if (!controllerMouseAssistActive) return false
        val mouseButton = AndroidControllerMouseAssist.mouseButtonForTrigger(left) ?: return false
        return setControllerMouseButton(mouseButton, pressed)
    }

    private fun sendBurstLimitedGamepadState(controllerId: Int = activeControllerId): Boolean {
        if (!canSendInput(partiallyReliable = false, fallbackToReliable = true)) return false
        val immediateControllerId = synchronized(gamepadStateBurstLock) {
            val immediate = gamepadStateBurstLimiter.offer(
                controllerId = controllerId,
                nowMs = SystemClock.elapsedRealtime(),
                hatButtons = physicalHatButtons,
                leftTrigger = lastLeftTrigger,
                rightTrigger = lastRightTrigger,
            )
            if (immediate != null) {
                // The immediate snapshot includes the pending stick state. A later motion sample
                // must schedule its flush from this new send time, not from the superseded job.
                gamepadStateBurstFlushJob?.cancel()
                gamepadStateBurstFlushJob = null
            } else if (gamepadStateBurstFlushJob?.isActive != true) {
                scheduleGamepadStateBurstFlushLocked()
            }
            immediate
        }
        return immediateControllerId?.let(::sendCurrentGamepadState) ?: true
    }

    /** Must be called with [gamepadStateBurstLock] held. */
    private fun scheduleGamepadStateBurstFlushLocked() {
        gamepadStateBurstFlushJob = scope.launch {
            while (true) {
                val waitMs = synchronized(gamepadStateBurstLock) {
                    gamepadStateBurstLimiter.delayUntilFlushMs(SystemClock.elapsedRealtime())
                }
                if (waitMs == null) {
                    synchronized(gamepadStateBurstLock) { gamepadStateBurstFlushJob = null }
                    return@launch
                }
                if (waitMs > 0L) delay(waitMs)
                val controllerId = synchronized(gamepadStateBurstLock) {
                    gamepadStateBurstLimiter.flush(SystemClock.elapsedRealtime()).also {
                        gamepadStateBurstFlushJob = null
                    }
                }
                controllerId?.let(::sendCurrentGamepadState)
                return@launch
            }
        }
    }

    private fun sendCurrentGamepadState(controllerId: Int = activeControllerId): Boolean {
        // A gamepad packet is a full-state snapshot. Keep snapshots ordered: an older packet that
        // arrives late on the loss-tolerant channel can undo a newer button or stick state.
        val partiallyReliable = false
        val buttons =
            physicalSteamOverlayChord.effectiveButtons(physicalButtons) or
                physicalHatButtons or
                virtualSteamOverlayChord.effectiveButtons(virtualButtons) or
                steamMenuChordButtons
        val leftTrigger = max(lastLeftTrigger, virtualLeftTrigger)
        val rightTrigger = max(lastRightTrigger, virtualRightTrigger)
        val leftStickX = effectiveLeftStickX()
        val leftStickY = effectiveLeftStickY()
        val rightStickX = effectiveRightStickX()
        val rightStickY = effectiveRightStickY()
        val bitmap = currentGamepadBitmap(controllerId)
        val packet = inputEncoder.encodeGamepadState(
            controllerId = controllerId,
            buttons = buttons,
            leftTrigger = leftTrigger,
            rightTrigger = rightTrigger,
            leftStickX = leftStickX,
            leftStickY = leftStickY,
            rightStickX = rightStickX,
            rightStickY = rightStickY,
            bitmap = bitmap,
            partiallyReliable = partiallyReliable,
        )
        val sent = sendInput(
            packet,
            partiallyReliable = partiallyReliable,
            fallbackToReliable = !partiallyReliable,
            resultDiagnosticKey = "controller.send.$controllerId",
        )
        NativeInputDiagnostics.retainThrottled(
            key = "controller.packet.$controllerId",
            minimumIntervalMs = GAMEPAD_PACKET_DIAGNOSTIC_INTERVAL_MS,
        ) {
            "gamepad packet slot=$controllerId sent=$sent partialRequested=$partiallyReliable " +
                "bitmap=$bitmap buttons=$buttons triggers=$leftTrigger,$rightTrigger " +
                "left=$leftStickX,$leftStickY right=$rightStickX,$rightStickY " +
                "physicalActive=$physicalControllerActive virtualVisible=$virtualControllerVisible " +
                inputChannelStateSummary()
        }
        if (leftStickX != 0 || leftStickY != 0 || rightStickX != 0 || rightStickY != 0) {
            NativeInputDiagnostics.retainThrottled(
                key = "controller.last-stick.$controllerId",
                minimumIntervalMs = ANALOG_DIAGNOSTIC_INTERVAL_MS,
            ) {
                "gamepad stick packet slot=$controllerId sent=$sent left=$leftStickX,$leftStickY right=$rightStickX,$rightStickY " +
                    "leftSource=${if (virtualLeftStickActive) "virtual" else "physical"} " +
                    "rightSource=${when {
                        virtualRightStickActive -> "virtual"
                        else -> "physical"
                    }} " +
                    inputChannelStateSummary()
            }
        }
        return sent
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

    private fun schedulePhysicalSteamOverlayChordRelease(controllerId: Int) {
        physicalSteamOverlayChordReleaseJob?.cancel()
        physicalSteamOverlayChordReleaseJob = scope.launch {
            delay(GAMEPAD_GUIDE_AUTO_RELEASE_MS)
            if (!physicalSteamOverlayChord.releaseChord()) return@launch
            sendCurrentGamepadState(controllerId = controllerId)
            NativeInputDiagnostics.add("physical View+Start sent Steam Menu Home+A chord slot=$controllerId")
        }
    }

    private fun scheduleVirtualSteamOverlayChordRelease() {
        virtualSteamOverlayChordReleaseJob?.cancel()
        virtualSteamOverlayChordReleaseJob = scope.launch {
            delay(GAMEPAD_GUIDE_AUTO_RELEASE_MS)
            if (!virtualSteamOverlayChord.releaseChord()) return@launch
            sendCurrentGamepadState()
            NativeInputDiagnostics.add("touch View+Start sent Steam Menu Home+A chord")
        }
    }

    private fun effectiveLeftStickX(): Int = if (virtualLeftStickActive) virtualLeftStickX else lastLeftStickX
    private fun effectiveLeftStickY(): Int = if (virtualLeftStickActive) virtualLeftStickY else lastLeftStickY
    private fun effectiveRightStickX(): Int =
        when {
            virtualRightStickActive -> virtualRightStickX
            controllerMouseAssistActive -> 0
            else -> lastRightStickX
        }

    private fun effectiveRightStickY(): Int =
        when {
            virtualRightStickActive -> virtualRightStickY
            controllerMouseAssistActive -> 0
            else -> lastRightStickY
        }

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

    private fun hasActiveControllerInput(): Boolean =
        physicalButtons != 0 ||
            physicalHatButtons != 0 ||
            virtualButtons != 0 ||
            steamMenuChordButtons != 0 ||
            lastLeftTrigger != 0 ||
            lastRightTrigger != 0 ||
            virtualLeftTrigger != 0 ||
            virtualRightTrigger != 0 ||
            effectiveLeftStickX() != 0 ||
            effectiveLeftStickY() != 0 ||
            effectiveRightStickX() != 0 ||
            effectiveRightStickY() != 0

    private fun sendInput(bytes: ByteArray, partiallyReliable: Boolean): Boolean =
        sendInput(bytes, partiallyReliable, fallbackToReliable = true)

    private fun sendReliableInput(bytes: ByteArray): Boolean {
        if (sendInput(bytes, partiallyReliable = false)) return true
        val sentPartial = sendInput(bytes, partiallyReliable = true, fallbackToReliable = false)
        if (sentPartial) {
            NativeInputDiagnostics.add("reliable input used partial fallback ${inputChannelStateSummary()} bytes=${bytes.size}")
        }
        return sentPartial
    }

    private fun sendInput(
        bytes: ByteArray,
        partiallyReliable: Boolean,
        fallbackToReliable: Boolean,
        resultDiagnosticKey: String? = null,
    ): Boolean {
        nvstTransport?.let { return it.sendInput(bytes, partiallyReliable) }
        val queuedChannel = openInputChannel(partiallyReliable, fallbackToReliable)
        if (queuedChannel == null) {
            resultDiagnosticKey?.let { key ->
                NativeInputDiagnostics.retainResult(key, succeeded = false) {
                    "path=queue reason=noOpenChannel requestedPartial=$partiallyReliable ${inputChannelStateSummary()}"
                }
            }
            if (!inputDropLogged) {
                inputDropLogged = true
                NativeInputDiagnostics.addRetained(
                    key = "input.last-drop",
                    message = "input dropped noOpenChannel requestedPartial=$partiallyReliable ${inputChannelStateSummary()} bytes=${bytes.size}",
                )
            }
            return false
        }
        // WebRTC normally accepts sends from the dedicated input worker. A small set of Android
        // WebRTC builds instead returns false there without throwing; the old code ignored that
        // Boolean and continued reporting every packet as sent. Fall back to the caller only after
        // an observed worker rejection, and only after the old worker queue has drained so packet
        // ordering is preserved. The direct fallback deliberately uses the cached OPEN state above
        // and calls only send(), avoiding the state()/bufferedAmount() JNI calls implicated in the
        // original input-dispatch ANR.
        if (synchronousInputFallback.get() && pendingInputSends.get() == 0) {
            return sendInputSynchronously(queuedChannel, bytes, partiallyReliable, resultDiagnosticKey)
        }
        val pending = pendingInputSends.incrementAndGet()
        if (pending > MAX_PENDING_INPUT_SENDS) {
            pendingInputSends.decrementAndGet()
            resultDiagnosticKey?.let { key ->
                NativeInputDiagnostics.retainResult(key, succeeded = false) {
                    "path=queue reason=senderBackpressure requestedPartial=$partiallyReliable pending=$pending"
                }
            }
            NativeInputDiagnostics.retainThrottled(
                key = "input.last-drop",
                minimumIntervalMs = INPUT_BACKPRESSURE_DIAGNOSTIC_INTERVAL_MS,
            ) {
                "input dropped senderQueue pending=$pending limit=$MAX_PENDING_INPUT_SENDS " +
                    "requestedPartial=$partiallyReliable bytes=${bytes.size}"
            }
            return false
        }
        // Do not call any DataChannel JNI accessor from Android's input-dispatch thread.
        // state(), bufferedAmount(), and send() can all contend on WebRTC/native locks; keeping
        // the complete sequence on the dedicated sender prevents a slow network/native lock from
        // turning a touch event into an Input dispatching timed out ANR.
        val queuedAtMs = SystemClock.uptimeMillis()
        inputScope.launch {
            try {
                sendInputOnWorker(queuedChannel, bytes, partiallyReliable, resultDiagnosticKey, queuedAtMs)
            } finally {
                pendingInputSends.decrementAndGet()
            }
        }
        return true
    }

    private fun openInputChannel(partiallyReliable: Boolean, fallbackToReliable: Boolean): DataChannel? =
        when {
            partiallyReliable && partiallyReliableInputState == DataChannel.State.OPEN -> partiallyReliableInput
            partiallyReliable && !fallbackToReliable -> null
            reliableInputState == DataChannel.State.OPEN -> reliableInput
            else -> null
        }

    // Rate limiters must check the active transport, not require a WebRTC DataChannel:
    // NVST owns its SCTP channels in Rust and has no Java DataChannel objects.
    private fun canSendInput(partiallyReliable: Boolean, fallbackToReliable: Boolean): Boolean =
        nvstTransport?.inputReady ?: (openInputChannel(partiallyReliable, fallbackToReliable) != null)

    private fun hasOpenInputChannel(): Boolean =
        nvstTransport?.inputReady ?: (reliableInputState == DataChannel.State.OPEN ||
            partiallyReliableInputState == DataChannel.State.OPEN)

    private fun hasReadyInputChannel(): Boolean = inputHandshakeReady && hasOpenInputChannel()

    private fun inputChannelStateSummary(): String =
        nvstTransport?.let { "nvst=${if (it.inputReady) "ready" else "waiting"}" }
            ?: "reliable=${reliableInputState?.name ?: "none"} partial=${partiallyReliableInputState?.name ?: "none"}"

    private fun sendInputOnWorker(
        channel: DataChannel,
        bytes: ByteArray,
        partiallyReliable: Boolean,
        resultDiagnosticKey: String?,
        queuedAtMs: Long,
    ) {
        val queueDelayMs = (SystemClock.uptimeMillis() - queuedAtMs).coerceAtLeast(0L)
        runCatching {
            if (channel.state() != DataChannel.State.OPEN) {
                resultDiagnosticKey?.let { key ->
                    NativeInputDiagnostics.retainResult(key, succeeded = false) {
                        "path=worker reason=channelClosed requestedPartial=$partiallyReliable"
                    }
                }
                return@runCatching
            }
            val bufferedAmount = channel.bufferedAmount()
            // Inputs explicitly routed to the loss-tolerant channel may be dropped early instead
            // of letting that channel accumulate lag. Ordered relative mouse deltas and critical
            // one-shot events use the reliable threshold and are dropped only when the channel is
            // genuinely backed up. Key this on requested reliability so a critical event using the
            // partial fallback stays critical.
            val dropThreshold = if (partiallyReliable) {
                INPUT_PARTIAL_BACKPRESSURE_DROP_THRESHOLD
            } else {
                INPUT_RELIABLE_BACKPRESSURE_DROP_THRESHOLD
            }
            if (bufferedAmount > dropThreshold) {
                resultDiagnosticKey?.let { key ->
                    NativeInputDiagnostics.retainResult(key, succeeded = false) {
                        "path=worker reason=dataChannelBackpressure requestedPartial=$partiallyReliable bufferedAmount=$bufferedAmount"
                    }
                }
                NativeInputDiagnostics.retainThrottled(
                    key = "input.last-drop",
                    minimumIntervalMs = INPUT_BACKPRESSURE_DIAGNOSTIC_INTERVAL_MS,
                ) {
                    "input dropped backpressure requestedPartial=$partiallyReliable label=${channel.label()} " +
                        "bufferedAmount=$bufferedAmount threshold=$dropThreshold bytes=${bytes.size}"
                }
                return@runCatching
            }
            restampProtocolV3OuterTimestamp(bytes)
            val accepted = channel.send(DataChannel.Buffer(java.nio.ByteBuffer.wrap(bytes), true))
            resultDiagnosticKey?.let { key ->
                NativeInputDiagnostics.retainResult(key, accepted) {
                    "path=worker requestedPartial=$partiallyReliable queueDelayMs=$queueDelayMs"
                }
            }
            if (accepted) {
                if (workerInputSendConfirmed.compareAndSet(false, true)) {
                    NativeInputDiagnostics.addRetained(
                        key = "input.send-path",
                        message = "input data channel accepted path=worker requestedPartial=$partiallyReliable " +
                            "bytes=${bytes.size}",
                    )
                }
            } else {
                synchronousInputFallback.set(true)
                NativeInputDiagnostics.retainThrottled(
                    key = "input.last-send-error",
                    minimumIntervalMs = INPUT_BACKPRESSURE_DIAGNOSTIC_INTERVAL_MS,
                ) {
                    "input send rejected path=worker requestedPartial=$partiallyReliable " +
                        "bytes=${bytes.size}; directFallback=true"
                }
            }
        }.onFailure { error ->
            resultDiagnosticKey?.let { key ->
                NativeInputDiagnostics.retainResult(key, succeeded = false) {
                    "path=worker reason=${error.javaClass.simpleName} requestedPartial=$partiallyReliable"
                }
            }
            synchronousInputFallback.set(true)
            NativeInputDiagnostics.retainThrottled(
                key = "input.last-send-error",
                minimumIntervalMs = INPUT_BACKPRESSURE_DIAGNOSTIC_INTERVAL_MS,
            ) {
                "input send failed requestedPartial=$partiallyReliable bytes=${bytes.size} " +
                    "error=${error.javaClass.simpleName}"
            }
        }
    }

    private fun sendInputSynchronously(
        channel: DataChannel,
        bytes: ByteArray,
        partiallyReliable: Boolean,
        resultDiagnosticKey: String? = null,
    ): Boolean = runCatching {
        restampProtocolV3OuterTimestamp(bytes)
        channel.send(DataChannel.Buffer(java.nio.ByteBuffer.wrap(bytes), true))
    }.fold(
        onSuccess = { accepted ->
            resultDiagnosticKey?.let { key ->
                NativeInputDiagnostics.retainResult(key, accepted) {
                    "path=direct-fallback requestedPartial=$partiallyReliable"
                }
            }
            if (accepted) {
                if (directInputSendConfirmed.compareAndSet(false, true)) {
                    NativeInputDiagnostics.addRetained(
                        key = "input.send-path",
                        message = "input data channel accepted path=direct-fallback requestedPartial=$partiallyReliable " +
                            "bytes=${bytes.size}",
                    )
                }
            } else {
                NativeInputDiagnostics.retainThrottled(
                    key = "input.last-send-error",
                    minimumIntervalMs = INPUT_BACKPRESSURE_DIAGNOSTIC_INTERVAL_MS,
                ) {
                    "input send rejected path=direct-fallback requestedPartial=$partiallyReliable " +
                        "bytes=${bytes.size}"
                }
            }
            accepted
        },
        onFailure = { error ->
            resultDiagnosticKey?.let { key ->
                NativeInputDiagnostics.retainResult(key, succeeded = false) {
                    "path=direct-fallback reason=${error.javaClass.simpleName} requestedPartial=$partiallyReliable"
                }
            }
            NativeInputDiagnostics.retainThrottled(
                key = "input.last-send-error",
                minimumIntervalMs = INPUT_BACKPRESSURE_DIAGNOSTIC_INTERVAL_MS,
            ) {
                "input send failed path=direct-fallback requestedPartial=$partiallyReliable " +
                    "bytes=${bytes.size} error=${error.javaClass.simpleName}"
            }
            false
        },
    )

    private fun clearPhysicalControllerInputState() {
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
        physicalLeftStickX = 0f
        physicalLeftStickY = 0f
        physicalRightStickX = 0f
        physicalRightStickY = 0f
    }

    private fun refreshConnectedPhysicalControllers() {
        controllerAxisAvailability.clear()
        val connectedDevices = connectedControllerDevices()
        val connectedDeviceIds = connectedDevices.mapTo(mutableSetOf()) { it.id }
        val removedControllerSlots = AndroidControllerSlotRegistry.retainConnected(
            controllerSlots = controllerSlots,
            connectedDeviceIds = connectedDeviceIds,
        )
        if (removedControllerSlots.isNotEmpty()) {
            removedControllerSlots.values.forEach(controllerFamiliesBySlot::remove)
            NativeInputDiagnostics.add(
                "physical gamepad slots released=${removedControllerSlots.entries.joinToString { "${it.key}:${it.value}" }}",
            )
        }
        connectedDevices.forEach { device ->
            controllerSlots[device.id]?.let { slot ->
                AndroidControllerInput.controllerFamily(device)?.let { family ->
                    controllerFamiliesBySlot[slot] = family
                }
            }
        }
        val activeControllerDisconnected = activeControllerId in removedControllerSlots.values
        val connected = connectedDevices.isNotEmpty()
        val connectionChanged = connected != physicalControllerConnected
        val connectionMessage =
            "physical gamepad connected=$connected devices=${connectedDevices.joinToString { device ->
                val family = AndroidControllerInput.controllerFamily(device)
                "${device.id}:${device.name}:family=$family:vendor=0x${device.vendorId.toString(16)}:product=0x${device.productId.toString(16)}"
            }}"
        if (connectionChanged) {
            NativeInputDiagnostics.addRetained("controller.connection", connectionMessage)
        } else {
            NativeInputDiagnostics.retain("controller.connection", connectionMessage)
        }
        physicalControllerConnected = connected
        if (connected && !physicalControllerActive && controllerSlots.isEmpty()) {
            activeControllerId = controllerIdFor(connectedDevices.first().id)
        }
        if (physicalControllerActive && (!connected || activeControllerDisconnected)) {
            clearPhysicalControllerInputState()
            if (connected) {
                activeControllerId = controllerIdFor(connectedDevices.first().id)
            }
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
        NativeInputDiagnostics.addRetained(
            key = "controller.prime",
            message = "gamepad state prime reason=$reason sent=$sent connected=$physicalControllerConnected active=$physicalControllerActive slot=$activeControllerId ${inputChannelStateSummary()}",
        )
    }

    private fun updateHapticsAdvertisement(force: Boolean = false) {
        if (!inputHandshakeReady || (reliableInputState != DataChannel.State.OPEN && nvstTransport?.inputReady != true)) return
        val now = SystemClock.elapsedRealtime()
        // Periodically re-advertise: the controller can connect (or start reporting a vibrator)
        // after the session began, and once advertised with enabled=false the server keeps
        // haptics disabled for the whole session unless we re-advertise enabled=true.
        if (!force && hapticsAdvertised != null && now - lastHapticsAdvertisementAtMs < HAPTICS_ADVERTISEMENT_REFRESH_MS) return
        val enabled = hapticsOutputAvailable()
        if (hapticsAdvertised == enabled && now - lastHapticsAdvertisementAtMs < HAPTICS_ADVERTISEMENT_REFRESH_MS) return
        if (sendReliableInput(inputEncoder.encodeHapticsEnabled(enabled))) {
            hapticsAdvertised = enabled
            lastHapticsAdvertisementAtMs = now
            NativeInputDiagnostics.add("gamepad haptics advertised enabled=$enabled force=$force")
        }
    }

    private fun hapticsOutputAvailable(): Boolean =
        selectHapticsOutputTarget(
            vibrationEnabled = vibrationEnabled,
            controllerRumbleAvailable = hapticControllerDevices().isNotEmpty(),
            deviceHapticsAvailable = hasDeviceHaptics(),
            preference = hapticsOutputPreference,
        ) != HapticsOutputTarget.None

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
        val now = SystemClock.elapsedRealtime()
        if (!isStop && lastRumbleEffectAtMs[slot] != 0L && now - lastRumbleEffectAtMs[slot] <= RUMBLE_THROTTLE_MS) {
            return
        }
        val device = findHapticControllerDevice(slot)
        val outputTarget = selectHapticsOutputTarget(
            vibrationEnabled = vibrationEnabled,
            controllerRumbleAvailable = device != null,
            deviceHapticsAvailable = hasDeviceHaptics(),
            preference = hapticsOutputPreference,
        )
        if (outputTarget == HapticsOutputTarget.None) {
            logHapticsWarning(
                "input haptics unavailable controller=$controllerId vibrationEnabled=$vibrationEnabled " +
                    "output=$hapticsOutputPreference controllerRumble=${device != null} device=${hasDeviceHaptics()}",
            )
            return
        }
        lastRumbleEffectAtMs[slot] = if (isStop) 0L else now

        if (isStop) {
            when (outputTarget) {
                HapticsOutputTarget.Controller -> device?.let(::cancelControllerRumble)
                HapticsOutputTarget.Device -> cancelDeviceHaptics()
                HapticsOutputTarget.None -> Unit
            }
            return
        }
        if (device != null && !hapticsSupportLogged[slot]) {
            hapticsSupportLogged[slot] = true
            NativeInputDiagnostics.add("gamepad haptics available controller=$slot device=${device.id}:${device.name}")
        }
        if (Build.VERSION.SDK_INT >= Build.VERSION_CODES.O) {
            when (outputTarget) {
                HapticsOutputTarget.Controller -> device?.let { vibrateController(it, profile) }
                HapticsOutputTarget.Device -> vibrateDeviceHaptics(profile)
                HapticsOutputTarget.None -> Unit
            }
        } else {
            @Suppress("DEPRECATION")
            when (outputTarget) {
                HapticsOutputTarget.Controller -> device?.vibrator?.vibrate(RUMBLE_EFFECT_MS)
                HapticsOutputTarget.Device -> vibrateDeviceHapticsLegacy()
                HapticsOutputTarget.None -> Unit
            }
        }
    }

    private fun stopAllGamepadRumble() {
        hapticControllerDevices().forEach { device ->
            cancelControllerRumble(device)
        }
        cancelDeviceHaptics()
        for (index in 0 until GAMEPAD_MAX_CONTROLLERS) {
            lastRumbleEffectAtMs[index] = 0L
            hapticsSupportLogged[index] = false
        }
        deviceHapticsSupportLogged = false
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

    // Dispatched only from the SDK_INT >= O branch of the haptics entry point.
    @RequiresApi(Build.VERSION_CODES.O)
    private fun vibrateController(device: InputDevice, profile: RumbleEffectProfile) {
        if (Build.VERSION.SDK_INT >= Build.VERSION_CODES.S) {
            val manager = device.vibratorManager
            val vibrators = manager.vibratorIds
                .map(manager::getVibrator)
                .filter(Vibrator::hasVibrator)
            if (vibrators.size >= 2) {
                // Input-device VibratorManager implementations vary across OEM builds. Drive the
                // controller motors through their individual Vibrator handles, which is also the
                // Android game-controller API's documented path, instead of relying on a combined
                // stereo effect that some devices accept without producing physical rumble.
                updateControllerVibrator(vibrators[0], profile.strongAmplitude)
                updateControllerVibrator(vibrators[1], profile.weakAmplitude)
                return
            }
            if (vibrators.isNotEmpty()) {
                updateControllerVibrator(vibrators[0], profile.combinedAmplitude)
                return
            }
        }
        @Suppress("DEPRECATION")
        device.vibrator.vibrateAsMedia(createRumbleEffect(profile.combinedAmplitude))
    }

    @RequiresApi(Build.VERSION_CODES.O)
    private fun updateControllerVibrator(vibrator: Vibrator, amplitude: Int) {
        if (amplitude <= 0) {
            vibrator.cancel()
        } else {
            vibrator.vibrateAsMedia(createRumbleEffect(amplitude))
        }
    }

    /**
     * Rumble is media output, not touch feedback.
     *
     * A bare `vibrate(effect)` is classified `USAGE_UNKNOWN`, which the platform folds into the
     * system "Touch feedback" setting — off by default on several gaming handhelds, and the reason
     * rumble could arrive from the host and produce nothing at all. Tagging it `USAGE_MEDIA` puts
     * it where game rumble belongs and takes it out from under that switch.
     */
    @RequiresApi(Build.VERSION_CODES.O)
    private fun Vibrator.vibrateAsMedia(effect: VibrationEffect) {
        if (Build.VERSION.SDK_INT >= Build.VERSION_CODES.TIRAMISU) {
            vibrate(effect, mediaVibrationAttributes())
        } else {
            vibrate(effect, gameAudioAttributes())
        }
    }

    @Suppress("DEPRECATION")
    private fun cancelControllerRumble(device: InputDevice) {
        if (Build.VERSION.SDK_INT >= Build.VERSION_CODES.S) {
            val manager = device.vibratorManager
            if (manager.vibratorIds.isNotEmpty()) {
                manager.vibratorIds.forEach { vibratorId ->
                    manager.getVibrator(vibratorId).cancel()
                }
                return
            }
        }
        device.vibrator.cancel()
    }

    private fun hasDeviceHaptics(): Boolean =
        if (Build.VERSION.SDK_INT >= Build.VERSION_CODES.S) {
            appContext.getSystemService(VibratorManager::class.java)?.let { manager ->
                manager.vibratorIds.any { manager.getVibrator(it).hasVibrator() }
            } == true
        } else {
            @Suppress("DEPRECATION")
            (appContext.getSystemService(Context.VIBRATOR_SERVICE) as? Vibrator)?.hasVibrator() == true
        }

    // Only reached from the SDK_INT >= O branch of the haptics dispatcher.
    @RequiresApi(Build.VERSION_CODES.O)
    private fun createRumbleEffect(amplitude: Int): VibrationEffect =
        VibrationEffect.createOneShot(RUMBLE_EFFECT_MS, amplitude.coerceIn(1, 255))

    @RequiresApi(Build.VERSION_CODES.O)
    private fun vibrateDeviceHaptics(profile: RumbleEffectProfile) {
        if (!deviceHapticsSupportLogged) {
            deviceHapticsSupportLogged = true
            NativeInputDiagnostics.add("gamepad haptics using device fallback")
        }
        if (Build.VERSION.SDK_INT >= Build.VERSION_CODES.S) {
            val manager = appContext.getSystemService(VibratorManager::class.java)
            if (manager != null && manager.vibratorIds.any { manager.getVibrator(it).hasVibrator() }) {
                // VibratorManager is API 31+, so VibrationAttributes is always available here.
                manager.vibrate(
                    CombinedVibration.createParallel(createRumbleEffect(profile.combinedAmplitude)),
                    mediaVibrationAttributes(),
                )
                return
            }
        }
        @Suppress("DEPRECATION")
        (appContext.getSystemService(Context.VIBRATOR_SERVICE) as? Vibrator)
            ?.vibrateAsMedia(createRumbleEffect(profile.combinedAmplitude))
    }

    @Suppress("DEPRECATION")
    private fun vibrateDeviceHapticsLegacy() {
        if (!deviceHapticsSupportLogged) {
            deviceHapticsSupportLogged = true
            NativeInputDiagnostics.add("gamepad haptics using device fallback")
        }
        (appContext.getSystemService(Context.VIBRATOR_SERVICE) as? Vibrator)?.vibrate(RUMBLE_EFFECT_MS)
    }

    private fun cancelDeviceHaptics() {
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
        val connectedDevices = connectedControllerDevices()
        val connectedDeviceIds = connectedDevices.mapTo(mutableSetOf()) { it.id }
        val assignment = AndroidControllerSlotRegistry.assign(
            controllerSlots = controllerSlots,
            deviceId = deviceId,
            connectedDeviceIds = connectedDeviceIds,
            maxControllers = GAMEPAD_MAX_CONTROLLERS,
        )
        if (physicalControllerActive && activeControllerId in assignment.removedDevices.values) {
            clearPhysicalControllerInputState()
        }
        if (assignment.removedDevices.isNotEmpty()) {
            assignment.removedDevices.values.forEach(controllerFamiliesBySlot::remove)
            NativeInputDiagnostics.add(
                "physical gamepad slots reconciled removed=${assignment.removedDevices.entries.joinToString { "${it.key}:${it.value}" }} " +
                    "device=$deviceId slot=${assignment.slot}",
            )
        }
        connectedDevices
            .firstOrNull { controllerSlots[it.id] == assignment.slot }
            ?.let(AndroidControllerInput::controllerFamily)
            ?.let { controllerFamiliesBySlot[assignment.slot] = it }
        return assignment.slot
    }

    private fun connectedControllerDevices(): List<InputDevice> =
        InputDevice.getDeviceIds()
            .map(InputDevice::getDevice)
            .filterNotNull()
            .filter(AndroidControllerInput::isControllerDevice)

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
        val physicalFamily = if (physicalControllerConnected || physicalControllerActive) {
            controllerFamiliesBySlot[id]
        } else {
            null
        }
        return androidGamepadConnectionBitmap(
            controllerId = id,
            connected = true,
            physicalControllerFamily = physicalFamily,
            playStationRumbleCompatibility = usesPlayStationRumbleCompatibility(
                vibrationEnabled = vibrationEnabled,
                preference = hapticsOutputPreference,
            ),
        )
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

    private fun MotionEvent.axisAvailability(): AndroidGamepadAxisAvailability {
        val cacheKey = deviceId
        if (cacheKey >= 0) {
            controllerAxisAvailability[cacheKey]?.let { return it }
        }
        return AndroidGamepadAxisAvailability(
            x = hasMotionAxis(MotionEvent.AXIS_X),
            y = hasMotionAxis(MotionEvent.AXIS_Y),
            z = hasMotionAxis(MotionEvent.AXIS_Z),
            rz = hasMotionAxis(MotionEvent.AXIS_RZ),
            rx = hasMotionAxis(MotionEvent.AXIS_RX),
            ry = hasMotionAxis(MotionEvent.AXIS_RY),
            hatX = hasMotionAxis(MotionEvent.AXIS_HAT_X),
            hatY = hasMotionAxis(MotionEvent.AXIS_HAT_Y),
        ).also { availability ->
            if (cacheKey >= 0) {
                controllerAxisAvailability[cacheKey] = availability
            }
        }
    }

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

    private fun Int.toGfnMouseButton(): Int = when {
        this and MotionEvent.BUTTON_PRIMARY != 0 -> 1
        this and MotionEvent.BUTTON_TERTIARY != 0 -> 2
        this and MotionEvent.BUTTON_SECONDARY != 0 -> 3
        this and MotionEvent.BUTTON_BACK != 0 -> 4
        this and MotionEvent.BUTTON_FORWARD != 0 -> 5
        else -> 1
    }

    private companion object {
        private const val ANDROID_TV_CODEC_RELEASE_SETTLE_MS = 180L
        private const val LIVE_BITRATE_UPDATE_DEBOUNCE_MS = 350L
        private const val EXTERNAL_MOUSE_ABSOLUTE_DELTA_LIMIT_PX = 240f
        private const val GAMEPAD_MAX_CONTROLLERS = 4
        private const val RUMBLE_EFFECT_MS = 90L
        private const val RUMBLE_THROTTLE_MS = 35L
        private const val HAPTICS_LOG_INTERVAL_MS = 5000L
        private const val HAPTICS_ADVERTISEMENT_REFRESH_MS = 5000L
        private const val ANALOG_ACTIVITY_THRESHOLD = 0.01f
        private const val ANALOG_DIAGNOSTIC_INTERVAL_MS = 250L
        private const val GAMEPAD_PACKET_DIAGNOSTIC_INTERVAL_MS = 1_000L
        private const val INPUT_BACKPRESSURE_DIAGNOSTIC_INTERVAL_MS = 1_000L
        private const val MOUSE_MOVE_MIN_SEND_INTERVAL_MS = 8L
        private const val GAMEPAD_STATE_MIN_SEND_INTERVAL_MS = 16L
        private const val MAX_PENDING_INPUT_SENDS = 256
        // State inputs are superseded by newer packets, so they are dropped well before the queue
        // can grow into lag; one-shot critical events keep the generous reliable threshold.
        private const val INPUT_PARTIAL_BACKPRESSURE_DROP_THRESHOLD = 16_384L
        private const val INPUT_RELIABLE_BACKPRESSURE_DROP_THRESHOLD = 65_536L
    }

    private fun radialDeadzoneScale(x: Float, y: Float, deadzone: Float = 0.15f): Float {
        val magnitude = kotlin.math.sqrt((x * x + y * y).toDouble()).toFloat()
        if (magnitude < deadzone) return 0f
        val scaled = ((magnitude - deadzone) / (1f - deadzone)).coerceIn(0f, 1f)
        return scaled / magnitude
    }

    private fun normalizeToInt16(value: Float): Int = (value.coerceIn(-1f, 1f) * 32767).roundToInt().coerceIn(-32768, 32767)
    private fun normalizeToUint8(value: Float): Int = (value.coerceIn(0f, 1f) * 255).roundToInt().coerceIn(0, 255)
    private fun normalizeTriggerAxis(value: Float): Float = if (value < 0f) ((value + 1f) / 2f).coerceIn(0f, 1f) else value.coerceIn(0f, 1f)
    private fun Float.formatAxis(): String = String.format(Locale.US, "%.3f", this)
}

open class SimpleSdpObserver : SdpObserver {
    override fun onCreateSuccess(description: SessionDescription?) = Unit
    override fun onSetSuccess() = Unit
    override fun onCreateFailure(error: String?) = Unit
    override fun onSetFailure(error: String?) = Unit
}

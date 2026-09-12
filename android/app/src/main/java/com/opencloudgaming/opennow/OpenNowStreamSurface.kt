package com.opencloudgaming.opennow

import android.Manifest
import android.app.Activity
import android.content.pm.PackageManager
import android.os.Build
import android.view.KeyEvent
import android.view.MotionEvent
import android.view.PointerIcon
import android.view.View
import android.view.ViewGroup
import android.widget.Toast
import androidx.activity.compose.BackHandler
import androidx.activity.compose.rememberLauncherForActivityResult
import androidx.activity.result.contract.ActivityResultContracts
import androidx.compose.animation.AnimatedVisibility
import androidx.compose.animation.fadeIn
import androidx.compose.animation.fadeOut
import androidx.compose.animation.scaleIn
import androidx.compose.animation.scaleOut
import androidx.compose.animation.slideInVertically
import androidx.compose.animation.slideOutVertically
import androidx.compose.animation.core.tween
import androidx.compose.foundation.gestures.detectTapGestures
import androidx.compose.foundation.background
import androidx.compose.foundation.border
import androidx.compose.foundation.layout.Arrangement
import androidx.compose.foundation.layout.Box
import androidx.compose.foundation.layout.Column
import androidx.compose.foundation.layout.PaddingValues
import androidx.compose.foundation.layout.Row
import androidx.compose.foundation.layout.Spacer
import androidx.compose.foundation.layout.WindowInsets
import androidx.compose.foundation.layout.aspectRatio
import androidx.compose.foundation.layout.fillMaxHeight
import androidx.compose.foundation.layout.fillMaxSize
import androidx.compose.foundation.layout.fillMaxWidth
import androidx.compose.foundation.layout.height
import androidx.compose.foundation.layout.ime
import androidx.compose.foundation.layout.imePadding
import androidx.compose.foundation.layout.offset
import androidx.compose.foundation.layout.padding
import androidx.compose.foundation.layout.size
import androidx.compose.foundation.layout.width
import androidx.compose.foundation.shape.RoundedCornerShape
import androidx.compose.material3.ButtonDefaults
import androidx.compose.material3.Icon
import androidx.compose.material.icons.Icons
import androidx.compose.material.icons.rounded.Check
import androidx.compose.material3.MaterialTheme
import androidx.compose.material3.Text
import androidx.compose.runtime.Composable
import androidx.compose.runtime.LaunchedEffect
import androidx.compose.runtime.getValue
import androidx.compose.runtime.mutableFloatStateOf
import androidx.compose.runtime.mutableIntStateOf
import androidx.compose.runtime.mutableStateOf
import androidx.compose.runtime.remember
import androidx.compose.runtime.rememberUpdatedState
import androidx.compose.runtime.key
import androidx.compose.runtime.setValue
import androidx.compose.runtime.DisposableEffect
import androidx.compose.ui.Alignment
import androidx.compose.ui.Modifier
import androidx.compose.ui.draw.clip
import androidx.compose.ui.draw.clipToBounds
import androidx.compose.ui.geometry.Offset
import androidx.compose.ui.graphics.Color
import androidx.compose.ui.graphics.graphicsLayer
import androidx.compose.ui.input.pointer.pointerInput
import androidx.compose.ui.input.pointer.pointerInteropFilter
import androidx.compose.ui.input.key.key
import androidx.compose.ui.layout.onSizeChanged
import androidx.compose.ui.platform.LocalConfiguration
import androidx.compose.ui.platform.LocalContext
import androidx.compose.ui.platform.LocalDensity
import androidx.compose.ui.platform.LocalFocusManager
import androidx.compose.ui.platform.LocalSoftwareKeyboardController
import androidx.compose.ui.platform.LocalView
import androidx.compose.ui.res.stringResource
import androidx.compose.ui.semantics.contentDescription
import androidx.compose.ui.text.font.FontWeight
import androidx.compose.ui.text.input.TextFieldValue
import androidx.compose.ui.text.style.TextOverflow
import androidx.compose.ui.unit.dp
import androidx.compose.ui.unit.IntSize
import androidx.compose.ui.viewinterop.AndroidView
import androidx.core.content.ContextCompat
import kotlinx.coroutines.delay
import kotlinx.coroutines.launch
import java.util.Locale
import com.opencloudgaming.opennow.ui.theme.OpenNowPalette
import kotlin.math.sqrt

@Composable
internal fun StreamScreen(
    state: OpenNowUiState,
    viewModel: OpenNowViewModel,
    onMicrophoneCaptureActiveChange: (Boolean) -> Unit,
) {
    val context = LocalContext.current
    val activity = context as? Activity
    val openNowHaptics = LocalOpenNowHaptics.current
    val keyboardController = LocalSoftwareKeyboardController.current
    val focusManager = LocalFocusManager.current
    val density = LocalDensity.current
    val audioController = remember(context) { AndroidNerdAudioController(context.applicationContext) }
    val gyroscopeAvailable = remember(context) { hasMobileGyroscope(context) }
    val session = state.streamSession
    val game = state.streamGame
    var streamState by remember { mutableStateOf("Preparing") }
    var initialVideoFrameRendered by remember(session?.sessionId) { mutableStateOf(false) }
    val markInitialVideoFrameRendered by rememberUpdatedState<() -> Unit> {
        initialVideoFrameRendered = true
    }
    var controlsOpen by remember { mutableStateOf(false) }
    var exitConfirmOpen by remember { mutableStateOf(false) }
    var keyboardOpen by remember { mutableStateOf(false) }
    var keyboardValue by remember(session?.sessionId) { mutableStateOf(TextFieldValue()) }
    var keyboardSyncedText by remember(session?.sessionId) { mutableStateOf<String?>(null) }
    var audioMuted by remember { mutableStateOf(false) }
    var touchLayoutEditing by remember { mutableStateOf(false) }
    var streamGuideOpen by remember(session?.sessionId) { mutableStateOf(false) }
    var streamGuideStep by remember(session?.sessionId) { mutableStateOf(StreamGuideStep.OpenControls) }
    var statsVisible by remember(state.settings.showStatsOnLaunch) { mutableStateOf(state.settings.showStatsOnLaunch) }
    // Bitrate ceiling (kbps) the live session is currently capped at; mirrors client.liveBitrateLimitKbps.
    var liveBitrateLimitKbps by remember(session?.sessionId) { mutableStateOf<Int?>(null) }
    var streamStats by remember { mutableStateOf(StreamRuntimeStats()) }
    var networkNotice by remember(session?.sessionId) { mutableStateOf<StreamNetworkWarning?>(null) }
    var networkNoticeSequence by remember(session?.sessionId) { mutableIntStateOf(0) }
    val networkWarningGate = remember(session?.sessionId) { StreamNetworkWarningGate() }
    var videoTransportFallbackReason by remember { mutableStateOf<String?>(null) }
    var controllerMouseAssistEnabled by remember(session?.sessionId) { mutableStateOf(false) }
    var controllerMouseEmulationEnabled by remember(session?.sessionId) { mutableStateOf(state.settings.controllerMouseEmulation) }
    val streamReady = state.isNativeStreamReady()
    val tvProfile = state.androidTvProfile
    LaunchedEffect(session?.sessionId) {
        videoTransportFallbackReason = null
    }
    val physicalControllerConnected = rememberPhysicalControllerConnected(enabled = streamReady)
    val physicalKeyboardMouse = rememberPhysicalKeyboardMouseConnection(enabled = streamReady)
    val physicalMouseConnected = physicalKeyboardMouse.mouseConnected
    val physicalKeyboardMouseConnected = physicalKeyboardMouse.connected
    var showTouchControlsWithPhysicalController by remember(session?.sessionId) { mutableStateOf(false) }
    var showTouchControlsWithPhysicalMouse by remember(session?.sessionId) { mutableStateOf(false) }
    var preferVirtualController by remember(session?.sessionId) { mutableStateOf(false) }
    var physicalControllerPromptOpen by remember(session?.sessionId) { mutableStateOf(false) }
    var physicalControllerPromptHandled by remember(session?.sessionId) { mutableStateOf(false) }
    var physicalControllerPromptDoNotShowAgain by remember(session?.sessionId) { mutableStateOf(false) }
    val touchInputEnabled = !state.androidPictureInPictureActive
    val touchControlsSuppressedByPhysicalController =
        physicalControllerConnected &&
            state.settings.androidTouch.enabled &&
            !showTouchControlsWithPhysicalController
    val builtInGameTouchSupported = !tvProfile && game?.let(::catalogClaimsTouchSupport) == true
    val nativeTouchAvailable = !tvProfile && shouldUseNativeTouch(
        state.settings.androidTouch.effectiveNativeTouchMode(),
        game,
        state.activeStreamSettings ?: state.settings.stream,
    )
    val launchInputMode = state.streamInputModeAtLaunch ?: streamInputModeAtStart(
        nativeTouchAvailable = nativeTouchAvailable,
        keyboardMouseConnected = physicalKeyboardMouseConnected,
    )
    var streamInputMode by remember(session?.sessionId) { mutableStateOf(launchInputMode) }
    val nativeTouchProvisionedForSession = launchInputMode == StreamInputMode.NativeTouch
    var keyboardMouseBaselineCaptured by remember(session?.sessionId) { mutableStateOf(false) }
    var previousKeyboardMouseConnected by remember(session?.sessionId) {
        mutableStateOf(physicalKeyboardMouseConnected)
    }
    var pendingInputModePrompt by remember(session?.sessionId) {
        mutableStateOf<StreamInputModePrompt?>(null)
    }
    var inputModePromptOpen by remember(session?.sessionId) {
        mutableStateOf<StreamInputModePrompt?>(null)
    }
    // Native game touch and the virtual controller need exclusive ownership of the same fingers.
    // Catalog touch remains the default, while a player's in-session controller choice wins.
    val nativeTouchActive = !tvProfile && shouldUseNativeTouchForStream(
        state.settings.androidTouch.effectiveNativeTouchMode(),
        game,
        state.activeStreamSettings ?: state.settings.stream,
        preferVirtualController = preferVirtualController,
        preferKeyboardMouse = streamInputMode == StreamInputMode.KeyboardMouse,
    )
    val touchControlsVisible = shouldShowAndroidTouchControls(
        tvProfile = tvProfile,
        touchInputEnabled = touchInputEnabled,
        touchControlsEnabled = state.settings.androidTouch.enabled,
        suppressedByPhysicalController = touchControlsSuppressedByPhysicalController,
        physicalMouseConnected = physicalMouseConnected,
        allowWithPhysicalMouse = showTouchControlsWithPhysicalMouse,
    ) && !nativeTouchActive
    val touchMouseActive =
        streamReady && touchInputEnabled && state.settings.androidTouch.mousePad && !nativeTouchActive
    val fallbackSessionStartedAtMs = remember(session?.sessionId) { System.currentTimeMillis() }
    val sessionStartedAtMs = session?.timerStartedAtMs ?: fallbackSessionStartedAtMs
    var timerNowMs by remember(session?.sessionId) { mutableStateOf(System.currentTimeMillis()) }
    val smartSessionLimit = smartSessionLimitFor(state.subscriptionInfo, state.authSession?.user?.membershipTier)
    val buttonToneEnabled = state.settings.controllerUiSounds
    val stretchToFit = state.settings.stretchStreamToFit
    val playButtonTone = {
        audioController.playButtonTone(buttonToneEnabled)
    }
    val launchStreamSettings = state.activeStreamSettings ?: state.settings.stream
    // activeStreamSettings tracks the transport profile and can deliberately
    // change during safe-codec recovery. Keep the original launch profile so
    // requested, server-selected, decoded, and recovery modes remain distinct.
    val requestedStreamSettings = remember(session?.sessionId) {
        state.settings.stream.eligibleForAndroidLaunch(
            subscriptionInfo = state.subscriptionInfo,
            fallbackMembershipTier = state.authSession?.user?.membershipTier,
            androidTvProfile = tvProfile,
        )
    }
    val microphoneRequested = launchStreamSettings.microphoneMode != MicrophoneMode.Disabled
    val initialMicrophonePermissionGranted = remember(session?.sessionId, microphoneRequested) {
        !microphoneRequested ||
            ContextCompat.checkSelfPermission(context, Manifest.permission.RECORD_AUDIO) ==
            PackageManager.PERMISSION_GRANTED
    }
    var microphonePermissionGranted by remember(session?.sessionId, microphoneRequested) {
        mutableStateOf(initialMicrophonePermissionGranted)
    }
    var microphonePermissionResolved by remember(session?.sessionId, microphoneRequested) {
        mutableStateOf(!microphoneRequested || initialMicrophonePermissionGranted)
    }
    var microphoneEnabled by remember(session?.sessionId) { mutableStateOf(false) }
    val microphonePermissionLauncher = rememberLauncherForActivityResult(
        ActivityResultContracts.RequestPermission(),
    ) { granted ->
        microphonePermissionGranted = granted
        microphonePermissionResolved = true
        if (!granted) {
            Toast.makeText(
                context,
                context.getString(R.string.settings_microphone_permission_denied),
                Toast.LENGTH_LONG,
            ).show()
        }
    }
    val streamSettings = launchStreamSettings.copy(
        mouseSensitivity = state.settings.stream.mouseSensitivity,
        mouseAcceleration = state.settings.stream.mouseAcceleration,
        streamSharpeningEnabled = launchStreamSettings.streamSharpeningEnabled && state.settings.stream.streamSharpeningEnabled,
        streamSharpeningAmount = state.settings.stream.streamSharpeningAmount,
        mouseScrollSensitivity = state.settings.stream.mouseScrollSensitivity,
    )
    val streamTransportIdentity = session?.nativeStreamTransportIdentity()
    val statsAlignment = when (state.settings.streamStatsPosition) {
        StreamStatsPosition.Left -> Alignment.TopStart
        StreamStatsPosition.Center -> Alignment.TopCenter
        StreamStatsPosition.Right -> Alignment.TopEnd
    }
    val openStreamKeyboard = {
        NativeStreamInputRouter.setStreamUiActive(true)
        controlsOpen = false
        exitConfirmOpen = false
        physicalControllerPromptOpen = false
        inputModePromptOpen = null
        keyboardOpen = true
    }
    val dismissStreamGuide = {
        streamGuideOpen = false
        if (!state.settings.androidStreamGuideDismissed) {
            viewModel.updateSettings(state.settings.copy(androidStreamGuideDismissed = true))
        }
    }
    val openControlsForGuide = {
        // Claim UI routing before Compose replaces the launcher with the panel. Waiting for the
        // keyed effect below leaves a short window where native touch can forward the activating
        // gesture into the game or retarget its trailing event into the newly opened menu.
        NativeStreamInputRouter.setStreamUiActive(true)
        keyboardOpen = false
        exitConfirmOpen = false
        physicalControllerPromptOpen = false
        inputModePromptOpen = null
        if (streamGuideOpen && streamGuideStep == StreamGuideStep.OpenControls) {
            streamGuideStep = StreamGuideStep.PressDone
        }
        controlsOpen = true
    }
    LaunchedEffect(state.remoteStreamMenuRequestToken) {
        if (state.remoteStreamMenuRequestToken > 0 && streamReady) {
            openControlsForGuide()
        }
    }
    LaunchedEffect(state.remoteStatsToggleRequestToken) {
        if (state.remoteStatsToggleRequestToken > 0 && streamReady) {
            statsVisible = !statsVisible
        }
    }
    val streamOverlayOpen = controlsOpen || exitConfirmOpen || keyboardOpen || streamGuideOpen ||
        physicalControllerPromptOpen || inputModePromptOpen != null || touchLayoutEditing
    val streamKeyboardImeVisible = keyboardOpen && WindowInsets.ime.getBottom(density) > 0
    val externalMousePointerCaptureActive = shouldEnableExternalMousePointerCapture(
        streamReady = streamReady,
        streamOverlayOpen = streamOverlayOpen,
        pointerLockEnabled = state.settings.externalMousePointerLock,
    )
    val handleStreamBack = {
        when {
            streamGuideOpen && streamGuideStep == StreamGuideStep.OpenControls -> openControlsForGuide()
            streamGuideOpen && streamGuideStep == StreamGuideStep.PressDone && controlsOpen -> {
                controlsOpen = false
                dismissStreamGuide()
            }
            streamGuideOpen -> dismissStreamGuide()
            exitConfirmOpen -> exitConfirmOpen = false
            keyboardOpen -> keyboardOpen = false
            inputModePromptOpen != null -> inputModePromptOpen = null
            physicalControllerPromptOpen -> physicalControllerPromptOpen = false
            controlsOpen -> controlsOpen = false
            else -> {
                NativeStreamInputRouter.setStreamUiActive(true)
                controlsOpen = true
            }
        }
    }
    BackHandler(enabled = streamReady) {
        handleStreamBack()
    }
    val client = remember {
        NativeStreamClient(
            context = context.applicationContext,
            onState = {
                streamState = it
                viewModel.recordNativeStreamState(it)
                if (it == "Streaming") viewModel.markStreamConnected()
            },
            onError = {
                streamState = it
                viewModel.markStreamError(it)
            },
            onSessionRecoveryRequired = {
                streamState = it
                viewModel.recoverStreamSession(it)
            },
            onFirstVideoFrameRendered = {
                markInitialVideoFrameRendered()
            },
            onStats = {
                streamStats = it
                viewModel.updateStreamRuntimeStats(it)
            },
            onControllerMouseAssistChanged = {
                controllerMouseAssistEnabled = it
            },
        )
    }

    DisposableEffect(Unit) {
        val decor = activity?.window?.decorView
        NativeStreamInputRouter.attach(client)
        NativeStreamInputRouter.setAndroidTvProfile(tvProfile)
        onDispose {
            if (Build.VERSION.SDK_INT >= 26) {
                decor?.let(AndroidPointerCapture::release)
            }
            NativeStreamInputRouter.clearUiTouchPassthroughBounds()
            NativeStreamInputRouter.clearStreamPanelTouchPassthroughBounds()
            NativeStreamInputRouter.setSystemMenuHandler(null)
            NativeStreamInputRouter.setSystemBackHandler(null)
            NativeStreamInputRouter.setAndroidTvProfile(false)
            NativeStreamInputRouter.setStreamUiActive(false)
            NativeStreamInputRouter.setTouchControllerVisible(false)
            client.setVirtualControllerVisible(false)
            client.setTouchMouseEnabled(false)
            NativeStreamInputRouter.detach(client)
            client.release()
        }
    }
    DisposableEffect(audioController) {
        onDispose {
            audioController.release()
        }
    }

    LaunchedEffect(streamReady, streamOverlayOpen, streamGuideOpen, streamGuideStep, touchLayoutEditing) {
        NativeStreamInputRouter.setStreamUiActive(streamReady && streamOverlayOpen)
        NativeStreamInputRouter.setSystemMenuHandler {
            openControlsForGuide()
        }
        NativeStreamInputRouter.setSystemBackHandler {
            handleStreamBack()
        }
    }

    LaunchedEffect(client, tvProfile) {
        client.updateAndroidTvProfile(tvProfile)
        client.updateControllerMouseAssistAutoArm(tvProfile)
    }

    // StreamScreen owns the effective controller/mouse modes even when TouchOverlay is absent.
    // Re-sync on every session so closeTransport(clearInputState=false) cannot carry stale virtual
    // controller presence into a Finger Mouse-only session.
    LaunchedEffect(client, session?.sessionId, touchControlsVisible) {
        client.setVirtualControllerVisible(touchControlsVisible)
        NativeStreamInputRouter.setTouchControllerVisible(touchControlsVisible)
    }

    LaunchedEffect(streamReady, session?.sessionId, controlsOpen) {
        while (streamReady && controlsOpen) {
            liveBitrateLimitKbps = client.liveBitrateLimitKbps
            delay(1000L)
        }
    }

    LaunchedEffect(streamReady, state.settings.androidStreamGuideDismissed, session?.sessionId) {
        val shouldOpenGuide = streamReady && !state.settings.androidStreamGuideDismissed
        streamGuideOpen = shouldOpenGuide
        if (shouldOpenGuide) {
            streamGuideStep = StreamGuideStep.OpenControls
        }
    }

    LaunchedEffect(controlsOpen, streamGuideOpen, streamGuideStep) {
        if (controlsOpen && streamGuideOpen && streamGuideStep == StreamGuideStep.OpenControls) {
            streamGuideStep = StreamGuideStep.PressDone
        }
    }

    LaunchedEffect(
        physicalControllerConnected,
        touchControlsSuppressedByPhysicalController,
        streamGuideOpen,
        controlsOpen,
        exitConfirmOpen,
        keyboardOpen,
        inputModePromptOpen,
        pendingInputModePrompt,
    ) {
        if (!physicalControllerConnected) {
            showTouchControlsWithPhysicalController = false
            physicalControllerPromptOpen = false
            return@LaunchedEffect
        }
        if (
            !tvProfile &&
            touchControlsSuppressedByPhysicalController &&
            !state.settings.androidPhysicalControllerPromptDismissed &&
            !physicalControllerPromptHandled &&
            !streamGuideOpen &&
            !controlsOpen &&
            !exitConfirmOpen &&
            !keyboardOpen &&
            inputModePromptOpen == null &&
            pendingInputModePrompt == null
        ) {
            physicalControllerPromptOpen = true
        }
    }

    LaunchedEffect(streamReady, physicalKeyboardMouseConnected, session?.sessionId) {
        if (!streamReady) return@LaunchedEffect
        if (!keyboardMouseBaselineCaptured) {
            keyboardMouseBaselineCaptured = true
            previousKeyboardMouseConnected = physicalKeyboardMouseConnected
            if (physicalKeyboardMouseConnected) {
                streamInputMode = StreamInputMode.KeyboardMouse
            }
            return@LaunchedEffect
        }
        if (physicalKeyboardMouseConnected == previousKeyboardMouseConnected) {
            return@LaunchedEffect
        }
        previousKeyboardMouseConnected = physicalKeyboardMouseConnected
        inputModePromptOpen = null
        pendingInputModePrompt = streamInputModePromptForConnectionChange(
            currentMode = streamInputMode,
            keyboardMouseConnected = physicalKeyboardMouseConnected,
            nativeTouchProvisionedForSession = nativeTouchProvisionedForSession,
        )
    }

    LaunchedEffect(physicalMouseConnected) {
        if (!physicalMouseConnected) {
            showTouchControlsWithPhysicalMouse = false
        }
    }

    LaunchedEffect(
        pendingInputModePrompt,
        streamGuideOpen,
        controlsOpen,
        exitConfirmOpen,
        keyboardOpen,
        physicalControllerPromptOpen,
        touchLayoutEditing,
    ) {
        val prompt = pendingInputModePrompt ?: return@LaunchedEffect
        if (
            !streamGuideOpen &&
            !controlsOpen &&
            !exitConfirmOpen &&
            !keyboardOpen &&
            !physicalControllerPromptOpen &&
            !touchLayoutEditing
        ) {
            inputModePromptOpen = prompt
            pendingInputModePrompt = null
        }
    }

    LaunchedEffect(streamReady, state.settings.sessionCounterEnabled, session?.sessionId, sessionStartedAtMs, smartSessionLimit) {
        var previousRemainingSeconds: Int? = null
        val sentSessionWarnings = mutableSetOf<Int>()
        while (streamReady && state.settings.sessionCounterEnabled) {
            val nowMs = System.currentTimeMillis()
            timerNowMs = nowMs
            val remainingSeconds = sessionRemainingSeconds(smartSessionLimit, sessionStartedAtMs, nowMs)
            sessionWarningThresholdCrossed(previousRemainingSeconds, remainingSeconds)?.let { thresholdSeconds ->
                if (sentSessionWarnings.add(thresholdSeconds)) {
                    Toast.makeText(
                        context,
                        "${formatSessionWarningThreshold(thresholdSeconds)} left in this session",
                        Toast.LENGTH_SHORT,
                    ).show()
                }
            }
            previousRemainingSeconds = remainingSeconds
            delay(1000L)
        }
    }

    // Also gated on nativeTouchActive: dispatchTouch would take the native branch first anyway, but
    // leaving two input modes both flagged "enabled" is how they end up fighting later.
    LaunchedEffect(streamReady, touchInputEnabled, state.settings.androidTouch.mousePad, nativeTouchActive) {
        NativeStreamInputRouter.setTouchMouseEnabled(touchMouseActive)
        client.setTouchMouseEnabled(touchMouseActive)
    }
    // Gated on touchInputEnabled as well as the setting: finger touches already stop at
    // setTouchMouseEnabled during PiP, but external mouse and touchpad events reach direct click
    // through their own path and would otherwise be mapped against the tiny PiP window.
    LaunchedEffect(state.settings.androidTouch.mouseDirectClick, touchInputEnabled) {
        NativeStreamInputRouter.setMouseDirectClick(
            state.settings.androidTouch.mouseDirectClick && touchInputEnabled,
        )
    }
    LaunchedEffect(
        streamReady,
        touchInputEnabled,
        nativeTouchActive,
        physicalMouseConnected,
        state.streamGame?.id,
    ) {
        val activeGame = state.streamGame
        val enabled = streamReady && touchInputEnabled && nativeTouchActive
        NativeStreamInputRouter.setNativeTouchEnabled(enabled)
        // Records what the catalog says about this game even when we leave touch off, so the fixed
        // list in NativeTouchGames.kt can be filled in — and eventually retired — from real data.
        if (activeGame != null && streamReady) {
            NativeInputDiagnostics.add(
                nativeTouchDiagnostics(
                    game = activeGame,
                    enabled = enabled,
                    physicalMouseConnected = physicalMouseConnected,
                ),
            )
        }
    }

    LaunchedEffect(streamReady, touchInputEnabled, state.settings.androidTouch.mousePad, nativeTouchActive, controlsOpen, exitConfirmOpen, keyboardOpen, streamGuideOpen, touchControlsVisible) {
        NativeStreamInputRouter.setCaptureAllTouch(
            streamReady &&
                touchInputEnabled &&
                (state.settings.androidTouch.mousePad || nativeTouchActive) &&
                !controlsOpen &&
                !exitConfirmOpen &&
                !keyboardOpen &&
                !streamGuideOpen,
        )
    }
    DisposableEffect(Unit) {
        onDispose {
            NativeStreamInputRouter.setCaptureAllTouch(false)
        }
    }

    LaunchedEffect(streamReady, microphoneRequested, microphonePermissionResolved, session?.sessionId) {
        if (streamReady && microphoneRequested && !microphonePermissionResolved) {
            microphonePermissionLauncher.launch(Manifest.permission.RECORD_AUDIO)
        }
    }
    LaunchedEffect(
        streamTransportIdentity,
        streamReady,
        microphonePermissionGranted,
        microphonePermissionResolved,
    ) {
        if (session != null && streamReady && microphonePermissionResolved) {
            val captureMicrophone = shouldCaptureMicrophone(
                mode = launchStreamSettings.microphoneMode,
                permissionGranted = microphonePermissionGranted,
            )
            // Promote the already-running stream service while the activity is visible and before
            // WebRTC opens AudioRecord. Android 14+ rejects that promotion from the background.
            onMicrophoneCaptureActiveChange(captureMicrophone)
            microphoneEnabled = captureMicrophone
            client.setMicrophoneEnabled(captureMicrophone)
            client.setVirtualControllerVisible(touchControlsVisible)
            client.setTouchMouseEnabled(touchMouseActive)
            client.start(
                session,
                launchStreamSettings.copy(
                    microphoneMode = if (captureMicrophone) {
                        launchStreamSettings.microphoneMode
                    } else {
                        MicrophoneMode.Disabled
                    },
                ),
            )
        }
    }
    LaunchedEffect(client, controllerMouseEmulationEnabled, streamReady) {
        if (streamReady) {
            client.setControllerMouseEmulationActive(controllerMouseEmulationEnabled)
        }
    }
    val activeStreamMode = activeStreamModeStatus(
        requestedSettings = requestedStreamSettings,
        transportSettings = launchStreamSettings,
        decodedResolution = streamStats.resolution,
        serverNegotiatedResolution = session?.monitorSnapshot?.returnedResolution
            ?: session?.negotiatedStreamProfile?.resolution,
        serverFinalSelectedResolution = session?.monitorSnapshot?.finalSelectedResolution,
    )
    LaunchedEffect(
        session?.sessionId,
        streamReady,
        activeStreamMode,
    ) {
        if (streamReady && activeStreamMode != null) {
            viewModel.recordActiveStreamMode(activeStreamMode)
        }
    }
    LaunchedEffect(streamReady, streamStats) {
        val candidate = if (streamReady) {
            streamNetworkWarning(streamStats)
        } else {
            null
        }
        networkWarningGate.update(candidate)?.let { warning ->
            networkNotice = warning
            networkNoticeSequence += 1
        }
    }
    LaunchedEffect(networkNoticeSequence) {
        if (networkNoticeSequence <= 0) return@LaunchedEffect
        val displayedSequence = networkNoticeSequence
        delay(STREAM_NETWORK_NOTICE_DURATION_MS)
        if (networkNoticeSequence == displayedSequence) networkNotice = null
    }

    Box(Modifier.fillMaxSize().background(Color.Black)) {
        if (state.activeSessionDecision != null) {
            ActiveSessionDecisionScreen(
                state = state,
                onResumeSession = viewModel::resumeActiveSession,
                onReplaceSession = viewModel::terminateActiveSessionAndStartNew,
                onCancel = viewModel::dismissActiveSessionDecision,
            )
        } else if (session == null && state.streamStatus != "idle") {
            QueueLoadingScreen(state, viewModel)
        } else if (session == null) {
            NoActiveStreamScreen(
                canResumeSession = state.activeSession != null,
                canEndSession = state.authSession != null,
                onBack = { viewModel.setPage(AppPage.Home) },
                onResumeSession = viewModel::resumeActiveSession,
                onEndSession = viewModel::stopStream,
            )
        } else if (!streamReady) {
            QueueLoadingScreen(state, viewModel)
        } else {
            StreamVideoSurface(
                client = client,
                settings = streamSettings,
                viewportSettings = requestedStreamSettings,
                decodedResolution = streamStats.resolution,
                serverNegotiatedResolution = session.monitorSnapshot?.returnedResolution
                    ?: session.negotiatedStreamProfile?.resolution,
                serverFinalSelectedResolution = session.monitorSnapshot?.finalSelectedResolution,
                hideExternalMousePointer = externalMousePointerCaptureActive,
                touchMouseEnabled =
                    touchMouseActive,
                pinchZoomEnabled = streamPinchZoomEnabled(
                    touchMouseEnabled =
                        touchInputEnabled && state.settings.androidTouch.mousePad && !nativeTouchActive,
                    touchControllerVisible = touchControlsVisible,
                ),
                externalMouseRoot = activity?.window?.decorView,
                onMouseCaptureInput = { (activity as? MainActivity)?.enforceStreamSystemUiFromInput() },
                stretchToFit = stretchToFit,
                vibrationEnabled = state.settings.vibrationEnabled,
                hapticsOutput = state.settings.hapticsOutput,
            )
            if (statsVisible) {
                StreamStatsPill(
                    streamStats = streamStats,
                    streamSettings = requestedStreamSettings,
                    style = state.settings.streamStatsStyle,
                    metrics = state.settings.streamStatsMetrics,
                    serverLocation = session.reportedServerZone(),
                    keyboardButtonEnabled = !state.settings.hideStreamButtons,
                    onKeyboardOpen = openStreamKeyboard,
                    modifier = Modifier.align(statsAlignment),
                )
            }
            MobileGyroscopeAim(
                client = client,
                settings = state.settings.androidTouch,
                active = streamReady && touchControlsVisible && !streamOverlayOpen,
            )
            if (networkNotice != null || activeStreamMode != null) {
                Column(
                    modifier = Modifier
                        .align(Alignment.TopCenter)
                        .padding(top = if (statsVisible && statsAlignment == Alignment.TopCenter) 48.dp else 8.dp),
                    horizontalAlignment = Alignment.CenterHorizontally,
                    verticalArrangement = Arrangement.spacedBy(6.dp),
                ) {
                    networkNotice?.let { StreamNetworkQualityNotice(it) }
                    activeStreamMode?.let { status ->
                        ActiveStreamModePill(
                            status = status,
                            recoveryReason = videoTransportFallbackReason,
                            bugReportSubmission = state.bugReportSubmission,
                            bugReportVersionCheck = state.bugReportVersionCheck,
                            update = state.androidUpdate,
                            onBugReportSubmit = viewModel::submitBugReport,
                            onBugReportReset = viewModel::resetBugReportSubmission,
                            onBugReportVersionCheck = viewModel::verifyBugReportVersion,
                            onOpenUpdate = viewModel::performAndroidUpdatePrimaryAction,
                        )
                    }
                }
            }
            if (touchControlsVisible) {
                TouchOverlay(
                    client = client,
                    touch = state.settings.androidTouch.copy(enabled = true),
                    // FLAG_IGNORE_GLOBAL_SETTING stopped working in Android 13, so the on-screen
                    // buttons went silent on any device with system touch feedback off. Drive the
                    // vibrator directly instead — see OpenNowHaptics.
                    onButtonTone = { openNowHaptics?.play(HapticCue.Activate) },
                    layoutEditing = touchLayoutEditing,
                    onSaveAllOffsets = { allOffsets ->
                        var touch = state.settings.androidTouch
                        allOffsets.forEach { (key, offset) ->
                            touch = touch.withOffset(key, offset.x, offset.y)
                        }
                        viewModel.updateSettings(state.settings.copy(androidTouch = touch))
                    },
                    modifier = Modifier.align(Alignment.BottomCenter),
                )
            }
            AnimatedVisibility(
                visible = !initialVideoFrameRendered,
                enter = fadeIn(animationSpec = tween(180)) + scaleIn(initialScale = 0.96f),
                exit = fadeOut(animationSpec = tween(180)) + scaleOut(targetScale = 0.98f),
                modifier = Modifier.align(Alignment.Center),
            ) {
                InitialStreamConnectionOverlay(
                    gameTitle = game?.title,
                    status = initialStreamConnectionStatus(streamState),
                )
            }
            if (touchLayoutEditing) {
                val doneButtonTone = playButtonTone
                Box(
                    Modifier
                        .align(Alignment.Center),
                    contentAlignment = Alignment.Center,
                ) {
                    Button(
                        onClick = {
                            doneButtonTone()
                            touchLayoutEditing = false
                        },
                        shape = RoundedCornerShape(999.dp),
                        colors = ButtonDefaults.buttonColors(
                            containerColor = MaterialTheme.colorScheme.primary,
                        ),
                        contentPadding = PaddingValues(horizontal = 28.dp, vertical = 14.dp),
                        elevation = ButtonDefaults.buttonElevation(defaultElevation = 8.dp),
                        modifier = Modifier.pointerInteropFilter { event ->
                            if (event.action == MotionEvent.ACTION_UP ||
                                event.action == MotionEvent.ACTION_DOWN
                            ) {
                                false // let Button's click handling still work
                            } else {
                                false
                            }
                        },
                    ) {
                        Icon(
                            Icons.Rounded.Check,
                            contentDescription = null,
                            modifier = Modifier.size(18.dp),
                        )
                        Spacer(Modifier.width(8.dp))
                        Text(
                            stringResource(R.string.stream_panel_done),
                            style = MaterialTheme.typography.labelLarge,
                            fontWeight = FontWeight.SemiBold,
                        )
                    }
                }
            }
            if (streamGuideOpen) {
                AnimatedLaunchOverlay(Modifier.align(Alignment.Center)) {
                    StreamFirstLaunchGuide(
                        step = streamGuideStep,
                        controlsOpen = controlsOpen,
                        touchControlsEnabled = touchControlsVisible,
                        onOpenControls = {
                            playButtonTone()
                            openControlsForGuide()
                        },
                        onSkip = {
                            playButtonTone()
                            controlsOpen = false
                            dismissStreamGuide()
                        },
                    )
                }
            }
            if (physicalControllerPromptOpen) {
                PhysicalControllerTouchControlsDialog(
                    doNotShowAgain = physicalControllerPromptDoNotShowAgain,
                    onDoNotShowAgainChange = { physicalControllerPromptDoNotShowAgain = it },
                    onOk = {
                        physicalControllerPromptHandled = true
                        physicalControllerPromptOpen = false
                        showTouchControlsWithPhysicalController = false
                        if (physicalControllerPromptDoNotShowAgain) {
                            viewModel.updateSettings(
                                state.settings.copy(androidPhysicalControllerPromptDismissed = true),
                            )
                        }
                    },
                    onUndo = {
                        physicalControllerPromptHandled = true
                        physicalControllerPromptOpen = false
                        showTouchControlsWithPhysicalController = true
                        if (physicalMouseConnected) {
                            showTouchControlsWithPhysicalMouse = true
                        }
                        if (physicalControllerPromptDoNotShowAgain) {
                            viewModel.updateSettings(
                                state.settings.copy(androidPhysicalControllerPromptDismissed = true),
                            )
                        }
                    },
                )
            }
            inputModePromptOpen?.let { prompt ->
                StreamInputModeSwitchDialog(
                    prompt = prompt,
                    onStay = { inputModePromptOpen = null },
                    onSwitch = {
                        streamInputMode = when (prompt) {
                            StreamInputModePrompt.SwitchToKeyboardMouse -> StreamInputMode.KeyboardMouse
                            StreamInputModePrompt.SwitchToNativeTouch -> StreamInputMode.NativeTouch
                        }
                        inputModePromptOpen = null
                    },
                )
            }
            // Keep the decoded frame untouched when the Quick Menu is open. A full-screen
            // translucent wash over SurfaceViewRenderer looked like a stuck grey compositor
            // layer on physical devices; the panel has its own opaque fill and border.
            AnimatedVisibility(
                visible = controlsOpen,
                enter = fadeIn() + slideInVertically(initialOffsetY = { it / 4 }) + scaleIn(initialScale = 0.96f),
                exit = fadeOut() + slideOutVertically(targetOffsetY = { it / 4 }) + scaleOut(targetScale = 0.96f),
                modifier = Modifier.align(Alignment.BottomEnd),
            ) {
                StreamControlsPanel(
                    gameTitle = game?.title ?: stringResource(R.string.settings_section_stream),
                    status = (state.queuePosition?.let { "Queue $it" } ?: streamState).takeUnless(::shouldHideStreamStatusText),
                    settings = state.settings,
                    tvProfile = tvProfile,
                    touchControlsVisible = touchControlsVisible,
                    builtInGameTouchSupported = builtInGameTouchSupported,
                    nativeTouchActive = nativeTouchActive,
                    gyroscopeAvailable = gyroscopeAvailable,
                    controllerMouseAssistEnabled = controllerMouseAssistEnabled,
                    controllerMouseEmulationEnabled = controllerMouseEmulationEnabled,
                    showSessionTimer = state.settings.sessionCounterEnabled,
                    sessionTimerLimit = smartSessionLimit,
                    sessionStartedAtMs = sessionStartedAtMs,
                    sessionNowMs = timerNowMs,
                    audioMuted = audioMuted,
                    microphoneRequested = microphoneRequested,
                    microphonePermissionGranted = microphonePermissionGranted,
                    microphoneEnabled = microphoneEnabled,
                    statsVisible = statsVisible,
                    liveBitrateLimitKbps = liveBitrateLimitKbps,
                    touchLayoutEditing = touchLayoutEditing,
                    bugReportSubmission = state.bugReportSubmission,
                    bugReportVersionCheck = state.bugReportVersionCheck,
                    update = state.androidUpdate,
                    bugReportPreflightProvider = {
                        buildBugReportPreflightDeck(
                            BugReportPreflightEvidence(
                                requestedSettings = requestedStreamSettings,
                                recommendedSettings = state.recommendedStreamSettings,
                                nativeLowLatencyDecoderEnabled = state.settings.nativeLowLatencyDecoder,
                                runtimeStats = streamStats,
                                runtimeDiagnostics = AndroidRuntimeDiagnostics.snapshot(context),
                                deliveredResolution = activeStreamMode?.displayedResolution
                                    ?: session.monitorSnapshot?.returnedResolution
                                    ?: streamStats.resolution,
                                deliveredCodec = activeStreamMode?.transportCodec?.name
                                    ?: streamStats.codec,
                                codecReport = state.codecReport,
                                androidTvProfile = tvProfile,
                                serverZone = session.reportedServerZone(),
                                manuallySelectedServer = state.manuallySelectedServerForReport,
                                inputDiagnostics = NativeInputDiagnostics.snapshot(),
                            ),
                        )
                    },
                    onAudioToggle = {
                        audioMuted = !audioMuted
                        client.setAudioMuted(audioMuted)
                    },
                    onMicrophoneToggle = {
                        if (!microphonePermissionGranted) {
                            microphonePermissionLauncher.launch(Manifest.permission.RECORD_AUDIO)
                        } else {
                            microphoneEnabled = !microphoneEnabled
                            client.setMicrophoneEnabled(microphoneEnabled)
                        }
                    },
                    onStatsToggle = {
                        statsVisible = !statsVisible
                        viewModel.updateSettings(state.settings.copy(showStatsOnLaunch = statsVisible))
                    },
                    onStatsStyleCycle = {
                        viewModel.updateSettings(state.settings.copy(streamStatsStyle = state.settings.streamStatsStyle.next()))
                    },
                    onStatsPositionCycle = {
                        viewModel.updateSettings(state.settings.copy(streamStatsPosition = state.settings.streamStatsPosition.next()))
                    },
                    onStatsMetricsChange = { metrics ->
                        viewModel.updateSettings(state.settings.copy(streamStatsMetrics = metrics))
                    },
                    onKeyboardButtonToggle = {
                        viewModel.updateSettings(
                            state.settings.copy(hideStreamButtons = !state.settings.hideStreamButtons),
                        )
                    },
                    onVibrationToggle = {
                        viewModel.updateSettings(state.settings.copy(vibrationEnabled = !state.settings.vibrationEnabled))
                    },
                    onTouchLayoutEditingToggle = {
                        touchLayoutEditing = !touchLayoutEditing
                    },
                    onKeyboardOpen = openStreamKeyboard,
                    onEsc = { client.sendKeyCode(KeyEvent.KEYCODE_ESCAPE) },
                    onEnter = { client.sendKeyCode(KeyEvent.KEYCODE_ENTER) },
                    onBackspace = { client.sendKeyCode(KeyEvent.KEYCODE_DEL) },
                    onSteamMenuOpen = {
                        controlsOpen = false
                        client.openSteamMenu()
                    },
                    onControllerMouseAssistToggle = {
                        client.setControllerMouseAssistEnabled(!controllerMouseAssistEnabled)
                    },
                    onControllerMouseEmulationToggle = {
                        val newState = !controllerMouseEmulationEnabled
                        controllerMouseEmulationEnabled = newState
                        client.setControllerMouseEmulationActive(newState)
                    },
                    onExit = {
                        controlsOpen = false
                        exitConfirmOpen = true
                    },
                    onTouchControlsToggle = {
                        when {
                            nativeTouchActive -> {
                                preferVirtualController = true
                                if (physicalControllerConnected) {
                                    showTouchControlsWithPhysicalController = true
                                }
                                if (physicalMouseConnected) {
                                    showTouchControlsWithPhysicalMouse = true
                                }
                                if (!state.settings.androidTouch.enabled) {
                                    viewModel.updateSettings(
                                        state.settings.copy(
                                            androidTouch = state.settings.androidTouch.copy(enabled = true),
                                        ),
                                    )
                                }
                            }
                            preferVirtualController && nativeTouchAvailable && touchControlsVisible -> {
                                // Turning the overlay back off restores the game's built-in touch
                                // without changing the player's persisted controller preference.
                                preferVirtualController = false
                            }
                            physicalControllerConnected && !touchControlsVisible -> {
                                showTouchControlsWithPhysicalController = true
                                if (physicalMouseConnected) {
                                    showTouchControlsWithPhysicalMouse = true
                                }
                                if (!state.settings.androidTouch.enabled) {
                                    viewModel.updateSettings(
                                        state.settings.copy(
                                            androidTouch = state.settings.androidTouch.copy(enabled = true),
                                        ),
                                    )
                                }
                            }
                            physicalMouseConnected && !touchControlsVisible -> {
                                showTouchControlsWithPhysicalMouse = true
                                if (!state.settings.androidTouch.enabled) {
                                    viewModel.updateSettings(
                                        state.settings.copy(
                                            androidTouch = state.settings.androidTouch.copy(enabled = true),
                                        ),
                                    )
                                }
                            }
                            else -> {
                                viewModel.updateSettings(
                                    state.settings.copy(
                                        androidTouch = state.settings.androidTouch.copy(
                                            enabled = !state.settings.androidTouch.enabled,
                                        ),
                                    ),
                                )
                            }
                        }
                    },
                    onMousePadToggle = {
                        viewModel.updateSettings(
                            state.settings.copy(
                                androidTouch = state.settings.androidTouch.copy(mousePad = !state.settings.androidTouch.mousePad),
                            ),
                        )
                    },
                    onMouseDirectClickToggle = {
                        viewModel.updateSettings(
                            state.settings.copy(
                                androidTouch = state.settings.androidTouch.copy(mouseDirectClick = !state.settings.androidTouch.mouseDirectClick),
                            ),
                        )
                    },
                    onToggleTouchControllerStyle = {
                        viewModel.updateSettings(
                            state.settings.copy(
                                androidTouch = state.settings.androidTouch.copy(
                                    touchControllerStyle = nextTouchControllerStyle(
                                        state.settings.androidTouch.touchControllerStyle,
                                    ),
                                ),
                            ),
                        )
                    },
                    onTouchButtonLabelsToggle = {
                        viewModel.updateSettings(
                            state.settings.copy(
                                androidTouch = state.settings.androidTouch.copy(
                                    touchButtonLabels = !state.settings.androidTouch.touchButtonLabels,
                                ),
                            ),
                        )
                    },
                    onJoystickModeToggle = {
                        val nextMode = if (state.settings.androidTouch.joystickMode == TouchJoystickMode.Fixed) {
                            TouchJoystickMode.Dynamic
                        } else {
                            TouchJoystickMode.Fixed
                        }
                        viewModel.updateSettings(
                            state.settings.copy(
                                androidTouch = state.settings.androidTouch.copy(joystickMode = nextMode),
                            ),
                        )
                    },
                    onTouchAimModeToggle = {
                        val nextMode = if (state.settings.androidTouch.aimMode == TouchAimMode.LockJoystick) {
                            TouchAimMode.LockZone
                        } else {
                            TouchAimMode.LockJoystick
                        }
                        viewModel.updateSettings(
                            state.settings.copy(
                                androidTouch = state.settings.androidTouch.copy(aimMode = nextMode),
                            ),
                        )
                    },
                    onJoystickDeadZoneChange = { value ->
                        viewModel.updateSettings(
                            state.settings.copy(
                                androidTouch = state.settings.androidTouch.copy(joystickDeadZone = value),
                            ),
                        )
                    },
                    onSharpeningToggle = {
                        viewModel.updateStreamSettings { settings ->
                            settings.copy(streamSharpeningEnabled = !settings.streamSharpeningEnabled)
                        }
                    },
                    onSharpeningAmountChange = { value ->
                        viewModel.updateStreamSettings { settings ->
                            settings.copy(streamSharpeningAmount = value)
                        }
                    },
                    onStretchToFitToggle = {
                        val next = !state.settings.stretchStreamToFit
                        viewModel.updateSettings(
                            state.settings.copy(
                                legacyCropStreamToFill = false,
                                stretchStreamToFit = next,
                            ),
                        )
                    },
                    onMaxBitrateChange = { value ->
                        viewModel.updateStreamSettings { s -> s.copy(maxBitrateMbps = value) }
                        // Preserve the active WSS/ICE transport. The new b=AS ceiling is queued for
                        // the next legitimate offer because replacing a healthy transport here can
                        // strand the allocated cloud session on a stale signaling endpoint.
                        client.updateBitrateLimit(value * 1000)
                        // Optimistic indicator for the requested next-offer ceiling.
                        liveBitrateLimitKbps = value * 1000
                    },
                    onTouchScaleChange = { value ->
                        viewModel.updateSettings(state.settings.copy(androidTouch = state.settings.androidTouch.copy(scale = value)))
                    },
                    onButtonScaleChange = { value ->
                        viewModel.updateSettings(state.settings.copy(androidTouch = state.settings.androidTouch.copy(buttonScale = value)))
                    },
                    onStickScaleChange = { value ->
                        viewModel.updateSettings(state.settings.copy(androidTouch = state.settings.androidTouch.copy(stickScale = value)))
                    },
                    onOpacityChange = { value ->
                        viewModel.updateSettings(state.settings.copy(androidTouch = state.settings.androidTouch.copy(opacity = value)))
                    },
                    onMouseSensitivityChange = { value ->
                        viewModel.updateStreamSettings { s -> s.copy(mouseSensitivity = value) }
                    },
                    onMouseScrollSensitivityChange = { value ->
                        viewModel.updateStreamSettings { s -> s.copy(mouseScrollSensitivity = value) }
                    },
                    onTouchEdgePaddingChange = { value ->
                        viewModel.updateSettings(state.settings.copy(androidTouch = state.settings.androidTouch.copy(edgePaddingDp = value)))
                    },
                    onTouchBottomPaddingChange = { value ->
                        viewModel.updateSettings(state.settings.copy(androidTouch = state.settings.androidTouch.copy(bottomPaddingDp = value)))
                    },
                    onTouchLeftOffsetChange = { value ->
                        viewModel.updateSettings(state.settings.copy(androidTouch = state.settings.androidTouch.copy(leftOffsetYDp = value)))
                    },
                    onTouchRightOffsetChange = { value ->
                        viewModel.updateSettings(state.settings.copy(androidTouch = state.settings.androidTouch.copy(rightOffsetYDp = value)))
                    },
                    onTouchLayoutReset = {
                        viewModel.updateSettings(
                            state.settings.copy(
                                androidTouch = state.settings.androidTouch.withResetOffsets()
                            )
                        )
                    },
                    onTouchSettingsChange = { touch ->
                        viewModel.updateSettings(state.settings.copy(androidTouch = touch))
                    },
                    onBugReportSubmit = { title, description, knownIssueOverrideKey ->
                        viewModel.submitBugReport(title, description, knownIssueOverrideKey)
                    },
                    onBugReportReset = viewModel::resetBugReportSubmission,
                    onBugReportVersionCheck = viewModel::verifyBugReportVersion,
                    onOpenUpdate = viewModel::performAndroidUpdatePrimaryAction,
                    onButtonTone = playButtonTone,
                    highlightDone = streamGuideOpen && streamGuideStep == StreamGuideStep.PressDone,
                    onClose = {
                        controlsOpen = false
                        if (streamGuideOpen && streamGuideStep == StreamGuideStep.PressDone) {
                            dismissStreamGuide()
                        }
                    },
                )
            }
            if (keyboardOpen) {
                if (streamKeyboardImeVisible) {
                    Box(
                        Modifier
                            .matchParentSize()
                            .pointerInput(Unit) {
                                detectTapGestures {
                                    keyboardController?.hide()
                                    focusManager.clearFocus(force = true)
                                }
                            },
                    )
                }
                AnimatedLaunchOverlay(
                    modifier = Modifier
                        .align(Alignment.BottomCenter)
                        .imePadding(),
                ) {
                    StreamKeyboardBar(
                        value = keyboardValue,
                        clearConfirmationEnabled = !state.settings.streamKeyboardClearConfirmationDisabled,
                        onValueChange = { next ->
                            if (next.text.length <= MAX_STREAM_KEYBOARD_TEXT_LENGTH) {
                                client.syncText(keyboardSyncedText, next.text)
                                keyboardValue = next
                                keyboardSyncedText = next.text
                            }
                        },
                        onClear = {
                            client.clearText()
                            keyboardValue = TextFieldValue()
                            keyboardSyncedText = ""
                        },
                        onDisableClearConfirmation = {
                            viewModel.updateSettings(
                                state.settings.copy(streamKeyboardClearConfirmationDisabled = true),
                            )
                        },
                        onEnter = {
                            client.sendTextControlKey(KeyEvent.KEYCODE_ENTER)
                            if (keyboardSyncedText != null) {
                                keyboardValue = TextFieldValue()
                                keyboardSyncedText = null
                            }
                        },
                        onEsc = { client.sendKeyCode(KeyEvent.KEYCODE_ESCAPE) },
                        onDone = { keyboardOpen = false },
                    )
                }
            }
            if (exitConfirmOpen) {
                AnimatedLaunchOverlay(Modifier.align(Alignment.Center)) {
                    StreamExitConfirmation(
                        gameTitle = game?.title ?: "this game",
                        onKeepPlaying = { exitConfirmOpen = false },
                        onExit = {
                            exitConfirmOpen = false
                            viewModel.stopStream()
                        },
                    )
                }
            }
        }
    }
}

internal fun shouldShowAndroidTouchControls(
    tvProfile: Boolean,
    touchInputEnabled: Boolean,
    touchControlsEnabled: Boolean,
    suppressedByPhysicalController: Boolean,
    physicalMouseConnected: Boolean = false,
    allowWithPhysicalMouse: Boolean = false,
): Boolean =
    !tvProfile &&
        touchInputEnabled &&
        touchControlsEnabled &&
        !suppressedByPhysicalController &&
        (!physicalMouseConnected || allowWithPhysicalMouse)

private data class SessionTimerDisplay(
    val label: String,
    val value: String,
    val detail: String,
    val progress: Float,
    val warning: Boolean,
)

internal enum class StreamGuideStep {
    OpenControls,
    PressDone,
}

private fun sessionTimerDisplay(limit: SmartSessionLimit, startedAtMs: Long, nowMs: Long): SessionTimerDisplay {
    val elapsedSeconds = sessionElapsedSeconds(startedAtMs, nowMs)
    val limitSeconds = limit.limitHours * 60 * 60
    val remainingSeconds = sessionRemainingSeconds(limit, startedAtMs, nowMs)
    val warning = remainingSeconds <= 10 * 60
    val progress = if (limitSeconds > 0) (elapsedSeconds.toFloat() / limitSeconds).coerceIn(0f, 1f) else 0f
    return when (limit.mode) {
        SessionTimerMode.Countdown -> SessionTimerDisplay(
            label = "${limit.tierLabel} countdown",
            value = formatSessionTimerDuration(remainingSeconds),
            detail = "${limit.limitHours}h session limit",
            progress = progress,
            warning = warning,
        )
        SessionTimerMode.Stopwatch -> SessionTimerDisplay(
            label = "${limit.tierLabel} session",
            value = "${formatSessionTimerDuration(elapsedSeconds)} / ${limit.limitHours}h",
            detail = "Session stopwatch",
            progress = progress,
            warning = warning,
        )
    }
}

@Composable
internal fun StreamSessionTimerMenuRow(
    limit: SmartSessionLimit,
    startedAtMs: Long,
    nowMs: Long,
    modifier: Modifier = Modifier,
) {
    val display = sessionTimerDisplay(limit, startedAtMs, nowMs)
    val progressColor = when {
        display.warning -> OpenNowPalette.StatusNotice
        else -> MaterialTheme.colorScheme.primary
    }
    Column(
        modifier
            .fillMaxWidth()
            .clip(RoundedCornerShape(12.dp))
            .background(Color.White.copy(alpha = 0.06f))
            .padding(horizontal = 12.dp, vertical = 10.dp),
        verticalArrangement = Arrangement.spacedBy(7.dp),
    ) {
        Row(verticalAlignment = Alignment.CenterVertically, modifier = Modifier.fillMaxWidth()) {
            Column(Modifier.weight(1f)) {
                Text(stringResource(R.string.session_timer), fontWeight = FontWeight.SemiBold)
                Text(display.label, color = TextMuted, style = MaterialTheme.typography.labelSmall)
            }
            Text(
                display.value,
                color = if (display.warning) OpenNowPalette.StatusNotice else TextPrimary,
                style = MaterialTheme.typography.labelMedium,
                fontWeight = FontWeight.Bold,
                maxLines = 1,
                overflow = TextOverflow.Ellipsis,
            )
        }
        Box(
            Modifier
                .fillMaxWidth()
                .height(4.dp)
                .clip(RoundedCornerShape(999.dp))
                .background(Color.White.copy(alpha = 0.12f)),
        ) {
            Box(
                Modifier
                    .fillMaxWidth(display.progress)
                    .height(4.dp)
                    .background(progressColor),
            )
        }
        Text(display.detail, color = TextMuted, style = MaterialTheme.typography.labelSmall)
    }
}

internal fun formatSessionTimerDuration(totalSeconds: Int): String {
    val seconds = totalSeconds.coerceAtLeast(0)
    val hours = seconds / 3600
    val minutes = (seconds % 3600) / 60
    val remainingSeconds = seconds % 60
    return if (hours > 0) {
        "%d:%02d:%02d".format(Locale.US, hours, minutes, remainingSeconds)
    } else {
        "%d:%02d".format(Locale.US, minutes, remainingSeconds)
    }
}

private fun formatSessionWarningThreshold(thresholdSeconds: Int): String {
    val minutes = thresholdSeconds / 60
    return if (minutes == 1) "1 minute" else "$minutes minutes"
}

@Composable
private fun StreamVideoSurface(
    client: NativeStreamClient,
    settings: StreamSettings,
    viewportSettings: StreamSettings,
    decodedResolution: String?,
    serverNegotiatedResolution: String?,
    serverFinalSelectedResolution: String?,
    hideExternalMousePointer: Boolean,
    touchMouseEnabled: Boolean,
    pinchZoomEnabled: Boolean,
    externalMouseRoot: android.view.View?,
    onMouseCaptureInput: () -> Unit,
    stretchToFit: Boolean,
    vibrationEnabled: Boolean,
    hapticsOutput: HapticsOutputPreference,
    modifier: Modifier = Modifier,
) {
    val rootView = LocalView.current
    val configuration = LocalConfiguration.current
    val pointerRootView = externalMouseRoot ?: rootView
    val currentOnMouseCaptureInput by rememberUpdatedState(onMouseCaptureInput)
    var zoomScale by remember { mutableFloatStateOf(1f) }
    var zoomOffset by remember { mutableStateOf(Offset.Zero) }
    var viewportSize by remember { mutableStateOf(IntSize.Zero) }
    val streamAspectRatio = remember(
        viewportSettings.resolution,
        viewportSettings.aspectRatio,
        decodedResolution,
        serverNegotiatedResolution,
        serverFinalSelectedResolution,
    ) {
        streamRendererAspectRatio(
            settings = viewportSettings,
            decodedResolution = decodedResolution,
            serverNegotiatedResolution = serverNegotiatedResolution,
            serverFinalSelectedResolution = serverFinalSelectedResolution,
        )
    }
    val stretchContentAspectRatio = remember(decodedResolution, streamAspectRatio) {
        streamStretchContentAspectRatio(
            selectedAspectRatio = streamAspectRatio,
            decodedResolution = decodedResolution,
        )
    }
    val viewportAspectRatio = remember(viewportSize) {
        if (viewportSize.width > 0 && viewportSize.height > 0) {
            viewportSize.width.toFloat() / viewportSize.height.toFloat()
        } else {
            0f
        }
    }
    val rendererModifier = if (viewportAspectRatio <= 0f) {
        Modifier.fillMaxSize()
    } else if (viewportAspectRatio > streamAspectRatio) {
        // Screen is wider than stream (e.g. 2400×1080 screen, 1920×1080 stream).
        // Fit by height so the renderer has no black bars internally; horizontal
        // stretch (if enabled) is applied later via View.scaleX.
        Modifier
            .fillMaxHeight()
            .aspectRatio(streamAspectRatio)
    } else {
        // Screen is taller than stream — fit by width; vertical stretch via scaleY.
        Modifier
            .fillMaxWidth()
            .aspectRatio(streamAspectRatio)
    }

    // SCALE_ASPECT_FIT preserves every decoded pixel. Stretching the View on only
    // the mismatching axis removes the bars without cropping HUD or edge content.
    val stretchScale = remember(stretchToFit, viewportAspectRatio, stretchContentAspectRatio) {
        streamStretchScale(
            enabled = stretchToFit,
            viewportAspectRatio = viewportAspectRatio,
            streamAspectRatio = stretchContentAspectRatio,
        )
    }
    LaunchedEffect(
        viewportSettings.resolution,
        viewportSettings.aspectRatio,
        settings.streamSharpeningEnabled,
        touchMouseEnabled,
        pinchZoomEnabled,
        stretchToFit,
        streamAspectRatio,
        configuration.orientation,
        configuration.screenWidthDp,
        configuration.screenHeightDp,
    ) {
        zoomScale = 1f
        zoomOffset = Offset.Zero
    }
    LaunchedEffect(stretchToFit) {
        NativeStreamInputRouter.setStretchToFit(stretchToFit)
    }
    LaunchedEffect(zoomScale, zoomOffset) {
        NativeStreamInputRouter.setPresentationTransform(
            zoomScale = zoomScale,
            translationX = zoomOffset.x,
            translationY = zoomOffset.y,
        )
    }
    LaunchedEffect(
        settings.mouseSensitivity,
        settings.mouseScrollSensitivity,
        settings.mouseAcceleration,
        settings.streamSharpeningEnabled,
        settings.streamSharpeningAmount,
        stretchToFit,
        vibrationEnabled,
        hapticsOutput,
    ) {
        client.applyLiveSettings(settings, vibrationEnabled, hapticsOutput, stretchToFit)
    }
    LaunchedEffect(streamAspectRatio) {
        NativeStreamInputRouter.setRenderingAspectRatio(streamAspectRatio)
    }
    DisposableEffect(client, rootView, pointerRootView, hideExternalMousePointer) {
        NativeStreamInputRouter.setExternalMousePointerCaptureEnabled(hideExternalMousePointer)
        pointerRootView.configureAndroidMousePointerCapture(hideExternalMousePointer, { currentOnMouseCaptureInput() }) { event ->
            client.dispatchMotion(event)
        }
        if (hideExternalMousePointer) {
            pointerRootView.hideAndroidPointerTree()
        } else {
            pointerRootView.showAndroidPointerTree()
        }
        onDispose {
            NativeStreamInputRouter.setPresentationTransform(1f, 0f, 0f)
            NativeStreamInputRouter.setExternalMousePointerCaptureEnabled(false)
            pointerRootView.clearAndroidMousePointerCapture()
            pointerRootView.showAndroidPointerTree()
        }
    }
    Box(
        modifier
            .fillMaxSize()
            .background(Color.Black)
            .onSizeChanged {
                if (viewportSize != it) {
                    viewportSize = it
                    zoomScale = 1f
                    zoomOffset = Offset.Zero
                } else {
                    zoomOffset = clampStreamZoomOffset(zoomOffset, zoomScale, it)
                }
            }
            .clipToBounds(),
        contentAlignment = Alignment.Center,
    ) {
        Box(
            Modifier
                .matchParentSize()
                .graphicsLayer {
                    scaleX = zoomScale
                    scaleY = zoomScale
                    translationX = zoomOffset.x
                    translationY = zoomOffset.y
                },
            contentAlignment = Alignment.Center,
        ) {
            // The sharpness drawer is always attached (Streaming.kt createRenderer), so toggling
            // sharpening mid-session is handled entirely by the update lambda below via
            // applyLiveSettings → drawer.amount. Re-keying this AndroidView on that flag used to
            // tear down and recreate the SurfaceViewRenderer on every toggle, causing a visible
            // restart/flicker of the video surface.
            AndroidView(
                modifier = rendererModifier,
                factory = { ctx ->
                    client.createRenderer(ctx, settings).apply {
                        isFocusable = false
                        isFocusableInTouchMode = false
                        hideAndroidPointerTree()
                        scaleX = stretchScale.first
                        scaleY = stretchScale.second
                    }
                },
                update = { renderer ->
                    client.applyLiveSettings(settings, vibrationEnabled, hapticsOutput, stretchToFit)
                    renderer.scaleX = stretchScale.first
                    renderer.scaleY = stretchScale.second
                    renderer.isFocusable = false
                    renderer.isFocusableInTouchMode = false
                    pointerRootView.configureAndroidMousePointerCapture(hideExternalMousePointer, { currentOnMouseCaptureInput() }) { event ->
                        client.dispatchMotion(event)
                    }
                    if (hideExternalMousePointer) {
                        pointerRootView.hideAndroidPointerTree()
                        renderer.hideAndroidPointerTree()
                    } else {
                        pointerRootView.showAndroidPointerTree()
                        renderer.showAndroidPointerTree()
                    }
                    renderer.setOnKeyListener(null)
                    renderer.setOnGenericMotionListener { _, event ->
                        if (hideExternalMousePointer) pointerRootView.hideAndroidPointerTree()
                        client.dispatchMotion(event)
                    }
                    renderer.setOnTouchListener { view, event ->
                        NativeStreamInputRouter.dispatchTouch(event, view.width, view.height)
                    }
                },
                onRelease = client::releaseRenderer,
            )
        }
        FingerMouseInputLayer(
            enabled = touchMouseEnabled,
            pinchZoomEnabled = pinchZoomEnabled,
            onZoomGesture = { scaleChange, pan, centroid ->
                val previousScale = zoomScale
                val nextScale = (zoomScale * scaleChange).coerceIn(1f, 3f)
                val appliedScaleChange = nextScale / previousScale
                val viewportCenter = Offset(viewportSize.width / 2f, viewportSize.height / 2f)
                zoomScale = nextScale
                zoomOffset = if (nextScale <= 1.001f) {
                    Offset.Zero
                } else {
                    // Keep the content under the pinch centroid anchored while scaling, then
                    // apply the fingers' pan. Scaling around the viewport centre without this
                    // correction makes an off-centre zoom appear to slide away from the user.
                    val focalCorrection =
                        (centroid - viewportCenter) * (1f - appliedScaleChange)
                    clampStreamZoomOffset(
                        zoomOffset * appliedScaleChange + focalCorrection + pan,
                        nextScale,
                        viewportSize,
                    )
                }
            },
            modifier = Modifier.matchParentSize(),
        )
    }
}

internal fun streamRendererAspectRatio(
    settings: StreamSettings,
    decodedResolution: String? = null,
    serverNegotiatedResolution: String? = null,
    serverFinalSelectedResolution: String? = null,
): Float {
    val selectedAspectRatio = streamAspectRatioForPixels(streamResolutionPixels(settings))
    val decodedPixels = parseResolutionPixelsOrNull(decodedResolution) ?: return selectedAspectRatio
    val authoritativeServerPixels = listOf(serverFinalSelectedResolution, serverNegotiatedResolution)
        .mapNotNull(::parseResolutionPixelsOrNull)
    if (decodedPixels !in authoritativeServerPixels) return selectedAspectRatio
    return streamAspectRatioForPixels(decodedPixels)
}

internal fun streamStretchContentAspectRatio(
    selectedAspectRatio: Float,
    decodedResolution: String?,
): Float {
    val decodedPixels = parseResolutionPixelsOrNull(decodedResolution) ?: return selectedAspectRatio
    val decodedAspectRatio = decodedPixels.first.toFloat() / decodedPixels.second.toFloat()
    return decodedAspectRatio.takeIf { it.isFinite() && it > 0f } ?: selectedAspectRatio
}

internal fun streamStretchScale(
    enabled: Boolean,
    viewportAspectRatio: Float,
    streamAspectRatio: Float,
): Pair<Float, Float> {
    if (!enabled || viewportAspectRatio <= 0f || streamAspectRatio <= 0f) return 1f to 1f
    return when {
        viewportAspectRatio > streamAspectRatio ->
            (viewportAspectRatio / streamAspectRatio).coerceIn(1f, 3f) to 1f
        viewportAspectRatio < streamAspectRatio ->
            1f to (streamAspectRatio / viewportAspectRatio).coerceIn(1f, 3f)
        else -> 1f to 1f
    }
}

internal fun streamPinchZoomEnabled(
    touchMouseEnabled: Boolean,
    touchControllerVisible: Boolean,
): Boolean = touchMouseEnabled && !touchControllerVisible

internal fun shouldEnableExternalMousePointerCapture(
    streamReady: Boolean,
    streamOverlayOpen: Boolean,
    pointerLockEnabled: Boolean,
): Boolean = streamReady && !streamOverlayOpen && pointerLockEnabled

private fun streamAspectRatioForPixels(pixels: Pair<Int, Int>): Float {
    val (width, height) = pixels
    if (width <= 0 || height <= 0) return 16f / 9f
    return width.toFloat() / height.toFloat()
}

private fun clampStreamZoomOffset(offset: Offset, zoomScale: Float, viewportSize: IntSize): Offset {
    if (zoomScale <= 1.001f || viewportSize.width <= 0 || viewportSize.height <= 0) return Offset.Zero
    val maxX = viewportSize.width * (zoomScale - 1f) / 2f
    val maxY = viewportSize.height * (zoomScale - 1f) / 2f
    return Offset(
        x = offset.x.coerceIn(-maxX, maxX),
        y = offset.y.coerceIn(-maxY, maxY),
    )
}

private fun androidNullPointerIcon(view: android.view.View): PointerIcon? =
    if (Build.VERSION.SDK_INT >= 24) {
        runCatching { PointerIcon.getSystemIcon(view.context, PointerIcon.TYPE_NULL) }
            .onFailure { error -> NativeInputDiagnostics.add("pointer icon unavailable error=${error.javaClass.simpleName}") }
            .getOrNull()
    } else {
        null
    }

private fun View.configureAndroidMousePointerCapture(enabled: Boolean, onCaptureInput: () -> Unit = {}, onMotion: (MotionEvent) -> Boolean) {
    if (Build.VERSION.SDK_INT < 26) return
    AndroidPointerCapture.configure(this, enabled, onCaptureInput, onMotion)
}

private fun View.clearAndroidMousePointerCapture() {
    if (Build.VERSION.SDK_INT < 26) return
    AndroidPointerCapture.clear(this)
}

private fun android.view.View.hideAndroidPointerTree() {
    if (Build.VERSION.SDK_INT < 24) return
    val icon = androidNullPointerIcon(this)
    applyAndroidPointerIconTree(icon)
}

private fun android.view.View.showAndroidPointerTree() {
    if (Build.VERSION.SDK_INT < 24) return
    applyAndroidPointerIconTree(null)
}

private fun android.view.View.applyAndroidPointerIconTree(icon: PointerIcon?) {
    if (Build.VERSION.SDK_INT < 24) return
    runCatching { pointerIcon = icon }
        .onFailure { error -> NativeInputDiagnostics.add("pointer icon apply failed error=${error.javaClass.simpleName}") }
    if (this is ViewGroup) {
        for (index in 0 until childCount) {
            getChildAt(index).applyAndroidPointerIconTree(icon)
        }
    }
}

@Composable
private fun FingerMouseInputLayer(
    enabled: Boolean,
    pinchZoomEnabled: Boolean,
    onZoomGesture: (scaleChange: Float, pan: Offset, centroid: Offset) -> Unit,
    modifier: Modifier = Modifier,
) {
    if (!enabled) return
    var width by remember { mutableStateOf(0) }
    var height by remember { mutableStateOf(0) }
    var pinchActive by remember { mutableStateOf(false) }
    var lastPinchDistance by remember { mutableFloatStateOf(0f) }
    var lastPinchCentroid by remember { mutableStateOf(Offset.Zero) }
    Box(
        modifier
            .onSizeChanged {
                width = it.width
                height = it.height
            }
            .pointerInteropFilter { event ->
                if (NativeStreamInputRouter.isNativeUiTouchGestureActive()) {
                    pinchActive = false
                    lastPinchDistance = 0f
                    lastPinchCentroid = Offset.Zero
                    return@pointerInteropFilter NativeStreamInputRouter.dispatchTouch(event, width, height)
                }
                if (event.pointerCount >= 2) {
                    // 3-finger touch is reserved for the Direct Click toggle gesture
                    // (handled in NativeStreamInputRouter.dispatchTouch). Do not
                    // interpret it as a pinch-zoom — reset pinch state and let it through.
                    if (event.pointerCount >= 3) {
                        pinchActive = false
                        lastPinchDistance = 0f
                        lastPinchCentroid = Offset.Zero
                        NativeStreamInputRouter.dispatchTouch(event, width, height)
                        return@pointerInteropFilter true
                    }
                    NativeStreamInputRouter.cancelTouchMouse()
                    if (!pinchZoomEnabled) {
                        // Multiple fingers while the touch controller is visible are
                        // controller input, not a request to crop the video surface.
                        pinchActive = true
                        lastPinchDistance = 0f
                        lastPinchCentroid = Offset.Zero
                        return@pointerInteropFilter true
                    }
                    val distance = event.firstTwoPointerDistance()
                    val centroid = event.firstTwoPointerCentroid()
                    if (pinchActive && lastPinchDistance > 0f && distance > 0f) {
                        onZoomGesture(
                            (distance / lastPinchDistance).coerceIn(0.82f, 1.22f),
                            centroid - lastPinchCentroid,
                            centroid,
                        )
                    }
                    pinchActive = true
                    lastPinchDistance = distance
                    lastPinchCentroid = centroid
                    return@pointerInteropFilter true
                }
                if (pinchActive) {
                    if (event.actionMasked == MotionEvent.ACTION_UP || event.actionMasked == MotionEvent.ACTION_CANCEL) {
                        pinchActive = false
                        lastPinchDistance = 0f
                        lastPinchCentroid = Offset.Zero
                    }
                    return@pointerInteropFilter true
                }
                if (event.actionMasked == MotionEvent.ACTION_DOWN) {
                    NativeInputDiagnostics.retainTouchRoute("compose.finger-layer") {
                        "compose finger layer down size=${width}x$height"
                    }
                }
                NativeStreamInputRouter.dispatchTouch(event, width, height)
            },
    )
}

private fun MotionEvent.firstTwoPointerDistance(): Float {
    if (pointerCount < 2) return 0f
    val dx = getX(1) - getX(0)
    val dy = getY(1) - getY(0)
    return sqrt(dx * dx + dy * dy)
}

private fun MotionEvent.firstTwoPointerCentroid(): Offset =
    if (pointerCount >= 2) {
        Offset((getX(0) + getX(1)) / 2f, (getY(0) + getY(1)) / 2f)
    } else {
        Offset.Zero
    }

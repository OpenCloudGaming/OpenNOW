package com.opencloudgaming.opennow

import android.Manifest
import android.app.PictureInPictureParams
import android.content.Context
import android.content.Intent
import android.content.pm.ActivityInfo
import android.content.pm.PackageManager
import android.content.res.Configuration
import android.media.AudioManager
import android.os.Build
import android.os.Bundle
import android.os.SystemClock
import android.util.Log
import androidx.annotation.RequiresApi
import android.util.Rational
import android.view.Display
import android.view.InputDevice
import android.view.KeyCharacterMap
import android.view.KeyEvent
import android.view.MotionEvent
import android.view.PointerIcon
import android.view.View
import android.view.ViewGroup
import android.view.WindowInsets
import android.view.WindowInsetsController
import android.view.WindowManager
import androidx.compose.foundation.background
import androidx.compose.foundation.layout.Box
import androidx.compose.foundation.layout.fillMaxSize
import androidx.compose.material3.CircularProgressIndicator
import androidx.compose.runtime.CompositionLocalProvider
import androidx.compose.runtime.LaunchedEffect
import androidx.compose.runtime.getValue
import androidx.compose.runtime.mutableStateOf
import androidx.compose.runtime.remember
import androidx.compose.runtime.setValue
import androidx.compose.ui.Alignment
import androidx.compose.ui.Modifier
import androidx.compose.ui.graphics.Color
import androidx.activity.ComponentActivity
import androidx.activity.compose.setContent
import androidx.activity.enableEdgeToEdge
import androidx.activity.result.contract.ActivityResultContracts
import androidx.activity.viewModels
import androidx.core.view.WindowCompat
import androidx.lifecycle.lifecycleScope
import kotlinx.coroutines.Job
import kotlinx.coroutines.delay
import kotlinx.coroutines.launch

class MainActivity : ComponentActivity() {
    private val viewModel: OpenNowViewModel by viewModels()
    private val mascotActivity = MascotActivityTracker()
    private val queueStatusNotifier by lazy { AndroidQueueStatusNotifier(this) }
    private val streamKeepAliveNotifier by lazy { AndroidStreamKeepAliveNotifier(this) }
    private var notificationPermissionRequested = false
    private var lastHatXKeyCode: Int? = null
    private var lastHatYKeyCode: Int? = null
    private var streamSystemUiActive = false
    private var streamDisplayRefreshActive = false
    private var streamDisplayRefreshFps = 60
    private var streamSystemUiEnforcerJob: Job? = null
    private var lastStreamSystemUiInputReapplyMs = 0L
    private var externalMousePointerCaptureRequestPending = false
    private var defaultRequestedOrientation = ActivityInfo.SCREEN_ORIENTATION_FULL_SENSOR
    private var phoneStreamOrientationLocked = false
    private var streamPictureInPictureReady = false
    private var streamPictureInPictureAspectRatio = Rational(16, 9)
    private var systemWallpaperWindowVisible: Boolean? = null
    private var startupDataReady = false
    private var pendingExternalLaunchIntent: Intent? = null
    private var pendingLocalNetworkIntent: Intent? = null
    private val localNetworkPermissionLauncher = registerForActivityResult(
        ActivityResultContracts.RequestPermission(),
    ) { granted ->
        val pendingIntent = pendingLocalNetworkIntent
        pendingLocalNetworkIntent = null
        if (granted && startupDataReady && pendingIntent != null) {
            viewModel.handleExternalLaunchIntent(pendingIntent)
        }
    }

    override fun attachBaseContext(newBase: Context) {
        super.attachBaseContext(localizedAndroidContext(newBase))
    }

    override fun onCreate(savedInstanceState: Bundle?) {
        super.onCreate(savedInstanceState)
        enableEdgeToEdge()
        defaultRequestedOrientation = requestedOrientation
        volumeControlStream = AudioManager.STREAM_MUSIC
        val openNowApplication = application as OpenNowApplication
        pendingExternalLaunchIntent = intent
        setContent {
            var ready by remember { mutableStateOf(false) }
            LaunchedEffect(openNowApplication) {
                openNowApplication.awaitStartupData()
                ready = true
            }
            if (ready) {
                CompositionLocalProvider(LocalMascotActivity provides mascotActivity) {
                    OpenNowApp(
                        viewModel = viewModel,
                        onMicrophoneCaptureActiveChange = streamKeepAliveNotifier::setMicrophoneCaptureActive,
                    )
                }
            } else {
                Box(
                    modifier = Modifier.fillMaxSize().background(Color(0xFF05070B)),
                    contentAlignment = Alignment.Center,
                ) {
                    CircularProgressIndicator(color = Color(0xFF69E6FF))
                }
            }
        }
        lifecycleScope.launch {
            openNowApplication.awaitStartupData()
            startupDataReady = true
            viewModel.setAndroidPictureInPictureActive(isAndroidPictureInPictureActive())
            pendingExternalLaunchIntent?.let(::handleExternalLaunchIntent)
            pendingExternalLaunchIntent = null
            viewModel.state.collect { state ->
                requestQueueNotificationPermissionIfNeeded(state)
                queueStatusNotifier.update(state)
                streamKeepAliveNotifier.update(state)
                val streamActive = state.page == AppPage.Stream && state.streamStatus != "idle"
                applySystemWallpaperWindow(
                    shouldShowSystemWallpaperBackground(
                        settings = state.settings,
                        inStream = state.page == AppPage.Stream,
                    ),
                )
                applyPhoneStreamOrientationLock(
                    shouldLockPhoneStreamLandscape(state, resources.configuration.smallestScreenWidthDp),
                )
                updateStreamPictureInPicture(
                    ready = state.page == AppPage.Stream &&
                        state.streamStatus == "streaming" &&
                        state.streamSession?.isReadyForStream() == true,
                    settings = state.activeStreamSettings ?: state.settings.stream,
                )
                applyStreamSystemUi(streamActive)
                applyStreamDisplayRefreshRate(streamActive, state.activeStreamSettings?.fps ?: state.settings.stream.fps)
            }
        }
    }

    /**
     * Lets Android's wallpaper service remain the owner of static, live, and motion wallpapers.
     * Copying a wallpaper bitmap here would freeze live/4D effects and can consume significant
     * memory, so OpenNOW instead makes only its app-shell window transparent. The theme keeps the
     * window surface transparent from creation; changing its pixel format after Compose has attached
     * does not reliably recreate the surface on OEM Android builds.
     */
    private fun applySystemWallpaperWindow(visible: Boolean) {
        if (systemWallpaperWindowVisible == visible) return
        systemWallpaperWindowVisible = visible
        if (visible) {
            window.addFlags(WindowManager.LayoutParams.FLAG_SHOW_WALLPAPER)
        } else {
            window.clearFlags(WindowManager.LayoutParams.FLAG_SHOW_WALLPAPER)
        }
    }

    override fun onNewIntent(intent: Intent) {
        super.onNewIntent(intent)
        setIntent(intent)
        if (startupDataReady) {
            handleExternalLaunchIntent(intent)
        } else {
            pendingExternalLaunchIntent = intent
        }
    }

    private fun handleExternalLaunchIntent(intent: Intent) {
        if (isLocalTvPairUri(intent.data) && !hasAndroidLocalNetworkAccess()) {
            pendingLocalNetworkIntent = intent
            localNetworkPermissionLauncher.launch(Manifest.permission.ACCESS_LOCAL_NETWORK)
            return
        }
        viewModel.handleExternalLaunchIntent(intent)
    }

    override fun onResume() {
        super.onResume()
        mascotActivity.updateResumed(true)
        if (startupDataReady) {
            viewModel.setAndroidPictureInPictureActive(isAndroidPictureInPictureActive())
            // A process that was frozen or killed by an aggressive OEM memory manager comes back
            // with dead sockets and possibly stale tokens. The catalogue is fetched once at
            // startup and never again, so without this the Store stays empty until a manual pull.
            viewModel.onAppForegrounded()
        }
        if (streamSystemUiActive) {
            applyStreamSystemUi(true, force = true)
            applyStreamDisplayRefreshRate(streamDisplayRefreshActive, streamDisplayRefreshFps, force = true)
        }
        if (phoneStreamOrientationLocked) {
            applyPhoneStreamOrientationLock(true, force = true)
        }
    }

    override fun onPause() {
        mascotActivity.updateResumed(false)
        super.onPause()
    }

    override fun dispatchKeyEvent(event: KeyEvent): Boolean {
        mascotActivity.onKey(event)
        if (streamSystemUiActive && event.action == KeyEvent.ACTION_DOWN && event.shouldReapplyStreamSystemUi()) {
            enforceStreamSystemUiFromInput()
        }
        if (streamSystemUiActive && event.isAndroidVolumeKey()) {
            return super.dispatchKeyEvent(event)
        }
        if (NativeStreamInputRouter.dispatchKey(event)) {
            return true
        }
        val normalizedStreamUiKeyCode = NativeStreamInputRouter.normalizedStreamUiKeyCode(event)
        if (normalizedStreamUiKeyCode != null && normalizedStreamUiKeyCode != event.keyCode) {
            return dispatchSyntheticStreamUiKey(normalizedStreamUiKeyCode, event)
        }
        if (NativeStreamInputRouter.isControllerAppBackKey(event)) {
            // Invoke ComponentActivity's Back dispatcher so Compose's nearest BackHandler gets
            // first refusal. Synthesizing KEYCODE_BACK through the Window can finish the Activity
            // without visiting the nested settings handler on some Android builds.
            if (event.action == KeyEvent.ACTION_UP) {
                onBackPressedDispatcher.onBackPressed()
            }
            return true
        }
        if (event.shouldVirtualizeControllerUiNavigation()) {
            // Android TV keyboards reliably accept virtual D-pad events (the same shape remotes
            // emit), while several IMEs ignore navigation events from a physical gamepad device.
            return dispatchSyntheticStreamUiKey(event.keyCode, event)
        }
        val normalizedAppUiKeyCode = NativeStreamInputRouter.normalizedAppUiKeyCode(event)
        if (normalizedAppUiKeyCode != null && normalizedAppUiKeyCode != event.keyCode) {
            return dispatchSyntheticStreamUiKey(normalizedAppUiKeyCode, event)
        }
        return super.dispatchKeyEvent(event)
    }

    override fun onUserLeaveHint() {
        super.onUserLeaveHint()
        enterStreamPictureInPictureIfReady()
    }

    override fun onPictureInPictureModeChanged(isInPictureInPictureMode: Boolean, newConfig: Configuration) {
        super.onPictureInPictureModeChanged(isInPictureInPictureMode, newConfig)
        if (startupDataReady) {
            viewModel.setAndroidPictureInPictureActive(isInPictureInPictureMode)
        }
        NativeStreamInputRouter.releaseInputForLifecycle("picture-in-picture-changed")
    }

    private fun isAndroidPictureInPictureActive(): Boolean =
        Build.VERSION.SDK_INT >= Build.VERSION_CODES.N && isInPictureInPictureMode

    override fun onStop() {
        super.onStop()
        // Backgrounding mid-tap gives us no UP or CANCEL, so without this the host keeps the mouse
        // button held down. PiP does not stop the activity, hence the separate call above.
        NativeStreamInputRouter.releaseInputForLifecycle("activity-stopped")
    }

    private fun KeyEvent.isAndroidVolumeKey(): Boolean =
        keyCode == KeyEvent.KEYCODE_VOLUME_UP ||
            keyCode == KeyEvent.KEYCODE_VOLUME_DOWN ||
            keyCode == KeyEvent.KEYCODE_VOLUME_MUTE

    private fun KeyEvent.shouldVirtualizeControllerUiNavigation(): Boolean {
        if (!AndroidControllerInput.isControllerEvent(source, deviceId)) return false
        return when (keyCode) {
            KeyEvent.KEYCODE_DPAD_UP,
            KeyEvent.KEYCODE_DPAD_DOWN,
            KeyEvent.KEYCODE_DPAD_LEFT,
            KeyEvent.KEYCODE_DPAD_RIGHT,
            KeyEvent.KEYCODE_DPAD_CENTER,
            KeyEvent.KEYCODE_ENTER,
            KeyEvent.KEYCODE_NUMPAD_ENTER,
            -> true
            else -> false
        }
    }

    override fun dispatchGenericMotionEvent(event: MotionEvent): Boolean {
        mascotActivity.onMotion(event)
        val mouseLikePointer = event.isMouseLikePointerEvent()
        if (streamSystemUiActive && (mouseLikePointer || event.isControllerMotionEvent())) {
            enforceStreamSystemUiFromInput()
            if (mouseLikePointer) {
                requestExternalMousePointerCaptureIfNeeded(event)
            }
        }
        return NativeStreamInputRouter.dispatchMotion(event) ||
            dispatchGamepadHatNavigation(event) ||
            super.dispatchGenericMotionEvent(event)
    }

    private fun requestExternalMousePointerCaptureIfNeeded(event: MotionEvent) =
        requestExternalMousePointerCaptureIfNeeded(source = event.source, deviceId = event.deviceId)

    private fun requestExternalMousePointerCaptureIfNeeded(source: Int? = null, deviceId: Int? = null) {
        if (Build.VERSION.SDK_INT < Build.VERSION_CODES.O) return
        val decorView = window?.decorView ?: return
        if (
            !shouldRequestAndroidMousePointerCapture(
                streamActive = streamSystemUiActive,
                captureEnabled = NativeStreamInputRouter.isExternalMousePointerCaptureEnabled(),
                windowFocused = decorView.hasWindowFocus(),
                hasPointerCapture = AndroidPointerCapture.hasCapture(decorView),
                mouseLikePointer = true,
            ) || externalMousePointerCaptureRequestPending
        ) {
            return
        }
        externalMousePointerCaptureRequestPending = true
        val requestOrigin = if (source != null && deviceId != null) {
            "source=$source device=$deviceId"
        } else {
            "window-focus"
        }
        decorView.post {
            externalMousePointerCaptureRequestPending = false
            if (
                !shouldRequestAndroidMousePointerCapture(
                    streamActive = streamSystemUiActive,
                    captureEnabled = NativeStreamInputRouter.isExternalMousePointerCaptureEnabled(),
                    windowFocused = decorView.hasWindowFocus(),
                    hasPointerCapture = AndroidPointerCapture.hasCapture(decorView),
                    mouseLikePointer = true,
                )
            ) {
                return@post
            }
            decorView.isFocusable = true
            decorView.isFocusableInTouchMode = true
            decorView.requestFocus()
            // Compose/SurfaceView can move focus to a descendant after capture starts. Refresh the
            // listener across the current tree so the focused child keeps forwarding deltas.
            AndroidPointerCapture.installRecursively(decorView, ::dispatchCapturedStreamPointer)
            runCatching { AndroidPointerCapture.request(decorView) }
                .onSuccess {
                    NativeInputDiagnostics.addRetained(
                        key = "mouse.pointer-capture",
                        message = "external mouse pointer capture requested origin=$requestOrigin",
                    )
                }
                .onFailure { error ->
                    NativeInputDiagnostics.addRetained(
                        key = "mouse.pointer-capture",
                        message = "external mouse pointer capture request failed origin=$requestOrigin error=${error.javaClass.simpleName}",
                    )
                }
        }
    }

    override fun dispatchTouchEvent(event: MotionEvent): Boolean {
        mascotActivity.onTouch(event)
        try {
            val decorView = window?.decorView
            if (streamSystemUiActive && event.isMouseLikePointerEvent()) {
                enforceStreamSystemUiFromInput()
                requestExternalMousePointerCaptureIfNeeded(event)
            }
            if (NativeStreamInputRouter.shouldConsumeUiTransitionTouchBeforeViews(event)) return true
            if (decorView != null && NativeStreamInputRouter.dispatchExternalMouseTouch(event, decorView.width, decorView.height)) return true
            if (decorView != null && NativeStreamInputRouter.shouldForwardTouchBeforeViews(event, decorView.width, decorView.height)) {
                if (event.actionMasked == MotionEvent.ACTION_DOWN) {
                    NativeInputDiagnostics.retainTouchRoute("activity.forward-before-views") {
                        "activity touch forwardBeforeViews size=${decorView.width}x${decorView.height}"
                    }
                }
                val forwarded = NativeStreamInputRouter.dispatchTouch(event, decorView.width, decorView.height)
                if (NativeStreamInputRouter.shouldCaptureTouchBeforeViews(event, decorView.width, decorView.height) && forwarded) {
                    return true
                }
            }
            val handled = super.dispatchTouchEvent(event)
            if (handled) {
                if (event.actionMasked == MotionEvent.ACTION_DOWN) {
                    NativeInputDiagnostics.retainTouchRoute("activity.consumed-by-view") {
                        "activity touch consumedByView action=${event.actionMasked}"
                    }
                }
                return true
            }
            return if (decorView != null) {
                if (event.actionMasked == MotionEvent.ACTION_DOWN) {
                    NativeInputDiagnostics.retainTouchRoute("activity.fallback") {
                        "activity touch fallback size=${decorView.width}x${decorView.height}"
                    }
                }
                NativeStreamInputRouter.dispatchTouch(event, decorView.width, decorView.height)
            } else {
                false
            }
        } finally {
            NativeStreamInputRouter.postDispatchTouch(event)
        }
    }

    override fun onDestroy() {
        if (isFinishing) {
            queueStatusNotifier.cancel()
            // Keep the foreground service alive long enough for onTaskRemoved()
            // to end the exact cloud session. Normal in-app exits already move
            // stream state to idle and cancel the service through update().
            if (!startupDataReady || !shouldKeepAndroidStreamAlive(viewModel.state.value)) {
                streamKeepAliveNotifier.cancel()
            }
        }
        super.onDestroy()
    }

    override fun onWindowFocusChanged(hasFocus: Boolean) {
        super.onWindowFocusChanged(hasFocus)
        mascotActivity.updateFocused(hasFocus)
        if (!hasFocus) {
            // Pixel desktop and other freeform environments can intercept a shortcut after its
            // DOWN event (for example Alt+Tab), so no matching UP reaches this window.
            NativeStreamInputRouter.releaseInputForLifecycle("window-focus-lost")
        } else if (streamSystemUiActive) {
            applyStreamSystemUi(true, force = true)
            applyStreamDisplayRefreshRate(streamDisplayRefreshActive, streamDisplayRefreshFps, force = true)
            requestExternalMousePointerCaptureIfNeeded()
        }
    }

    override fun onPointerCaptureChanged(hasCapture: Boolean) {
        super.onPointerCaptureChanged(hasCapture)
        if (!streamSystemUiActive && !hasCapture) return
        val decorView = window?.decorView
        NativeInputDiagnostics.addRetained(
            key = "mouse.pointer-capture-state",
            message = "external mouse pointer capture changed granted=$hasCapture " +
                "streamActive=$streamSystemUiActive " +
                "enabled=${NativeStreamInputRouter.isExternalMousePointerCaptureEnabled()} " +
                "windowFocused=${decorView?.hasWindowFocus() == true}",
        )
    }

    fun enforceStreamSystemUiFromInput() {
        if (!streamSystemUiActive) return
        val now = SystemClock.uptimeMillis()
        if (now - lastStreamSystemUiInputReapplyMs < STREAM_SYSTEM_UI_INPUT_REAPPLY_MS) return
        lastStreamSystemUiInputReapplyMs = now
        applyStreamSystemBars(active = true)
    }

    private fun applyStreamSystemUi(active: Boolean, force: Boolean = false) {
        if (!force && streamSystemUiActive == active) {
            applyStreamKeepAwake(active)
            updateStreamSystemUiEnforcer(active)
            return
        }
        streamSystemUiActive = active
        applyStreamPointerIcon(active && NativeStreamInputRouter.isExternalMousePointerCaptureEnabled())
        applyStreamKeepAwake(active)
        updateStreamSystemUiEnforcer(active)

        if (Build.VERSION.SDK_INT >= Build.VERSION_CODES.O) {
            val decorView = window.decorView
            if (active) {
                AndroidPointerCapture.installRecursively(decorView, ::dispatchCapturedStreamPointer)
            } else {
                AndroidPointerCapture.clearRecursively(decorView)
            }
            if (!active) {
                runCatching { AndroidPointerCapture.release(decorView) }
            }
        }

        applyStreamSystemBars(active)
    }

    /**
     * Pointer capture is delivered to the focused view. The Activity retry path focuses and asks
     * the decor view for capture, so that same view must own a forwarding listener; otherwise a
     * Bluetooth mouse can be captured successfully while its events never reach the stream view.
     */
    private fun dispatchCapturedStreamPointer(event: MotionEvent): Boolean {
        if (!shouldRouteCapturedAndroidMousePointer(streamSystemUiActive, event.isMouseLikePointerEvent())) {
            return false
        }
        NativeInputDiagnostics.retainThrottled(
            key = "mouse.captured-route",
            minimumIntervalMs = 250L,
        ) {
            "captured mouse routed source=${event.source} device=${event.deviceId} " +
                "relativeX=${event.getAxisValue(MotionEvent.AXIS_RELATIVE_X)} " +
                "relativeY=${event.getAxisValue(MotionEvent.AXIS_RELATIVE_Y)}"
        }
        enforceStreamSystemUiFromInput()
        return NativeStreamInputRouter.dispatchMotion(event)
    }

    /** Reapplies only immersive bars; pointer-icon traversal and window flags are state changes. */
    private fun applyStreamSystemBars(active: Boolean) {
        if (Build.VERSION.SDK_INT >= Build.VERSION_CODES.R) {
            WindowCompat.setDecorFitsSystemWindows(window, false)
            window.insetsController?.let { controller ->
                if (active) {
                    controller.systemBarsBehavior = WindowInsetsController.BEHAVIOR_SHOW_TRANSIENT_BARS_BY_SWIPE
                    controller.hide(WindowInsets.Type.statusBars() or WindowInsets.Type.navigationBars())
                } else {
                    controller.show(WindowInsets.Type.statusBars() or WindowInsets.Type.navigationBars())
                }
            }
        } else {
            @Suppress("DEPRECATION")
            window.decorView.systemUiVisibility = if (active) {
                View.SYSTEM_UI_FLAG_FULLSCREEN or
                    View.SYSTEM_UI_FLAG_HIDE_NAVIGATION or
                    View.SYSTEM_UI_FLAG_IMMERSIVE_STICKY or
                    View.SYSTEM_UI_FLAG_LAYOUT_FULLSCREEN or
                    View.SYSTEM_UI_FLAG_LAYOUT_HIDE_NAVIGATION or
                    View.SYSTEM_UI_FLAG_LAYOUT_STABLE
            } else {
                0
            }
        }
    }

    private fun updateStreamPictureInPicture(ready: Boolean, settings: StreamSettings) {
        val aspectRatio = pictureInPictureAspectRatioFor(settings)
        val shouldUpdateParams = streamPictureInPictureReady != ready ||
            streamPictureInPictureAspectRatio != aspectRatio
        streamPictureInPictureReady = ready
        streamPictureInPictureAspectRatio = aspectRatio
        if (Build.VERSION.SDK_INT >= Build.VERSION_CODES.O && shouldUpdateParams) {
            runCatching {
                setPictureInPictureParams(buildStreamPictureInPictureParams())
            }.onFailure { error ->
                Log.w(MAIN_ACTIVITY_LOG_TAG, "Unable to update stream picture-in-picture params", error)
            }
        }
    }

    private fun enterStreamPictureInPictureIfReady() {
        if (Build.VERSION.SDK_INT < Build.VERSION_CODES.O) return
        if (!streamPictureInPictureReady || isInPictureInPictureMode) return
        runCatching {
            enterPictureInPictureMode(buildStreamPictureInPictureParams())
        }.onFailure { error ->
            Log.w(MAIN_ACTIVITY_LOG_TAG, "Unable to enter stream picture-in-picture", error)
        }
    }

    // Both callers gate on SDK_INT >= O; the annotation is what lets lint see that.
    @RequiresApi(Build.VERSION_CODES.O)
    private fun buildStreamPictureInPictureParams(): PictureInPictureParams =
        PictureInPictureParams.Builder()
            .setAspectRatio(streamPictureInPictureAspectRatio)
            .apply {
                if (Build.VERSION.SDK_INT >= Build.VERSION_CODES.S) {
                    setAutoEnterEnabled(streamPictureInPictureReady)
                    setSeamlessResizeEnabled(true)
                }
            }
            .build()

    private fun pictureInPictureAspectRatioFor(settings: StreamSettings): Rational {
        val (width, height) = streamResolutionPixels(settings)
        if (width <= 0 || height <= 0) return Rational(16, 9)
        val ratio = width.toFloat() / height.toFloat()
        return if (ratio in MIN_PIP_ASPECT_RATIO..MAX_PIP_ASPECT_RATIO) {
            Rational(width, height)
        } else {
            Rational(16, 9)
        }
    }

    private fun applyStreamKeepAwake(active: Boolean) {
        if (active) {
            window.addFlags(WindowManager.LayoutParams.FLAG_KEEP_SCREEN_ON)
        } else {
            window.clearFlags(WindowManager.LayoutParams.FLAG_KEEP_SCREEN_ON)
        }
    }

    private fun applyPhoneStreamOrientationLock(active: Boolean, force: Boolean = false) {
        if (!force && phoneStreamOrientationLocked == active) return
        phoneStreamOrientationLocked = active
        val nextOrientation = if (active) {
            ActivityInfo.SCREEN_ORIENTATION_SENSOR_LANDSCAPE
        } else {
            defaultRequestedOrientation
        }
        if (requestedOrientation != nextOrientation) {
            requestedOrientation = nextOrientation
        }
    }

    private fun updateStreamSystemUiEnforcer(active: Boolean) {
        if (!active) {
            streamSystemUiEnforcerJob?.cancel()
            streamSystemUiEnforcerJob = null
            return
        }
        if (streamSystemUiEnforcerJob?.isActive == true) return
        streamSystemUiEnforcerJob = lifecycleScope.launch {
            while (streamSystemUiActive) {
                delay(STREAM_SYSTEM_UI_ENFORCE_INTERVAL_MS)
                val navigationBarsVisible = if (Build.VERSION.SDK_INT >= Build.VERSION_CODES.R) {
                    window.decorView.rootWindowInsets?.isVisible(WindowInsets.Type.navigationBars()) == true
                } else {
                    false
                }
                if (
                    shouldPeriodicallyEnforceStreamSystemUi(
                        streamActive = streamSystemUiActive,
                        navigationBarsVisible = navigationBarsVisible,
                        pointerLockEnabled = NativeStreamInputRouter.isExternalMousePointerCaptureEnabled(),
                    )
                ) {
                    // Keep the immersive fallback without repeatedly walking the complete View
                    // hierarchy to reapply pointer icons or rewriting unchanged window flags.
                    applyStreamSystemBars(active = true)
                }
            }
        }
    }

    private fun applyStreamPointerIcon(active: Boolean) {
        if (Build.VERSION.SDK_INT < Build.VERSION_CODES.N) return
        runCatching {
            val icon = if (active) PointerIcon.getSystemIcon(this, PointerIcon.TYPE_NULL) else null
            window.decorView.applyPointerIconRecursive(icon)
        }.onFailure { error ->
            Log.w(MAIN_ACTIVITY_LOG_TAG, "Unable to apply stream pointer icon", error)
        }
    }

    private fun applyStreamDisplayRefreshRate(active: Boolean, requestedFps: Int, force: Boolean = false) {
        streamDisplayRefreshActive = active
        streamDisplayRefreshFps = requestedFps
        if (Build.VERSION.SDK_INT < Build.VERSION_CODES.M) {
            DisplayRefreshDiagnostics.update(
                active = active,
                requestedFps = requestedFps,
                currentMode = null,
                selectedMode = null,
                supportedModes = emptyList(),
                preferredModeId = 0,
                preferredRefreshRate = 0f,
                applied = false,
            )
            return
        }

        val display = window.decorView.display
        val supportedModes = display?.supportedModes.orEmpty().map { it.toDisplayRefreshMode() }
        val currentMode = display?.mode?.toDisplayRefreshMode()
        val selectedMode = if (active) {
            selectStreamDisplayMode(
                supportedModes = supportedModes,
                currentMode = currentMode,
                requestedFps = requestedFps,
            )
        } else {
            null
        }
        val preferredModeId = selectedMode?.id ?: 0
        val preferredRefreshRate = selectedMode?.refreshRate ?: if (active) normalizedStreamDisplayFps(requestedFps) else 0f
        val attributes = window.attributes
        if (!force &&
            attributes.preferredDisplayModeId == preferredModeId &&
            kotlin.math.abs(attributes.preferredRefreshRate - preferredRefreshRate) < 0.01f
        ) {
            DisplayRefreshDiagnostics.update(
                active = active,
                requestedFps = requestedFps,
                currentMode = currentMode,
                selectedMode = selectedMode,
                supportedModes = supportedModes,
                preferredModeId = preferredModeId,
                preferredRefreshRate = preferredRefreshRate,
                applied = true,
            )
            return
        }
        var applied = false
        var failure: Throwable? = null
        runCatching {
            window.attributes = attributes.apply {
                preferredDisplayModeId = preferredModeId
                this.preferredRefreshRate = preferredRefreshRate
            }
            applied = true
        }.onFailure { error ->
            failure = error
            Log.w(MAIN_ACTIVITY_LOG_TAG, "Unable to apply stream display refresh preference", error)
        }
        DisplayRefreshDiagnostics.update(
            active = active,
            requestedFps = requestedFps,
            currentMode = currentMode,
            selectedMode = selectedMode,
            supportedModes = supportedModes,
            preferredModeId = preferredModeId,
            preferredRefreshRate = preferredRefreshRate,
            applied = applied,
            error = failure,
        )
    }

    private fun Display.Mode.toDisplayRefreshMode(): DisplayRefreshMode =
        DisplayRefreshMode(
            id = modeId,
            refreshRate = refreshRate,
            physicalWidth = physicalWidth,
            physicalHeight = physicalHeight,
        )

    @Suppress("DEPRECATION", "OVERRIDE_DEPRECATION")
    override fun onRequestPermissionsResult(requestCode: Int, permissions: Array<String>, grantResults: IntArray) {
        super.onRequestPermissionsResult(requestCode, permissions, grantResults)
        if (requestCode == 4210 && grantResults.firstOrNull() == PackageManager.PERMISSION_GRANTED) {
            queueStatusNotifier.update(viewModel.state.value)
        }
    }

    private fun requestQueueNotificationPermissionIfNeeded(state: OpenNowUiState) {
        if (notificationPermissionRequested) return
        if (!shouldShowQueueLaunchStatus(state)) return
        if (Build.VERSION.SDK_INT < Build.VERSION_CODES.TIRAMISU) return
        if (checkSelfPermission(Manifest.permission.POST_NOTIFICATIONS) == PackageManager.PERMISSION_GRANTED) return
        notificationPermissionRequested = true
        requestPermissions(arrayOf(Manifest.permission.POST_NOTIFICATIONS), 4210)
    }

    private fun dispatchGamepadHatNavigation(event: MotionEvent): Boolean {
        if (event.actionMasked != MotionEvent.ACTION_MOVE) return false
        if ((event.source and InputDevice.SOURCE_JOYSTICK) != InputDevice.SOURCE_JOYSTICK) return false

        val nextX = when {
            event.getAxisValue(MotionEvent.AXIS_HAT_X) <= -0.5f -> KeyEvent.KEYCODE_DPAD_LEFT
            event.getAxisValue(MotionEvent.AXIS_HAT_X) >= 0.5f -> KeyEvent.KEYCODE_DPAD_RIGHT
            else -> null
        }
        val nextY = when {
            event.getAxisValue(MotionEvent.AXIS_HAT_Y) <= -0.5f -> KeyEvent.KEYCODE_DPAD_UP
            event.getAxisValue(MotionEvent.AXIS_HAT_Y) >= 0.5f -> KeyEvent.KEYCODE_DPAD_DOWN
            else -> null
        }

        val handledX = updateSyntheticDpadKey(lastHatXKeyCode, nextX, event)
        val handledY = updateSyntheticDpadKey(lastHatYKeyCode, nextY, event)
        lastHatXKeyCode = nextX
        lastHatYKeyCode = nextY
        return handledX || handledY
    }

    private fun updateSyntheticDpadKey(previous: Int?, next: Int?, sourceEvent: MotionEvent): Boolean {
        var handled = false
        if (previous != null && previous != next) {
            handled = dispatchSyntheticDpadKey(previous, KeyEvent.ACTION_UP, sourceEvent) || handled
        }
        if (next != null && previous != next) {
            handled = dispatchSyntheticDpadKey(next, KeyEvent.ACTION_DOWN, sourceEvent) || handled
        }
        return handled
    }

    private fun dispatchSyntheticDpadKey(keyCode: Int, action: Int, sourceEvent: MotionEvent): Boolean {
        val event = KeyEvent(
            sourceEvent.downTime,
            sourceEvent.eventTime,
            action,
            keyCode,
            0,
            sourceEvent.metaState,
            KeyCharacterMap.VIRTUAL_KEYBOARD,
            0,
            0,
            InputDevice.SOURCE_DPAD,
        )
        return super.dispatchKeyEvent(event)
    }

    private fun dispatchSyntheticStreamUiKey(keyCode: Int, sourceEvent: KeyEvent): Boolean {
        val event = KeyEvent(
            sourceEvent.downTime,
            sourceEvent.eventTime,
            sourceEvent.action,
            keyCode,
            sourceEvent.repeatCount,
            sourceEvent.metaState,
            KeyCharacterMap.VIRTUAL_KEYBOARD,
            sourceEvent.scanCode,
            sourceEvent.flags,
            InputDevice.SOURCE_DPAD,
        )
        return super.dispatchKeyEvent(event)
    }

    private fun View.applyPointerIconRecursive(icon: PointerIcon?) {
        if (Build.VERSION.SDK_INT < Build.VERSION_CODES.N) return
        pointerIcon = icon
        if (this is ViewGroup) {
            for (index in 0 until childCount) {
                getChildAt(index).applyPointerIconRecursive(icon)
            }
        }
    }

    private fun MotionEvent.isMouseLikePointerEvent(): Boolean {
        val controllerSource =
            AndroidControllerInput.hasControllerSource(source) ||
                AndroidControllerInput.isControllerEvent(source, deviceId)
        return (source and InputDevice.SOURCE_MOUSE) == InputDevice.SOURCE_MOUSE ||
            (source and InputDevice.SOURCE_MOUSE_RELATIVE) == InputDevice.SOURCE_MOUSE_RELATIVE ||
            ((source and InputDevice.SOURCE_TOUCHPAD) == InputDevice.SOURCE_TOUCHPAD && !controllerSource)
    }

    private fun MotionEvent.isControllerMotionEvent(): Boolean =
        AndroidControllerInput.isControllerEvent(source, deviceId)

    private fun KeyEvent.shouldReapplyStreamSystemUi(): Boolean =
        keyCode == KeyEvent.KEYCODE_MENU ||
            AndroidControllerInput.isControllerEvent(source, deviceId) ||
            keyCode in KeyEvent.KEYCODE_BUTTON_A..KeyEvent.KEYCODE_BUTTON_MODE

    private companion object {
        private const val MAIN_ACTIVITY_LOG_TAG = "OpenNOWMainActivity"
        private const val STREAM_SYSTEM_UI_ENFORCE_INTERVAL_MS = 500L
        private const val STREAM_SYSTEM_UI_INPUT_REAPPLY_MS = 250L
        private const val MIN_PIP_ASPECT_RATIO = 1f / 2.39f
        private const val MAX_PIP_ASPECT_RATIO = 2.39f
    }
}

internal fun shouldPeriodicallyEnforceStreamSystemUi(
    streamActive: Boolean,
    navigationBarsVisible: Boolean,
    pointerLockEnabled: Boolean,
): Boolean = streamActive && (pointerLockEnabled || !navigationBarsVisible)

internal fun shouldRouteCapturedAndroidMousePointer(
    streamActive: Boolean,
    mouseLikePointer: Boolean,
): Boolean = streamActive && mouseLikePointer

internal fun shouldRequestAndroidMousePointerCapture(
    streamActive: Boolean,
    captureEnabled: Boolean,
    windowFocused: Boolean,
    hasPointerCapture: Boolean,
    mouseLikePointer: Boolean,
): Boolean =
    streamActive &&
        captureEnabled &&
        windowFocused &&
        !hasPointerCapture &&
        mouseLikePointer

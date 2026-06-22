package com.opencloudgaming.opennow

import android.Manifest
import android.content.Intent
import android.content.pm.PackageManager
import android.os.Build
import android.os.Bundle
import android.os.SystemClock
import android.view.Display
import android.view.InputDevice
import android.view.KeyEvent
import android.view.MotionEvent
import android.view.PointerIcon
import android.view.View
import android.view.ViewGroup
import android.view.WindowInsets
import android.view.WindowInsetsController
import android.view.WindowManager
import androidx.activity.ComponentActivity
import androidx.activity.compose.setContent
import androidx.activity.viewModels
import androidx.lifecycle.lifecycleScope
import kotlinx.coroutines.Job
import kotlinx.coroutines.delay
import kotlinx.coroutines.launch

class MainActivity : ComponentActivity() {
    private val viewModel: OpenNowViewModel by viewModels()
    private val queueStatusNotifier by lazy { AndroidQueueStatusNotifier(this) }
    private var notificationPermissionRequested = false
    private var lastHatXKeyCode: Int? = null
    private var lastHatYKeyCode: Int? = null
    private var streamSystemUiActive = false
    private var streamDisplayRefreshActive = false
    private var streamDisplayRefreshFps = 60
    private var streamSystemUiEnforcerJob: Job? = null
    private var lastStreamSystemUiInputReapplyMs = 0L

    override fun onCreate(savedInstanceState: Bundle?) {
        super.onCreate(savedInstanceState)
        setContent {
            OpenNowApp(viewModel)
        }
        lifecycleScope.launch {
            viewModel.state.collect { state ->
                requestQueueNotificationPermissionIfNeeded(state)
                queueStatusNotifier.update(state)
                val streamActive = state.page == AppPage.Stream && state.streamStatus != "idle"
                applyStreamSystemUi(streamActive)
                applyStreamDisplayRefreshRate(streamActive, state.activeStreamSettings?.fps ?: state.settings.stream.fps)
            }
        }
        viewModel.handleExternalLaunchIntent(intent)
    }

    override fun onNewIntent(intent: Intent) {
        super.onNewIntent(intent)
        setIntent(intent)
        viewModel.handleExternalLaunchIntent(intent)
    }

    override fun onResume() {
        super.onResume()
        if (streamSystemUiActive) {
            applyStreamSystemUi(true, force = true)
            applyStreamDisplayRefreshRate(streamDisplayRefreshActive, streamDisplayRefreshFps, force = true)
        }
    }

    override fun dispatchKeyEvent(event: KeyEvent): Boolean {
        if (streamSystemUiActive && event.action == KeyEvent.ACTION_DOWN && event.shouldReapplyStreamSystemUi()) {
            enforceStreamSystemUiFromInput()
        }
        if (NativeStreamInputRouter.dispatchKey(event)) {
            return true
        }
        val normalizedStreamUiKeyCode = NativeStreamInputRouter.normalizedStreamUiKeyCode(event)
        if (normalizedStreamUiKeyCode != null && normalizedStreamUiKeyCode != event.keyCode) {
            return dispatchSyntheticStreamUiKey(normalizedStreamUiKeyCode, event)
        }
        if (NativeStreamInputRouter.isControllerAppBackKey(event)) {
            if (event.action == KeyEvent.ACTION_UP) {
                viewModel.handleControllerBackNavigation()
            }
            return true
        }
        val normalizedAppUiKeyCode = NativeStreamInputRouter.normalizedAppUiKeyCode(event)
        if (normalizedAppUiKeyCode != null && normalizedAppUiKeyCode != event.keyCode) {
            return dispatchSyntheticStreamUiKey(normalizedAppUiKeyCode, event)
        }
        return super.dispatchKeyEvent(event)
    }

    override fun dispatchGenericMotionEvent(event: MotionEvent): Boolean {
        if (streamSystemUiActive && (event.isMouseLikePointerEvent() || event.isControllerMotionEvent())) {
            enforceStreamSystemUiFromInput()
        }
        return NativeStreamInputRouter.dispatchMotion(event) ||
            dispatchGamepadHatNavigation(event) ||
            super.dispatchGenericMotionEvent(event)
    }

    override fun dispatchTouchEvent(event: MotionEvent): Boolean {
        val decorView = window?.decorView
        if (decorView != null && NativeStreamInputRouter.dispatchExternalMouseTouch(event, decorView.width, decorView.height)) return true
        if (decorView != null && NativeStreamInputRouter.shouldForwardTouchBeforeViews(event, decorView.width, decorView.height)) {
            if (event.actionMasked == MotionEvent.ACTION_DOWN) {
                NativeInputDiagnostics.add("activity touch forwardBeforeViews size=${decorView.width}x${decorView.height}")
            }
            val forwarded = NativeStreamInputRouter.dispatchTouch(event, decorView.width, decorView.height)
            if (NativeStreamInputRouter.shouldCaptureTouchBeforeViews(event, decorView.width, decorView.height) && forwarded) {
                return true
            }
        }
        val handled = super.dispatchTouchEvent(event)
        if (handled) {
            if (event.actionMasked == MotionEvent.ACTION_DOWN) {
                NativeInputDiagnostics.add("activity touch consumedByView action=${event.actionMasked}")
            }
            return true
        }
        return if (decorView != null) {
            if (event.actionMasked == MotionEvent.ACTION_DOWN) {
                NativeInputDiagnostics.add("activity touch fallback size=${decorView.width}x${decorView.height}")
            }
            NativeStreamInputRouter.dispatchTouch(event, decorView.width, decorView.height)
        } else {
            false
        }
    }

    override fun onDestroy() {
        if (isFinishing) {
            queueStatusNotifier.cancel()
        }
        super.onDestroy()
    }

    override fun onWindowFocusChanged(hasFocus: Boolean) {
        super.onWindowFocusChanged(hasFocus)
        if (hasFocus && streamSystemUiActive) {
            applyStreamSystemUi(true, force = true)
            applyStreamDisplayRefreshRate(streamDisplayRefreshActive, streamDisplayRefreshFps, force = true)
        }
    }

    fun enforceStreamSystemUiFromInput() {
        if (!streamSystemUiActive) return
        val now = SystemClock.uptimeMillis()
        if (now - lastStreamSystemUiInputReapplyMs < STREAM_SYSTEM_UI_INPUT_REAPPLY_MS) return
        lastStreamSystemUiInputReapplyMs = now
        applyStreamSystemUi(true, force = true)
    }

    private fun applyStreamSystemUi(active: Boolean, force: Boolean = false) {
        if (!force && streamSystemUiActive == active) {
            applyStreamKeepAwake(active)
            updateStreamSystemUiEnforcer(active)
            return
        }
        streamSystemUiActive = active
        applyStreamPointerIcon(active)
        applyStreamKeepAwake(active)
        updateStreamSystemUiEnforcer(active)

        if (Build.VERSION.SDK_INT >= Build.VERSION_CODES.P) {
            window.attributes = window.attributes.apply {
                layoutInDisplayCutoutMode = if (active) {
                    WindowManager.LayoutParams.LAYOUT_IN_DISPLAY_CUTOUT_MODE_SHORT_EDGES
                } else {
                    WindowManager.LayoutParams.LAYOUT_IN_DISPLAY_CUTOUT_MODE_DEFAULT
                }
            }
        }

        if (Build.VERSION.SDK_INT >= Build.VERSION_CODES.R) {
            window.setDecorFitsSystemWindows(!active)
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

    private fun applyStreamKeepAwake(active: Boolean) {
        if (active) {
            window.addFlags(WindowManager.LayoutParams.FLAG_KEEP_SCREEN_ON)
        } else {
            window.clearFlags(WindowManager.LayoutParams.FLAG_KEEP_SCREEN_ON)
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
                if (streamSystemUiActive) {
                    applyStreamSystemUi(true, force = true)
                }
            }
        }
    }

    private fun applyStreamPointerIcon(active: Boolean) {
        if (Build.VERSION.SDK_INT < Build.VERSION_CODES.N) return
        val icon = if (active) PointerIcon.getSystemIcon(this, PointerIcon.TYPE_NULL) else null
        window.decorView.applyPointerIconRecursive(icon)
    }

    private fun applyStreamDisplayRefreshRate(active: Boolean, requestedFps: Int, force: Boolean = false) {
        streamDisplayRefreshActive = active
        streamDisplayRefreshFps = requestedFps
        if (Build.VERSION.SDK_INT < Build.VERSION_CODES.M) return

        val display = window.decorView.display
        val selectedMode = if (active) {
            selectStreamDisplayMode(
                supportedModes = display?.supportedModes.orEmpty().map { it.toDisplayRefreshMode() },
                currentMode = display?.mode?.toDisplayRefreshMode(),
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
            return
        }
        window.attributes = attributes.apply {
            preferredDisplayModeId = preferredModeId
            this.preferredRefreshRate = preferredRefreshRate
        }
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
            sourceEvent.deviceId,
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
            sourceEvent.deviceId,
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
            (source and InputDevice.SOURCE_JOYSTICK) == InputDevice.SOURCE_JOYSTICK ||
                (source and InputDevice.SOURCE_GAMEPAD) == InputDevice.SOURCE_GAMEPAD
        return (source and InputDevice.SOURCE_MOUSE) == InputDevice.SOURCE_MOUSE ||
            (source and InputDevice.SOURCE_MOUSE_RELATIVE) == InputDevice.SOURCE_MOUSE_RELATIVE ||
            ((source and InputDevice.SOURCE_TOUCHPAD) == InputDevice.SOURCE_TOUCHPAD && !controllerSource)
    }

    private fun MotionEvent.isControllerMotionEvent(): Boolean =
        (source and InputDevice.SOURCE_JOYSTICK) == InputDevice.SOURCE_JOYSTICK ||
            (source and InputDevice.SOURCE_GAMEPAD) == InputDevice.SOURCE_GAMEPAD

    private fun KeyEvent.shouldReapplyStreamSystemUi(): Boolean =
        keyCode == KeyEvent.KEYCODE_BACK ||
            keyCode == KeyEvent.KEYCODE_MENU ||
            (source and InputDevice.SOURCE_GAMEPAD) == InputDevice.SOURCE_GAMEPAD ||
            (source and InputDevice.SOURCE_JOYSTICK) == InputDevice.SOURCE_JOYSTICK ||
            (source and InputDevice.SOURCE_DPAD) == InputDevice.SOURCE_DPAD ||
            keyCode in KeyEvent.KEYCODE_BUTTON_A..KeyEvent.KEYCODE_BUTTON_MODE

    private companion object {
        private const val STREAM_SYSTEM_UI_ENFORCE_INTERVAL_MS = 500L
        private const val STREAM_SYSTEM_UI_INPUT_REAPPLY_MS = 250L
    }
}

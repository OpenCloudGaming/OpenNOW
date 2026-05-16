package com.opencloudgaming.opennow

import android.Manifest
import android.content.Intent
import android.content.pm.PackageManager
import android.os.Build
import android.os.Bundle
import android.view.InputDevice
import android.view.KeyEvent
import android.view.MotionEvent
import androidx.activity.ComponentActivity
import androidx.activity.compose.setContent
import androidx.activity.viewModels
import androidx.lifecycle.lifecycleScope
import kotlinx.coroutines.launch

class MainActivity : ComponentActivity() {
    private val viewModel: OpenNowViewModel by viewModels()
    private val queueStatusNotifier by lazy { AndroidQueueStatusNotifier(this) }
    private var notificationPermissionRequested = false
    private var lastHatXKeyCode: Int? = null
    private var lastHatYKeyCode: Int? = null

    override fun onCreate(savedInstanceState: Bundle?) {
        super.onCreate(savedInstanceState)
        setContent {
            OpenNowApp(viewModel)
        }
        lifecycleScope.launch {
            viewModel.state.collect { state ->
                requestQueueNotificationPermissionIfNeeded(state)
                queueStatusNotifier.update(state)
            }
        }
        viewModel.handleExternalLaunchIntent(intent)
    }

    override fun onNewIntent(intent: Intent) {
        super.onNewIntent(intent)
        setIntent(intent)
        viewModel.handleExternalLaunchIntent(intent)
    }

    override fun dispatchKeyEvent(event: KeyEvent): Boolean {
        return NativeStreamInputRouter.dispatchKey(event) || super.dispatchKeyEvent(event)
    }

    override fun dispatchGenericMotionEvent(event: MotionEvent): Boolean {
        return NativeStreamInputRouter.dispatchMotion(event) ||
            dispatchGamepadHatNavigation(event) ||
            super.dispatchGenericMotionEvent(event)
    }

    override fun dispatchTouchEvent(event: MotionEvent): Boolean {
        val decorView = window?.decorView
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
}

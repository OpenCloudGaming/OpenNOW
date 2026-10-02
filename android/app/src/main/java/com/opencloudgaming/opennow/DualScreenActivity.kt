package com.opencloudgaming.opennow

import android.content.Context
import android.os.Bundle
import android.view.KeyEvent
import android.view.MotionEvent
import android.view.WindowManager
import androidx.activity.ComponentActivity
import androidx.activity.OnBackPressedCallback
import androidx.activity.compose.setContent
import androidx.compose.runtime.snapshotFlow
import androidx.core.view.WindowCompat
import androidx.core.view.WindowInsetsCompat
import androidx.core.view.WindowInsetsControllerCompat
import androidx.lifecycle.lifecycleScope
import kotlinx.coroutines.launch

/**
 * Companion deck on the second built-in display (AYN Thor bottom screen).
 *
 * Normally the window is not focusable: controller and keyboard focus stays with the game window
 * on the top screen, while touch on this display still works, and any hardware event that does
 * arrive here is forwarded to MainActivity. While a top-screen panel is hosted here (Stream
 * Controls, store or server picker) the window becomes focusable so its text fields can take
 * typing; dismiss keys still go to MainActivity, which owns closing those panels.
 */
class DualScreenActivity : ComponentActivity() {
    private var hostingPanel = false

    override fun attachBaseContext(newBase: Context) {
        super.attachBaseContext(localizedAndroidContext(newBase))
    }

    override fun onCreate(savedInstanceState: Bundle?) {
        super.onCreate(savedInstanceState)
        window.addFlags(WindowManager.LayoutParams.FLAG_NOT_FOCUSABLE)
        WindowCompat.setDecorFitsSystemWindows(window, false)
        WindowInsetsControllerCompat(window, window.decorView).apply {
            hide(WindowInsetsCompat.Type.systemBars())
            systemBarsBehavior = WindowInsetsControllerCompat.BEHAVIOR_SHOW_TRANSIENT_BARS_BY_SWIPE
        }
        // The deck lives exactly as long as MainActivity wants it; Back never closes it.
        onBackPressedDispatcher.addCallback(this, object : OnBackPressedCallback(true) {
            override fun handleOnBackPressed() = Unit
        })
        DualScreenBridge.attachDeck(this)
        lifecycleScope.launch {
            snapshotFlow { DualScreenBridge.hostedContent.value != null }.collect { hosting ->
                hostingPanel = hosting
                if (hosting) {
                    window.clearFlags(WindowManager.LayoutParams.FLAG_NOT_FOCUSABLE)
                } else {
                    window.addFlags(WindowManager.LayoutParams.FLAG_NOT_FOCUSABLE)
                }
            }
        }
        setContent {
            DualScreenDeck(onAction = DualScreenBridge::dispatch)
        }
    }

    override fun onDestroy() {
        DualScreenBridge.detachDeck(this)
        super.onDestroy()
    }

    override fun dispatchKeyEvent(event: KeyEvent): Boolean {
        val forward = !hostingPanel || event.keyCode in PANEL_DISMISS_KEYS
        return (forward && DualScreenBridge.forwardToMain { it.dispatchKeyEvent(event) }) ||
            super.dispatchKeyEvent(event)
    }

    override fun dispatchGenericMotionEvent(event: MotionEvent): Boolean =
        (!hostingPanel && DualScreenBridge.forwardToMain { it.dispatchGenericMotionEvent(event) }) ||
            super.dispatchGenericMotionEvent(event)

    private companion object {
        val PANEL_DISMISS_KEYS = setOf(
            KeyEvent.KEYCODE_BACK,
            KeyEvent.KEYCODE_BUTTON_B,
            KeyEvent.KEYCODE_ESCAPE,
        )
    }
}

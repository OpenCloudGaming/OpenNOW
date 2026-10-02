package com.opencloudgaming.opennow

import android.content.Context
import android.os.Bundle
import android.view.KeyEvent
import android.view.MotionEvent
import android.view.WindowManager
import androidx.activity.ComponentActivity
import androidx.activity.compose.setContent
import androidx.core.view.WindowCompat
import androidx.core.view.WindowInsetsCompat
import androidx.core.view.WindowInsetsControllerCompat

/**
 * Companion deck on the second built-in display (AYN Thor bottom screen).
 *
 * The window is never focusable: controller and keyboard focus stays with the game window on the
 * top screen, while touch on this display still works. Any hardware event that does arrive here is
 * forwarded to MainActivity so a button press can never be swallowed by the deck.
 */
class DualScreenActivity : ComponentActivity() {
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
        DualScreenBridge.attachDeck(this)
        setContent {
            DualScreenDeck(onAction = DualScreenBridge::dispatch)
        }
    }

    override fun onDestroy() {
        DualScreenBridge.detachDeck(this)
        super.onDestroy()
    }

    override fun dispatchKeyEvent(event: KeyEvent): Boolean =
        DualScreenBridge.forwardToMain { it.dispatchKeyEvent(event) } || super.dispatchKeyEvent(event)

    override fun dispatchGenericMotionEvent(event: MotionEvent): Boolean =
        DualScreenBridge.forwardToMain { it.dispatchGenericMotionEvent(event) } ||
            super.dispatchGenericMotionEvent(event)
}

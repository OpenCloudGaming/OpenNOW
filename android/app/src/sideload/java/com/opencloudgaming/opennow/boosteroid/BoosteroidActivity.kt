package com.opencloudgaming.opennow.boosteroid

import android.content.pm.ActivityInfo
import android.content.res.Configuration
import android.graphics.Color
import android.os.Bundle
import android.view.KeyEvent
import android.view.MotionEvent
import android.view.WindowManager
import androidx.activity.ComponentActivity
import androidx.activity.SystemBarStyle
import androidx.activity.compose.BackHandler
import androidx.activity.compose.setContent
import androidx.activity.enableEdgeToEdge
import androidx.activity.viewModels
import androidx.core.view.WindowCompat
import androidx.core.view.WindowInsetsCompat
import androidx.core.view.WindowInsetsControllerCompat
import androidx.lifecycle.compose.collectAsStateWithLifecycle
import androidx.compose.runtime.LaunchedEffect
import androidx.compose.runtime.getValue
import androidx.compose.runtime.mutableStateOf
import androidx.compose.runtime.saveable.rememberSaveable
import androidx.compose.runtime.setValue
import com.opencloudgaming.opennow.AppSettings
import com.opencloudgaming.opennow.OpenNowTheme
import com.opencloudgaming.opennow.UiAccent
import com.opencloudgaming.opennow.isAndroidTvProfile
import com.opencloudgaming.opennow.rememberPhysicalControllerConnected

internal const val EXTRA_CHOOSE_CLOUD_SERVICE = "choose_cloud_service"

internal class BoosteroidActivity : ComponentActivity() {
    private val model: BoosteroidViewModel by viewModels()
    private var streamClient: BoosteroidStreamClient? = null
    private var streamMenuOpen = false
    private var immersive = false

    override fun onCreate(savedInstanceState: Bundle?) {
        super.onCreate(savedInstanceState)
        enableEdgeToEdge(
            statusBarStyle = SystemBarStyle.dark(Color.TRANSPARENT),
            navigationBarStyle = SystemBarStyle.dark(Color.TRANSPARENT),
        )
        val tv = isAndroidTvProfile(this)
        setContent {
            val chooseOnEntry = intent.getBooleanExtra(EXTRA_CHOOSE_CLOUD_SERVICE, false)
            var choosingService by rememberSaveable { mutableStateOf(chooseOnEntry) }
            var usingBoosteroid by rememberSaveable { mutableStateOf(!chooseOnEntry) }
            val controllerConnected = rememberPhysicalControllerConnected(enabled = true)

            OpenNowTheme(
                settings = AppSettings(uiAccent = if (usingBoosteroid) UiAccent.Violet else UiAccent.OpenNow),
                physicalControllerConnected = controllerConnected,
            ) {
                if (choosingService) {
                    BackHandler(enabled = usingBoosteroid) { choosingService = false }
                    CloudServicePicker(
                        usingBoosteroid = usingBoosteroid,
                        onNvidia = ::finish,
                        onBoosteroid = {
                            usingBoosteroid = true
                            choosingService = false
                        },
                    )
                } else {
                    val state by model.state.collectAsStateWithLifecycle()
                    LaunchedEffect(state.launch != null) { updateStreamWindow(state.launch != null, tv) }
                    BoosteroidScreen(
                        state = state,
                        model = model,
                        tv = tv,
                        onNvidia = ::finish,
                        onSwitchService = { choosingService = true },
                        onStreamInputChanged = { client, menuOpen ->
                            if (streamClient !== client || (menuOpen && !streamMenuOpen)) streamClient?.releaseInput()
                            streamClient = client
                            streamMenuOpen = menuOpen
                        },
                    )
                }
            }
        }
    }

    override fun onWindowFocusChanged(hasFocus: Boolean) {
        super.onWindowFocusChanged(hasFocus)
        if (hasFocus && immersive) applySystemBars()
        if (!hasFocus) streamClient?.releaseInput()
    }

    override fun onStop() {
        streamClient?.releaseInput()
        super.onStop()
    }

    override fun dispatchKeyEvent(event: KeyEvent): Boolean {
        if (!streamMenuOpen && event.keyCode !in androidSystemKeys && streamClient?.dispatchKey(event) == true) {
            return true
        }
        return super.dispatchKeyEvent(event)
    }

    override fun dispatchGenericMotionEvent(event: MotionEvent): Boolean =
        (!streamMenuOpen && streamClient?.dispatchMotion(event) == true) || super.dispatchGenericMotionEvent(event)

    private fun updateStreamWindow(active: Boolean, tv: Boolean) {
        immersive = active
        if (active) window.addFlags(WindowManager.LayoutParams.FLAG_KEEP_SCREEN_ON)
        else window.clearFlags(WindowManager.LayoutParams.FLAG_KEEP_SCREEN_ON)
        if (!tv && resources.configuration.smallestScreenWidthDp < 600) {
            requestedOrientation = if (active) ActivityInfo.SCREEN_ORIENTATION_SENSOR_LANDSCAPE
            else ActivityInfo.SCREEN_ORIENTATION_UNSPECIFIED
        }
        applySystemBars()
    }

    override fun onConfigurationChanged(newConfig: Configuration) {
        super.onConfigurationChanged(newConfig)
        if (immersive) applySystemBars()
    }

    private fun applySystemBars() {
        val controller = WindowCompat.getInsetsController(window, window.decorView)
        if (immersive) {
            controller.systemBarsBehavior = WindowInsetsControllerCompat.BEHAVIOR_SHOW_TRANSIENT_BARS_BY_SWIPE
            controller.hide(WindowInsetsCompat.Type.systemBars())
        } else {
            controller.show(WindowInsetsCompat.Type.systemBars())
        }
    }

    private companion object {
        val androidSystemKeys = setOf(
            KeyEvent.KEYCODE_BACK,
            KeyEvent.KEYCODE_VOLUME_UP,
            KeyEvent.KEYCODE_VOLUME_DOWN,
            KeyEvent.KEYCODE_VOLUME_MUTE,
            KeyEvent.KEYCODE_MENU,
        )
    }
}

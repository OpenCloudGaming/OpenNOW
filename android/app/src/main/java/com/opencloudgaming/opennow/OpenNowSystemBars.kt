package com.opencloudgaming.opennow

import android.view.Window
import androidx.core.view.WindowCompat
import androidx.core.view.WindowInsetsCompat
import androidx.core.view.WindowInsetsControllerCompat

/** The window owns visibility; stream input, pointer capture and keep-awake remain independent. */
internal fun applyOpenNowSystemBars(window: Window, immersive: Boolean) {
    // Keep edge-to-edge active when returning from a stream on API 24–29 too. Clearing
    // systemUiVisibility to zero restores decor fitting while Compose still applies insets.
    WindowCompat.setDecorFitsSystemWindows(window, false)
    val controller = WindowCompat.getInsetsController(window, window.decorView)
    val bars = WindowInsetsCompat.Type.statusBars() or WindowInsetsCompat.Type.navigationBars()
    if (immersive) {
        controller.systemBarsBehavior = WindowInsetsControllerCompat.BEHAVIOR_SHOW_TRANSIENT_BARS_BY_SWIPE
        controller.hide(bars)
    } else {
        controller.show(bars)
    }
}

/** Space UI stays landscape even while stream settings are open. */
internal fun openNowRequestedOrientation(defaultOrientation: Int, streamLocked: Boolean,
    streamPanelExpanded: Boolean, spaceEnabled: Boolean): Int = when {
    spaceEnabled -> android.content.pm.ActivityInfo.SCREEN_ORIENTATION_SENSOR_LANDSCAPE
    streamPanelExpanded -> android.content.pm.ActivityInfo.SCREEN_ORIENTATION_FULL_SENSOR
    streamLocked -> android.content.pm.ActivityInfo.SCREEN_ORIENTATION_SENSOR_LANDSCAPE
    else -> defaultOrientation
}

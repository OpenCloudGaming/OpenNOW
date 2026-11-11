package com.opencloudgaming.opennow

import android.os.Build
import android.view.View
import androidx.activity.ComponentActivity
import androidx.activity.enableEdgeToEdge
import androidx.compose.foundation.layout.Box
import androidx.compose.foundation.layout.fillMaxSize
import androidx.compose.ui.Modifier
import androidx.compose.ui.platform.testTag
import androidx.compose.ui.test.assertHeightIsEqualTo
import androidx.compose.ui.test.assertWidthIsEqualTo
import androidx.compose.ui.test.junit4.createAndroidComposeRule
import androidx.compose.ui.test.onNodeWithTag
import androidx.compose.ui.unit.dp
import androidx.core.view.ViewCompat
import androidx.core.view.WindowInsetsCompat
import org.junit.Assert.assertEquals
import org.junit.Rule
import org.junit.Test

class SystemBarsInstrumentedTest {
    @get:Rule val compose = createAndroidComposeRule<ComponentActivity>()

    @Test fun restoringBarsAfterImmersiveKeepsTheFullWindowViewport() {
        compose.runOnUiThread { compose.activity.enableEdgeToEdge() }
        compose.setContent { Box(Modifier.fillMaxSize().testTag("viewport")) }
        compose.waitUntil(5_000) { compose.activity.window.decorView.hasWindowFocus() }

        // Repeat to cover stream restart and returning from expanded stream settings.
        repeat(2) {
            compose.runOnUiThread { applyOpenNowSystemBars(compose.activity.window, true) }
            awaitBars(visible = false)
            assertFullWindowViewport()
            compose.runOnUiThread { applyOpenNowSystemBars(compose.activity.window, false) }
            awaitBars(visible = true)
            assertFullWindowViewport()
        }
    }

    @Suppress("DEPRECATION")
    private fun awaitBars(visible: Boolean) {
        compose.waitUntil(5_000) {
            val decor = compose.activity.window.decorView
            if (Build.VERSION.SDK_INT < Build.VERSION_CODES.R) {
                // Before API 30, compat visibility is inferred from inset sizes. Stable
                // edge-to-edge insets can remain nonzero even while the bars are hidden.
                val hiddenFlags = View.SYSTEM_UI_FLAG_FULLSCREEN or View.SYSTEM_UI_FLAG_HIDE_NAVIGATION
                decor.systemUiVisibility and hiddenFlags == if (visible) 0 else hiddenFlags
            } else {
                val insets = ViewCompat.getRootWindowInsets(decor)
                insets != null &&
                    insets.isVisible(WindowInsetsCompat.Type.statusBars()) == visible &&
                    insets.isVisible(WindowInsetsCompat.Type.navigationBars()) == visible
            }
        }
        compose.waitForIdle()
    }

    @Suppress("DEPRECATION")
    private fun assertFullWindowViewport() {
        val decor = compose.activity.window.decorView
        val density = compose.activity.resources.displayMetrics.density
        compose.onNodeWithTag("viewport")
            .assertWidthIsEqualTo((decor.width / density).dp)
            .assertHeightIsEqualTo((decor.height / density).dp)
        if (Build.VERSION.SDK_INT < Build.VERSION_CODES.R) {
            val layoutFlags = View.SYSTEM_UI_FLAG_LAYOUT_FULLSCREEN or
                View.SYSTEM_UI_FLAG_LAYOUT_HIDE_NAVIGATION or View.SYSTEM_UI_FLAG_LAYOUT_STABLE
            assertEquals(layoutFlags, decor.systemUiVisibility and layoutFlags)
        }
    }
}

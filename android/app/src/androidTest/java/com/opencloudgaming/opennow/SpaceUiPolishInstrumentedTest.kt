package com.opencloudgaming.opennow

import android.graphics.Bitmap
import androidx.activity.ComponentActivity
import androidx.compose.foundation.background
import androidx.compose.foundation.layout.Box
import androidx.compose.foundation.layout.fillMaxSize
import androidx.compose.foundation.layout.padding
import androidx.compose.foundation.layout.size
import androidx.compose.runtime.CompositionLocalProvider
import androidx.compose.runtime.mutableStateOf
import androidx.compose.runtime.getValue
import androidx.compose.runtime.setValue
import androidx.compose.ui.Modifier
import androidx.compose.ui.graphics.asAndroidBitmap
import androidx.compose.ui.input.InputMode
import androidx.compose.ui.input.InputModeManager
import androidx.compose.ui.platform.LocalInputModeManager
import androidx.compose.ui.platform.LocalDensity
import androidx.compose.ui.platform.testTag
import androidx.compose.ui.test.*
import androidx.compose.ui.test.junit4.createAndroidComposeRule
import androidx.compose.ui.unit.dp
import androidx.compose.ui.unit.Density
import androidx.test.platform.app.InstrumentationRegistry
import com.opencloudgaming.opennow.ui.theme.LocalReduceMotion
import org.junit.Assert.assertEquals
import org.junit.Assert.assertFalse
import org.junit.Assert.assertTrue
import org.junit.Rule
import org.junit.Test
import java.io.File

class SpaceUiPolishInstrumentedTest {
    @get:Rule val compose = createAndroidComposeRule<ComponentActivity>()

    @Test fun browseButtonOpensCatalogAndBackKeepsPreviewWithoutLaunching() {
        var launches = 0
        showDiscovery { launches++ }
        compose.onNodeWithTag("space-browse-all").assertIsDisplayed().performClick()
        compose.onNodeWithTag("space-discover-catalog").assertIsDisplayed()
        compose.onNodeWithTag("space-game-1").assertIsSelected()
        compose.onNodeWithTag("space-catalog-back").performClick()
        compose.onNodeWithTag("space-game-rail").assertIsDisplayed()
        compose.onNodeWithTag("space-game-1").assertIsSelected()
        compose.runOnIdle { assertEquals(0, launches) }
        capture("space-ui-polish")
    }

    @Test fun landscapeDiscoveryKeepsBrowseAndGameRailVisible() {
        compose.runOnUiThread {
            compose.activity.requestedOrientation = android.content.pm.ActivityInfo.SCREEN_ORIENTATION_LANDSCAPE
        }
        compose.waitUntil(5_000) {
            compose.activity.resources.configuration.orientation == android.content.res.Configuration.ORIENTATION_LANDSCAPE
        }
        showDiscovery {}
        compose.onNodeWithTag("space-browse-all").assertIsDisplayed()
        compose.onNodeWithTag("space-game-art-1", useUnmergedTree = true).assertIsDisplayed()
        capture("space-ui-polish-landscape")
    }

    @Test fun shortPhoneWindowWithLargerTextKeepsTheWholeGameTileVisible() {
        showDiscovery(compactWindow = true) {}
        compose.onNodeWithTag("space-browse-all").assertIsDisplayed()
        val context = InstrumentationRegistry.getInstrumentation().targetContext
        compose.onNodeWithContentDescription(context.getString(R.string.nav_library)).assertIsDisplayed()
        compose.onNodeWithContentDescription(context.getString(R.string.nav_settings)).assertIsDisplayed()
        val scene = compose.onNodeWithTag("space-scene-content").getUnclippedBoundsInRoot()
        val tile = compose.onNodeWithTag("space-game-1").getUnclippedBoundsInRoot()
        assertTrue("The full touch target and focus gutter fit above the viewport edge", tile.bottom <= scene.bottom)
        capture("space-ui-polish-short-window")
    }

    private fun showDiscovery(compactWindow: Boolean = false, onSelect: () -> Unit) {
        val context = InstrumentationRegistry.getInstrumentation().targetContext
        val art = "android.resource://${context.packageName}/${R.drawable.catalog_colorful_abstract_background}"
        val games = listOf(
            GameInfo(id = "1", title = "Beyond the horizon", imageUrl = art, tvBannerUrl = art,
                publisherName = "Horizon Studio", description = "Find your next adventure in a world beyond the horizon."),
            GameInfo(id = "2", title = "Nightfall", imageUrl = art, tvBannerUrl = art),
        )
        compose.setContent {
            val density = LocalDensity.current
            CompositionLocalProvider(LocalReduceMotion provides true,
                LocalDensity provides Density(density.density, if (compactWindow) 1.3f else density.fontScale)) {
                SpaceUiTheme {
                    Box(if (compactWindow) Modifier.size(width = 360.dp, height = 280.dp) else Modifier.fillMaxSize()) {
                        SpaceUiScreen(OpenNowUiState(games = games), onNavigate = {}, onSelectGame = { onSelect() },
                            onRefresh = {}, onSearchChange = {}, onResumeSession = {}, inputEnabled = true, settingsContent = {})
                    }
                }
            }
        }
    }

    @Test fun keyboardGetsAVisibleFocusFrameWithoutAControllerAndReducedMotionStaysStill() {
        val inputMode = object : InputModeManager {
            override var inputMode by mutableStateOf(InputMode.Touch)
            override fun requestInputMode(inputMode: InputMode): Boolean {
                this.inputMode = inputMode
                return true
            }
        }
        compose.setContent {
            CompositionLocalProvider(LocalInputModeManager provides inputMode,
                LocalSpaceControllerConnected provides false, LocalReduceMotion provides true) {
                Box(Modifier.size(120.dp).background(SpaceInk).testTag("focus-sample").padding(24.dp)) {
                    Box(Modifier.fillMaxSize().spaceFocusFrame(true, 8.dp).background(SpaceInk))
                }
            }
        }
        val touch = compose.onNodeWithTag("focus-sample").captureToImage().asAndroidBitmap()
        compose.runOnIdle { assertTrue(inputMode.requestInputMode(InputMode.Keyboard)) }
        val keyboard = compose.onNodeWithTag("focus-sample").captureToImage().asAndroidBitmap()
        assertFalse("Keyboard-only users need the same visible frame as controller users", touch.sameAs(keyboard))
        compose.mainClock.advanceTimeBy(3_000)
        val settled = compose.onNodeWithTag("focus-sample").captureToImage().asAndroidBitmap()
        assertTrue("Reduced motion must keep focus lighting static", keyboard.sameAs(settled))
    }

    private fun capture(name: String) {
        val bitmap = compose.onRoot().captureToImage().asAndroidBitmap()
        val context = InstrumentationRegistry.getInstrumentation().targetContext
        File(context.getExternalFilesDir(null), "$name-${bitmap.width}x${bitmap.height}.png").outputStream().use {
            bitmap.compress(Bitmap.CompressFormat.PNG, 100, it)
        }
    }
}

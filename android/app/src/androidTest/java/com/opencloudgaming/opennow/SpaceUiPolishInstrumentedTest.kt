package com.opencloudgaming.opennow

import android.graphics.Bitmap
import androidx.activity.ComponentActivity
import androidx.compose.foundation.background
import androidx.compose.foundation.layout.Box
import androidx.compose.foundation.layout.fillMaxSize
import androidx.compose.foundation.layout.padding
import androidx.compose.foundation.layout.size
import androidx.compose.runtime.CompositionLocalProvider
import androidx.compose.runtime.SideEffect
import androidx.compose.runtime.mutableStateOf
import androidx.compose.runtime.getValue
import androidx.compose.runtime.setValue
import androidx.compose.ui.Modifier
import androidx.compose.ui.geometry.Offset
import androidx.compose.ui.input.key.Key
import androidx.compose.ui.input.nestedscroll.NestedScrollConnection
import androidx.compose.ui.input.nestedscroll.NestedScrollDispatcher
import androidx.compose.ui.input.nestedscroll.NestedScrollSource
import androidx.compose.ui.input.nestedscroll.nestedScroll
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
import com.opencloudgaming.opennow.ui.theme.LocalReduceForegroundMotion
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

    @Test fun focusScrollingDoesNotExpandTheCatalogWithoutATouchGesture() {
        var expansions = 0
        val dispatcher = NestedScrollDispatcher()
        compose.setContent {
            Box(Modifier.size(300.dp).expandSpaceCatalogOnSwipe(true) { expansions++ }) {
                Box(Modifier.fillMaxSize().nestedScroll(object : NestedScrollConnection {}, dispatcher))
            }
        }
        compose.runOnIdle {
            // Scrollable containers also dispatch keyboard/focus movement as UserInput.
            dispatcher.dispatchPreScroll(Offset(0f, -500f), NestedScrollSource.UserInput)
            assertEquals(0, expansions)
        }
    }

    @Test fun onlyASingleIntentionalUpwardTouchSwipeExpandsTheCatalog() {
        var expansions = 0
        compose.setContent {
            Box(Modifier.size(300.dp).testTag("swipe-area")
                .expandSpaceCatalogOnSwipe(true) { expansions++ })
        }
        val area = compose.onNodeWithTag("swipe-area")
        repeat(3) {
            area.performTouchInput { swipe(center, center - Offset(0f, 20.dp.toPx())) }
        }
        area.performTouchInput { swipeLeft() }
        compose.runOnIdle { assertEquals(0, expansions) }
        area.performTouchInput { swipeUp() }
        compose.runOnIdle { assertEquals(1, expansions) }
    }

    @Test fun remoteBackReturnsToTheRailAndStaysThere() {
        compose.runOnUiThread {
            compose.activity.requestedOrientation = android.content.pm.ActivityInfo.SCREEN_ORIENTATION_LANDSCAPE
        }
        compose.waitUntil(5_000) {
            compose.activity.resources.configuration.orientation == android.content.res.Configuration.ORIENTATION_LANDSCAPE
        }
        showDiscovery {}
        repeat(3) {
            compose.onNodeWithTag("space-game-1").performSemanticsAction(
                androidx.compose.ui.semantics.SemanticsActions.RequestFocus) { it() }
            compose.onNodeWithTag("space-game-1").performKeyInput { pressKey(Key.DirectionDown) }
            compose.onNodeWithTag("space-discover-catalog").assertIsDisplayed()
            compose.runOnUiThread { compose.activity.onBackPressedDispatcher.onBackPressed() }
            compose.waitForIdle()
            compose.mainClock.advanceTimeBy(1_000)
            compose.onNodeWithTag("space-discover-catalog").assertDoesNotExist()
            compose.onNodeWithTag("space-game-rail").assertIsDisplayed()
            compose.onNodeWithTag("space-game-1").assertIsFocused()
        }
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
        val artwork = compose.onNodeWithTag("space-game-art-1", useUnmergedTree = true).assertIsDisplayed().getUnclippedBoundsInRoot()
        assertEquals("Landscape phones keep square discovery cards", 1f,
            (artwork.right - artwork.left).value / (artwork.bottom - artwork.top).value, .02f)
        capture("space-ui-polish-landscape")
    }

    @Test fun tvDiscoverUsesTallCoversAndBrowseAllKeepsPortraitCovers() {
        compose.runOnUiThread {
            compose.activity.requestedOrientation = android.content.pm.ActivityInfo.SCREEN_ORIENTATION_LANDSCAPE
        }
        compose.waitUntil(5_000) {
            compose.activity.resources.configuration.orientation == android.content.res.Configuration.ORIENTATION_LANDSCAPE
        }
        showDiscovery(tvProfile = true) {}
        fun assertAspect(expected: Float) {
            val art = compose.onNodeWithTag("space-game-art-1", useUnmergedTree = true).getUnclippedBoundsInRoot()
            assertEquals(expected, (art.right - art.left).value / (art.bottom - art.top).value, .02f)
        }
        assertAspect(628f / 888f)
        capture("space-ui-tv-covers")
        compose.onNodeWithTag("space-browse-all").performClick()
        compose.onNodeWithTag("space-discover-catalog").assertIsDisplayed()
        assertAspect(628f / 888f)
        compose.onNodeWithTag("space-catalog-back").performClick()
        assertAspect(628f / 888f)
        compose.onNodeWithTag("space-game-1").assertIsSelected()
    }

    @Test fun tvGameDetailsUseTallCoverArtworkAndKeepPlayVisible() {
        compose.runOnUiThread {
            compose.activity.requestedOrientation = android.content.pm.ActivityInfo.SCREEN_ORIENTATION_LANDSCAPE
        }
        compose.waitUntil(5_000) {
            compose.activity.resources.configuration.orientation == android.content.res.Configuration.ORIENTATION_LANDSCAPE
        }
        val context = InstrumentationRegistry.getInstrumentation().targetContext
        val art = "android.resource://${context.packageName}/${R.drawable.catalog_colorful_abstract_background}"
        val game = GameInfo("1", title = "Beyond the horizon", imageUrl = art, tvBannerUrl = art,
            publisherName = "Horizon Studio")
        var launches = 0
        compose.setContent {
            CompositionLocalProvider(LocalReduceMotion provides true, LocalReduceForegroundMotion provides true) {
                SpaceGameHub(game, false, null, onPlay = { launches++ }, onChooseStore = {}, onFavorite = {},
                    connectedTvName = null, onPlayOnTv = {}, removeLabel = null, removeConfirmationText = null,
                    removeEnabled = false, onRemove = null, onDismiss = {}, tvProfile = true)
            }
        }
        val artwork = compose.onNodeWithTag("space-hub-artwork").getUnclippedBoundsInRoot()
        assertEquals(628f / 888f, (artwork.right - artwork.left).value / (artwork.bottom - artwork.top).value, .02f)
        compose.onNodeWithTag("space-hub-play").assertIsDisplayed().performClick()
        compose.runOnIdle { assertEquals(1, launches) }
        capture("space-hub-tv-cover")
    }

    @Test fun tvSafeAreaInsetsContentWhileTheSceneFillsTheWindow() {
        showDiscovery(safeAreaPadding = 40.dp) {}
        val root = compose.onRoot().getUnclippedBoundsInRoot()
        val scene = compose.onNodeWithTag("space-ui").getUnclippedBoundsInRoot()
        val content = compose.onNodeWithTag("space-scene-content").getUnclippedBoundsInRoot()
        assertEquals(root.left.value, scene.left.value, .1f)
        assertEquals(root.right.value, scene.right.value, .1f)
        assertEquals(root.top.value, scene.top.value, .1f)
        assertEquals(root.bottom.value, scene.bottom.value, .1f)
        assertTrue(content.left >= scene.left + 40.dp)
        assertTrue(content.right <= scene.right - 40.dp)
        assertTrue(content.top >= scene.top + 40.dp)
        assertTrue(content.bottom <= scene.bottom - 40.dp)
        capture("space-ui-tv-safe-area")
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

    private fun showDiscovery(compactWindow: Boolean = false, tvProfile: Boolean = false, safeAreaPadding: androidx.compose.ui.unit.Dp = 0.dp, onSelect: () -> Unit) {
        val context = InstrumentationRegistry.getInstrumentation().targetContext
        val art = "android.resource://${context.packageName}/${R.drawable.catalog_colorful_abstract_background}"
        val games = listOf(
            GameInfo(id = "1", title = "Beyond the horizon", imageUrl = art, tvBannerUrl = art,
                publisherName = "Horizon Studio", description = "Find your next adventure in a world beyond the horizon."),
            GameInfo(id = "2", title = "Nightfall", imageUrl = art, tvBannerUrl = art),
        )
        compose.setContent {
            val density = LocalDensity.current
            CompositionLocalProvider(LocalReduceMotion provides true, LocalReduceForegroundMotion provides true,
                LocalDensity provides Density(density.density, if (compactWindow) 1.3f else density.fontScale)) {
                SpaceUiTheme {
                    Box(if (compactWindow) Modifier.size(width = 360.dp, height = 280.dp) else Modifier.fillMaxSize()) {
                        SpaceUiScreen(OpenNowUiState(games = games, androidTvProfile = tvProfile), onNavigate = {}, onSelectGame = { onSelect() },
                            onRefresh = {}, onSearchChange = {}, onResumeSession = {}, inputEnabled = true,
                            safeAreaPadding = safeAreaPadding, settingsContent = {})
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
                LocalSpaceControllerConnected provides false, LocalReduceMotion provides true, LocalReduceForegroundMotion provides true) {
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

    @Test fun lowPowerSpaceFocusAndNavigationStillAnimateUnlessExplicitlyReduced() {
        compose.mainClock.autoAdvance = false
        var route by mutableStateOf(1)
        var explicitReduceMotion by mutableStateOf(false)
        compose.setContent {
            CompositionLocalProvider(LocalReduceMotion provides true,
                LocalReduceForegroundMotion provides explicitReduceMotion,
                LocalSpaceControllerConnected provides true) {
                Box(Modifier.size(200.dp).background(SpaceInk).testTag("motion-sample").padding(30.dp)) {
                    Box(Modifier.fillMaxSize().spaceArrival(route).spaceFocusFrame(true, 8.dp)
                        .background(SpaceIce))
                }
            }
        }
        compose.waitUntil(5_000) { compose.activity.hasWindowFocus() }
        compose.mainClock.advanceTimeBy(1_000)
        compose.runOnIdle { route = 2 }
        compose.mainClock.advanceTimeBy(32)
        val sample = compose.onNodeWithTag("motion-sample")
        val arriving = sample.captureToImage().asAndroidBitmap()
        compose.mainClock.advanceTimeBy(180)
        assertFalse("Low-power mode must preserve navigation transitions",
            arriving.sameAs(sample.captureToImage().asAndroidBitmap()))
        compose.mainClock.advanceTimeBy(1_000)
        val focused = sample.captureToImage().asAndroidBitmap()
        compose.mainClock.advanceTimeBy(250)
        assertFalse("Low-power mode must preserve animated controller focus",
            focused.sameAs(sample.captureToImage().asAndroidBitmap()))
        compose.runOnIdle { explicitReduceMotion = true }
        compose.mainClock.advanceTimeBy(1_000)
        val reduced = sample.captureToImage().asAndroidBitmap()
        compose.runOnIdle { route = 3 }
        compose.mainClock.advanceTimeBy(250)
        assertTrue("Explicit reduced motion must leave focus and navigation static",
            reduced.sameAs(sample.captureToImage().asAndroidBitmap()))
    }

    @Test fun focusAnimationUpdatesDrawingWithoutRecomposingItsOwner() {
        compose.mainClock.autoAdvance = false
        var focused by mutableStateOf(false)
        var compositions = 0
        compose.setContent {
            CompositionLocalProvider(LocalReduceForegroundMotion provides false) {
                val interaction = rememberSpaceInteraction(focused, tile = true)
                SideEffect { compositions++ }
                Box(Modifier.size(200.dp).background(SpaceInk).testTag("animation-owner").padding(30.dp)) {
                    Box(Modifier.fillMaxSize().then(interaction.modifier).background(SpaceIce))
                }
            }
        }
        compose.mainClock.advanceTimeBy(1_000)
        compose.runOnIdle { focused = true }
        compose.mainClock.advanceTimeBy(32)
        val before = compose.onNodeWithTag("animation-owner").captureToImage().asAndroidBitmap()
        var afterFocus = 0
        compose.runOnIdle { afterFocus = compositions }
        compose.mainClock.advanceTimeBy(96)
        val during = compose.onNodeWithTag("animation-owner").captureToImage().asAndroidBitmap()
        assertFalse("Focus must still animate visibly", before.sameAs(during))
        compose.runOnIdle {
            assertEquals("Animation frames must update the layer without recomposing its owner", afterFocus, compositions)
        }
    }

    private fun capture(name: String) {
        val bitmap = compose.onRoot().captureToImage().asAndroidBitmap()
        val context = InstrumentationRegistry.getInstrumentation().targetContext
        File(context.getExternalFilesDir(null), "$name-${bitmap.width}x${bitmap.height}.png").outputStream().use {
            bitmap.compress(Bitmap.CompressFormat.PNG, 100, it)
        }
    }
}

package com.opencloudgaming.opennow

import android.content.pm.ActivityInfo
import androidx.activity.ComponentActivity
import androidx.compose.foundation.background
import androidx.compose.foundation.layout.Box
import androidx.compose.foundation.layout.fillMaxSize
import androidx.compose.material3.MaterialTheme
import androidx.compose.material3.LocalContentColor
import androidx.compose.material3.Text
import androidx.compose.runtime.*
import androidx.compose.ui.graphics.Color
import androidx.compose.ui.graphics.toPixelMap
import androidx.compose.ui.graphics.luminance
import androidx.compose.ui.graphics.asAndroidBitmap
import androidx.compose.ui.Modifier
import androidx.compose.ui.platform.LocalDensity
import androidx.compose.ui.platform.testTag
import androidx.compose.ui.test.*
import androidx.compose.ui.test.junit4.createAndroidComposeRule
import androidx.compose.ui.unit.Density
import androidx.compose.ui.unit.dp
import androidx.test.espresso.Espresso.closeSoftKeyboard
import org.junit.Assert.*
import org.junit.Before
import org.junit.Rule
import org.junit.Test

/** Exercises the shared app-settings/stream flow in a compact landscape window. */
class OnScreenControlsSettingsInstrumentedTest {
    @get:Rule val compose = createAndroidComposeRule<ComponentActivity>()

    @Before fun landscape() {
        compose.activity.requestedOrientation = ActivityInfo.SCREEN_ORIENTATION_LANDSCAPE
        compose.waitUntil(10_000) {
            compose.activity.resources.configuration.orientation == android.content.res.Configuration.ORIENTATION_LANDSCAPE
        }
    }

    private fun show(
        initial: AndroidTouchSettings = AndroidTouchSettings(),
        fontScale: Float = 1f,
        spaceTheme: Boolean = false,
        onChange: (AndroidTouchSettings) -> Unit = {},
        onModeChange: ((Boolean) -> Unit)? = null,
        visible: Boolean? = null,
        onVisibilityChange: ((Boolean) -> Unit)? = null,
        onEdit: (() -> Unit)? = null,
    ) {
        var stream by mutableStateOf(StreamSettings())
        var touch by mutableStateOf(initial)
        var effectiveVisible by mutableStateOf(visible)
        compose.setContent {
            val density = LocalDensity.current
            CompositionLocalProvider(LocalDensity provides Density(density.density, fontScale)) {
                OpenNowTheme(AppSettings(), physicalControllerConnected = false) {
                    SpaceUiTheme(enabled = spaceTheme) {
                        androidx.compose.material3.Surface(Modifier.fillMaxSize()) {
                            OnScreenControlsSettings(
                                touch = touch, stream = stream,
                                onTouchMouseSensitivityChange = { stream = stream.copy(touchMouseSensitivity = it) },
                                presets = emptyList(), gyroscopeAvailable = false,
                                onChange = { touch = it; onChange(it) }, onPresetsChange = {}, onBack = {},
                                controlsVisible = effectiveVisible ?: touch.enabled,
                                onVisibilityChange = { enabled ->
                                    if (visible != null) effectiveVisible = enabled
                                    if (onVisibilityChange != null) onVisibilityChange(enabled)
                                    else touch = touch.copy(enabled = enabled)
                                },
                                onModeChange = { enabled ->
                                    touch = touch.copy(keyboardModeEnabled = enabled)
                                    onModeChange?.invoke(enabled)
                                },
                                onEditLayout = onEdit,
                                modifier = Modifier.fillMaxSize(),
                            )
                        }
                    }
                }
            }
        }
    }

    private fun capture(name: String) {
        val bitmap = compose.onRoot().captureToImage().asAndroidBitmap()
        val folder = androidx.test.platform.app.InstrumentationRegistry.getInstrumentation().targetContext.getExternalFilesDir(null)!!
        java.io.FileOutputStream(java.io.File(folder, name)).use {
            bitmap.compress(android.graphics.Bitmap.CompressFormat.PNG, 100, it)
        }
    }

    @Test fun bothControlTypesAreDiscoverableWithoutOpeningAdvancedSettings() {
        show()
        compose.onNodeWithText("Gamepad buttons").assertIsDisplayed()
        compose.onNodeWithText("Keyboard & mouse buttons").assertIsDisplayed()
        capture("on-screen-controls.png")
        compose.onNodeWithText("Keyboard & mouse buttons").performClick()
        compose.onNodeWithText("Add button").assertIsDisplayed()
        compose.onNode(hasScrollToIndexAction()).performScrollToNode(hasText("WASD movement stick"))
        compose.onNodeWithText("WASD movement stick").assertIsDisplayed()
    }

    @Test fun browsingAndCustomizingKeyboardControlsDoesNotSelectThem() {
        var saved = AndroidTouchSettings()
        val selections = mutableListOf<Boolean>()
        show(onChange = { saved = it }, onModeChange = selections::add)
        compose.onNodeWithText("Keyboard & mouse buttons").performClick()
        compose.runOnIdle { assertTrue(selections.isEmpty()) }
        compose.onNode(hasScrollToIndexAction()).performScrollToNode(hasText("WASD movement stick"))
        compose.onNodeWithText("WASD movement stick").performClick()
        compose.runOnIdle {
            assertFalse(saved.keyboardModeEnabled)
            assertTrue(selections.isEmpty())
        }
        compose.onNodeWithContentDescription("Back").performClick()
        compose.onNodeWithText("Gamepad buttons").performScrollTo().performClick()
        compose.onNodeWithText("Use this layout").performScrollTo().assertIsEnabled()
        compose.runOnIdle { assertTrue(selections.isEmpty()) }
    }

    @Test fun browsingGamepadControlsPreservesAnExplicitKeyboardChoice() {
        val selections = mutableListOf<Boolean>()
        show(initial = AndroidTouchSettings(keyboardModeEnabled = true), onModeChange = selections::add)
        compose.onNodeWithText("Gamepad buttons").performClick()
        compose.runOnIdle { assertTrue(selections.isEmpty()) }
        compose.onNodeWithContentDescription("Back").performClick()
        compose.onNodeWithText("Keyboard & mouse buttons").performClick()
        compose.onNodeWithText("Use this layout").assertIsEnabled()
        compose.runOnIdle { assertTrue(selections.isEmpty()) }
        // Reapplying a saved choice must still reach the host if native touch is currently active.
        compose.onNodeWithText("Use this layout").performClick()
        compose.runOnIdle { assertEquals(listOf(true), selections) }
    }

    @Test fun keyboardMouseSensitivitySliderIsReachableAndAdjustable() {
        show()
        compose.onNodeWithText("Keyboard & mouse buttons").performClick()
        compose.onNodeWithText("Touch cursor sensitivity").performScrollTo().assertIsDisplayed()
        compose.onNode(SemanticsMatcher.keyIsDefined(androidx.compose.ui.semantics.SemanticsProperties.ProgressBarRangeInfo)).performScrollTo()
            .performSemanticsAction(androidx.compose.ui.semantics.SemanticsActions.SetProgress) { it(2.5f) }
        compose.onNodeWithText("2.50×").assertIsDisplayed()
    }

    @Test fun keyboardButtonCanBeAddedResizedAndRemovedFromSettingsWithLargeText() {
        var saved = AndroidTouchSettings()
        show(fontScale = 1.5f, onChange = { saved = it })
        compose.onNodeWithText("Keyboard & mouse buttons").performScrollTo().performClick()
        compose.onNodeWithText("Use this layout").performClick()
        compose.onNode(hasScrollToIndexAction()).performScrollToNode(hasText("Add button"))
        compose.onNodeWithText("Add button").performClick()
        compose.onNode(hasSetTextAction()).performTextInput("space")
        closeSoftKeyboard()
        compose.onNodeWithText("Space", substring = false).performClick()
        compose.onNode(hasScrollToIndexAction()).performScrollToNode(hasText("Space", substring = false))
        compose.onNodeWithText("Space", substring = false).performClick()
        compose.onNodeWithContentDescription("Button size").performScrollTo()
            .performSemanticsAction(androidx.compose.ui.semantics.SemanticsActions.SetProgress) { it(1.5f) }
        compose.onNodeWithText("Save", substring = false).assertIsDisplayed().performClick()
        val added = saved.keyboardButtons.single { it.actionId == "space" }
        assertTrue(saved.keyboardModeEnabled)
        assertEquals(1.5f, saved.buttonAppearances.getValue(added.appearanceKey()).sizeScale, 0f)
        compose.onNodeWithText("Space", substring = false).performClick()
        compose.onNodeWithText("Remove button").performScrollTo().performClick()
        compose.runOnIdle {
            assertFalse(saved.keyboardButtons.any { it.id == added.id })
            assertFalse(saved.buttonAppearances.containsKey(added.appearanceKey()))
            assertEquals(AndroidTouchSettings().offsets, saved.offsets)
        }
    }

    @Test fun liveVisibilityAndModeUseHostCallbacksAndLayoutEditingRemainsReachable() {
        var visibility: Boolean? = null
        var mode: Boolean? = null
        var edited = false
        // Persisted enabled can differ from effective visibility after physical-controller suppression.
        show(visible = false, onVisibilityChange = { visibility = it }, onModeChange = { mode = it }, onEdit = { edited = true })
        compose.onNodeWithText("Show on-screen controls").performClick()
        compose.runOnIdle { assertEquals(true, visibility) }
        compose.onNodeWithText("Keyboard & mouse buttons").performClick()
        compose.runOnIdle { assertNull(mode) }
        compose.onNodeWithText("Use this layout").performClick()
        compose.runOnIdle { assertEquals(true, mode) }
        compose.onNodeWithText("Move buttons on screen").performScrollTo().performClick()
        compose.runOnIdle { assertTrue(edited) }
        compose.onNodeWithContentDescription("Back").performClick()
        compose.onNodeWithText("Gamepad buttons").performScrollTo().performClick()
        compose.runOnIdle { assertEquals(true, mode) }
        compose.onNodeWithText("Use this layout").performClick()
        compose.runOnIdle { assertEquals(false, mode) }
        compose.onNodeWithText("Visible controls").performScrollTo().assertIsDisplayed()
        compose.onNodeWithText("Programmable buttons").performScrollTo().assertIsDisplayed()
    }
    @Test fun compactStreamMenuKeepsControlsAudioAndStatusBarVisible() {
        var settings by mutableStateOf(AppSettings())
        var screenshotBusy by mutableStateOf(false)
        compose.setContent {
            OpenNowTheme(AppSettings(), physicalControllerConnected = false) {
                androidx.compose.foundation.layout.Box(Modifier.fillMaxSize(), contentAlignment = androidx.compose.ui.Alignment.Center) {
                    StreamControlsPanel(
                        gameTitle = "Test game",
                        status = null,
                        settings = settings,
                        tvProfile = false,
                        touchControlsVisible = settings.androidTouch.enabled,
                        builtInGameTouchSupported = false,
                        nativeTouchActive = false,
                        keyboardOverlayActive = settings.androidTouch.keyboardModeEnabled,
                        gyroscopeAvailable = false,
                        controllerMouseAssistEnabled = false,
                        controllerMouseEmulationEnabled = false,
                        showSessionTimer = true,
                        sessionTimerLimit = SmartSessionLimit("Free", 1, SessionTimerMode.Countdown),
                        sessionStartedAtMs = 0L,
                        sessionNowMs = 0L,
                        audioMuted = false,
                        microphoneRequested = false,
                        microphonePermissionGranted = false,
                        microphoneEnabled = false,
                        statsVisible = false,
                        statusBarDragging = false,
                        receivedBitrateKbps = null,
                        recordingPhase = null,
                        screenshotEnabled = !screenshotBusy,
                        screenshotBusy = screenshotBusy,
                        onScreenshot = { screenshotBusy = true },
                        touchLayoutEditing = false,
                        bugReportSubmission = BugReportSubmissionState(),
                        bugReportVersionCheck = AndroidBugReportVersionCheckState(),
                        update = AndroidUpdateState(),
                        bugReportPreflightProvider = { error("Not used by this test") },
                        onAudioToggle = { },
                        onMicrophoneToggle = { },
                        onStatsToggle = { },
                        onStatusBarDraggingToggle = { },
                        onStatsStyleCycle = { },
                        onStatsPositionCycle = { },
                        onStatsMetricsChange = { _ -> },
                        onStatsBackgroundEnabledChange = { _ -> },
                        onStatsBackgroundOpacityChange = { _ -> },
                        onKeyboardButtonToggle = { },
                        onVibrationToggle = { },
                        onTouchLayoutEditingToggle = { },
                        onKeyboardOpen = { },
                        onRecordingToggle = { },
                        onEsc = { },
                        onEnter = { },
                        onBackspace = { },
                        onSteamMenuOpen = { },
                        onControllerMouseAssistToggle = { },
                        onControllerMouseEmulationToggle = { },
                        onExit = { },
                        onKeyboardModeToggle = { enabled -> settings = settings.copy(androidTouch = settings.androidTouch.copy(keyboardModeEnabled = enabled)) },
                        onTouchControlsToggle = { },
                        onMousePadToggle = { },
                        onMouseDirectClickToggle = { },
                        onSharpeningToggle = { },
                        onSharpeningAmountChange = { _ -> },
                        onMaxBitrateChange = { _ -> },
                        onStretchToFitToggle = { },
                        onMouseSensitivityChange = { _ -> },
                        onTouchMouseSensitivityChange = { value -> settings = settings.copy(stream = settings.stream.copy(touchMouseSensitivity = value)) },
                        onMouseScrollSensitivityChange = { _ -> },
                        onPhysicalStickDeadZoneChange = { _ -> },
                        onTouchSettingsChange = { settings = settings.copy(androidTouch = it) },
                        onTouchPresetsChange = { _ -> },
                        onBugReportSubmit = { _, _, _, _, _ -> },
                        onBugReportReset = { },
                        onBugReportVersionCheck = { },
                        onOpenUpdate = { },
                        onButtonTone = { },
                        onClose = {},
                    )
                }
            }
        }
        compose.onNodeWithText("Screenshot").assertIsDisplayed().performClick()
        compose.onNodeWithText("Saving screenshot…").assertIsNotEnabled()
        compose.runOnIdle { screenshotBusy = false }
        capture("stream-controls.png")
        compose.onNodeWithText("On-screen controls").assertIsDisplayed()
        compose.onNodeWithText("Audio").assertIsDisplayed()
        compose.onNodeWithText("Status bar").assertIsDisplayed()
        fun scrollToRow(label: String): SemanticsNodeInteraction {
            compose.onNode(hasScrollToIndexAction()).performScrollToNode(hasText(label))
            return compose.onNodeWithText(label)
        }
        scrollToRow("Full gamepad mouse mode").assertIsDisplayed().performClick()
        compose.onNodeWithText("Enable Mouse Mode").assertIsDisplayed()
        compose.onNodeWithText("Back").performClick()
        scrollToRow("More controls").performClick()
        scrollToRow("Stretch to fit").assertIsDisplayed()
        scrollToRow("Fewer controls").performClick()
        scrollToRow("On-screen controls").performClick()
        compose.onNodeWithText("Keyboard & mouse buttons").assertIsDisplayed().performClick()
        compose.onNodeWithText("Add button").assertIsDisplayed()
    }

    @Test fun streamRowTextKeepsReadableContrastUnderALightHostTheme() {
        compose.setContent {
            MaterialTheme {
                CompositionLocalProvider(
                    com.opencloudgaming.opennow.ui.controls.LocalControlRowStyle provides
                        com.opencloudgaming.opennow.ui.controls.ControlRowStyle.stream(),
                ) {
                    androidx.compose.material3.Surface(color = com.opencloudgaming.opennow.ui.theme.OpenNowPalette.PanelOverVideo) {
                        com.opencloudgaming.opennow.ui.controls.ControlNavigationRow("On-screen controls", {})
                    }
                }
            }
        }
        val pixels = compose.onNodeWithText("On-screen controls").captureToImage().toPixelMap()
        var lightest = 0f
        var darkest = 1f
        for (y in 0 until pixels.height) for (x in 0 until pixels.width) {
            val luminance = pixels[x, y].luminance()
            lightest = maxOf(lightest, luminance)
            darkest = minOf(darkest, luminance)
        }
        val contrast = (lightest + 0.05f) / (darkest + 0.05f)
        assertTrue("Stream label contrast was $contrast:1", contrast >= 4.5f)
    }

    private fun assertReadableText(text: String) {
        val pixels = compose.onNodeWithText(text, substring = false).captureToImage().toPixelMap()
        var lightest = 0f
        var darkest = 1f
        for (y in 0 until pixels.height) for (x in 0 until pixels.width) {
            val luminance = pixels[x, y].luminance()
            lightest = maxOf(lightest, luminance)
            darkest = minOf(darkest, luminance)
        }
        val contrast = (lightest + 0.05f) / (darkest + 0.05f)
        assertTrue("Text '$text' contrast was $contrast:1", contrast >= 4.5f)
    }

    @Test fun headerAndHelpRemainReadableWhenTheHostContentColorIsBlack() {
        var touch by mutableStateOf(AndroidTouchSettings())
        compose.setContent {
            OpenNowTheme(AppSettings(dynamicColor = false), physicalControllerConnected = false) {
                // MaterialTheme alone does not provide LocalContentColor. Reproduce the app's
                // background-only host, which used to leave header/help text black on dark.
                CompositionLocalProvider(LocalContentColor provides Color.Black) {
                    Box(Modifier.fillMaxSize().background(MaterialTheme.colorScheme.background)) {
                        OnScreenControlsSettings(
                            touch = touch, stream = StreamSettings(),
                            onTouchMouseSensitivityChange = {}, presets = emptyList(),
                            gyroscopeAvailable = false, onChange = { touch = it },
                            onPresetsChange = {}, onBack = {},
                        )
                    }
                }
            }
        }
        assertReadableText("On-screen controls")
        assertReadableText("Gamepad, keyboard & mouse buttons, layouts and appearance")
        compose.onNodeWithText("Keyboard & mouse buttons").performScrollTo().performClick()
        assertReadableText("Keyboard & mouse buttons")
        assertReadableText("Add any keyboard key or mouse button. Tap a button below to change its size, style or remove it.")
        capture("on-screen-controls-keyboard.png")
    }

    @Test fun spaceThemeKeepsModeChoicesAndCustomizationReachable() {
        show(spaceTheme = true)
        compose.onNodeWithText("Keyboard & mouse buttons").performScrollTo().assertIsDisplayed()
        assertReadableText("Gamepad buttons")
        capture("on-screen-controls-space.png")
        // Search through the lazy list so prefetched, unplaced rows are scrolled into view.
        compose.onNode(hasScrollToIndexAction()).performScrollToNode(hasText("Appearance"))
        compose.onNodeWithText("Appearance").assertIsDisplayed().performClick()
        compose.onNodeWithText("Customize buttons").performScrollTo().assertIsDisplayed()
        compose.onNodeWithContentDescription("Back").performClick()
        compose.onNode(hasScrollToIndexAction()).performScrollToNode(hasText("Presets and themes"))
        compose.onNodeWithText("Presets and themes").assertIsDisplayed().performClick()
        compose.onNodeWithText("Save current controls").performScrollTo().assertIsDisplayed()
    }

    @Test fun settingsPagesAnimateInBothDirectionsAndHonorReducedMotion() {
        var destination by mutableStateOf(0)
        var reduceMotion by mutableStateOf(false)
        compose.mainClock.autoAdvance = false
        compose.setContent {
            MaterialTheme {
                Box(Modifier.fillMaxSize()) {
                    SettingsRouteContent(destination, reduceMotion, depth = { it }) {
                        Text("Page $it", Modifier.testTag("destination"))
                    }
                }
            }
        }
        compose.mainClock.advanceTimeByFrame()
        fun position() = compose.onNodeWithTag("destination").fetchSemanticsNode().positionInRoot.x
        val rest = position()
        compose.runOnIdle { destination = 1 }
        compose.mainClock.advanceTimeBy(32)
        assertTrue("Forward navigation should slide in from the right", position() > rest)
        compose.mainClock.advanceTimeBy(500)
        assertEquals(rest, position(), 0.5f)
        compose.runOnIdle { destination = 0 }
        compose.mainClock.advanceTimeBy(32)
        assertTrue("Back navigation should slide in from the left", position() < rest)
        compose.mainClock.advanceTimeBy(500)
        compose.runOnIdle { reduceMotion = true; destination = 2 }
        compose.mainClock.advanceTimeByFrame()
        assertEquals("Reduced motion should place the page immediately", rest, position(), 0.5f)
        compose.onNodeWithText("Page 2").assertIsDisplayed()
        compose.mainClock.autoAdvance = true
    }

    @Test fun portraitSettingsKeepLargeTextAndCustomizationReachable() {
        compose.activity.requestedOrientation = ActivityInfo.SCREEN_ORIENTATION_PORTRAIT
        compose.waitUntil(10_000) {
            compose.activity.resources.configuration.orientation == android.content.res.Configuration.ORIENTATION_PORTRAIT
        }
        show(fontScale = 1.5f)
        compose.onNodeWithText("On-screen controls").assertIsDisplayed()
        compose.onNodeWithText("Keyboard & mouse buttons").performScrollTo().assertIsDisplayed()
        capture("on-screen-controls-portrait.png")
        compose.onNode(hasScrollToIndexAction()).performScrollToNode(hasText("Presets and themes"))
        compose.onNodeWithText("Presets and themes").assertIsDisplayed().performClick()
        compose.onNodeWithText("Save current controls").performScrollTo().assertIsDisplayed()
    }

}

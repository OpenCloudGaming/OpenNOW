package com.opencloudgaming.opennow

import android.graphics.Bitmap
import androidx.compose.foundation.background
import androidx.compose.foundation.layout.WindowInsets
import androidx.compose.foundation.layout.Box
import androidx.compose.foundation.layout.fillMaxSize
import androidx.compose.material.icons.Icons
import androidx.compose.material.icons.outlined.Science
import androidx.compose.material3.MaterialTheme
import androidx.compose.material3.Text
import androidx.compose.runtime.*
import androidx.compose.ui.Modifier
import androidx.compose.ui.graphics.asAndroidBitmap
import androidx.compose.ui.graphics.Color
import androidx.compose.ui.graphics.toPixelMap
import androidx.compose.ui.unit.dp
import com.opencloudgaming.opennow.ui.theme.LocalReduceMotion
import com.opencloudgaming.opennow.ui.theme.LocalReduceForegroundMotion
import androidx.compose.ui.input.key.Key
import androidx.compose.ui.input.key.onKeyEvent
import androidx.compose.ui.input.key.key
import androidx.compose.ui.platform.testTag
import androidx.compose.ui.platform.LocalInputModeManager
import androidx.compose.ui.input.InputMode
import androidx.compose.ui.input.InputModeManager
import androidx.compose.ui.test.*
import androidx.compose.ui.test.junit4.createAndroidComposeRule
import androidx.activity.ComponentActivity
import androidx.activity.enableEdgeToEdge
import androidx.test.platform.app.InstrumentationRegistry
import org.junit.Assert.assertEquals
import org.junit.Assert.assertFalse
import org.junit.Assert.assertTrue
import org.junit.Rule
import org.junit.Test
import java.io.File

class SpaceUiInstrumentedTest {
    @get:Rule val compose = createAndroidComposeRule<ComponentActivity>()
    private lateinit var inputModeManager: InputModeManager
    private val games = listOf(
        GameInfo(id = "1", title = "Beyond the horizon", genres = listOf("Adventure", "Exploration"),
            description = "A new world is waiting. Find your next adventure and make it your own."),
        GameInfo(id = "2", title = "Nightfall"),
        GameInfo(id = "3", title = "The last frontier"),
        GameInfo(id = "4", title = "Velocity"),
        GameInfo(id = "5", title = "Echoes"),
        GameInfo(id = "6", title = "Worlds apart"),
        GameInfo(id = "7", title = "Aurora"),
        GameInfo(id = "8", title = "Solstice"),
    )

    @Test fun controllerFocusLaunchCallbackAndBumpersWork() {
        var state by mutableStateOf(OpenNowUiState(games = games, libraryGames = games))
        var selected: String? = null
        compose.setContent {
            val manager = LocalInputModeManager.current
            SideEffect { inputModeManager = manager }
            MaterialTheme {
                SpaceUiScreen(state, onNavigate = { state = state.copy(page = it) },
                    onSelectGame = { selected = it.id }, onRefresh = {}, onSearchChange = {}, onResumeSession = {},
                    inputEnabled = true, settingsContent = { Text("Space settings") })
            }
        }
        focusFirstGame()
        compose.onNodeWithTag("space-game-1").performKeyInput { pressKey(Key.DirectionRight) }
        awaitFocused("space-game-2")
        // MainActivity normalizes the controller's confirm button to D-pad center.
        compose.onNodeWithTag("space-game-2").performKeyInput { pressKey(Key.DirectionCenter) }
        compose.runOnIdle { assertEquals("2", selected) }
        compose.onNodeWithTag("space-ui").performKeyInput { pressKey(Key.ButtonR1) }
        compose.runOnIdle { assertEquals(AppPage.Library, state.page) }
        focusFirstGame()
        compose.onNodeWithTag("space-ui").performKeyInput { pressKey(Key.ButtonR1) }
        compose.onNodeWithText("Space settings").assertIsDisplayed()
        compose.onNodeWithTag("space-ui").performKeyInput { pressKey(Key.ButtonR1) }
        compose.runOnIdle { assertEquals(AppPage.Home, state.page) }
        if (isPortrait()) focusFirstGame("space-game-2") else awaitFocused("space-game-2")
    }

    @OptIn(ExperimentalTestApi::class)
    @Test fun mouseHoverPreviewsGameAndClickLaunchesOnlyOnce() {
        var launches = 0
        compose.setContent {
            SpaceUiTheme {
                SpaceUiScreen(OpenNowUiState(games = games), onNavigate = {},
                    onSelectGame = { launches++ }, onRefresh = {}, onSearchChange = {},
                    onResumeSession = {}, inputEnabled = true, settingsContent = {})
            }
        }
        val second = compose.onNodeWithTag("space-game-2")
        if (isPortrait()) compose.onNodeWithTag("space-game-rail").performScrollToNode(hasTestTag("space-game-2"))
        second.performMouseInput { enter(center) }
        second.assertIsSelected()
        compose.runOnIdle { assertEquals(0, launches) }
        capture("space-mouse-hover")
        second.performMouseInput { click() }
        compose.runOnIdle { assertEquals(1, launches) }
        second.performMouseInput { exit() }
    }

    @Test fun touchSelectionKeepsTheChosenGameHighlighted() {
        var chosen: String? = null
        compose.setContent {
            SpaceUiTheme {
                SpaceUiScreen(OpenNowUiState(games = games), onNavigate = {},
                    onSelectGame = { chosen = it.id }, onRefresh = {}, onSearchChange = {},
                    onResumeSession = {}, inputEnabled = true, settingsContent = {})
            }
        }
        val second = compose.onNodeWithTag("space-game-2")
        if (isPortrait()) compose.onNodeWithTag("space-game-rail").performScrollToNode(hasTestTag("space-game-2"))
        second.performClick().assertIsSelected()
        compose.runOnIdle { assertEquals("2", chosen) }
    }

    @Test fun emptyCatalogStillHasWorkingNavigationAndRefresh() {
        var refreshes = 0
        compose.setContent {
            val manager = LocalInputModeManager.current
            SideEffect { inputModeManager = manager }
            MaterialTheme {
                SpaceUiScreen(OpenNowUiState(), onNavigate = {}, onSelectGame = {}, onRefresh = { refreshes++ },
                    onSearchChange = {}, onResumeSession = {}, inputEnabled = true, settingsContent = {})
            }
        }
        compose.onNodeWithText("No games loaded").assertIsDisplayed()
        compose.onAllNodesWithContentDescription("Refresh games").onLast().performClick()
        compose.runOnIdle { assertEquals(1, refreshes) }
        compose.onNodeWithTag("space-game-rail").assertDoesNotExist()
    }

    @Test fun experimentalToggleCanBeEnabledAndDisabledWithoutAControllerOrExplanation() {
        var optIn by mutableStateOf(false)
        compose.setContent {
            val manager = LocalInputModeManager.current
            SideEffect { inputModeManager = manager }
            MaterialTheme {
                androidx.compose.foundation.layout.Column {
                    SpaceUiSettingsToggle(optIn) { optIn = it }
                    if (shouldUseSpaceUi(optIn, AppPage.Home)) Text("Space active")
                }
            }
        }
        compose.onNode(isToggleable()).assertIsEnabled()
        compose.onNodeWithContentDescription("Show description").assertDoesNotExist()
        compose.onNodeWithText("Space UI").performClick()
        compose.onNodeWithText("Space active").assertIsDisplayed()
        compose.runOnIdle { assertTrue(optIn) }
        compose.onNodeWithText("Space UI").performClick()
        compose.onNodeWithText("Space active").assertDoesNotExist()
        compose.runOnIdle { assertFalse(optIn) }
    }

    @Test fun gameHubPreservesMaintenanceConfirmationAndLaunchActions() {
        var launches = 0
        var dismisses = 0
        val game = games.first().copy(playabilityState = "MAINTENANCE")
        compose.setContent {
            val manager = LocalInputModeManager.current
            SideEffect { inputModeManager = manager }
            MaterialTheme {
                SpaceGameHub(game, favorite = false, defaultVariantId = null,
                    onPlay = { launches++ }, onChooseStore = {}, onFavorite = {}, connectedTvName = null,
                    onPlayOnTv = {}, removeLabel = null, removeConfirmationText = null, removeEnabled = true,
                    onRemove = null, onDismiss = { dismisses++ })
            }
        }
        awaitFocused("space-hub-play")
        compose.onNodeWithTag("space-hub-play").performClick()
        compose.onNode(isDialog()).assertExists()
        compose.runOnIdle { assertEquals(0, launches) }
        compose.onNodeWithText("Cancel").performClick()
        compose.runOnIdle { assertEquals(0, launches) }
        compose.onNodeWithTag("space-hub-play").performClick()
        compose.onNodeWithText("Continue anyway").performClick()
        compose.runOnIdle { assertEquals(1, launches); assertEquals(1, dismisses) }
    }

    @Test fun collectionGridKeepsControllerNavigationAndSelection() {
        compose.setContent {
            val manager = LocalInputModeManager.current
            SideEffect { inputModeManager = manager }
            MaterialTheme {
                CompositionLocalProvider(LocalReduceMotion provides true, LocalReduceForegroundMotion provides true) {
                    SpaceUiScreen(OpenNowUiState(page = AppPage.Library, libraryGames = games),
                        onNavigate = {}, onSelectGame = {}, onRefresh = {}, onSearchChange = {}, onResumeSession = {},
                        inputEnabled = true, settingsContent = {})
                }
            }
        }
        compose.onNodeWithTag("space-library-grid").assertIsDisplayed()
        focusFirstGame()
        val first = compose.onNodeWithTag("space-game-art-1", useUnmergedTree = true).getUnclippedBoundsInRoot()
        val second = compose.onNodeWithTag("space-game-art-2", useUnmergedTree = true).getUnclippedBoundsInRoot()
        val fourth = compose.onNodeWithTag("space-game-art-4", useUnmergedTree = true).getUnclippedBoundsInRoot()
        val fifth = compose.onNodeWithTag("space-game-art-5", useUnmergedTree = true).getUnclippedBoundsInRoot()
        assertEquals("Library keeps the square Discover artwork", (first.right - first.left).value, (first.bottom - first.top).value, 1f)
        if (isPortrait()) assertTrue("Phone library cards need room for readable artwork", fourth.top > first.top)
        else assertEquals(first.top.value, fourth.top.value, 1f)
        assertTrue("The fifth game starts the next row", fifth.top > first.top)
        assertEquals("Only the focus gutters separate adjacent cards", 24f, (second.left - first.right).value, 1f)
        val target = compose.onNodeWithTag("space-game-1").getUnclippedBoundsInRoot()
        assertEquals("Artwork fills its grid cell inside the focus gutters", (target.right - target.left).value - 24f,
            (first.right - first.left).value, 1f)
        compose.onNodeWithTag("space-game-1").performKeyInput { pressKey(Key.DirectionRight) }
        awaitFocused("space-game-2")
        capture("space-collection")
    }

    @Test fun premiumMarkerSettingAppliesToDiscoverRailCatalogAndLibrary() {
        val samples = games.map { if (it.id == "1") it.copy(membershipTierLabel = "PREMIUM") else it }
        var state by mutableStateOf(OpenNowUiState(games = samples, libraryGames = samples,
            settings = AppSettings(showPremiumMarker = false)))
        val marker = InstrumentationRegistry.getInstrumentation().targetContext.getString(R.string.catalog_premium_marker)
        compose.setContent {
            CompositionLocalProvider(LocalReduceMotion provides true, LocalReduceForegroundMotion provides true) {
                SpaceUiTheme {
                    SpaceUiScreen(state, onNavigate = {}, onSelectGame = {}, onRefresh = {}, onSearchChange = {},
                        onResumeSession = {}, inputEnabled = true, settingsContent = {})
                }
            }
        }
        fun assertMarkerToggle() {
            compose.onNodeWithText(marker, useUnmergedTree = true).assertDoesNotExist()
            compose.runOnIdle { state = state.copy(settings = state.settings.copy(showPremiumMarker = true)) }
            compose.onNodeWithText(marker, useUnmergedTree = true).assertIsDisplayed()
            compose.onNodeWithContentDescription("${samples.first().title}, $marker").assertExists()
            compose.onNodeWithContentDescription(samples[1].title).assertExists()
            capture("space-premium-${state.page}")
            compose.runOnIdle { state = state.copy(settings = state.settings.copy(showPremiumMarker = false)) }
            compose.onNodeWithText(marker, useUnmergedTree = true).assertDoesNotExist()
            compose.onNodeWithContentDescription(samples.first().title).assertExists()
        }
        assertMarkerToggle()
        compose.onNodeWithTag("space-discover-scroll").performTouchInput { swipeUp() }
        compose.onNodeWithTag("space-discover-catalog").assertIsDisplayed()
        assertMarkerToggle()
        compose.runOnIdle { state = state.copy(page = AppPage.Library) }
        compose.onNodeWithTag("space-library-grid").assertIsDisplayed()
        assertMarkerToggle()
    }

    @Test fun renderGameHubAndLaunch() {
        val context = InstrumentationRegistry.getInstrumentation().targetContext
        val art = "android.resource://${context.packageName}/${R.drawable.catalog_colorful_abstract_background}"
        val game = games.first().copy(imageUrl = art, tvBannerUrl = art, screenshotUrl = art, publisherName = "Horizon Studio",
            variants = listOf(GameVariant("steam", "STEAM"), GameVariant("epic", "EPIC")))
        var showLaunch by mutableStateOf(false)
        compose.setContent {
            val manager = LocalInputModeManager.current
            SideEffect { inputModeManager = manager }
            if (showLaunch) SpaceLaunchScreen(OpenNowUiState(streamGame = game, streamStatus = "queue"),
                status = "Queue position: 12", onMinimize = {}, onCancel = {}, queuePosition = 12)
            else SpaceGameHub(game, false, null, onPlay = { showLaunch = true }, onChooseStore = {}, onFavorite = {},
                connectedTvName = null, onPlayOnTv = {}, removeLabel = null, removeConfirmationText = null,
                removeEnabled = true, onRemove = null, onDismiss = {})
        }
        awaitFocused("space-hub-play")
        val play = compose.onNodeWithTag("space-hub-play").getUnclippedBoundsInRoot()
        val favorite = compose.onNodeWithTag("space-hub-favorite").getUnclippedBoundsInRoot()
        assertEquals((play.top.value + play.bottom.value) / 2, (favorite.top.value + favorite.bottom.value) / 2, 2f)
        compose.onNodeWithText(context.getString(R.string.space_ui_choose_store)).assertDoesNotExist()
        compose.onNodeWithText(context.getString(R.string.controller_action_favorite)).assertDoesNotExist()
        val screen = compose.onNodeWithTag("space-game-hub").getUnclippedBoundsInRoot()
        val viewport = compose.onNodeWithTag("space-hub-scroll").getUnclippedBoundsInRoot()
        val author = compose.onNodeWithTag("space-hub-author").getUnclippedBoundsInRoot()
        assertTrue("Play sits directly below the developer", play.top >= author.bottom && play.top - author.bottom < 24.dp)
        assertEquals("Play aligns with the developer name", author.left.value, play.left.value, 2f)
        compose.onNodeWithTag("space-hub-about").assertIsDisplayed()
        val details = compose.onNodeWithTag("space-hub-details").getUnclippedBoundsInRoot()
        assertTrue("Additional details start below the initial viewport", details.top >= viewport.bottom)
        compose.onNodeWithTag("space-hub-play").assertIsDisplayed()
        capture("space-hub")
        compose.onNodeWithTag("space-hub-details").performScrollTo()
        val movedPlay = compose.onNodeWithTag("space-hub-play").getUnclippedBoundsInRoot()
        assertTrue("Actions scroll upward with the hero", movedPlay.top < play.top)
        compose.onNodeWithTag("space-ownership-badge").performScrollTo()
        val badge = compose.onNodeWithTag("space-ownership-badge").getUnclippedBoundsInRoot()
        assertTrue("Not owned is a compact badge", (badge.right - badge.left) < (screen.right - screen.left) * .75f)
        compose.onNodeWithContentDescription("Gallery").performClick()
        compose.onNodeWithTag("space-gallery-image").assertIsDisplayed()
        capture("space-gallery")
        compose.runOnIdle { inputModeManager.requestInputMode(InputMode.Keyboard) }
        compose.onNodeWithTag("space-gallery-image").performSemanticsAction(androidx.compose.ui.semantics.SemanticsActions.RequestFocus) { it() }
        compose.onNodeWithTag("space-gallery-image").performKeyInput { pressKey(Key.DirectionDown) }
        compose.onNodeWithTag("space-hub-description").assertIsDisplayed()
        compose.onNodeWithContentDescription("Overview").performClick()
        compose.onNodeWithTag("space-hub-play").performClick()
        compose.onNodeWithTag("space-launch").assertIsDisplayed()
        compose.onNodeWithTag("space-queue-status").assertTextEquals("Queue position: 12")
        compose.onNodeWithText("12", substring = false).assertDoesNotExist()
        compose.onNodeWithText("OPENNOW / EXPERIMENTAL").assertDoesNotExist()
        val minimizeText = compose.onNodeWithText(context.getString(R.string.action_minimize), useUnmergedTree = true).getUnclippedBoundsInRoot()
        val cancelText = compose.onNodeWithText(context.getString(R.string.action_cancel), useUnmergedTree = true).getUnclippedBoundsInRoot()
        assertEquals((minimizeText.top.value + minimizeText.bottom.value) / 2,
            (cancelText.top.value + cancelText.bottom.value) / 2, 2f)
        capture("space-launch")
    }

    @Test fun gameHubBumpersSwitchTabsOnceAndKeepFocusInsideTheHub() {
        val context = InstrumentationRegistry.getInstrumentation().targetContext
        val art = "android.resource://${context.packageName}/${R.drawable.catalog_colorful_abstract_background}"
        val game = games.first().copy(screenshotUrl = art)
        var leakedBumpers = 0
        compose.setContent {
            Box(Modifier.fillMaxSize().onKeyEvent {
                if (it.key == Key.ButtonL1 || it.key == Key.ButtonR1) leakedBumpers++
                false
            }) {
                SpaceGameHub(game, false, null, onPlay = {}, onChooseStore = {}, onFavorite = {},
                    connectedTvName = null, onPlayOnTv = {}, removeLabel = null, removeConfirmationText = null,
                    removeEnabled = false, onRemove = null, onDismiss = {})
            }
        }
        awaitFocused("space-hub-play")
        compose.onNodeWithTag("space-game-hub").performKeyInput { keyDown(Key.ButtonR1) }
        compose.onNodeWithContentDescription("Overview").assertIsSelected()
        compose.onNodeWithTag("space-game-hub").performKeyInput { keyUp(Key.ButtonR1) }
        compose.onNode(hasContentDescription("Gallery") and hasClickAction()).assertIsSelected()
        awaitFocused("space-gallery-image")
        compose.onNodeWithTag("space-game-hub").performKeyInput { pressKey(Key.ButtonL1) }
        compose.onNodeWithContentDescription("Overview").assertIsSelected()
        awaitFocused("space-hub-play")
        // Both bumpers wrap between the two tabs, including when focus is inside content.
        compose.onNodeWithTag("space-hub-play").performKeyInput { pressKey(Key.ButtonL1) }
        awaitFocused("space-gallery-image")
        compose.onNodeWithTag("space-gallery-image").performKeyInput { pressKey(Key.ButtonR1) }
        awaitFocused("space-hub-play")
        compose.runOnIdle { assertEquals(0, leakedBumpers) }
    }

    @Test fun gameHubShowsFullDescriptionAndAvailableCatalogFacts() {
        val context = InstrumentationRegistry.getInstrumentation().targetContext
        val art = "android.resource://${context.packageName}/${R.drawable.catalog_colorful_abstract_background}"
        val fullDescription = "Explore a living world, build your team, and discover the secrets beyond the horizon. ".repeat(100).trim()
        val game = games.first().copy(imageUrl = art, tvBannerUrl = art, description = fullDescription, longDescription = "Different provider-only description",
            publisherName = "Horizon Studio", playabilityState = "AVAILABLE", membershipTierLabel = "Ultimate",
            contentRatings = listOf("ESRB T"), featureLabels = listOf("SINGLE_PLAYER", "MULTIPLAYER", "RTX"),
            variants = listOf(GameVariant("steam", "STEAM", supportedControls = listOf("GAMEPAD", "KEYBOARD_MOUSE"))))
        compose.setContent {
            val inputMode = LocalInputModeManager.current
            SideEffect { inputMode.requestInputMode(InputMode.Keyboard) }
            SpaceGameHub(game, false, null, onPlay = {}, onChooseStore = {}, onFavorite = {},
                connectedTvName = null, onPlayOnTv = {}, removeLabel = null, removeConfirmationText = null,
                removeEnabled = false, onRemove = null, onDismiss = {})
        }
        awaitFocused("space-hub-play")
        compose.onNodeWithTag("space-hub-author").assertTextEquals("Horizon Studio").assertIsDisplayed()
        // No screenshots means bumpers are contained and cannot open an empty gallery.
        compose.onNodeWithTag("space-hub-play").performKeyInput { pressKey(Key.ButtonR1); pressKey(Key.ButtonL1) }
        compose.onNodeWithContentDescription("Gallery").assertDoesNotExist()
        compose.onNodeWithTag("space-hub-play").performKeyInput { pressKey(Key.DirectionDown) }
        awaitFocused("space-hub-description")
        compose.onNodeWithTag("space-hub-description").assertTextEquals(fullDescription)
        fun descriptionLines(): Int {
            val results = mutableListOf<androidx.compose.ui.text.TextLayoutResult>()
            compose.onNodeWithTag("space-hub-description").performSemanticsAction(androidx.compose.ui.semantics.SemanticsActions.GetTextLayoutResult) { it(results) }
            return results.first().lineCount
        }
        assertEquals("About is collapsed to five lines", 5, descriptionLines())
        compose.onNodeWithTag("space-hub-description-toggle").performScrollTo().performClick()
        assertTrue("Show more reveals the full description", descriptionLines() > 5)
        awaitFocused("space-hub-description")
        val beforeScroll = compose.onNodeWithTag("space-hub-scroll").fetchSemanticsNode()
            .config[androidx.compose.ui.semantics.SemanticsProperties.VerticalScrollAxisRange].value()
        capture("space-description-expanded")
        compose.onNodeWithTag("space-game-hub").performKeyInput { pressKey(Key.DirectionDown) }
        val afterScroll = compose.onNodeWithTag("space-hub-scroll").fetchSemanticsNode()
            .config[androidx.compose.ui.semantics.SemanticsProperties.VerticalScrollAxisRange].value()
        assertTrue("Joystick Down scrolls within expanded About: before=$beforeScroll after=$afterScroll bounds=${compose.onNodeWithTag("space-hub-description").getUnclippedBoundsInRoot()}", afterScroll > beforeScroll)
        compose.onNodeWithTag("space-hub-description").assertIsFocused()
        for (step in 0..50) {
            if (compose.onAllNodes(hasTestTag("space-hub-detail-App ID") and isFocused()).fetchSemanticsNodes().isNotEmpty()) break
            compose.onNodeWithTag("space-game-hub").performKeyInput { pressKey(Key.DirectionDown) }
        }
        compose.onNodeWithTag("space-hub-detail-App ID").assertIsFocused().assertIsDisplayed()
        for (field in listOf("Genres", "Status", "Publisher", "Controls", "Features", "Membership", "Rating", "Stores", "App ID")) {
            compose.onNodeWithTag("space-hub-detail-$field").performScrollTo().assertIsDisplayed()
        }
        compose.onNodeWithTag("space-hub-detail-Publisher").assertExists()
        compose.onAllNodesWithText("Controller, Mouse and keyboard").assertCountEquals(2)
        compose.onNodeWithText("ESRB T").assertExists()
        compose.onNodeWithTag("space-hub-detail-App ID").assertExists()
        compose.onNodeWithTag("space-hub-detail-Genres").performScrollTo()
        val genres = compose.onNodeWithTag("space-hub-detail-Genres").getUnclippedBoundsInRoot()
        val status = compose.onNodeWithTag("space-hub-detail-Status").getUnclippedBoundsInRoot()
        assertEquals("Facts share a two-column row", genres.top.value, status.top.value, 1f)
        assertTrue("Second fact is in the right column", status.left > genres.right)
        capture("space-hub-metadata")
        compose.onNodeWithTag("space-hub-description-toggle").performScrollTo().performClick()
        assertEquals("Show less restores five lines", 5, descriptionLines())
        compose.onNodeWithTag("space-hub-description").performScrollTo()
            .performSemanticsAction(androidx.compose.ui.semantics.SemanticsActions.RequestFocus) { it() }
        for (step in 0..5) {
            if (compose.onAllNodes(hasTestTag("space-hub-play") and isFocused()).fetchSemanticsNodes().isNotEmpty()) break
            compose.onNodeWithTag("space-game-hub").performKeyInput { pressKey(Key.DirectionUp) }
        }
        awaitFocused("space-hub-play")
        val returnedScroll = compose.onNodeWithTag("space-hub-scroll").fetchSemanticsNode()
            .config[androidx.compose.ui.semantics.SemanticsProperties.VerticalScrollAxisRange].value()
        assertEquals("Returning to Play reveals the entire opening section", 0f, returnedScroll, .1f)
        compose.onNodeWithTag("space-hub-author").assertIsDisplayed()
    }

    @Test fun discoverTouchScrollOpensAFullCatalogWithVerticalCovers() {
        compose.setContent {
            SpaceUiScreen(OpenNowUiState(games = games), onNavigate = {}, onSelectGame = {}, onRefresh = {},
                onSearchChange = {}, onResumeSession = {}, inputEnabled = true, settingsContent = {})
        }
        compose.onNodeWithTag("space-discover-scroll").performTouchInput { swipeUp() }
        compose.onNodeWithTag("space-discover-catalog").assertIsDisplayed()
        compose.onNodeWithTag("space-game-rail").assertDoesNotExist()
        val cover = compose.onNodeWithTag("space-game-art-1", useUnmergedTree = true).getUnclippedBoundsInRoot()
        assertTrue("Catalog covers must be vertical", cover.bottom - cover.top > cover.right - cover.left)
        compose.onNodeWithTag("space-discover-catalog").performScrollToNode(hasTestTag("space-game-8"))
        compose.onNodeWithTag("space-game-8").assertIsDisplayed()
        capture("space-discover-catalog")
        compose.onNodeWithTag("space-catalog-back").performClick()
        compose.onNodeWithTag("space-game-rail").assertIsDisplayed()
        compose.onNodeWithTag("space-discover-catalog").assertDoesNotExist()
    }

    @Test fun discoverDownOpensTheCatalogAndFocusFramesHaveTheirOwnGutter() {
        compose.setContent {
            val manager = LocalInputModeManager.current
            SideEffect { inputModeManager = manager }
            SpaceUiScreen(OpenNowUiState(games = games), onNavigate = {}, onSelectGame = {}, onRefresh = {},
                onSearchChange = {}, onResumeSession = {}, inputEnabled = true, settingsContent = {})
        }
        focusFirstGame()
        val rail = compose.onNodeWithTag("space-game-rail").getUnclippedBoundsInRoot()
        val art = compose.onNodeWithTag("space-game-art-1", useUnmergedTree = true).getUnclippedBoundsInRoot()
        val target = compose.onNodeWithTag("space-game-1").getUnclippedBoundsInRoot()
        assertTrue("Focus decoration must fit inside its scroll target", art.left - target.left >= 8.dp)
        assertTrue("The first frame must be protected from the rail edge", art.left - rail.left >= 8.dp)
        capture("space-focus-gutter")
        compose.onNodeWithTag("space-game-1").performKeyInput { pressKey(Key.DirectionDown) }
        compose.onNodeWithTag("space-discover-catalog").assertIsDisplayed()
        awaitFocused("space-game-1")
        compose.onNodeWithTag("space-game-1").performKeyInput { pressKey(Key.DirectionDown) }
        compose.onNodeWithTag("space-game-1").assertIsNotFocused()
        compose.runOnUiThread { compose.activity.onBackPressedDispatcher.onBackPressed() }
        compose.onNodeWithTag("space-game-rail").assertIsDisplayed()
        compose.onNodeWithTag("space-discover-catalog").assertDoesNotExist()
    }

    @Test fun cameraSafeViewportClipsZoomAndArrivalAtEveryFrame() {
        compose.mainClock.autoAdvance = false
        compose.setContent {
            Box(Modifier.fillMaxSize().spaceSafeViewport(WindowInsets(left = 48.dp, top = 20.dp, right = 24.dp, bottom = 16.dp))) {
                Box(Modifier.fillMaxSize().spaceArtworkArrival("art").spaceArrival("art").background(Color.Cyan))
            }
        }
        for (frame in listOf(16L, 160L, 350L, 800L)) {
            compose.mainClock.advanceTimeBy(frame)
            val pixels = compose.onRoot().captureToImage().toPixelMap()
            val width = pixels.width
            val height = pixels.height
            val density = compose.activity.resources.displayMetrics.density
            for ((x, y) in listOf(1 to height / 2, (47 * density).toInt() to height / 2,
                width - 2 to height / 2, width / 2 to 1, width / 2 to height - 2)) {
                assertEquals("Artwork leaked into the camera margin at frame $frame", SpaceInk, pixels[x, y])
            }
        }
    }

    @Test fun platformCutoutKeepsQueueAndDiscoverInsideTheSafeScene() {
        compose.runOnUiThread {
            compose.activity.enableEdgeToEdge()
            applyOpenNowSystemBars(compose.activity.window, true)
        }
        val context = InstrumentationRegistry.getInstrumentation().targetContext
        val art = "android.resource://${context.packageName}/${R.drawable.catalog_colorful_abstract_background}"
        val samples = games.map { it.copy(imageUrl = art, tvBannerUrl = art) }
        var queue by mutableStateOf(true)
        compose.setContent {
            if (queue) SpaceLaunchScreen(OpenNowUiState(streamGame = samples.first(), streamStatus = "queue"),
                status = "Queue position: 7", queuePosition = 7, onMinimize = {}, onCancel = {})
            else SpaceUiScreen(OpenNowUiState(games = samples), onNavigate = {}, onSelectGame = {}, onRefresh = {},
                onSearchChange = {}, onResumeSession = {}, inputEnabled = true, settingsContent = {})
        }
        fun assertSafe(tag: String) {
            compose.waitForIdle()
            val cutout = compose.activity.window.decorView.rootWindowInsets.displayCutout
            val density = compose.activity.resources.displayMetrics.density
            val root = compose.onRoot().getUnclippedBoundsInRoot()
            val content = compose.onNodeWithTag(tag).getUnclippedBoundsInRoot()
            if (cutout != null) {
                assertTrue("Content overlaps camera on the left", content.left.value >= cutout.safeInsetLeft / density)
                assertTrue("Content overlaps camera at the top", content.top.value >= cutout.safeInsetTop / density)
                assertTrue("Content overlaps camera on the right", content.right.value <= root.right.value - cutout.safeInsetRight / density)
                assertTrue("Content overlaps camera at the bottom", content.bottom.value <= root.bottom.value - cutout.safeInsetBottom / density)
            }
        }
        assertSafe("space-launch-content")
        compose.onNodeWithContentDescription("Cancel").assertIsDisplayed()
        capture("space-camera-queue")
        compose.runOnIdle { queue = false }
        assertSafe("space-scene-content")
        capture("space-camera-discover")
    }

    @Test fun queueMotionUsesOneRealPositionAndStopsForReducedMotion() {
        // Keep the clock manual from creation: the test idle policy cancels infinite animations in auto mode.
        compose.mainClock.autoAdvance = false
        var position by mutableIntStateOf(12)
        var reduced by mutableStateOf(false)
        compose.setContent {
            CompositionLocalProvider(LocalReduceMotion provides reduced, LocalReduceForegroundMotion provides reduced) {
                SpaceLaunchScreen(OpenNowUiState(streamGame = games.first(), streamStatus = "queue"),
                    status = "Queue position: $position", queuePosition = position, onMinimize = {}, onCancel = {})
            }
        }
        compose.waitUntil(5000) { compose.activity.hasWindowFocus() }
        compose.mainClock.advanceTimeBy(1000)
        compose.onNodeWithTag("space-queue-status").assertTextEquals("Queue position: 12")
        compose.runOnIdle { position = 1 }
        compose.mainClock.advanceTimeBy(800)
        compose.onNodeWithTag("space-queue-status").assertTextEquals("Queue position: 1")
        compose.onAllNodesWithText("Queue position: 1").assertCountEquals(1)
        compose.onNodeWithText("1", substring = false).assertDoesNotExist()
        capture("space-queue-hot")
        compose.mainClock.autoAdvance = false
        compose.mainClock.advanceTimeBy(1000)
        val movingBefore = compose.onRoot().captureToImage().asAndroidBitmap()
        var changed = false
        repeat(6) {
            compose.mainClock.advanceTimeBy(47)
            val movingAfter = compose.onRoot().captureToImage().asAndroidBitmap()
            changed = changed || !movingBefore.sameAs(movingAfter)
        }
        assertTrue("Hot queue digits should animate while focused (window=${compose.activity.hasWindowFocus()})", changed)
        compose.mainClock.autoAdvance = true
        compose.runOnIdle { reduced = true }
        compose.waitForIdle()
        compose.mainClock.autoAdvance = false
        compose.mainClock.advanceTimeBy(1000)
        val before = compose.onRoot().captureToImage().asAndroidBitmap()
        compose.mainClock.advanceTimeBy(1000)
        val after = compose.onRoot().captureToImage().asAndroidBitmap()
        assertTrue("Reduced motion must leave a static queue scene", before.sameAs(after))
        compose.runOnIdle { position = 3 }
        compose.mainClock.advanceTimeBy(32)
        compose.onNodeWithTag("space-queue-status").assertTextEquals("Queue position: 3")
    }

    @Test fun lowPowerQueueSpinnerMovesAndExplicitReducedMotionStopsIt() {
        compose.mainClock.autoAdvance = false
        var explicitReduceMotion by mutableStateOf(false)
        compose.setContent {
            // The Chromecast's automatic low-power policy keeps decorative animation off.
            CompositionLocalProvider(LocalReduceMotion provides true,
                LocalReduceForegroundMotion provides explicitReduceMotion) {
                SpaceLaunchScreen(OpenNowUiState(streamGame = games.first(), streamStatus = "queue"),
                    status = "Queue position: 12", queuePosition = 12, onMinimize = {}, onCancel = {})
            }
        }
        compose.waitUntil(5_000) { compose.activity.hasWindowFocus() }
        compose.mainClock.advanceTimeBy(1_000)
        val indicator = compose.onNodeWithTag("space-launch-progress", useUnmergedTree = true)
        val before = indicator.captureToImage().asAndroidBitmap()
        compose.mainClock.advanceTimeBy(250)
        assertFalse("Low-power mode must keep the queue spinner moving",
            before.sameAs(indicator.captureToImage().asAndroidBitmap()))
        compose.runOnIdle { explicitReduceMotion = true }
        compose.mainClock.advanceTimeBy(1_000)
        val reduced = indicator.captureToImage().asAndroidBitmap()
        compose.mainClock.advanceTimeBy(1_000)
        assertTrue("The user's reduced-motion setting must still stop the spinner",
            reduced.sameAs(indicator.captureToImage().asAndroidBitmap()))
    }

    @Test fun minimizedQueueUsesTheTopRailAndKeepsRestoreAndCancelActions() {
        var restores = 0
        var cancels = 0
        val state = OpenNowUiState(games = games, streamGame = games.first(), streamStatus = "queue",
            streamLaunchMinimized = true, queuePosition = 12)
        compose.setContent {
            SpaceUiTheme {
                SpaceUiScreen(state, onNavigate = {}, onSelectGame = {}, onRefresh = {}, onSearchChange = {},
                    onResumeSession = {}, inputEnabled = true, settingsContent = {},
                    queueContent = { compact ->
                        SpaceMinimizedQueueControl(state, compact, onRestore = { restores++ }, onCancel = { cancels++ })
                    })
            }
        }
        val rail = compose.onNodeWithTag("space-top-rail").getUnclippedBoundsInRoot()
        val queue = compose.onNodeWithTag("space-queue-rail").getUnclippedBoundsInRoot()
        assertTrue("The minimized queue must fit inside the top rail", queue.top >= rail.top && queue.bottom <= rail.bottom)
        assertTrue("The queue controls must fit horizontally", queue.left >= rail.left && queue.right <= rail.right)
        compose.onNodeWithTag("space-queue-restore").performClick()
        compose.onNodeWithTag("space-queue-rail-cancel").performClick()
        compose.runOnIdle { assertEquals(1, restores); assertEquals(1, cancels) }
        capture("space-queue-rail")
    }

    @Test fun rigSetupCircleDisplaysSuppliedProgressAndHoldsWithoutSpinning() {
        compose.mainClock.autoAdvance = false
        var progress by mutableFloatStateOf(0f)
        compose.setContent {
            CompositionLocalProvider(LocalReduceMotion provides true, LocalReduceForegroundMotion provides true) {
                SpaceLaunchScreen(OpenNowUiState(streamGame = games.first(), streamStatus = "connecting"),
                    status = "Setting up rig", connectingProgress = { progress }, onMinimize = {}, onCancel = {})
            }
        }
        compose.mainClock.advanceTimeBy(1000)
        val empty = compose.onRoot().captureToImage().asAndroidBitmap()
        compose.runOnIdle { progress = .45f }
        compose.mainClock.advanceTimeBy(32)
        val halfway = compose.onRoot().captureToImage().asAndroidBitmap()
        assertFalse("The setup circle must display changing progress even with reduced motion", empty.sameAs(halfway))
        compose.runOnIdle { progress = .9f }
        compose.mainClock.advanceTimeBy(32)
        val capped = compose.onRoot().captureToImage().asAndroidBitmap()
        assertFalse("The circle must continue filling to 90%", halfway.sameAs(capped))
        compose.mainClock.advanceTimeBy(2000)
        assertTrue("Determinate setup progress must hold instead of spinning", capped.sameAs(compose.onRoot().captureToImage().asAndroidBitmap()))
    }

    @Test fun consoleSettingsKeepSectionsAndExperimentalControlsAccessible() {
        var selected by mutableStateOf("general")
        var enabled by mutableStateOf(false)
        val sections = listOf(
            SpaceSettingsSection("general", "General", "Make yourself at home.", Icons.Outlined.Science),
            SpaceSettingsSection("advanced", "Advanced", "Experimental features", Icons.Outlined.Science),
        )
        compose.setContent {
            val manager = LocalInputModeManager.current
            SideEffect { inputModeManager = manager }
            SpaceUiTheme {
                Box(Modifier.fillMaxSize()) {
                    SpaceSceneBackdrop(null)
                    SpaceSettingsFrame(sections, selected, onSelect = { selected = it }) {
                        if (selected == "advanced") {
                            SpaceUiSettingsToggle(enabled) { enabled = it }
                            ChoiceMenuRow("Resolution", listOf(ChoiceMenuOption("1080", "1080p"), ChoiceMenuOption("720", "720p")),
                                selectedLabel = "1080p", onSelect = {})
                            NumberSlider(label = "Bitrate", value = 50f, min = 5f, max = 100f, step = 1f, unit = "Mbps") {}
                        } else Text("General controls")
                    }
                }
            }
        }
        compose.onNodeWithContentDescription("Advanced").performClick()
        compose.onNodeWithText("Space UI").assertIsDisplayed().performClick()
        compose.runOnIdle { assertTrue(enabled) }
        capture("space-settings")
    }

    @Test fun iconOpensTheExistingAccountDropdown() {
        var page by mutableStateOf(AppPage.Home)
        compose.setContent {
            val manager = LocalInputModeManager.current
            SideEffect { inputModeManager = manager }
            SpaceUiTheme {
                SpaceUiScreen(OpenNowUiState(page = page, games = games), onNavigate = { page = it },
                    onSelectGame = {}, onRefresh = {}, onSearchChange = {}, onResumeSession = {},
                    inputEnabled = true, settingsContent = { Text("Settings content") })
            }
        }
        val context = InstrumentationRegistry.getInstrumentation().targetContext
        compose.onNodeWithContentDescription(context.getString(R.string.profile_menu_description)).performClick()
        compose.onNodeWithText(context.getString(R.string.action_close_opennow)).assertIsDisplayed()
        compose.onAllNodesWithText(context.getString(R.string.nav_settings)).onLast().performClick()
        compose.runOnIdle { assertEquals(AppPage.Settings, page) }
        compose.onNodeWithText("Settings content").assertIsDisplayed()
    }

    @Test fun immersiveBarsHideInLandscapeAndRestoreForPortraitOrDisabledMode() {
        compose.setContent { Text("Window visibility check") }
        fun apply(enabled: Boolean, landscape: Boolean) {
            compose.runOnIdle {
                applyOpenNowSystemBars(compose.activity.window, shouldHideSpaceUiSystemBars(enabled, landscape, AppPage.Home))
            }
        }
        fun awaitVisible(expected: Boolean) {
            compose.waitUntil(5_000) {
                val insets = compose.activity.window.decorView.rootWindowInsets ?: return@waitUntil false
                val status = insets.isVisible(android.view.WindowInsets.Type.statusBars())
                val navigation = insets.isVisible(android.view.WindowInsets.Type.navigationBars())
                if (expected) status && navigation else !status && !navigation
            }
        }
        apply(true, true); awaitVisible(false)
        apply(true, false); awaitVisible(true)
        apply(true, true); awaitVisible(false)
        apply(false, true); awaitVisible(true)
    }

    @Test fun settingsTabDownEntersTheSettingsControls() {
        compose.setContent {
            val manager = LocalInputModeManager.current
            SideEffect { inputModeManager = manager }
            SpaceUiTheme {
                SpaceUiScreen(OpenNowUiState(page = AppPage.Settings), onNavigate = {}, onSelectGame = {},
                    onRefresh = {}, onSearchChange = {}, onResumeSession = {}, inputEnabled = true,
                    settingsContent = {
                        SpaceSettingsFrame(listOf(SpaceSettingsSection("general", "General", "", Icons.Outlined.Science)),
                            "general", onSelect = {}) {
                            SettingsActionButton("First setting", {}, Modifier.testTag("first-setting"))
                        }
                    })
            }
        }
        compose.runOnIdle { inputModeManager.requestInputMode(InputMode.Keyboard) }
        compose.onNodeWithContentDescription("Settings").assertIsFocused().performKeyInput { pressKey(Key.DirectionDown) }
        compose.onNode(isFocused() and (hasTestTag("first-setting") or hasAnyAncestor(hasTestTag("first-setting")))).assertExists()
        capture("space-settings-focus")
    }

    @Test fun launchPickersUseLightTextAndKeepTheirExistingCallbacks() {
        val game = games.first().copy(variants = listOf(GameVariant("steam", "STEAM"), GameVariant("epic", "EPIC")))
        var storePicker by mutableStateOf(true)
        var launched: String? = null
        // The selector and launch callback must also work when the device clock is behind.
        val queueServerTimeMs = System.currentTimeMillis() + 3_600_000L
        val queue = PrintedWasteQueueSnapshot(
            zones = mapOf("NP-LAX-03" to PrintedWasteZone(12, LastUpdated = queueServerTimeMs / 1000, Region = "US Southwest")),
            receivedEpochMs = queueServerTimeMs,
            receivedElapsedMs = android.os.SystemClock.elapsedRealtime(),
        )
        compose.setContent {
            SpaceUiTheme {
                if (storePicker) StoreLaunchSelector(game, null,
                    onLaunch = { _, variant -> launched = variant.id }, onSetDefaultStore = { _, _ -> },
                    onDismiss = { storePicker = false })
                else PrintedWasteSelectorContent(OpenNowUiState(
                    printedWasteQueue = queue,
                    printedWasteMapping = mapOf("NP-LAX-03" to PrintedWasteServerMappingEntry("Southern California", "US Southwest", is5080Server = true)),
                    regions = listOf(StreamRegion("Southern California", "https://us-california-south.cloudmatchbeta.nvidiagrid.net/"))), game,
                    onDismiss = {}, onRetry = {}, onLaunch = { launched = it }, onSettingsChange = {})
            }
        }
        fun assertLightTitle() {
            val results = mutableListOf<androidx.compose.ui.text.TextLayoutResult>()
            compose.onNodeWithText(game.title).performSemanticsAction(androidx.compose.ui.semantics.SemanticsActions.GetTextLayoutResult) { it(results) }
            assertTrue(results.first().layoutInput.style.color != androidx.compose.ui.graphics.Color.Black)
            assertTrue(results.first().layoutInput.style.color.red > .8f)
        }
        assertLightTitle()
        capture("space-launcher-picker")
        compose.onNodeWithContentDescription("Continue").performClick()
        compose.runOnIdle { assertEquals("steam", launched) }
        compose.onNodeWithContentDescription("Cancel").performClick()
        assertLightTitle()
        compose.onNodeWithText("12").assertIsDisplayed()
        capture("space-queue-picker")
        compose.onNodeWithContentDescription("Launch").performClick()
        compose.runOnIdle { assertEquals("https://us-california-south.cloudmatchbeta.nvidiagrid.net/", launched) }
    }

    @Test fun settingsActionsUseFullWidthConsoleRows() {
        compose.setContent {
            SpaceUiTheme {
                androidx.compose.foundation.layout.Column(Modifier.fillMaxSize()) {
                    SettingsActionButton("Clear cache", {}, Modifier.testTag("cache-action"))
                    SettingsActionButton("Restore backup", {}, Modifier.testTag("backup-action"))
                    SettingsButton({}) { Text("Apply") }
                }
            }
        }
        val layouts = mutableListOf<androidx.compose.ui.text.TextLayoutResult>()
        compose.onNodeWithText("Apply").performSemanticsAction(androidx.compose.ui.semantics.SemanticsActions.GetTextLayoutResult) { it(layouts) }
        assertTrue(layouts.first().layoutInput.style.color.red < .2f)
        compose.onNodeWithText("Clear cache").assertIsDisplayed()
        val cache = compose.onNodeWithTag("cache-action").getUnclippedBoundsInRoot()
        val backup = compose.onNodeWithTag("backup-action").getUnclippedBoundsInRoot()
        assertEquals(cache.left.value, backup.left.value, .1f)
        assertEquals(cache.right.value, backup.right.value, .1f)
        assertTrue((cache.right - cache.left).value > 300f)
    }

    private fun enterTouchMode() {
        val instrumentation = InstrumentationRegistry.getInstrumentation()
        instrumentation.waitForIdleSync()
        val metrics = compose.activity.resources.displayMetrics
        val now = android.os.SystemClock.uptimeMillis()
        for (action in listOf(android.view.MotionEvent.ACTION_DOWN, android.view.MotionEvent.ACTION_UP)) {
            val event = android.view.MotionEvent.obtain(now, now + action * 20L, action, metrics.widthPixels / 2f, metrics.heightPixels / 2f, 0)
            event.source = android.view.InputDevice.SOURCE_TOUCHSCREEN
            instrumentation.uiAutomation.injectInputEvent(event, true)
            event.recycle()
        }
        instrumentation.waitForIdleSync()
    }

    private fun isPortrait(): Boolean = compose.activity.resources.configuration.orientation == android.content.res.Configuration.ORIENTATION_PORTRAIT

    private fun focusFirstGame(tag: String = "space-game-1") {
        if (isPortrait()) {
            compose.runOnIdle { inputModeManager.requestInputMode(InputMode.Keyboard) }
        }
        awaitFocused(tag)
    }

    private fun capture(prefix: String) {
        compose.waitForIdle()
        val bitmap = compose.onRoot().captureToImage().asAndroidBitmap()
        val context = InstrumentationRegistry.getInstrumentation().targetContext
        File(context.getExternalFilesDir(null), "$prefix-${bitmap.width}x${bitmap.height}.png").outputStream().use {
            bitmap.compress(Bitmap.CompressFormat.PNG, 100, it)
        }
    }

    private fun awaitFocused(tag: String) {
        compose.waitUntil(5_000) {
            compose.onAllNodes(hasTestTag(tag) and isFocused()).fetchSemanticsNodes().isNotEmpty()
        }
        compose.onNodeWithTag(tag).assertIsFocused()
    }

    @Test fun renderSpaceUi() {
        if (isPortrait()) enterTouchMode()
        val context = InstrumentationRegistry.getInstrumentation().targetContext
        val art = "android.resource://${context.packageName}/${R.drawable.catalog_colorful_abstract_background}"
        val samples = games.map { it.copy(imageUrl = art, tvBannerUrl = art, publisherName = "Horizon Studio", genres = listOf("ADVENTURE", "FREE_TO_PLAY")) }
        compose.setContent {
            val manager = LocalInputModeManager.current
            SideEffect { inputModeManager = manager }
            var ready by remember { mutableStateOf(false) }
            LaunchedEffect(Unit) {
                if (compose.activity.resources.configuration.orientation == android.content.res.Configuration.ORIENTATION_PORTRAIT) {
                    manager.requestInputMode(InputMode.Touch)
                }
                ready = true
            }
            if (ready) MaterialTheme {
                SpaceUiScreen(OpenNowUiState(games = samples), onNavigate = {}, onSelectGame = {}, onRefresh = {},
                    onSearchChange = {}, onResumeSession = {}, inputEnabled = true, settingsContent = {})
            }
        }
        if (!isPortrait()) awaitFocused("space-game-1")
        compose.onNodeWithText("View game").assertDoesNotExist()
        compose.onNodeWithTag("space-library-grid").assertDoesNotExist()
        compose.onNodeWithContentDescription("Search").assertIsDisplayed()
        compose.onNodeWithText("SPACE").assertDoesNotExist()
        compose.onNodeWithText("OPENNOW / EXPERIMENTAL").assertDoesNotExist()
        compose.onNodeWithText("Navigate", substring = false).assertDoesNotExist()
        compose.onNodeWithText("Select", substring = false).assertDoesNotExist()
        compose.onNodeWithText("Switch tabs", substring = false).assertDoesNotExist()
        compose.onNodeWithText("Horizon Studio").assertIsDisplayed()
        compose.onNodeWithText("Adventure", substring = false).assertDoesNotExist()
        compose.onNodeWithText("Free To Play", substring = false).assertDoesNotExist()
        capture("space-ui")
        compose.onNodeWithTag("space-game-rail").performScrollToNode(hasTestTag("space-game-8"))
        compose.onNodeWithTag("space-game-8").assertIsDisplayed()
    }
}

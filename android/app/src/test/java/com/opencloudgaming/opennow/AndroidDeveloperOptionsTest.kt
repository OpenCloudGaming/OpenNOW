package com.opencloudgaming.opennow

import org.junit.Assert.assertEquals
import org.junit.Assert.assertFalse
import org.junit.Assert.assertTrue
import org.junit.Test

class AndroidDeveloperOptionsTest {

    @Test
    fun buildNumberTapsStaySilentUntilTheCountdownBegins() {
        (1 until DEVELOPER_OPTIONS_TAP_COUNTDOWN_FROM).forEach { tap ->
            assertEquals(
                "tap $tap",
                DeveloperOptionsTapResult.Silent,
                developerOptionsTapResult(tapCount = tap, alreadyUnlocked = false),
            )
        }
    }

    @Test
    fun buildNumberTapsCountDownThenUnlock() {
        assertEquals(
            DeveloperOptionsTapResult.Countdown(remaining = 3),
            developerOptionsTapResult(tapCount = 7, alreadyUnlocked = false),
        )
        assertEquals(
            DeveloperOptionsTapResult.Countdown(remaining = 1),
            developerOptionsTapResult(tapCount = 9, alreadyUnlocked = false),
        )
        assertEquals(
            DeveloperOptionsTapResult.Unlocked,
            developerOptionsTapResult(tapCount = DEVELOPER_OPTIONS_TAP_COUNT, alreadyUnlocked = false),
        )
    }

    @Test
    fun tappingAnAlreadyUnlockedBuildNumberNeverCountsDown() {
        assertEquals(
            DeveloperOptionsTapResult.AlreadyUnlocked,
            developerOptionsTapResult(tapCount = 1, alreadyUnlocked = true),
        )
        assertEquals(
            DeveloperOptionsTapResult.AlreadyUnlocked,
            developerOptionsTapResult(tapCount = 99, alreadyUnlocked = true),
        )
    }

    @Test
    fun unlockingAndLockingAreInverses() {
        val locked = AppSettings()
        assertFalse(locked.developerOptionsUnlocked)
        val unlocked = locked.unlockingDeveloperOptions()
        assertTrue(unlocked.developerOptionsUnlocked)
        assertEquals(locked, unlocked.lockingDeveloperOptions())
    }

    @Test
    fun replayingFirstLaunchRestoresEveryOneTimePromptWithoutTouchingUserContent() {
        val used = AppSettings(
            setupFlowCompletedVersion = SETUP_FLOW_VERSION,
            androidStreamGuideDismissed = true,
            androidPhysicalControllerPromptDismissed = true,
            streamPresentationProfileVersion = STREAM_PRESENTATION_PROFILE_VERSION,
            catalogFilterIds = listOf(CATALOG_FILTER_TOUCHSCREEN),
            librarySortId = LIBRARY_SORT_TITLE,
            favoriteGameIds = listOf("game-1"),
            defaultGameVariantIds = mapOf("game-1" to "variant-1"),
        )

        val replayed = used.replayingFirstLaunch()

        assertEquals(0, replayed.setupFlowCompletedVersion)
        assertFalse(replayed.androidStreamGuideDismissed)
        assertFalse(replayed.androidPhysicalControllerPromptDismissed)
        assertEquals(0, replayed.streamPresentationProfileVersion)
        assertEquals(emptyList<String>(), replayed.catalogFilterIds)
        assertEquals(AppSettings().librarySortId, replayed.librarySortId)
        // Content the user created is not a first-launch prompt and must survive.
        assertEquals(listOf("game-1"), replayed.favoriteGameIds)
        assertEquals(mapOf("game-1" to "variant-1"), replayed.defaultGameVariantIds)
    }

    @Test
    fun resettingInterfaceLeavesAccountAndStreamAlone() {
        val customized = AppSettings(
            uiAccent = UiAccent.HotPink,
            absoluteCinemaEffects = true,
            nerdCatalogBackground = true,
            systemWallpaperBackground = true,
            nerdCatalogBackgroundUri = "file:///data/wallpaper",
            posterSizeScale = MAX_GAME_CARD_SCALE,
            stream = StreamSettings(resolution = "3840x2160", fps = 120),
            favoriteGameIds = listOf("game-1"),
        )

        val reset = customized.resettingInterface()

        assertEquals(AppSettings().uiAccent, reset.uiAccent)
        assertFalse(reset.absoluteCinemaEffects)
        assertFalse(reset.nerdCatalogBackground)
        assertFalse(reset.systemWallpaperBackground)
        assertEquals(null, reset.nerdCatalogBackgroundUri)
        assertEquals(AppSettings().posterSizeScale, reset.posterSizeScale)
        assertEquals(customized.stream, reset.stream)
        assertEquals(listOf("game-1"), reset.favoriteGameIds)
    }

    @Test
    fun catalogueResetsAreIndependentOfEachOther() {
        val settings = AppSettings(
            catalogFilterIds = listOf(CATALOG_FILTER_TOUCHSCREEN),
            favoriteGameIds = listOf("game-1"),
            defaultGameVariantIds = mapOf("game-1" to "variant-1"),
            localAppPackageNames = listOf("com.example.app"),
        )

        assertEquals(listOf("game-1"), settings.resettingCatalogBrowsing().favoriteGameIds)
        assertEquals(listOf(CATALOG_FILTER_TOUCHSCREEN), settings.clearingFavorites().catalogFilterIds)
        assertEquals(emptyMap<String, String>(), settings.clearingStorePreferences().defaultGameVariantIds)
        assertEquals(listOf("game-1"), settings.clearingStorePreferences().favoriteGameIds)
        assertEquals(emptyList<String>(), settings.clearingLocalAppShelf().localAppPackageNames)
    }
}

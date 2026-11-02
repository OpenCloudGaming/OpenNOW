package com.opencloudgaming.opennow

import kotlinx.serialization.encodeToString
import kotlinx.serialization.decodeFromString
import org.junit.Assert.*
import org.junit.Test

class SpaceUiTest {
    @Test fun tallArtworkIsLimitedToTvAndTabletProfiles() {
        assertFalse(shouldUseSpacePortraitArtwork(tvProfile = false, smallestScreenWidthDp = 392))
        assertFalse(shouldUseSpacePortraitArtwork(tvProfile = false, smallestScreenWidthDp = 599))
        assertTrue(shouldUseSpacePortraitArtwork(tvProfile = false, smallestScreenWidthDp = 600))
        assertTrue(shouldUseSpacePortraitArtwork(tvProfile = false, smallestScreenWidthDp = 800))
        assertTrue(shouldUseSpacePortraitArtwork(tvProfile = true, smallestScreenWidthDp = 540))
    }

    @Test fun rapidSelectionAcceleratesButPausesAndRepeatedIdsDoNot() {
        val cadence = SpaceSelectionCadence()
        assertFalse(cadence.changed("1", 1000))
        assertTrue(cadence.changed("2", 1100))
        assertFalse(cadence.changed("2", 1150))
        assertTrue(cadence.changed("3", 1200))
        assertFalse(cadence.changed("4", 1600))
        assertTrue(cadence.changed("5", 1680))
    }

    @Test fun serializationDefaultsKeepOlderSavedSettingsOutsideTheExperiment() {
        assertFalse(AppSettings().experimentalSpaceUi)
        assertFalse(OpenNowJson.decodeFromString<AppSettings>("{}").experimentalSpaceUi)
        val restored = OpenNowJson.decodeFromString<AppSettings>(
            OpenNowJson.encodeToString(AppSettings(experimentalSpaceUi = true)),
        )
        assertTrue(restored.experimentalSpaceUi)
    }

    @Test fun optInSelectsTheShellWithoutAControllerAndStreamingKeepsItsOwnUi() {
        for (page in AppPage.entries) {
            for (optIn in listOf(false, true)) {
                assertEquals(optIn && page != AppPage.Stream, shouldUseSpaceUi(optIn, page))
            }
        }
        val saved = AppSettings(experimentalSpaceUi = true)
        assertTrue(shouldUseSpaceUi(saved.experimentalSpaceUi, AppPage.Home))
    }

    @Test fun landscapeSpaceUiHidesSystemBarsAndPortraitOrDisabledModeRestoresThem() {
        for (page in AppPage.entries) for (enabled in listOf(false, true)) for (landscape in listOf(false, true)) {
            assertEquals(enabled && landscape && page != AppPage.Stream,
                shouldHideSpaceUiSystemBars(enabled, landscape, page))
        }
    }

    @Test fun spaceUiForcesLandscapeAndDisablingRestoresTheOtherOrientationPolicies() {
        for (stream in listOf(false, true)) for (expanded in listOf(false, true)) {
            assertEquals(android.content.pm.ActivityInfo.SCREEN_ORIENTATION_SENSOR_LANDSCAPE,
                openNowRequestedOrientation(android.content.pm.ActivityInfo.SCREEN_ORIENTATION_UNSPECIFIED, stream, expanded, true))
        }
        assertEquals(android.content.pm.ActivityInfo.SCREEN_ORIENTATION_UNSPECIFIED,
            openNowRequestedOrientation(android.content.pm.ActivityInfo.SCREEN_ORIENTATION_UNSPECIFIED, false, false, false))
        assertEquals(android.content.pm.ActivityInfo.SCREEN_ORIENTATION_FULL_SENSOR,
            openNowRequestedOrientation(0, true, true, false))
    }

    @Test fun libraryKeepsItsSearchFiltersAndOrdering() {
        val games = listOf(
            GameInfo(id = "1", title = "Zenith", availableStores = listOf("STEAM"), isInLibrary = true),
            GameInfo(id = "2", title = "Astral", availableStores = listOf("EPIC"), isInLibrary = true),
            GameInfo(id = "3", title = "Astral Echo", availableStores = listOf("STEAM"), isInLibrary = true),
        )
        val state = OpenNowUiState(page = AppPage.Library, libraryGames = games, librarySortId = LIBRARY_SORT_TITLE)
        assertEquals(listOf("2", "3", "1"), spaceUiGames(state).map { it.id })
        assertEquals(listOf("2", "3"), spaceUiGames(state.copy(librarySearch = "astral")).map { it.id })
        val steamFilter = libraryStoreFilterOptions(games).first { it.rawId == "STEAM" }.id
        assertEquals(listOf("3"), spaceUiGames(state.copy(librarySearch = "astral", libraryFilterIds = listOf(steamFilter))).map { it.id })
    }

    @Test fun storeUsesTheSameCatalogFallbackAsTheRegularHome() {
        val game = GameInfo(id = "catalog", title = "Catalog game")
        val fallback = OpenNowUiState(catalogResult = CatalogBrowseResult(listOf(game)))
        assertEquals(listOf(game), spaceUiGames(fallback))
        val current = GameInfo(id = "query", title = "Query result")
        assertEquals(listOf(current), spaceUiGames(fallback.copy(games = listOf(current))))
    }
}

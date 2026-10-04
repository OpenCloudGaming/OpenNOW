package com.opencloudgaming.opennow

import org.junit.Assert.assertEquals
import org.junit.Assert.assertFalse
import org.junit.Assert.assertTrue
import org.junit.Test

class CatalogPresentationTest {
    @Test
    fun localAppPackagesAreTrimmedAndDeduplicatedInShelfOrder() {
        assertEquals(
            listOf("com.example.first", "com.example.second"),
            normalizeLocalAppPackageNames(
                listOf(" com.example.first ", "", "com.example.second", "com.example.first"),
            ),
        )
    }

    @Test
    fun librarySortOffersRecentFirstAndAlphabeticalModes() {
        val alpha = GameInfo(id = "a", title = "Alpha", lastPlayed = "2026-01-01")
        val beta = GameInfo(id = "b", title = "Beta", lastPlayed = "2026-08-20")
        val gamma = GameInfo(id = "c", title = "Gamma")

        assertEquals(listOf("Beta", "Alpha", "Gamma"), sortLibraryGames(listOf(gamma, alpha, beta), LIBRARY_SORT_RECENT).map { it.title })
        assertEquals(listOf("Alpha", "Beta", "Gamma"), sortLibraryGames(listOf(gamma, beta, alpha), LIBRARY_SORT_TITLE).map { it.title })
    }

    @Test
    fun touchFilterUsesCatalogControlMetadataInStoreAndLibrary() {
        val touchGame = GameInfo(
            id = "touch",
            title = "Touch game",
            variants = listOf(GameVariant("touch-variant", "STEAM", supportedControls = listOf("TOUCHSCREEN", "GAMEPAD"))),
        )
        val controllerGame = GameInfo(
            id = "controller",
            title = "Controller game",
            variants = listOf(GameVariant("controller-variant", "STEAM", supportedControls = listOf("GAMEPAD"))),
        )

        assertEquals(
            listOf(touchGame),
            filterCatalogGamesForLocalControls(listOf(touchGame, controllerGame), listOf(CATALOG_FILTER_TOUCHSCREEN)),
        )
        assertTrue(gameMatchesLibraryFilters(touchGame, listOf(CATALOG_FILTER_TOUCHSCREEN)))
        assertFalse(gameMatchesLibraryFilters(controllerGame, listOf(CATALOG_FILTER_TOUCHSCREEN)))
        assertEquals(listOf("Touchscreen", "Controller"), supportedControlLabels(touchGame))
    }

    @Test
    fun scopedTvQueriesMatchTheNormalMobilePageBudget() {
        assertEquals(3, catalogPageLimit(androidTvProfile = false, filterIds = emptyList()))
        assertEquals(
            MAX_CATALOG_REQUEST_PAGES,
            catalogPageLimit(androidTvProfile = false, filterIds = listOf(CATALOG_FILTER_TOUCHSCREEN)),
        )
        assertEquals(
            3,
            catalogPageLimit(androidTvProfile = true, filterIds = listOf(CATALOG_FILTER_TOUCHSCREEN)),
        )
        assertEquals(3, catalogPageLimit(androidTvProfile = true, filterIds = emptyList(), searchQuery = "halo"))
        assertEquals(1, catalogPageLimit(androidTvProfile = true, filterIds = emptyList()))
    }

    @Test
    fun libraryCombinesTouchAndLauncherCategoriesInsteadOfBroadeningThem() {
        val touchSteam = GameInfo(
            id = "touch-steam",
            title = "Touch Steam",
            isInLibrary = true,
            variants = listOf(GameVariant("one", "STEAM", supportedControls = listOf("TOUCHSCREEN"))),
        )
        val touchEpic = GameInfo(
            id = "touch-epic",
            title = "Touch Epic",
            isInLibrary = true,
            variants = listOf(GameVariant("two", "EPIC", supportedControls = listOf("TOUCHSCREEN"))),
        )
        val filters = listOf(CATALOG_FILTER_TOUCHSCREEN, "library_store:STEAM")

        assertTrue(gameMatchesLibraryFilters(touchSteam, filters))
        assertFalse(gameMatchesLibraryFilters(touchEpic, filters))
    }

    @Test
    fun searchTermsAreSplitOncePerQueryAndAllMustMatch() {
        val game = GameInfo(
            id = "a",
            title = "Alpha Strike",
            description = "A tactical shooter",
            publisherName = "Beta Studios",
        )

        // Every term has to hit somewhere in the haystack, not just the first.
        assertTrue(gameMatchesSearch(game, searchTermsFor("alpha shooter")))
        assertTrue(gameMatchesSearch(game, searchTermsFor("beta strike")))
        assertFalse(gameMatchesSearch(game, searchTermsFor("alpha racing")))

        // Blank and whitespace-only queries match everything rather than filtering to nothing.
        assertEquals(emptyList<String>(), searchTermsFor("   "))
        assertTrue(gameMatchesSearch(game, searchTermsFor("")))
        assertTrue(gameMatchesSearch(game, searchTermsFor("   ")))

        // Runs of whitespace collapse, and matching stays case-insensitive.
        assertEquals(listOf("alpha", "strike"), searchTermsFor("  ALPHA   Strike "))
        assertTrue(gameMatchesSearch(game, searchTermsFor("  ALPHA   Strike ")))

        // The String overload stays equivalent to splitting first.
        assertTrue(gameMatchesSearch(game, "alpha shooter"))
        assertFalse(gameMatchesSearch(game, "alpha racing"))
    }

    @Test
    fun librarySearchStillFindsMetadataWithoutBuildingDescriptionsIntoOneString() {
        val game = GameInfo(
            id = "metadata",
            title = "Orbit",
            longDescription = "A cooperative adventure",
            genres = listOf("Strategy"),
            featureLabels = listOf("Cross play"),
        )

        assertTrue(gameMatchesSearch(game, "cooperative strategy"))
        assertTrue(gameMatchesSearch(game, "cross orbit"))
        assertFalse(gameMatchesSearch(game, "cooperative racing"))
    }

}

package com.opencloudgaming.opennow

import androidx.compose.ui.unit.dp
import org.junit.Assert.assertEquals
import org.junit.Assert.assertFalse
import org.junit.Assert.assertTrue
import org.junit.Test

class StoreRailTest {
    @Test
    fun loadingStoreUsesTheKnownPersonalRailCount() {
        val game = GameInfo(id = "game", title = "Game")

        assertEquals(0, StoreStartRailGroups(emptyList(), emptyList(), emptyList()).visibleGroupCount)
        assertEquals(1, StoreStartRailGroups(listOf(game), emptyList(), emptyList()).visibleGroupCount)
        assertEquals(3, StoreStartRailGroups(listOf(game), listOf(game), listOf(game)).visibleGroupCount)
    }

    @Test
    fun recommendationSkeletonUsesTheExactSettingsResolvedGridColumns() {
        val threeColumnPhone = catalogGridMetrics(
            availableWidthDp = 360f,
            compact = false,
            landscapeLayout = false,
            posterSizeScale = 1f,
            handheldLayout = true,
        )
        val threeColumnCompactPhone = catalogGridMetrics(
            availableWidthDp = 360f,
            compact = true,
            landscapeLayout = false,
            posterSizeScale = 1f,
            handheldLayout = true,
        )
        val fourColumnCompactPhone = catalogGridMetrics(
            availableWidthDp = 411f,
            compact = true,
            landscapeLayout = false,
            posterSizeScale = 1f,
            handheldLayout = true,
        )
        val twoColumnLargeCards = catalogGridMetrics(
            availableWidthDp = 411f,
            compact = false,
            landscapeLayout = false,
            posterSizeScale = 1.4f,
            handheldLayout = true,
        )
        val sixColumnLandscape = catalogGridMetrics(
            availableWidthDp = 800f,
            compact = false,
            landscapeLayout = true,
            posterSizeScale = 1f,
            handheldLayout = true,
        )
        val fiveColumnTv = catalogGridMetrics(
            availableWidthDp = 960f,
            compact = false,
            landscapeLayout = true,
            posterSizeScale = 1f,
            handheldLayout = false,
        )

        assertEquals(3, threeColumnPhone.columnCount)
        assertEquals(3, threeColumnCompactPhone.columnCount)
        assertEquals(4, fourColumnCompactPhone.columnCount)
        assertEquals(2, twoColumnLargeCards.columnCount)
        assertEquals(6, sixColumnLandscape.columnCount)
        assertEquals(5, fiveColumnTv.columnCount)
        assertEquals(12, catalogSkeletonPlaceholderCount(threeColumnPhone.columnCount, storeLayout = true))
        assertEquals(9, catalogSkeletonPlaceholderCount(threeColumnPhone.columnCount, storeLayout = false))
    }

    @Test
    fun phoneLandscapeHeroDoesNotStackScreenPaddingBelowTopBar() {
        assertEquals(0.dp, storeScreenTopPadding(controlsInTopBar = true, phoneLandscapeHero = true))
        assertEquals(4.dp, storeScreenTopPadding(controlsInTopBar = true, phoneLandscapeHero = false))
        assertEquals(12.dp, storeScreenTopPadding(controlsInTopBar = false, phoneLandscapeHero = true))
    }

    @Test
    fun storeHeroUsesEveryDeviceLayoutAndHonorsHandheldLandscapeOptOut() {
        assertTrue(shouldShowStoreHero(tvProfile = false, landscape = false))
        assertTrue(shouldShowStoreHero(tvProfile = false, landscape = true))
        assertFalse(shouldShowStoreHero(tvProfile = false, landscape = true, landscapeEnabled = false))
        assertTrue(shouldShowStoreHero(tvProfile = true, landscape = false))
        assertTrue(shouldShowStoreHero(tvProfile = true, landscape = true))
        assertTrue(shouldShowStoreHero(tvProfile = true, landscape = true, landscapeEnabled = false))
    }

    @Test
    fun storeHeroPreservesTheNewGamesAddedProviderOrder() {
        val newest = GameInfo(id = "newest", title = "Newest", imageUrl = "poster")
        val next = GameInfo(id = "next", title = "Next", screenshotUrl = "wide")

        val result = newlyAddedStoreHeroGames(listOf(newest, next))

        assertEquals(listOf("newest", "next"), result.map(GameInfo::id))
    }

    @Test
    fun storeHeroDeduplicatesVariantsWithoutAddingOtherCatalogGames() {
        val steam = GameInfo(id = "same", title = "Game", availableStores = listOf("Steam"))
        val epic = steam.copy(availableStores = listOf("Epic Games Store"))

        val result = newlyAddedStoreHeroGames(listOf(steam, epic))

        assertEquals(1, result.size)
        assertEquals("same", result.single().id)
    }

    @Test
    fun storeHeroDoesNotRepeatGamesAlreadyInPersonalRails() {
        val recent = GameInfo(id = "recent", title = "Recent")
        val actuallyNew = GameInfo(id = "new", title = "Actually new")

        val result = newlyAddedStoreHeroGames(
            games = listOf(recent, actuallyNew),
            excludedGames = listOf(recent),
        )

        assertEquals(listOf("new"), result.map(GameInfo::id))
    }

    @Test
    fun storeHeroFallsBackToProviderOrderWhenEveryNewGameIsPersonal() {
        val newest = GameInfo(id = "newest", title = "Newest")
        val next = GameInfo(id = "next", title = "Next")

        val result = newlyAddedStoreHeroGames(
            games = listOf(newest, next),
            excludedGames = listOf(newest, next),
        )

        assertEquals(listOf("newest", "next"), result.map(GameInfo::id))
    }

    @Test
    fun storeHeroShowsSixWeeklyGames() {
        val games = (1..7).map { index -> GameInfo(id = "game-$index", title = "Game $index") }

        assertEquals((1..6).map { "game-$it" }, newlyAddedStoreHeroGames(games).map(GameInfo::id))
    }

    @Test
    fun catalogImageRequestsWaitDuringScrollUnlessTheImageIsAlreadyVisible() {
        assertFalse(shouldStartCatalogImageRequest(requestsPaused = true, imageAlreadyLoaded = false))
        assertTrue(shouldStartCatalogImageRequest(requestsPaused = true, imageAlreadyLoaded = true))
        assertTrue(shouldStartCatalogImageRequest(requestsPaused = false, imageAlreadyLoaded = false))
    }

    @Test
    fun catalogShimmerKeepsAnimatingWhileImageRequestsArePaused() {
        assertFalse(shouldStartCatalogImageRequest(requestsPaused = true, imageAlreadyLoaded = false))
        assertTrue(shouldAnimateCatalogLoading(loadingImageCount = 1, reduceMotion = false))
        assertFalse(shouldAnimateCatalogLoading(loadingImageCount = 0, reduceMotion = false))
        assertFalse(shouldAnimateCatalogLoading(loadingImageCount = 1, reduceMotion = true))
    }

    @Test
    fun storeHeroAnimationStopsWhileTheStoreIsMoving() {
        assertTrue(shouldAnimateStoreHero(pageCount = 6, focused = false, reduceMotion = false, storeScrolling = false))
        assertFalse(shouldAnimateStoreHero(pageCount = 6, focused = false, reduceMotion = false, storeScrolling = true))
        assertFalse(shouldAnimateStoreHero(pageCount = 6, focused = true, reduceMotion = false, storeScrolling = false))
        assertFalse(shouldAnimateStoreHero(pageCount = 6, focused = false, reduceMotion = true, storeScrolling = false))
    }

    @Test
    fun storeHeroSubtitleOmitsStoreNames() {
        val game = GameInfo(
            id = "game",
            title = "Game",
            publisherName = "  Publisher  ",
            availableStores = listOf("Steam", "Epic Games Store"),
        )
        val storeOnly = game.copy(publisherName = null)

        assertEquals("Publisher", storeHeroSubtitle(game))
        assertEquals(null, storeHeroSubtitle(storeOnly))
    }

    @Test
    fun newGamesAddedFeedIsOnlyAnUnfilteredProviderSort() {
        assertTrue(isNewlyAddedCatalogQuery("", NEWLY_ADDED_CATALOG_SORT_ID, emptyList()))
        assertTrue(isNewlyAddedCatalogQuery("", "latest", emptyList()))
        assertFalse(isNewlyAddedCatalogQuery("halo", NEWLY_ADDED_CATALOG_SORT_ID, emptyList()))
        assertFalse(isNewlyAddedCatalogQuery("", NEWLY_ADDED_CATALOG_SORT_ID, listOf("genre-action")))
    }

    @Test
    fun wideArtworkRequestsAreBoundedByTheDisplay() {
        assertEquals(960, boundedWideImageRequestWidth(networkWidth = 1920, displayWidth = 720))
        assertEquals(1280, boundedWideImageRequestWidth(networkWidth = 1920, displayWidth = 1080))
        assertEquals(1920, boundedWideImageRequestWidth(networkWidth = 1920, displayWidth = 1920))
    }

    @Test
    fun slowNetworkArtworkRequestsAreNotUpscaledToTheDisplay() {
        assertEquals(960, boundedWideImageRequestWidth(networkWidth = 960, displayWidth = 1440))
    }

    @Test
    fun phoneGridArtworkUsesDisplaySizedBuckets() {
        assertEquals(256, catalogCardImageRequestWidth(cardWidthPx = 220, tvProfile = false))
        assertEquals(384, catalogCardImageRequestWidth(cardWidthPx = 300, tvProfile = false))
        assertEquals(512, catalogCardImageRequestWidth(cardWidthPx = 420, tvProfile = false))
        assertEquals(640, catalogCardImageRequestWidth(cardWidthPx = 540, tvProfile = false))
        assertEquals(272, catalogCardImageRequestWidth(cardWidthPx = 540, tvProfile = true))
    }

    @Test
    fun storeCacheWindowGetsLeanerAsTheFrameDeadlineShrinks() {
        assertEquals(0.33f to 0.17f, catalogCacheWindowFractions(60f))
        assertEquals(0.4f to 0.17f, catalogCacheWindowFractions(90f))
        assertEquals(0.25f to 0.08f, catalogCacheWindowFractions(120f))
    }
}

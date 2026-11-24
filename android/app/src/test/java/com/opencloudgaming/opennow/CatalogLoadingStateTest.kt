package com.opencloudgaming.opennow

import org.junit.Assert.assertFalse
import org.junit.Assert.assertTrue
import org.junit.Test

/**
 * "No games loaded" is the answer to a finished request, not a stand-in for one still in flight.
 * These pin the rule that decides which of the two the reader sees.
 */
class CatalogLoadingStateTest {
    @Test
    fun aWarmCacheThatShowsNothingKeepsTheStoreLoading() {
        // The regression: a library-only cache satisfied "we have a cache" while leaving the Store
        // grid empty, so the spinner dropped and the empty state showed over a live fetch.
        assertTrue(catalogStillLoadingAfterCache(hasGamesToShow = false, keepRefreshVisible = false))
    }

    @Test
    fun cachedGamesStopTheSpinner() {
        assertFalse(catalogStillLoadingAfterCache(hasGamesToShow = true, keepRefreshVisible = false))
    }

    @Test
    fun aManualPullKeepsItsIndicatorEvenOverCachedGames() {
        assertTrue(catalogStillLoadingAfterCache(hasGamesToShow = true, keepRefreshVisible = true))
    }

    @Test
    fun theEmptyStateIsOnlyReachableOnceLoadingHasStopped() {
        // shouldShowCatalogLoadingPlaceholder is what stands between a live fetch and the
        // "No games loaded" text; with nothing on screen it must win.
        assertTrue(
            shouldShowCatalogLoadingPlaceholder(
                queryLoading = false,
                loadingGames = catalogStillLoadingAfterCache(hasGamesToShow = false, keepRefreshVisible = false),
                hasVisibleGames = false,
            ),
        )
    }

    @Test
    fun aFinishedFetchWithNoResultsStillReachesTheEmptyState() {
        // The flag must not be pinned true, or a genuinely empty catalogue would spin forever.
        assertFalse(
            shouldShowCatalogLoadingPlaceholder(
                queryLoading = false,
                loadingGames = false,
                hasVisibleGames = false,
            ),
        )
    }

}

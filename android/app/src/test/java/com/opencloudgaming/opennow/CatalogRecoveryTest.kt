package com.opencloudgaming.opennow

import org.junit.Assert.assertEquals
import org.junit.Assert.assertFalse
import org.junit.Assert.assertTrue
import org.junit.Test

class CatalogRecoveryTest {
    private fun game(id: String) = GameInfo(id = id, title = id)

    private fun retry(
        signedIn: Boolean = true,
        loadAttempted: Boolean = true,
        hasGames: Boolean = false,
        loadInFlight: Boolean = false,
        streamActive: Boolean = false,
        loadFailed: Boolean = false,
    ) = shouldRetryCatalogLoad(signedIn, loadAttempted, hasGames, loadInFlight, streamActive, loadFailed)

    @Test
    fun anEmptyStoreAfterAFailedLoadIsRetried() {
        assertTrue(retry())
    }

    @Test
    fun theFirstRunLoadIsNotRacedByTheForegroundHook() {
        // The Activity resumes before the bootstrap has asked for anything; retrying here would
        // run a second identical fetch alongside it.
        assertFalse(retry(loadAttempted = false))
    }

    @Test
    fun nothingIsRetriedWhileAFetchIsAlreadyRunning() {
        assertFalse(retry(loadInFlight = true))
    }

    @Test
    fun aPopulatedStoreIsLeftAlone() {
        assertFalse(retry(hasGames = true))
    }

    @Test
    fun fourCachedGamesDoNotSuppressRecoveryAfterANetworkTimeout() {
        val cached = OpenNowUiState(games = (1..4).map { game("cached-$it") })
        assertTrue(retry(hasGames = cached.hasLoadedCatalogGames(), loadFailed = true))
        assertFalse(retry(hasGames = cached.hasLoadedCatalogGames(), loadFailed = false))
    }

    @Test
    fun partialLibraryResultsDoNotSuppressRecoveryOfTheStore() {
        val partial = OpenNowUiState(libraryGames = listOf(game("library")))
        assertTrue(retry(hasGames = partial.hasLoadedCatalogGames(), loadFailed = true))
    }

    @Test
    fun cachedFailureRecoveryStillHonorsLifecycleGuards() {
        assertFalse(retry(hasGames = true, loadFailed = true, signedIn = false))
        assertFalse(retry(hasGames = true, loadFailed = true, loadAttempted = false))
        assertFalse(retry(hasGames = true, loadFailed = true, loadInFlight = true))
        assertFalse(retry(hasGames = true, loadFailed = true, streamActive = true))
    }

    @Test
    fun signedOutAndStreamingBothSuppressTheRetry() {
        assertFalse(retry(signedIn = false))
        assertFalse(retry(streamActive = true))
    }

    @Test
    fun backoffGrowsAndIsCapped() {
        assertEquals(CATALOG_RETRY_BASE_DELAY_MS, catalogRetryDelayMs(0))
        assertEquals(CATALOG_RETRY_BASE_DELAY_MS * 2, catalogRetryDelayMs(1))
        assertEquals(CATALOG_RETRY_BASE_DELAY_MS * 4, catalogRetryDelayMs(2))
        assertTrue(catalogRetryDelayMs(3) > catalogRetryDelayMs(2))
        assertEquals(CATALOG_RETRY_MAX_DELAY_MS, catalogRetryDelayMs(99))
    }

    @Test
    fun theWholeLadderFitsInsideAReasonableWait() {
        // Four attempts a reader would plausibly sit through rather than a minutes-long stall.
        val total = (0 until CATALOG_RETRY_MAX_ATTEMPTS).sumOf { catalogRetryDelayMs(it) }
        assertTrue("ladder took ${total}ms", total <= 60_000L)
    }

    @Test
    fun anyOneOfTheThreeListsCountsAsLoaded() {
        val empty = OpenNowUiState()
        assertFalse(empty.hasLoadedCatalogGames())
        assertTrue(empty.copy(games = listOf(game("a"))).hasLoadedCatalogGames())
        assertTrue(empty.copy(libraryGames = listOf(game("b"))).hasLoadedCatalogGames())
        assertTrue(
            empty.copy(catalogResult = CatalogBrowseResult(listOf(game("c")))).hasLoadedCatalogGames(),
        )
    }
}

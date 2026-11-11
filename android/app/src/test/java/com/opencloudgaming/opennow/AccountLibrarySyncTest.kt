package com.opencloudgaming.opennow

import kotlinx.coroutines.CancellationException
import kotlinx.coroutines.runBlocking
import org.junit.Assert.assertEquals
import org.junit.Assert.assertSame
import org.junit.Test

class AccountLibrarySyncTest {
    private val linked = AccountConnector("STEAM", "Steam", userIdentifier = "test-user")

    @Test fun linkingWithoutSyncMetadataRefreshesImmediatelyAndAfterDelayedSync() = runBlocking {
        var loads = 0
        val refreshes = mutableListOf<Int>()
        val waits = mutableListOf<Long>()
        refreshLibraryAfterAccountLink("steam", emptyList(), { true }, { waits += it },
            loadConnectors = { loads++; listOf(linked.copy(expiresInSeconds = 100L - loads)) },
            refreshLibrary = { refreshes += loads })
        assertEquals(listOf(1, 6), refreshes)
        assertEquals(listOf(5_000L) + List(5) { 10_000L }, waits)
    }

    @Test fun completingSyncRefreshesMembershipEvenWhenAlreadyLinked() = runBlocking {
        var loads = 0
        val refreshes = mutableListOf<Int>()
        val syncing = linked.copy(syncState = "SYNCING", syncedGameCount = 0)
        refreshLibraryAfterAccountLink("STEAM", listOf(syncing), { true }, {},
            loadConnectors = {
                loads++
                listOf(if (loads < 3) syncing else linked.copy(syncState = "COMPLETE", syncedGameCount = 12))
            }, refreshLibrary = { refreshes += loads })
        assertEquals(listOf(1, 3, 6), refreshes)
    }

    @Test fun unrelatedLinkedStoreDoesNotRefreshTheLibrary() = runBlocking {
        var refreshes = 0
        refreshLibraryAfterAccountLink("EPIC", emptyList(), { true }, {},
            loadConnectors = { listOf(linked) }, refreshLibrary = { refreshes++ })
        assertEquals(0, refreshes)
    }

    @Test fun switchingAccountsDuringTheRequestDiscardsItsResult() = runBlocking {
        var current = true
        var loads = 0
        var refreshes = 0
        refreshLibraryAfterAccountLink("STEAM", emptyList(), { current }, {},
            loadConnectors = { loads++; current = false; listOf(linked) },
            refreshLibrary = { refreshes++ })
        assertEquals(1, loads)
        assertEquals(0, refreshes)
    }

    @Test fun transientConnectorFailureDoesNotPreventLaterRefresh() = runBlocking {
        var loads = 0
        var refreshes = 0
        refreshLibraryAfterAccountLink("STEAM", emptyList(), { true }, {},
            loadConnectors = { loads++; if (loads < 3) null else listOf(linked) },
            refreshLibrary = { refreshes++ })
        assertEquals(6, loads)
        assertEquals(2, refreshes)
    }

    @Test fun cancellationStopsPolling() = runBlocking {
        val cancellation = CancellationException("Account changed")
        val failure = runCatching {
            refreshLibraryAfterAccountLink("STEAM", emptyList(), { true }, { throw cancellation },
                loadConnectors = { error("Must not load after cancellation") },
                refreshLibrary = { error("Must not refresh after cancellation") })
        }.exceptionOrNull()
        assertSame(cancellation, failure)
    }
}

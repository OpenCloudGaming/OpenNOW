package com.opencloudgaming.opennow

import kotlinx.coroutines.CancellationException
import kotlinx.coroutines.runBlocking
import org.junit.Assert.assertEquals
import org.junit.Assert.assertNull
import org.junit.Assert.assertTrue
import org.junit.Assert.fail
import org.junit.Test

class GfnLaunchRoutingTest {
    private val trove = GameInfo(id = "18106011", title = "Trove", playType = "INSTALL_TO_PLAY")
    private val storage = StorageAddon(
        regionName = "Bulgaria",
        regionCode = "NP-SOFMR-DC",
        status = "OK",
        subType = "PERMANENT_STORAGE",
    )
    private val subscription = SubscriptionInfo(membershipTier = "PERFORMANCE", storageAddon = storage)
    private val defaultBase = "https://prod.cloudmatchbeta.nvidiagrid.net/"
    private val bulgaria = StreamRegion("Bulgaria", "https://eu-bulgaria.cloudmatchbeta.nvidiagrid.net")
    private val india = StreamRegion("India", "https://ap-india.cloudmatchbeta.nvidiagrid.net")

    @Test
    fun persistentStorageOverridesBothAutomaticAndManuallySelectedIndia() = runBlocking {
        for (base in listOf(defaultBase, india.url)) {
            val route = resolveGfnLaunchRoute(trove, "NVIDIA", subscription, base, listOf(india, bulgaria)) {
                error("An advertised storage route should not require another request")
            }
            assertEquals("${bulgaria.url}/", route.baseUrl)
            assertEquals("Bulgaria", route.storageRegion)
        }
    }

    @Test
    fun refreshesRegionDiscoveryWhenStartupHasNotFinished() = runBlocking {
        var requests = 0
        val route = resolveGfnLaunchRoute(trove, "NVIDIA", subscription, defaultBase, emptyList()) {
            requests++
            listOf(india, bulgaria)
        }
        assertEquals("${bulgaria.url}/", route.baseUrl)
        assertEquals(1, requests)
    }

    @Test
    fun matchesTrimmedNamesAndAdvertisedCodesWithoutInventingHostnames() = runBlocking {
        for (name in listOf(" bulgaria ", "np-sofmr-dc")) {
            val route = resolveGfnLaunchRoute(
                trove, "nvidia", subscription, defaultBase, listOf(bulgaria.copy(name = name)),
            ) { error("Unexpected refresh") }
            assertEquals("${bulgaria.url}/", route.baseUrl)
        }
    }

    @Test
    fun optimizedGamesAndTemporaryStorageKeepTheSelectedServer() = runBlocking {
        val cases = listOf(
            trove.copy(playType = "FULLY_OPTIMIZED") to subscription,
            trove to subscription.copy(storageAddon = null),
            trove to subscription.copy(storageAddon = storage.copy(status = "EXPIRED")),
            trove to subscription.copy(storageAddon = storage.copy(subType = "TEMPORARY_STORAGE")),
        )
        for ((game, info) in cases) {
            val route = resolveGfnLaunchRoute(game, "NVIDIA", info, india.url, listOf(bulgaria)) {
                error("Unrelated launches must not fetch storage routes")
            }
            assertEquals(india.url, route.baseUrl)
            assertNull(route.storageRegion)
        }
    }

    @Test
    fun allianceProviderRoutingIsPreserved() = runBlocking {
        val providerBase = "https://prod.yes.geforcenow.nvidiagrid.net/"
        val route = resolveGfnLaunchRoute(trove, "YES", subscription, providerBase, listOf(bulgaria)) {
            error("Do not route an alliance account into NVIDIA infrastructure")
        }
        assertEquals(providerBase, route.baseUrl)
        assertNull(route.storageRegion)
    }

    @Test
    fun missingInvalidOrAmbiguousStorageRoutesNeverFallBackToIndia() = runBlocking {
        for (regions in listOf(
            listOf(india),
            listOf(bulgaria.copy(url = "http://eu-bulgaria.cloudmatchbeta.nvidiagrid.net")),
            listOf(bulgaria, bulgaria.copy(url = india.url)),
        )) {
            try {
                resolveGfnLaunchRoute(trove, "NVIDIA", subscription, india.url, regions) { regions }
                fail("An unresolved storage route must stop the launch")
            } catch (error: IllegalStateException) {
                assertTrue(error.message.orEmpty().contains("storage server for Bulgaria"))
            }
        }
    }

    @Test
    fun cancellationDuringRegionRefreshDoesNotReturnADefaultRoute() = runBlocking {
        val cancelled = CancellationException("Launch cancelled")
        try {
            resolveGfnLaunchRoute(trove, "NVIDIA", subscription, defaultBase, emptyList()) { throw cancelled }
            fail("Cancelled launches must stay cancelled")
        } catch (error: CancellationException) {
            assertTrue(error === cancelled)
        }
    }
}

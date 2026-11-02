package com.opencloudgaming.opennow

import java.io.IOException
import kotlinx.coroutines.CancellationException
import kotlinx.coroutines.async
import kotlinx.coroutines.awaitAll
import kotlinx.coroutines.coroutineScope
import kotlinx.coroutines.delay
import kotlinx.coroutines.runBlocking
import org.junit.Assert.assertEquals
import org.junit.Assert.assertFalse
import org.junit.Assert.assertTrue
import org.junit.Test

class GfnServerInfoTest {
    private val nvidia = "https://prod.cloudmatchbeta.nvidiagrid.net/"
    private val alliance = "https://prod.yes.geforcenow.nvidiagrid.net/"
    private val healthy = """{
        "requestStatus":{"statusCode":1,"serverId":"NP-AMS-06"},
        "metaData":[
          {"key":"EU West","value":"https://eu-west.cloudmatchbeta.nvidiagrid.net/"},
          {"key":"gfn-regions","value":"https://ignored.example/"},
          {"key":"Invalid","value":"http://insecure.example/"}
        ]
    }"""

    @Test fun discoveryParsesIdentityAndOnlyAdvertisedSecureRoutes() {
        val info = parseGfnServerInfo(200, healthy)
        assertEquals("NP-AMS-06", info.vpcId)
        assertEquals(listOf(StreamRegion("EU West", "https://eu-west.cloudmatchbeta.nvidiagrid.net/")), info.regions)
    }

    @Test fun outageAndMalformedIdentityCannotBecomeSuccessfulCatalogScope() {
        for ((code, body) in listOf(
            503 to "<html><body><h1>503 Service Unavailable</h1>No server is available to handle this request.</body></html>",
            200 to "<html>Unavailable</html>",
            200 to "{}",
            200 to """{"requestStatus":{"statusCode":1,"serverId":""}}""",
            200 to """{"requestStatus":{"statusCode":1,"serverId":"GFN-PC"}}""",
            200 to """{"requestStatus":{"statusCode":1,"serverId":123}}""",
            200 to """{"requestStatus":{"statusCode":14,"serverId":"NP-AMS-06"}}""",
        )) {
            assertTrue("$code $body", runCatching { parseGfnServerInfo(code, body) }.exceptionOrNull() is GfnServerInfoException)
        }
    }

    @Test fun concurrentCatalogSubscriptionAndRegionRequestsShareSuccessAndFailure() = runBlocking {
        for (success in listOf(false, true)) {
            var requests = 0
            val repository = GfnServerInfoRepository(fetch = { _, _ ->
                requests++
                delay(10)
                parseGfnServerInfo(if (success) 200 else 503, healthy)
            }, elapsedMs = { 0 })
            coroutineScope {
                (1..8).map {
                    async { runCatching { repository.get("token", nvidia.uppercase()) } }
                }.awaitAll().forEach { result -> assertEquals(success, result.isSuccess) }
            }
            assertEquals(1, requests)
            assertEquals(if (success) 1 else 0, repository.regions("token", nvidia).size)
            assertEquals(1, requests)
        }
    }

    @Test fun discoveryOutageStopsCatalogBeforeAnyFakeIdentityIsUsed() = runBlocking {
        val repository = GfnServerInfoRepository(fetch = { _, _ -> parseGfnServerInfo(503, "Unavailable") }, elapsedMs = { 0 })
        val catalog = GfnCatalogRepository(serverInfoRepository = repository)
        val failure = runCatching { catalog.getVpcId("token", nvidia) }.exceptionOrNull()
        assertTrue(failure is GfnServerInfoException)
        assertTrue(failure!!.message.orEmpty().contains("HTTP 503"))
    }

    @Test fun expiredIdentitySurvivesBriefOutageThenRecoversAndNeverCrossesProviders() = runBlocking {
        var now = 0L
        var available = true
        var requests = 0
        val repository = GfnServerInfoRepository(fetch = { _, _ ->
            requests++
            if (!available) throw IOException("offline")
            parseGfnServerInfo(200, healthy)
        }, elapsedMs = { now })
        val original = repository.get("token", nvidia)
        available = false
        now = 5 * 60 * 1000L
        assertEquals(original, repository.get("token", nvidia))
        assertEquals(original.regions, repository.regions("token", nvidia))
        assertEquals(2, requests)
        assertTrue(runCatching { repository.get("token", alliance) }.isFailure)
        now += 5_000
        available = true
        assertEquals(original, repository.get("refreshed-token", nvidia))
        assertEquals(4, requests)
        now += 30 * 60 * 1000L
        available = false
        assertTrue(runCatching { repository.get("token", nvidia) }.isFailure)
    }

    @Test fun coldStartRetriesAfterBackoffInsteadOfCachingFallbackForever() = runBlocking {
        var now = 0L
        var requests = 0
        val repository = GfnServerInfoRepository(fetch = { _, _ ->
            if (++requests == 1) parseGfnServerInfo(503, "Unavailable") else parseGfnServerInfo(200, healthy)
        }, elapsedMs = { now })
        assertTrue(runCatching { repository.get("token", nvidia) }.isFailure)
        assertTrue(repository.regions("token", nvidia).isEmpty())
        assertEquals(1, requests)
        now = 5_000
        assertEquals("NP-AMS-06", repository.get("token", nvidia).vpcId)
        assertEquals(2, requests)
    }

    @Test fun authenticationFailureDoesNotUseStaleDataOrBlockRefreshedToken() = runBlocking {
        var now = 0L
        val repository = GfnServerInfoRepository(fetch = { token, _ ->
            parseGfnServerInfo(if (token == "expired") 401 else 200, healthy)
        }, elapsedMs = { now })
        repository.get("valid", nvidia)
        now = 5 * 60 * 1000L
        val failure = runCatching { repository.get("expired", nvidia) }.exceptionOrNull() as GfnServerInfoException
        assertFalse(failure.retryable)
        assertEquals("NP-AMS-06", repository.get("refreshed", nvidia).vpcId)
    }

    @Test fun cancellationIsPropagatedWithoutCachingFailure() = runBlocking {
        var requests = 0
        val repository = GfnServerInfoRepository(fetch = { _, _ ->
            if (++requests == 1) throw CancellationException("cancelled")
            parseGfnServerInfo(200, healthy)
        }, elapsedMs = { 0 })
        assertTrue(runCatching { repository.regions("token", nvidia) }.exceptionOrNull() is CancellationException)
        assertEquals("NP-AMS-06", repository.get("token", nvidia).vpcId)
        assertEquals(2, requests)
    }

    @Test fun coldStartFailoverRestoresCatalogAndUsesSameLiveRouteForSessions() = runBlocking {
        val regional = "https://ap-japan.cloudmatchbeta.nvidiagrid.net/"
        val requests = mutableListOf<String>()
        val repository = GfnServerInfoRepository(fetch = { _, base ->
            requests += base
            if (base == nvidia) parseGfnServerInfo(503, "No server is available to handle this request.")
            else parseGfnServerInfo(200, healthy.replace("NP-AMS-06", "NP-TYO-01"))
        }, elapsedMs = { 0 }, fallbackBases = listOf(regional))
        val catalog = GfnCatalogRepository(serverInfoRepository = repository)

        assertEquals("NP-TYO-01", catalog.getVpcId("token", nvidia))
        assertEquals(1, repository.regions("token", nvidia).size)
        assertEquals(regional, repository.streamingBaseUrl("token", nvidia))
        assertEquals(listOf(nvidia, regional), requests)
    }

    @Test fun failoverUsesFirstHealthyResponseAndCancelsSlowProbes() = runBlocking {
        val slow = "https://eu-germany.cloudmatchbeta.nvidiagrid.net/"
        val fast = "https://ap-japan.cloudmatchbeta.nvidiagrid.net/"
        var slowCancelled = false
        val repository = GfnServerInfoRepository(fetch = { _, base ->
            when (base) {
                nvidia -> parseGfnServerInfo(503, "Unavailable")
                slow -> try {
                    delay(60_000)
                    parseGfnServerInfo(200, healthy)
                } finally { slowCancelled = true }
                else -> {
                    delay(10)
                    parseGfnServerInfo(200, healthy.replace("NP-AMS-06", "NP-TYO-01"))
                }
            }
        }, elapsedMs = { 0 }, fallbackBases = listOf(slow, fast))
        assertEquals(fast, repository.streamingBaseUrl("token", nvidia))
        assertTrue(slowCancelled)
    }

    @Test fun healthyAutomaticDiscoveryDoesNotProbeOrChangeTheStreamingRoute() = runBlocking {
        var requests = 0
        val repository = GfnServerInfoRepository(fetch = { _, base ->
            requests++
            assertEquals(nvidia, base)
            parseGfnServerInfo(200, healthy)
        }, elapsedMs = { 0 }, fallbackBases = listOf("https://ap-japan.cloudmatchbeta.nvidiagrid.net/"))
        assertEquals(nvidia, repository.streamingBaseUrl("token", nvidia))
        assertEquals(1, requests)
    }

    @Test fun regionalFailoverNeverRewritesExplicitServersOrAllianceProviders() = runBlocking {
        for (base in listOf(alliance, "https://eu-germany.cloudmatchbeta.nvidiagrid.net/")) {
            var requests = 0
            val repository = GfnServerInfoRepository(fetch = { _, requested ->
                requests++
                assertEquals(base, requested)
                parseGfnServerInfo(503, "Unavailable")
            }, elapsedMs = { 0 }, fallbackBases = listOf("https://ap-japan.cloudmatchbeta.nvidiagrid.net/"))
            assertEquals(base, repository.streamingBaseUrl("token", base))
            assertEquals(0, requests)
            assertTrue(runCatching { repository.get("token", base) }.isFailure)
            assertEquals(1, requests)
        }
    }

    @Test fun authenticationRejectionNeverTriggersRegionalFailover() = runBlocking {
        var requests = 0
        val repository = GfnServerInfoRepository(fetch = { _, base ->
            requests++
            assertEquals(nvidia, base)
            parseGfnServerInfo(401, "Unauthorized")
        }, elapsedMs = { 0 }, fallbackBases = listOf("https://ap-japan.cloudmatchbeta.nvidiagrid.net/"))
        val failure = runCatching { repository.get("token", nvidia) }.exceptionOrNull() as GfnServerInfoException
        assertFalse(failure.retryable)
        assertEquals(1, requests)
    }

    @Test fun failedRegionalProbesRemainFailuresAndRespectSharedBackoff() = runBlocking {
        var requests = 0
        val repository = GfnServerInfoRepository(fetch = { _, _ ->
            requests++
            parseGfnServerInfo(503, "Unavailable")
        }, elapsedMs = { 0 }, fallbackBases = listOf(
            "https://ap-japan.cloudmatchbeta.nvidiagrid.net/",
            "https://eu-germany.cloudmatchbeta.nvidiagrid.net/",
        ))
        assertTrue(runCatching { repository.get("token", nvidia) }.isFailure)
        assertTrue(repository.regions("token", nvidia).isEmpty())
        assertTrue(runCatching { repository.streamingBaseUrl("token", nvidia) }.isFailure)
        assertEquals(3, requests)
    }
}

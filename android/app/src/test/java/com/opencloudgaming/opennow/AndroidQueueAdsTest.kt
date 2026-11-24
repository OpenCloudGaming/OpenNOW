package com.opencloudgaming.opennow

import org.junit.Assert.assertEquals
import org.junit.Assert.assertFalse
import org.junit.Assert.assertNull
import org.junit.Assert.assertTrue
import org.junit.Test

class AndroidQueueAdsTest {
    @Test
    fun mediaSourcesKeepServerOrderAndFallBackToLegacyUrls() {
        val primary = "https://example.com/primary.mp4"
        val alternative = "https://example.com/alternative.mp4"
        val legacy = "https://example.com/legacy.mp4"
        val ad = SessionAdInfo(
            adId = "sponsor",
            adMediaFiles = listOf(
                SessionAdMediaFile(mediaFileUrl = " "),
                SessionAdMediaFile(mediaFileUrl = primary),
                SessionAdMediaFile(mediaFileUrl = alternative),
            ),
            adUrl = " $primary ",
            mediaUrl = legacy,
        )

        assertEquals(listOf(primary, alternative, legacy), sessionAdMediaUrls(ad))
        assertEquals(listOf(legacy), sessionAdMediaUrls(ad.copy(adMediaFiles = emptyList(), adUrl = "")))
        assertEquals(emptyList<String>(), sessionAdMediaUrls(SessionAdInfo(adId = "missing")))
    }

    @Test
    fun mergeQueueSessionStatePreservesAdsWhenServerTemporarilyOmitsThem() {
        val previous = session(
            adState = SessionAdState(
                isAdsRequired = true,
                sessionAdsRequired = true,
                sessionAds = listOf(ad("ad-1")),
                ads = listOf(ad("ad-1")),
            ),
        )
        val next = session(
            queuePosition = 4,
            adState = SessionAdState(
                isAdsRequired = true,
                sessionAdsRequired = true,
                serverSentEmptyAds = true,
            ),
        )

        val merged = mergeQueueSessionState(previous, next)

        assertEquals(4, merged.queuePosition)
        assertEquals(listOf("ad-1"), sessionAdItems(merged.adState).map { it.adId })
    }

    @Test
    fun mergeQueueSessionStateDoesNotRestoreAdsForReadySession() {
        val previous = session(
            adState = SessionAdState(
                isAdsRequired = true,
                sessionAdsRequired = true,
                sessionAds = listOf(ad("ad-1")),
                ads = listOf(ad("ad-1")),
            ),
        )
        val ready = session(
            status = 2,
            adState = SessionAdState(
                isAdsRequired = false,
                sessionAdsRequired = false,
                serverSentEmptyAds = true,
            ),
        )

        val merged = mergeQueueSessionState(previous, ready)

        assertEquals(2, merged.status)
        assertEquals(emptyList<SessionAdInfo>(), sessionAdItems(merged.adState))
    }

    @Test
    fun mergeQueueSessionStatePreservesAdsForReadyStatusWithoutStreamEndpoint() {
        val previous = session(
            adState = SessionAdState(
                isAdsRequired = true,
                sessionAdsRequired = true,
                sessionAds = listOf(ad("ad-1")),
                ads = listOf(ad("ad-1")),
            ),
        )
        val partialReady = session(
            status = 2,
            serverIp = "",
            signalingServer = "",
            signalingUrl = "",
            adState = SessionAdState(
                isAdsRequired = true,
                sessionAdsRequired = true,
                serverSentEmptyAds = true,
            ),
        )

        val merged = mergeQueueSessionState(previous, partialReady)

        assertEquals(2, merged.status)
        assertEquals(listOf("ad-1"), sessionAdItems(merged.adState).map { it.adId })
    }

    @Test
    fun mergeQueueAdStateDoesNotRestoreAfterExplicitLocalClear() {
        val previous = SessionAdState(
            isAdsRequired = true,
            sessionAdsRequired = true,
            sessionAds = listOf(ad("ad-1")),
            ads = listOf(ad("ad-1")),
        )
        val next = SessionAdState(
            isAdsRequired = true,
            sessionAdsRequired = true,
            serverSentEmptyAds = false,
        )

        val merged = mergeQueueAdState(previous, next)

        assertEquals(emptyList<SessionAdInfo>(), sessionAdItems(merged))
    }

    @Test
    fun mergeQueueAdStateKeepsMissingAdStateStableDuringQueue() {
        val previous = SessionAdState(
            isAdsRequired = true,
            sessionAdsRequired = true,
            sessionAds = listOf(ad("ad-1")),
            ads = listOf(ad("ad-1")),
        )

        val merged = mergeQueueAdState(previous, null)

        assertEquals(listOf("ad-1"), sessionAdItems(merged).map { it.adId })
    }

    @Test
    fun mergeQueueAdStateCanClearMissingAdStateAfterFinishReport() {
        val previous = SessionAdState(
            isAdsRequired = true,
            sessionAdsRequired = true,
            sessionAds = listOf(ad("ad-1")),
            ads = listOf(ad("ad-1")),
        )

        val merged = mergeQueueAdState(previous, null, preserveMissingAdState = false)

        assertNull(merged)
    }

    @Test
    fun mergeQueueAdStateDoesNotRestoreServerEmptyAdsAfterFinishReport() {
        val previous = SessionAdState(
            isAdsRequired = true,
            sessionAdsRequired = true,
            sessionAds = listOf(ad("ad-1")),
            ads = listOf(ad("ad-1")),
        )
        val next = SessionAdState(
            isAdsRequired = true,
            sessionAdsRequired = true,
            serverSentEmptyAds = true,
        )

        val merged = mergeQueueAdState(previous, next, preserveMissingAdState = false)

        assertEquals(emptyList<SessionAdInfo>(), sessionAdItems(merged))
    }

    @Test
    fun mergeQueueSessionStateDoesNotRestoreServerEmptyAdsAfterFinishReport() {
        val previous = session(
            adState = SessionAdState(
                isAdsRequired = true,
                sessionAdsRequired = true,
                sessionAds = listOf(ad("ad-1")),
                ads = listOf(ad("ad-1")),
            ),
        )
        val next = session(
            queuePosition = 5,
            adState = SessionAdState(
                isAdsRequired = true,
                sessionAdsRequired = true,
                serverSentEmptyAds = true,
            ),
        )

        val merged = mergeQueueSessionState(previous, next, preserveMissingAdState = false)

        assertEquals(5, merged.queuePosition)
        assertEquals(emptyList<SessionAdInfo>(), sessionAdItems(merged.adState))
    }

    @Test
    fun removeSessionAdItemDropsOnlyCompletedAd() {
        val state = SessionAdState(
            isAdsRequired = true,
            sessionAdsRequired = true,
            sessionAds = listOf(ad("ad-1"), ad("ad-2")),
            ads = listOf(ad("ad-1"), ad("ad-2")),
        )

        val updated = removeSessionAdItem(state, "ad-1")

        assertEquals(listOf("ad-2"), sessionAdItems(updated).map { it.adId })
    }

    @Test
    fun mergeQueueSessionStatePreservesRemainingAdsAfterCompletedAdIsRemoved() {
        val previous = session(
            adState = SessionAdState(
                isAdsRequired = true,
                sessionAdsRequired = true,
                sessionAds = listOf(ad("ad-1"), ad("ad-2")),
                ads = listOf(ad("ad-1"), ad("ad-2")),
            ),
        )
        val locallyAdvanced = removeSessionAdItem(previous, "ad-1")
        val serverOmittedAds = session(
            queuePosition = 5,
            adState = SessionAdState(
                isAdsRequired = true,
                sessionAdsRequired = true,
                serverSentEmptyAds = true,
            ),
        )

        val merged = mergeQueueSessionState(locallyAdvanced, serverOmittedAds)

        assertEquals(5, merged.queuePosition)
        assertEquals(listOf("ad-2"), sessionAdItems(merged.adState).map { it.adId })
    }

    @Test
    fun mergeQueueSessionStateDoesNotRestoreLastCompletedAdAfterLocalRemoval() {
        val previous = session(
            adState = SessionAdState(
                isAdsRequired = true,
                sessionAdsRequired = true,
                sessionAds = listOf(ad("ad-1")),
                ads = listOf(ad("ad-1")),
            ),
        )
        val locallyCompleted = removeSessionAdItem(previous, "ad-1")
        val serverOmittedAds = session(
            queuePosition = 5,
            adState = SessionAdState(
                isAdsRequired = true,
                sessionAdsRequired = true,
                serverSentEmptyAds = true,
            ),
        )

        val merged = mergeQueueSessionState(locallyCompleted, serverOmittedAds)

        assertEquals(5, merged.queuePosition)
        assertEquals(emptyList<SessionAdInfo>(), sessionAdItems(merged.adState))
    }

    @Test
    fun mergeQueueAdReportResultDropsCompletedAdFromEchoedServerList() {
        val previous = session(
            adState = SessionAdState(
                isAdsRequired = true,
                sessionAdsRequired = true,
                sessionAds = listOf(ad("ad-2")),
                ads = listOf(ad("ad-2")),
                serverSentEmptyAds = false,
            ),
        )
        val echoed = session(
            queuePosition = 5,
            adState = SessionAdState(
                isAdsRequired = true,
                sessionAdsRequired = true,
                sessionAds = listOf(ad("ad-1"), ad("ad-2")),
                ads = listOf(ad("ad-1"), ad("ad-2")),
            ),
        )

        val merged = mergeQueueAdReportResult(
            previous = previous,
            updated = echoed,
            adId = "ad-1",
            terminalAction = true,
        )

        assertEquals(5, merged.queuePosition)
        assertEquals(listOf("ad-2"), sessionAdItems(merged.adState).map { it.adId })
    }

    @Test
    fun mergeQueueAdReportResultKeepsLocallyAdvancedAdWhenServerOmitsList() {
        val locallyAdvanced = session(
            adState = SessionAdState(
                isAdsRequired = true,
                sessionAdsRequired = true,
                sessionAds = listOf(ad("ad-2")),
                ads = listOf(ad("ad-2")),
                serverSentEmptyAds = false,
            ),
        )
        val omitted = session(
            queuePosition = 4,
            adState = SessionAdState(
                isAdsRequired = true,
                sessionAdsRequired = true,
                serverSentEmptyAds = true,
            ),
        )

        val merged = mergeQueueAdReportResult(
            previous = locallyAdvanced,
            updated = omitted,
            adId = "ad-1",
            terminalAction = true,
        )

        assertEquals(4, merged.queuePosition)
        assertEquals(listOf("ad-2"), sessionAdItems(merged.adState).map { it.adId })
    }

    @Test
    fun nextSessionAdIdOnlyAdvancesToADifferentAd() {
        val ads = SessionAdState(
            isAdsRequired = true,
            sessionAdsRequired = true,
            sessionAds = listOf(ad("ad-1"), ad("ad-2")),
            ads = listOf(ad("ad-1"), ad("ad-2")),
        )

        assertEquals("ad-2", nextSessionAdId(ads, "ad-1"))
        assertNull(nextSessionAdId(ads, "ad-2"))
        assertNull(nextSessionAdId(ads.copy(sessionAds = listOf(ad("ad-1")), ads = emptyList()), "ad-1"))
        assertEquals("ad-2", nextSessionAdId(ads.copy(sessionAds = listOf(ad("ad-2")), ads = emptyList()), "ad-1"))
    }

    @Test
    fun mergeQueueAdStateReturnsNullWhenNoPriorAdStateExists() {
        assertNull(mergeQueueAdState(null, null))
    }

    @Test
    fun shouldWaitForQueueAdPlaybackRequiresPlayableAdItem() {
        val waiting = SessionAdState(
            isAdsRequired = true,
            sessionAdsRequired = true,
            sessionAds = listOf(ad("ad-1")),
            ads = listOf(ad("ad-1")),
        )
        val emptyRequired = SessionAdState(
            isAdsRequired = true,
            sessionAdsRequired = true,
            serverSentEmptyAds = true,
        )

        assertEquals(true, shouldWaitForQueueAdPlayback(waiting))
        assertEquals(false, shouldWaitForQueueAdPlayback(emptyRequired))
    }

    private fun ad(id: String): SessionAdInfo =
        SessionAdInfo(adId = id, mediaUrl = "https://example.invalid/$id.mp4")

    private fun session(
        status: Int = 1,
        queuePosition: Int? = 7,
        adState: SessionAdState? = null,
        serverIp: String = "np.example.invalid",
        signalingServer: String = "np.example.invalid:443",
        signalingUrl: String = "wss://np.example.invalid/nvst/",
    ): SessionInfo =
        SessionInfo(
            sessionId = "session-1",
            status = status,
            queuePosition = queuePosition,
            adState = adState,
            zone = "prod",
            streamingBaseUrl = "https://np.example.invalid",
            serverIp = serverIp,
            signalingServer = signalingServer,
            signalingUrl = signalingUrl,
        )
}

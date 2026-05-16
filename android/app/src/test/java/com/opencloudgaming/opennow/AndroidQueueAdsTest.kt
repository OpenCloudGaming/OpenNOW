package com.opencloudgaming.opennow

import org.junit.Assert.assertEquals
import org.junit.Assert.assertNull
import org.junit.Test

class AndroidQueueAdsTest {
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
    ): SessionInfo =
        SessionInfo(
            sessionId = "session-1",
            status = status,
            queuePosition = queuePosition,
            adState = adState,
            zone = "prod",
            streamingBaseUrl = "https://np.example.invalid",
            serverIp = "np.example.invalid",
            signalingServer = "",
            signalingUrl = "",
        )
}

package com.opencloudgaming.opennow

import org.junit.Assert.assertEquals
import org.junit.Assert.assertFalse
import org.junit.Assert.assertTrue
import org.junit.Test

class CatalogAdsTest {
    @Test
    fun catalogAdsResumeOnlyWhileBrowsingWithAMinimizedQueue() {
        val minimized = OpenNowUiState(
            page = AppPage.Home,
            streamStatus = "queue",
            streamLaunchMinimized = true,
            queuePosition = 12,
        )
        assertTrue(canShowCatalogAds(minimized))
        assertTrue(canShowCatalogAds(minimized.copy(page = AppPage.Library)))
        assertFalse(canShowCatalogAds(minimized.copy(page = AppPage.Stream, streamLaunchMinimized = false)))
        assertFalse(canShowCatalogAds(minimized.copy(page = AppPage.Settings)))
        assertFalse(canShowCatalogAds(minimized.copy(androidPictureInPictureActive = true)))
        assertFalse(canShowCatalogAds(minimized.copy(streamStatus = "connecting")))
        assertFalse(canShowCatalogAds(minimized.copy(streamStatus = "streaming")))
        assertTrue(canShowCatalogAds(OpenNowUiState(page = AppPage.Home)))
    }

    @Test
    fun catalogAdsStopWhenTheMinimizedQueueReachesRigSetupOrRequiresASponsorAd() {
        val session = SessionInfo(
            sessionId = "queued-session",
            status = 1,
            seatSetupStep = 1,
            serverIp = "",
            signalingServer = "",
            signalingUrl = "",
        )
        val minimized = OpenNowUiState(
            page = AppPage.Library,
            streamStatus = "queue",
            streamLaunchMinimized = true,
            streamSession = session,
        )
        assertTrue(canShowCatalogAds(minimized))
        assertFalse(canShowCatalogAds(minimized.copy(streamSession = session.copy(seatSetupStep = 3))))
        assertFalse(canShowCatalogAds(minimized.copy(streamSession = session.copy(status = 2))))
        val sponsorAdState = SessionAdState(
            sessionAdsRequired = true,
            sessionAds = listOf(SessionAdInfo(adId = "sponsor", adUrl = "https://example.com/ad.mp4")),
        )
        assertFalse(canShowCatalogAds(minimized.copy(streamSession = session.copy(adState = sponsorAdState))))
    }

    @Test
    fun slotsArePlacedAfterEveryThirtyGames() {
        assertEquals(emptyList<Int>(), CatalogAdSlots(29).stops.toList())
        assertEquals(listOf(30, 60, 90), CatalogAdSlots(99).stops.toList())
    }

    @Test
    fun prefetchStartsWithTwoSlotsAndFollowsForwardJumps() {
        assertEquals(listOf(30, 60), catalogAdPrefetchStops(250, 1..8).toList())
        assertEquals(listOf(30, 60, 90), catalogAdPrefetchStops(250, 24..30).toList())
        assertEquals(listOf(150, 180), catalogAdPrefetchStops(250, 131..139).toList())
        assertEquals(listOf(240), catalogAdPrefetchStops(250, 230..238).toList())
        assertEquals(emptyList<Int>(), catalogAdPrefetchStops(250, 241..250).toList())
    }

    @Test
    fun focusIndexAccountsForAdSlotsWhetherTheCreativeHasLoadedOrNot() {
        assertEquals(29, catalogGameGridIndex(29, hasAdSlots = true))
        assertEquals(31, catalogGameGridIndex(30, hasAdSlots = true))
        assertEquals(62, catalogGameGridIndex(60, hasAdSlots = true))
        assertEquals(2, catalogGameGridIndex(0, hasAdSlots = true, headerCount = 2))
        assertEquals(33, catalogGameGridIndex(30, hasAdSlots = true, headerCount = 2))
        assertEquals(32, catalogGameGridIndex(30, hasAdSlots = false, headerCount = 2))
    }
}

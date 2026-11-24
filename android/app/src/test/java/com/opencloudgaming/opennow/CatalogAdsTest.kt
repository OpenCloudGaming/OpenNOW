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
    fun slotsArePlacedAfterACompleteRowAtLeastThirtyGamesApart() {
        val threeColumnInterval = catalogAdInterval(3)
        val fourColumnInterval = catalogAdInterval(4)
        assertEquals(emptyList<Int>(), CatalogAdSlots(29, threeColumnInterval).stops.toList())
        assertEquals(listOf(30, 60, 90), CatalogAdSlots(99, threeColumnInterval).stops.toList())
        assertEquals(listOf(32, 64, 96), CatalogAdSlots(99, fourColumnInterval).stops.toList())
    }

    @Test
    fun prefetchStartsWithTwoSlotsAndFollowsForwardJumps() {
        assertEquals(listOf(32, 64), catalogAdPrefetchStops(250, 1..8, 32).toList())
        assertEquals(listOf(32, 64, 96), catalogAdPrefetchStops(250, 24..32, 32).toList())
        assertEquals(listOf(160, 192), catalogAdPrefetchStops(250, 131..139, 32).toList())
        assertEquals(listOf(224), catalogAdPrefetchStops(250, 222..230, 32).toList())
        assertEquals(emptyList<Int>(), catalogAdPrefetchStops(250, 225..250, 32).toList())
    }

    @Test
    fun focusIndexAccountsForAdSlotsWhetherTheCreativeHasLoadedOrNot() {
        assertEquals(31, catalogGameGridIndex(31, adInterval = 32, headerCount = 0))
        assertEquals(33, catalogGameGridIndex(32, adInterval = 32, headerCount = 0))
        assertEquals(66, catalogGameGridIndex(64, adInterval = 32, headerCount = 0))
        assertEquals(2, catalogGameGridIndex(0, adInterval = 32, headerCount = 2))
        assertEquals(35, catalogGameGridIndex(32, adInterval = 32, headerCount = 2))
        assertEquals(34, catalogGameGridIndex(32, adInterval = null, headerCount = 2))
    }
}

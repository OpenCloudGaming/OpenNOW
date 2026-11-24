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
    fun placementUsesWholeRowsNearThirtyGames() {
        assertEquals(30, catalogAdInterval(columns = 3))
        assertEquals(32, catalogAdInterval(columns = 4))
        assertEquals(35, catalogAdInterval(columns = 7))
    }

    @Test
    fun requestsOnlyNearbySlotsWithinTheCatalog() {
        assertEquals(emptyList<Int>(), catalogAdStops(90, 30, 1..8, 6))
        assertEquals(listOf(30), catalogAdStops(90, 30, 1..8, 24))
        assertEquals(listOf(30), catalogAdStops(90, 30, 22..27, 6))
        assertEquals(listOf(60), catalogAdStops(90, 30, 53..56, 6))
        assertEquals(emptyList<Int>(), catalogAdStops(29, 30, 24..29, 6))
    }

    @Test
    fun focusIndexAccountsForLoadedRowsBeforeTheTargetOnly() {
        val ads = setOf(30, 60)
        assertEquals(29, catalogGameGridIndex(29, ads))
        assertEquals(31, catalogGameGridIndex(30, ads))
        assertEquals(62, catalogGameGridIndex(60, ads))
        assertEquals(2, catalogGameGridIndex(0, ads, headerCount = 2))
        assertEquals(33, catalogGameGridIndex(30, ads, headerCount = 2))
    }

    @Test
    fun aRecentlyPassedSlotCanStillBeLoaded() {
        assertEquals(listOf(30), catalogAdStops(90, 30, 31..36, lookAhead = 6, lookBehind = 3))
        assertEquals(emptyList<Int>(), catalogAdStops(90, 30, 34..39, lookAhead = 6, lookBehind = 3))
        assertEquals(listOf(30), catalogAdStops(90, 30, 1..8, lookAhead = 24, lookBehind = 3))
    }
}

package com.opencloudgaming.opennow

import android.view.Display
import org.junit.Assert.assertEquals
import org.junit.Assert.assertFalse
import org.junit.Assert.assertNull
import org.junit.Assert.assertTrue
import org.junit.Test

class DualScreenTest {
    private val signedIn = OpenNowUiState(
        authSession = AuthSession(
            provider = LoginProvider(
                idpId = "test-idp",
                code = "TEST",
                displayName = "Test",
                streamingServiceUrl = "https://example.invalid",
            ),
            tokens = AuthTokens(accessToken = "token", expiresAt = 0L),
            user = AuthUser(userId = "user", displayName = "Zortos", membershipTier = "ULTIMATE"),
        ),
    )

    @Test
    fun signedOutDeckWinsOverEveryStreamState() {
        assertEquals(DualScreenPhase.SignedOut, dualScreenPhase(OpenNowUiState(streamStatus = "streaming")))
    }

    @Test
    fun streamStatusSelectsBrowseLaunchingOrStreaming() {
        assertEquals(DualScreenPhase.Browse, dualScreenPhase(signedIn))
        assertEquals(DualScreenPhase.Launching, dualScreenPhase(signedIn.copy(streamStatus = "queue")))
        assertEquals(DualScreenPhase.Launching, dualScreenPhase(signedIn.copy(streamStatus = "connecting")))
        assertEquals(DualScreenPhase.Streaming, dualScreenPhase(signedIn.copy(streamStatus = "streaming")))
    }

    @Test
    fun statsAreOnlyPublishedWhileStreaming() {
        val stats = StreamRuntimeStats(pingMs = 18, fps = 119, bitrateKbps = 48_000, packetLossPct = 0.1)

        assertNull(dualScreenSnapshot(signedIn.copy(streamStatus = "queue"), stats).stats)
        assertEquals(stats, dualScreenSnapshot(signedIn.copy(streamStatus = "streaming"), stats).stats)
    }

    @Test
    fun snapshotCarriesAccountAndActiveTargetFps() {
        val state = signedIn.copy(
            streamStatus = "streaming",
            activeStreamSettings = StreamSettings(fps = 120),
            settings = AppSettings(bottomScreenPlayMode = BottomScreenPlayMode.Trackpad),
        )

        val snapshot = dualScreenSnapshot(state, stats = null)

        assertEquals("Zortos", snapshot.accountName)
        assertEquals("ULTIMATE", snapshot.membershipTier)
        assertEquals(120, snapshot.targetFps)
        assertEquals(BottomScreenPlayMode.Trackpad, snapshot.playMode)
    }

    @Test
    fun deckSortAndFilterOnlyOffersTheStoreGroupsTheTopScreenShowed() {
        val store = CatalogFilterGroup("digital_store", "Store", listOf(CatalogFilterOption("steam", "STEAM", "Steam", "digital_store", "Store")))
        val hidden = CatalogFilterGroup("maturity", "Maturity", listOf(CatalogFilterOption("m", "M", "Mature", "maturity", "Maturity")))
        val state = signedIn.copy(
            catalogResult = CatalogBrowseResult(games = emptyList(), filterGroups = listOf(store, hidden)),
            catalogFilterIds = listOf("steam"),
            librarySortId = LIBRARY_SORT_TITLE,
        )

        val snapshot = dualScreenSnapshot(state, stats = null)

        assertEquals(listOf(store), snapshot.catalogFilterGroups)
        assertEquals(listOf("steam"), snapshot.catalogFilterIds)
        assertEquals(LIBRARY_SORT_TITLE, snapshot.librarySortId)
    }

    @Test
    fun singleScreenAndExternalDisplaysNeverStartTheDeck() {
        val main = BottomScreenCandidate(Display.DEFAULT_DISPLAY, Display.STATE_ON, 0)
        val tv = BottomScreenCandidate(2, Display.STATE_ON, Display.FLAG_PRESENTATION)
        val private = BottomScreenCandidate(3, Display.STATE_ON, Display.FLAG_PRIVATE)

        assertNull(selectBottomScreenDisplayId(listOf(main), knownDualScreenDevice = false))
        assertNull(selectBottomScreenDisplayId(listOf(main, tv, private), knownDualScreenDevice = false))
    }

    @Test
    fun builtInSecondPanelWinsOverADockedTv() {
        val main = BottomScreenCandidate(Display.DEFAULT_DISPLAY, Display.STATE_ON, 0)
        val bottom = BottomScreenCandidate(4, Display.STATE_ON, 0)
        val tv = BottomScreenCandidate(2, Display.STATE_ON, Display.FLAG_PRESENTATION)

        assertEquals(4, selectBottomScreenDisplayId(listOf(main, tv, bottom), knownDualScreenDevice = false))
        assertNull(selectBottomScreenDisplayId(listOf(main, bottom.copy(state = Display.STATE_OFF)), knownDualScreenDevice = false))
    }

    @Test
    fun knownDualScreenHandheldsAcceptAPresentationFlaggedBottomPanel() {
        val main = BottomScreenCandidate(Display.DEFAULT_DISPLAY, Display.STATE_ON, 0)
        val bottom = BottomScreenCandidate(1, Display.STATE_ON, Display.FLAG_PRESENTATION)
        val dock = BottomScreenCandidate(5, Display.STATE_ON, Display.FLAG_PRESENTATION)

        assertEquals(1, selectBottomScreenDisplayId(listOf(main, dock, bottom), knownDualScreenDevice = true))
        assertTrue(isKnownDualScreenDevice("AYN", "AYN", "Thor", "thor", "thor"))
        assertTrue(isKnownDualScreenDevice("AYANEO", "AYANEO", "AYANEO Pocket DS", "pocketds", "pocketds"))
        assertFalse(isKnownDualScreenDevice("AYN", "AYN", "Odin2", "odin2", "odin2"))
        assertFalse(isKnownDualScreenDevice("Google", "google", "Pixel 9", "tokay", "tokay"))
    }

    @Test
    fun bottomScreenIsOnWithTheStreamDeckByDefault() {
        val settings = AppSettings()

        assertTrue(settings.bottomScreenEnabled)
        assertEquals(BottomScreenPlayMode.StreamDeck, settings.bottomScreenPlayMode)
    }
}

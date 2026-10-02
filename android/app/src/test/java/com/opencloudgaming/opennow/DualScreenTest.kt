package com.opencloudgaming.opennow

import org.junit.Assert.assertEquals
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
    fun gameActionsFollowTheDetailsSheetOnlyWhileBrowsing() {
        val game = GameInfo(id = "elden", title = "Elden Ring")
        val browsing = signedIn.copy(page = AppPage.Library, selectedGame = game)

        assertEquals(game, dualScreenSnapshot(browsing, stats = null).selectedGame)
        assertNull(dualScreenSnapshot(browsing.copy(page = AppPage.Settings), stats = null).selectedGame)
        assertNull(dualScreenSnapshot(browsing.copy(streamStatus = "queue"), stats = null).selectedGame)
    }

    @Test
    fun bottomScreenIsOnWithTheStreamDeckByDefault() {
        val settings = AppSettings()

        assertTrue(settings.bottomScreenEnabled)
        assertEquals(BottomScreenPlayMode.StreamDeck, settings.bottomScreenPlayMode)
    }
}

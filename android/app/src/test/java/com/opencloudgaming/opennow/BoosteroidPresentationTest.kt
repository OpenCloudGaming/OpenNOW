package com.opencloudgaming.opennow

import org.junit.Assert.assertEquals
import org.junit.Assert.assertFalse
import org.junit.Assert.assertNull
import org.junit.Assert.assertSame
import org.junit.Assert.assertTrue
import org.junit.Test

class BoosteroidPresentationTest {
    private val nvidia = SavedAccount(userId = "same-id", displayName = "NVIDIA user")
    private val boosteroid = SavedAccount(
        userId = nvidia.userId,
        displayName = "Boosteroid user",
        providerCode = BOOSTEROID_PROVIDER_CODE,
    )
    private val nvidiaGame = GameInfo(id = "15", title = "NVIDIA game")
    private val boosteroidGame = GameInfo(id = "${BOOSTEROID_GAME_PREFIX}15", launchAppId = "15", title = "Boosteroid game")
    private val base = OpenNowUiState(
        page = AppPage.Settings,
        authSession = AuthSession(defaultProvider(), AuthTokens(accessToken = "nvidia-test-token", expiresAt = 0), AuthUser(nvidia.userId, nvidia.displayName)),
        savedAccounts = listOf(nvidia),
        games = listOf(nvidiaGame),
        libraryGames = listOf(nvidiaGame),
        selectedGame = nvidiaGame,
        error = "NVIDIA test error",
        settings = AppSettings(stream = StreamSettings(resolution = "1280x720")),
    )

    @Test
    fun boosteroidReusesTheCurrentPageWithAnIndependentCatalogAndAccount() {
        val screen = base.withBoosteroidPresentation(
            BoosteroidPresentation(
                active = true,
                account = boosteroid,
                store = listOf(boosteroidGame),
                configuredStreamSettings = StreamSettings(resolution = "1920x1080"),
            ),
        )

        assertEquals(AppPage.Settings, screen.page)
        assertTrue(screen.hasCloudAccount)
        assertEquals(boosteroid, screen.activeCloudAccount)
        assertEquals(listOf(nvidia, boosteroid), screen.savedAccounts)
        assertEquals(listOf(boosteroidGame), screen.games)
        assertTrue(screen.libraryGames.isEmpty())
        assertNull(screen.authSession)
        assertNull(screen.selectedGame)
        assertNull(screen.error)
        assertEquals("1920x1080", screen.settings.stream.resolution)
        assertEquals("1280x720", base.settings.stream.resolution)
        assertFalse(nvidiaGame.isBoosteroidGame)
        assertTrue(boosteroidGame.isBoosteroidGame)
    }

    @Test
    fun inactiveBoosteroidOnlyContributesItsSavedAccount() {
        val screen = base.withBoosteroidPresentation(
            BoosteroidPresentation(account = boosteroid, store = listOf(boosteroidGame), streamStatus = "queue"),
        )

        assertSame(base.authSession, screen.authSession)
        assertEquals(nvidia, screen.activeCloudAccount)
        assertEquals(base.games, screen.games)
        assertEquals(base.libraryGames, screen.libraryGames)
        assertEquals(base.page, screen.page)
        assertEquals(base.error, screen.error)
        assertEquals(listOf(nvidia, boosteroid), screen.savedAccounts)
    }

    @Test
    fun boosteroidCanOpenTheSharedShellWithoutAnNvidiaAccount() {
        val screen = OpenNowUiState(page = AppPage.Library).withBoosteroidPresentation(
            BoosteroidPresentation(active = true, account = boosteroid, streamStatus = "connecting"),
        )

        assertTrue(screen.hasCloudAccount)
        assertEquals(AppPage.Stream, screen.page)
        assertNull(screen.streamSession)
    }
}

package com.opencloudgaming.opennow

import org.junit.Assert.assertEquals
import org.junit.Assert.assertNull
import org.junit.Test

class GameStoreLinkTest {
    @Test
    fun storeDetailsKeepEachGraphQlUrlWithItsVariant() {
        val game = GameInfo(
            id = "game",
            title = "Game",
            availableStores = listOf("STEAM", "XBOX"),
            variants = listOf(
                GameVariant("steam", "STEAM", storeUrl = "https://store.steampowered.com/app/123"),
                GameVariant("xbox", "XBOX", storeUrl = "https://www.xbox.com/games/store/game/abc"),
            ),
        )

        assertEquals(
            listOf(
                GameStoreDetail("Steam", "https://store.steampowered.com/app/123"),
                GameStoreDetail("Xbox", "https://www.xbox.com/games/store/game/abc"),
            ),
            gameStoreDetails(game),
        )
    }

    @Test
    fun storeDetailsStayCombinedWhenGraphQlHasNoLinks() {
        val game = GameInfo(
            id = "game",
            title = "Game",
            availableStores = listOf("STEAM", "XBOX"),
            variants = listOf(GameVariant("steam", "STEAM"), GameVariant("xbox", "XBOX")),
        )

        assertEquals(listOf(GameStoreDetail("Steam, Xbox", null)), gameStoreDetails(game))
    }

    @Test
    fun unknownStoreIsOmittedWhenARealLauncherIsKnown() {
        val game = GameInfo(
            id = "hsr",
            title = "Honkai: Star Rail",
            availableStores = listOf("HOYOVERSE"),
            variants = listOf(GameVariant("unknown", "Unknown"), GameVariant("hoyo", "HOYOVERSE")),
        )

        assertEquals(listOf(GameStoreDetail("HoYoverse", null)), gameStoreDetails(game))
    }

    @Test
    fun externalStoreLinksOnlyAllowHostBackedHttpsUrls() {
        assertEquals("https://store.example/game", validExternalStoreUrl(" https://store.example/game "))
        assertNull(validExternalStoreUrl("http://store.example/game"))
        assertNull(validExternalStoreUrl("javascript:alert(1)"))
        assertNull(validExternalStoreUrl("https:///missing-host"))
    }

    @Test
    fun metadataMergeKeepsTheGraphQlStoreUrl() {
        val catalog = GameInfo(
            id = "game",
            title = "Game",
            variants = listOf(GameVariant("variant", "STEAM")),
        )
        val metadata = catalog.copy(
            variants = listOf(
                GameVariant("variant", "STEAM", storeUrl = "https://store.steampowered.com/app/123"),
            ),
        )

        assertEquals(
            "https://store.steampowered.com/app/123",
            mergeGameInfo(catalog, metadata).variants.single().storeUrl,
        )
    }
}

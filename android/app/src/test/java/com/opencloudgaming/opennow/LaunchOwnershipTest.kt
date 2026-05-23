package com.opencloudgaming.opennow

import org.junit.Assert.assertEquals
import org.junit.Assert.assertFalse
import org.junit.Assert.assertTrue
import org.junit.Test

class LaunchOwnershipTest {
    @Test
    fun ownedStatusesMatchDesktopContract() {
        assertTrue(isOwnedLibraryStatus("MANUAL"))
        assertTrue(isOwnedLibraryStatus("PLATFORM_SYNC"))
        assertTrue(isOwnedLibraryStatus("IN_LIBRARY"))
        assertFalse(isOwnedLibraryStatus("NOT_OWNED"))
        assertFalse(isOwnedLibraryStatus(null))
    }

    @Test
    fun accountLinkedRequiresOwnedVariantOrLibraryGame() {
        val unownedSteam = variant(store = "Steam", libraryStatus = "NOT_OWNED")
        val ownedSteam = variant(store = "Steam", libraryStatus = "PLATFORM_SYNC")
        val unownedEpic = variant(store = "Epic", libraryStatus = "NOT_OWNED")

        assertFalse(shouldLaunchWithAccountLinked(game(listOf(unownedSteam)), unownedSteam))
        assertFalse(shouldLaunchWithAccountLinked(game(listOf(unownedEpic)), unownedEpic))
        assertTrue(shouldLaunchWithAccountLinked(game(listOf(ownedSteam), isInLibrary = true), ownedSteam))
        assertTrue(shouldLaunchWithAccountLinked(game(listOf(unownedSteam, ownedSteam)), unownedSteam))
    }

    @Test
    fun installToPlayDoesNotUseAccountLinkedEvenWhenOwned() {
        val ownedSteam = variant(store = "Steam", libraryStatus = "IN_LIBRARY")

        assertFalse(
            shouldLaunchWithAccountLinked(
                game(listOf(ownedSteam), playType = "INSTALL_TO_PLAY", isInLibrary = true),
                ownedSteam,
            ),
        )
    }

    @Test
    fun launchableVariantsPreferOwnedStoreEntryOverUnownedDuplicate() {
        val unowned = variant(id = "public-steam", store = "Steam", libraryStatus = "NOT_OWNED", librarySelected = true)
        val owned = variant(id = "owned-steam", store = "Steam", libraryStatus = "PLATFORM_SYNC")

        val variants = launchableGameVariants(listOf(unowned, owned))

        assertEquals(listOf("owned-steam"), variants.map { it.id })
    }

    @Test
    fun mergesOwnedCatalogResultsIntoLibrary() {
        val library = game(
            variants = listOf(variant(id = "steam", store = "Steam", libraryStatus = "PLATFORM_SYNC")),
            isInLibrary = true,
        )
        val catalogOnlyOwned = game(
            id = "subnautica-2",
            uuid = "subnautica-2-uuid",
            title = "Subnautica 2",
            variants = listOf(variant(id = "subnautica-steam", store = "Steam", libraryStatus = "IN_LIBRARY")),
            isInLibrary = true,
        )
        val catalogUnowned = game(
            id = "catalog-only",
            uuid = "catalog-only-uuid",
            title = "Catalog Only",
            variants = listOf(variant(id = "catalog-steam", store = "Steam", libraryStatus = "NOT_OWNED")),
        )

        val merged = mergeKnownLibraryGames(listOf(library), listOf(catalogOnlyOwned, catalogUnowned))

        assertEquals(listOf("Game", "Subnautica 2"), merged.map { it.title })
    }

    private fun variant(
        id: String = "variant",
        store: String = "Steam",
        libraryStatus: String? = null,
        librarySelected: Boolean? = null,
    ): GameVariant =
        GameVariant(
            id = id,
            store = store,
            librarySelected = librarySelected,
            libraryStatus = libraryStatus,
        )

    private fun game(
        variants: List<GameVariant>,
        id: String = "game",
        uuid: String? = null,
        title: String = "Game",
        playType: String? = null,
        isInLibrary: Boolean = false,
    ): GameInfo =
        GameInfo(
            id = id,
            uuid = uuid,
            title = title,
            playType = playType,
            isInLibrary = isInLibrary,
            variants = variants,
        )
}

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
    fun explicitNotOwnedVariantIsMarkedBeforeLaunch() {
        val free = variant(libraryStatus = "NOT_OWNED", isFreeToPlay = true)
        val ownedFree = variant(libraryStatus = "MANUAL", isFreeToPlay = true)
        val paid = variant(libraryStatus = "NOT_OWNED")
        val unknown = variant(librarySelected = false)

        assertTrue(shouldMarkVariantOwnedBeforeLaunch(free))
        assertTrue(shouldMarkVariantOwnedBeforeLaunch(paid))
        assertFalse(shouldMarkVariantOwnedBeforeLaunch(ownedFree))
        assertFalse(shouldMarkVariantOwnedBeforeLaunch(unknown))
        assertFalse(shouldMarkVariantOwnedBeforeLaunch(null))
    }

    @Test
    fun markingVariantUpdatesSelectedOwnershipLocally() {
        val game = game(
            variants = listOf(
                variant(id = "ubisoft", store = "Ubisoft", librarySelected = true),
                variant(id = "steam", store = "Steam", isFreeToPlay = true),
            ),
        )

        val marked = game.withManuallyOwnedVariant("steam")

        assertTrue(marked.isInLibrary)
        assertEquals(1, marked.selectedVariantIndex)
        assertEquals(true, marked.variants[0].librarySelected)
        assertEquals(null, marked.variants[1].librarySelected)
        assertEquals("MANUAL", marked.variants[1].libraryStatus)
    }

    @Test
    fun launchableVariantsPreferOwnedStoreEntryOverUnownedDuplicate() {
        val unowned = variant(id = "public-steam", store = "Steam", libraryStatus = "NOT_OWNED", librarySelected = true)
        val owned = variant(id = "owned-steam", store = "Steam", libraryStatus = "PLATFORM_SYNC")

        val variants = launchableGameVariants(listOf(unowned, owned))

        assertEquals(listOf("owned-steam"), variants.map { it.id })
    }

    @Test
    fun publicCatalogMergeKeepsEveryStoreForDuplicateTitles() {
        val catalogUno = game(
            title = "UNO",
            variants = listOf(variant(id = "ubisoft", store = "Ubisoft Connect")),
        )
        val publicUnoSteam = game(
            id = "steam-uno",
            title = "UNO",
            variants = listOf(variant(id = "100236911", store = "Steam")),
        )
        val publicUnoUbisoft = game(
            id = "ubisoft-uno",
            title = "UNO",
            variants = listOf(variant(id = "100932011", store = "Ubisoft Connect")),
        )

        val merged = mergeSupplementalPublicGameVariants(
            games = listOf(catalogUno),
            publicGames = listOf(publicUnoSteam, publicUnoUbisoft),
        ).single()

        assertEquals(listOf("Ubisoft Connect", "Steam"), merged.variants.map { it.store })
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

    @Test
    fun metadataEnrichmentPreservesPanelLibraryOwnership() {
        val panelGame = game(
            variants = listOf(
                variant(
                    id = "steam",
                    store = "Steam",
                    libraryStatus = "MANUAL",
                    librarySelected = true,
                ),
            ),
            isInLibrary = true,
        )
        val metadataGame = game(
            variants = listOf(variant(id = "steam", store = "Steam")),
        ).copy(
            description = "Enriched description",
            genres = listOf("ACTION", "ROLE_PLAYING"),
            imageUrl = "game-box-art",
            screenshotUrls = listOf("screenshot-one", "screenshot-two"),
        )
        val panelWithFallbackArtwork = panelGame.copy(
            imageUrl = "panel-banner-fallback",
            screenshotUrls = listOf("screenshot-one"),
        )

        val merged = mergePanelGameWithMetadata(panelWithFallbackArtwork, metadataGame)

        assertTrue(merged.isInLibrary)
        assertEquals("MANUAL", merged.variants.single().libraryStatus)
        assertEquals(true, merged.variants.single().librarySelected)
        assertEquals("Enriched description", merged.description)
        assertEquals(listOf("ACTION", "ROLE_PLAYING"), merged.genres)
        assertEquals("game-box-art", merged.imageUrl)
        assertEquals(listOf("screenshot-one", "screenshot-two"), merged.screenshotUrls)
    }

    @Test
    fun metadataEnrichmentMergesFieldsForTheSameVariant() {
        val panelGame = game(
            variants = listOf(variant(id = "steam", libraryStatus = "PLATFORM_SYNC")),
            isInLibrary = true,
        )
        val metadataGame = game(
            variants = listOf(
                variant(id = "steam", isFreeToPlay = true).copy(supportedControls = listOf("GAMEPAD")),
            ),
        )

        val merged = mergePanelGameWithMetadata(panelGame, metadataGame)

        assertEquals("PLATFORM_SYNC", merged.variants.single().libraryStatus)
        assertTrue(merged.variants.single().isFreeToPlay)
        assertEquals(listOf("GAMEPAD"), merged.variants.single().supportedControls)
        assertEquals(
            "PLATFORM_SYNC",
            mergeGameInfo(
                metadataGame.copy(variants = listOf(variant(id = "steam", libraryStatus = "NOT_OWNED"))),
                panelGame,
            ).variants.single().libraryStatus,
        )
    }

    @Test
    fun libraryStoreFiltersOnlyUseOwnedStores() {
        val game = game(
            variants = listOf(
                variant(id = "epic", store = "Epic", libraryStatus = "PLATFORM_SYNC"),
                variant(id = "steam", store = "Steam", libraryStatus = "NOT_OWNED"),
                variant(id = "xbox", store = "Xbox"),
            ),
            isInLibrary = true,
        )

        assertEquals(listOf("Epic"), libraryStoreDisplayNames(game))
    }

    @Test
    fun libraryStoreFiltersFallBackToSelectedVariantForLegacyLibraryRows() {
        val game = game(
            variants = listOf(
                variant(id = "steam", store = "Steam"),
                variant(id = "xbox", store = "Xbox", librarySelected = true),
            ),
            isInLibrary = true,
            selectedVariantIndex = 0,
        )

        assertEquals(listOf("Xbox"), libraryStoreDisplayNames(game))
    }

    @Test
    fun unknownLibraryStoreDoesNotBecomeOwnedOnUnknownOrEveryAvailableStore() {
        val game = game(
            variants = listOf(variant(id = "launcher", store = "Unknown", libraryStatus = "PLATFORM_SYNC")),
            isInLibrary = true,
        ).copy(availableStores = listOf("STEAM", "EPIC"))

        assertEquals(emptyList<String>(), libraryStoreDisplayNames(game))
    }

    @Test
    fun explicitlyUnownedVariantIsNotColoredAsAnOwnedStore() {
        val game = game(
            variants = listOf(
                variant(id = "steam", store = "Steam", libraryStatus = "NOT_OWNED", librarySelected = true),
                variant(id = "hoyo", store = "HOYOVERSE", libraryStatus = "PLATFORM_SYNC"),
            ),
            isInLibrary = true,
        )

        assertEquals(listOf("HoYoverse"), libraryStoreDisplayNames(game))
    }

    @Test
    fun detailMetadataHydrationOnlyRunsWhenCatalogGenresAreMissing() {
        val catalogGame = game(
            variants = listOf(variant(libraryStatus = "NOT_OWNED")),
            uuid = "catalog-app",
        )

        assertTrue(shouldHydrateGameDetails(catalogGame))
        assertFalse(shouldHydrateGameDetails(catalogGame.copy(genres = listOf("ACTION"))))
        assertFalse(shouldHydrateGameDetails(catalogGame.copy(uuid = null)))
    }

    private fun variant(
        id: String = "variant",
        store: String = "Steam",
        libraryStatus: String? = null,
        librarySelected: Boolean? = null,
        isFreeToPlay: Boolean = false,
    ): GameVariant =
        GameVariant(
            id = id,
            store = store,
            librarySelected = librarySelected,
            libraryStatus = libraryStatus,
            isFreeToPlay = isFreeToPlay,
        )

    private fun game(
        variants: List<GameVariant>,
        id: String = "game",
        uuid: String? = null,
        title: String = "Game",
        playType: String? = null,
        isInLibrary: Boolean = false,
        selectedVariantIndex: Int = 0,
    ): GameInfo =
        GameInfo(
            id = id,
            uuid = uuid,
            title = title,
            playType = playType,
            isInLibrary = isInLibrary,
            variants = variants,
            selectedVariantIndex = selectedVariantIndex,
        )
}

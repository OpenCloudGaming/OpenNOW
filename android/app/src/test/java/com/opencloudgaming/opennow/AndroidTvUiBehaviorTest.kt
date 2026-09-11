package com.opencloudgaming.opennow

import com.opencloudgaming.opennow.ui.theme.OpenNowPalette
import androidx.compose.ui.graphics.Color
import androidx.compose.ui.input.key.Key
import org.junit.Assert.assertEquals
import org.junit.Assert.assertFalse
import org.junit.Assert.assertNull
import org.junit.Assert.assertTrue
import org.junit.Test

class AndroidTvUiBehaviorTest {
    @Test
    fun profileMenuRemainsOpaqueOverCustomWallpaperAndBonanzaEffects() {
        assertEquals(1f, ProfileMenuContainerColor.alpha, 0f)
        assertEquals(Panel, ProfileMenuContainerColor)
    }

    @Test
    fun chosenWallpaperAlsoBacksSettingsButNeverTheStream() {
        val wallpaper = AppSettings(nerdCatalogBackground = true)

        assertTrue(shouldShowAppWallpaper(AppPage.Home, inStream = false, wallpaper))
        assertTrue(shouldShowAppWallpaper(AppPage.Library, inStream = false, wallpaper))
        assertTrue(shouldShowAppWallpaper(AppPage.Settings, inStream = false, wallpaper))
        assertFalse(shouldShowAppWallpaper(AppPage.Stream, inStream = true, wallpaper))
        assertFalse(shouldShowAppWallpaper(AppPage.Settings, inStream = false, AppSettings()))
    }

    @Test
    fun systemWallpaperIsVisibleOnlyOutsideTheStreamSurface() {
        val systemWallpaper = AppSettings(systemWallpaperBackground = true)

        assertTrue(shouldShowSystemWallpaperBackground(systemWallpaper, inStream = false))
        assertFalse(shouldShowSystemWallpaperBackground(systemWallpaper, inStream = true))
        assertFalse(shouldShowCatalogWallpaper(systemWallpaper))
    }

    @Test
    fun settingsFocusScrollLeavesRoomBelowTheWholeFocusedCard() {
        assertEquals(
            30f,
            settingsFocusScrollDistance(
                itemOffsetPx = 650f,
                itemSizePx = 60f,
                containerSizePx = 720f,
                topClearancePx = 16f,
                bottomClearancePx = 40f,
            ),
            0f,
        )
        assertEquals(
            0f,
            settingsFocusScrollDistance(
                itemOffsetPx = 100f,
                itemSizePx = 60f,
                containerSizePx = 720f,
                topClearancePx = 16f,
                bottomClearancePx = 40f,
            ),
            0f,
        )
    }

    @Test
    fun defaultThemeUsesNvidiaStyleGreenAndWhiteSelectionEnergy() {
        val style = AppSettings().activeSelectionEffectStyle()

        assertEquals(OpenNowPalette.AccentDefault, UiAccent.OpenNow.color)
        assertEquals(OpenNowPalette.AccentDefaultSecondary, UiAccent.OpenNow.secondaryColor)
        assertEquals(Color.White, style.color)
        assertEquals(Color.White, style.secondaryColor)
        assertFalse(style.enabled)
        assertFalse(style.gameCardBordersEnabled)
        assertFalse(style.absoluteCinemaActive)
        assertFalse(style.absoluteCinemaEverywhere)
    }

    @Test
    fun absoluteCinemaRequiresItsToggleAndKeepsTheSelectedAccent() {
        val hotPink = AppSettings(uiAccent = UiAccent.HotPink)
        val cinema = hotPink.copy(absoluteCinemaEffects = true)
        val switch = AppSettings(uiAccent = UiAccent.Switch)
        val hotPinkStyle = hotPink.activeSelectionEffectStyle()
        val cinemaStyle = cinema.activeSelectionEffectStyle()
        val switchStyle = switch.activeSelectionEffectStyle()

        assertEquals(OpenNowPalette.AccentHotPink, hotPink.uiAccent.color)
        assertEquals(OpenNowPalette.AccentHotPink, hotPinkStyle.color)
        assertEquals(OpenNowPalette.AccentHotPink, cinemaStyle.color)
        assertEquals(OpenNowPalette.AccentHotPink, cinemaStyle.secondaryColor)
        assertTrue(cinemaStyle.absoluteCinemaActive)
        assertEquals(OpenNowPalette.AccentHotPink, cinema.uiAccent.color)
        assertEquals(OpenNowPalette.AccentSwitchRed, switchStyle.color)
        assertEquals(OpenNowPalette.AccentSwitchBlue, switchStyle.secondaryColor)
        val absoluteCinemaStyle = AppSettings(uiAccent = UiAccent.AbsoluteCinema)
            .activeSelectionEffectStyle()
        assertEquals(Color.White, absoluteCinemaStyle.color)
        assertEquals(Color.White, absoluteCinemaStyle.secondaryColor)
        assertEquals(Color.White, absoluteCinemaStyle.tintColor)
        assertEquals(OpenNowPalette.AccentDefault, UiAccent.AbsoluteCinema.themeColor)
        assertEquals(OpenNowPalette.AccentDefaultSecondary, UiAccent.AbsoluteCinema.themeSecondaryColor)
    }

    @Test
    fun absoluteCinemaAnimatedEnergyUsesClassicOrangeAndBlueWithoutTintingStaticBorders() {
        val staticStyle = AppSettings(uiAccent = UiAccent.AbsoluteCinema).activeSelectionEffectStyle()
        val (primaryEnergy, secondaryEnergy) = controllerFocusEnergyColors(
            absoluteCinemaPalette = true,
            tint = staticStyle.color,
            secondaryTint = staticStyle.secondaryColor,
        )

        assertEquals(Color.White, staticStyle.color)
        assertEquals(Color.White, staticStyle.secondaryColor)
        assertEquals(OpenNowPalette.AccentCinemaOrange, primaryEnergy)
        assertEquals(OpenNowPalette.AccentCinemaBlue, secondaryEnergy)
    }

    @Test
    fun accentPickerRestoresAbsoluteCinemaWithoutEnablingEffects() {
        val accents = selectableUiAccents()

        assertFalse(UiAccent.LegacyOrange in accents)
        assertTrue(UiAccent.AbsoluteCinema in accents)
        assertEquals(accents.size, accents.distinct().size)
        assertFalse(AppSettings(uiAccent = UiAccent.AbsoluteCinema).absoluteCinemaEffects)
    }

    @Test
    fun selectionBordersOnlyEnableWithAbsoluteCinema() {
        assertFalse(AppSettings().activeSelectionEffectStyle().enabled)
        assertFalse(AppSettings(uiAccent = UiAccent.Pixel).activeSelectionEffectStyle().enabled)
        assertFalse(
            AppSettings(uiAccent = UiAccent.Pixel, liveSelectedOutlines = false)
                .activeSelectionEffectStyle()
                .enabled,
        )
        val absoluteCinema = AppSettings(liveSelectedOutlines = false, absoluteCinemaEffects = true)
        assertTrue(absoluteCinema.activeSelectionEffectStyle().enabled)
    }

    @Test
    fun staticGameBordersAndAnimatedEffectsAreIndependent() {
        val borderOnly = AppSettings(
            liveSelectedOutlines = true,
            absoluteCinemaEffects = false,
        ).activeSelectionEffectStyle()
        val effectsOnly = AppSettings(
            liveSelectedOutlines = false,
            absoluteCinemaEffects = true,
        ).activeSelectionEffectStyle()

        assertTrue(borderOnly.gameCardBordersEnabled)
        assertFalse(borderOnly.absoluteCinemaActive)
        assertFalse(effectsOnly.gameCardBordersEnabled)
        assertTrue(effectsOnly.absoluteCinemaActive)
    }

    @Test
    fun catalogBordersFollowTheIndependentGameBorderToggle() {
        val accent = OpenNowPalette.AccentHotPink

        assertEquals(
            Color.Transparent,
            catalogCardBorderColor(selectionColor = accent, gameBorderEnabled = false),
        )
        assertEquals(
            Color.Transparent,
            cinemaBorderColor(absoluteCinemaEnabled = false, cinemaColor = Color.White),
        )
        assertEquals(
            accent,
            catalogCardBorderColor(selectionColor = accent, gameBorderEnabled = true),
        )
        assertEquals(Color.White, storeHeroBorderColor(gameBorderEnabled = true))
        assertEquals(Color.Transparent, storeHeroBorderColor(gameBorderEnabled = false))
    }

    @Test
    fun controllerFocusKeepsAStaticWhiteGameBorderWhenEffectsAreOff() {
        val accent = OpenNowPalette.AccentHotPink

        assertEquals(
            Color.White,
            catalogCardBorderColor(
                selectionColor = accent,
                gameBorderEnabled = false,
                controllerFocused = true,
                borderEffectsEnabled = false,
            ),
        )
        assertEquals(
            Color.Transparent,
            catalogCardBorderColor(
                selectionColor = accent,
                gameBorderEnabled = false,
                controllerFocused = true,
                borderEffectsEnabled = true,
            ),
        )
        assertEquals(
            Color.White,
            storeHeroBorderColor(
                gameBorderEnabled = false,
                controllerFocused = true,
                borderEffectsEnabled = false,
            ),
        )
    }

    @Test
    fun gameArtworkUsesThinBordersExceptForTheStaticControllerFocusCue() {
        assertEquals(1f, catalogCardBorderWidthDp(), 0f)
        assertEquals(
            3f,
            catalogCardBorderWidthDp(
                controllerFocused = true,
                borderEffectsEnabled = false,
            ),
            0f,
        )
        assertEquals(
            1f,
            catalogCardBorderWidthDp(
                controllerFocused = true,
                borderEffectsEnabled = true,
            ),
            0f,
        )
    }

    @Test
    fun crazyCinemaBroadeningStillRequiresAbsoluteCinema() {
        val crazy = AppSettings(
            liveSelectedOutlines = false,
            absoluteCinemaEffects = true,
            absoluteCinemaEverywhere = true,
        ).activeSelectionEffectStyle()
        val orphanedToggle = AppSettings(
            absoluteCinemaEffects = false,
            absoluteCinemaEverywhere = true,
        ).activeSelectionEffectStyle()

        assertTrue(crazy.enabled)
        assertTrue(crazy.absoluteCinemaActive)
        assertTrue(crazy.absoluteCinemaEverywhere)
        assertFalse(orphanedToggle.absoluteCinemaActive)
        assertFalse(orphanedToggle.absoluteCinemaEverywhere)
    }

    @Test
    fun catalogueWallpaperMakesTheHorizontalRailSubstantiallyDarker() {
        assertEquals(OpenNowPalette.ChromeScrim, navigationRailScrim(darkenForCatalogBackground = false))
        assertEquals(Color.Black.copy(alpha = 0.76f), navigationRailScrim(darkenForCatalogBackground = true))
    }

    @Test
    fun gameDetailsPreferCleanShortDescription() {
        val game = GameInfo(
            id = "fortnite",
            title = "Fortnite",
            description = "Clean short description",
            longDescription = "Provider long description",
        )

        assertEquals("Clean short description", gameDescriptionForDetails(game))
    }

    @Test
    fun restoresTvNavigationFocusOnlyWhenLeavingStream() {
        assertTrue(
            shouldRestoreTvNavigationFocus(
                previouslyInStream = true,
                currentlyInStream = false,
                tvProfile = true,
            ),
        )
        assertFalse(
            shouldRestoreTvNavigationFocus(
                previouslyInStream = true,
                currentlyInStream = false,
                tvProfile = false,
            ),
        )
        assertFalse(
            shouldRestoreTvNavigationFocus(
                previouslyInStream = false,
                currentlyInStream = false,
                tvProfile = true,
            ),
        )
    }

    @Test
    fun catalogWallpaperIsOptInOnBothTvAndMobile() {
        val defaults = AppSettings()

        assertFalse(shouldShowCatalogWallpaper(defaults))
        assertTrue(shouldShowCatalogWallpaper(defaults.copy(nerdCatalogBackground = true)))
    }

    @Test
    fun localAppsProfileActionStaysAvailableAsAnEnablementShortcut() {
        assertTrue(
            shouldShowLocalAppsProfileAction(
                localAppLauncherSupported = true,
                localAppsEnabled = false,
            ),
        )
        assertFalse(
            shouldShowLocalAppsProfileAction(
                localAppLauncherSupported = false,
                localAppsEnabled = true,
            ),
        )
        assertTrue(
            shouldShowLocalAppsProfileAction(
                localAppLauncherSupported = true,
                localAppsEnabled = true,
            ),
        )
    }

    @Test
    fun settingsControllerNavigationIncludesGamingHandheldControls() {
        assertTrue(shouldEnableSettingsControllerNavigation(false, null, gamingHandheld = true))
        assertTrue(
            shouldEnableSettingsControllerNavigation(
                tvProfile = false,
                controllerFamily = AndroidControllerFamily.Generic,
                gamingHandheld = false,
            ),
        )
        assertFalse(shouldEnableSettingsControllerNavigation(false, null, gamingHandheld = false))
    }

    @Test
    fun tvActivationKeysCanBeConsumedAcrossBothKeyPhases() {
        assertTrue(isTvActivationKey(Key.DirectionCenter))
        assertTrue(isTvActivationKey(Key.Enter))
        assertTrue(isTvActivationKey(Key.NumPadEnter))
        assertFalse(isTvActivationKey(Key.ButtonY))
    }

    @Test
    fun mobileCatalogCardsUseDisplaySizedGameBoxArtWithoutTitleOverlay() {
        val gameBoxArt = "https://img.nvidiagrid.net/apps/123/ZZ/GAME_BOX_ART_01_example.jpg"
        val game = GameInfo(
            id = "game",
            title = "Game",
            imageUrl = gameBoxArt,
            tvCardImageUrl = gameBoxArt,
        )

        assertEquals("$gameBoxArt;f=webp;w=512", catalogCardImageUrl(game, tvProfile = false))
        assertFalse(shouldOverlayCatalogCardTitle(tvProfile = false))
    }

    @Test
    fun mobileCatalogCardsRejectStaleTvBannerCacheEntries() {
        val game = GameInfo(
            id = "game",
            title = "Game",
            imageUrl = "https://img.nvidiagrid.net/apps/123/ZZ/TV_BANNER_01_example.jpg",
        )

        assertNull(catalogCardImageUrl(game, tvProfile = false))
    }

    @Test
    fun tvCatalogCardsUseTheSameArtworkAsMobileAtTvRequestSize() {
        val game = GameInfo(
            id = "game",
            title = "Game",
            imageUrl = "https://img.nvidiagrid.net/apps/123/ZZ/GAME_BOX_ART_01_example.jpg",
            tvCardImageUrl = "https://img.nvidiagrid.net/apps/123/ZZ/TV_BANNER_01_example.jpg",
        )

        assertEquals(
            "https://img.nvidiagrid.net/apps/123/ZZ/GAME_BOX_ART_01_example.jpg;f=webp;w=272",
            catalogCardImageUrl(game, tvProfile = true),
        )
        assertFalse(shouldOverlayCatalogCardTitle(tvProfile = true))
    }

    @Test
    fun tvCatalogCardsRetainDedicatedArtworkAsAMissingPosterFallback() {
        val game = GameInfo(
            id = "game",
            title = "Game",
            tvCardImageUrl = "https://img.nvidiagrid.net/apps/123/ZZ/TV_BANNER_01_example.jpg",
        )

        assertEquals(
            "https://img.nvidiagrid.net/apps/123/ZZ/TV_BANNER_01_example.jpg;f=webp;w=272",
            catalogCardImageUrl(game, tvProfile = true),
        )
    }

    @Test
    fun catalogFavoriteIconIsOptInForEveryDeviceLayout() {
        assertFalse(shouldShowCatalogFavoriteIcon(AppSettings()))
        assertTrue(shouldShowCatalogFavoriteIcon(AppSettings(showFavoriteIconOnGameCards = true)))
    }

    @Test
    fun liveSelectionOutlineRequiresBothSelectionAndUserOptIn() {
        assertTrue(shouldShowActiveSelectionOutline(selected = true, enabled = true))
        assertFalse(shouldShowActiveSelectionOutline(selected = false, enabled = true))
        assertFalse(shouldShowActiveSelectionOutline(selected = true, enabled = false))
    }

    @Test
    fun controllerBackMinimizesOnlyPendingStreamLaunches() {
        assertTrue(canMinimizeStreamLaunch(streamStatus = "queue", sessionReady = false))
        assertTrue(canMinimizeStreamLaunch(streamStatus = "connecting", sessionReady = false))
        assertFalse(canMinimizeStreamLaunch(streamStatus = "idle", sessionReady = false))
        assertFalse(canMinimizeStreamLaunch(streamStatus = "connecting", sessionReady = true))
    }

    @Test
    fun activeLogoFloatsBeforeItsQuickFlip() {
        assertEquals(0f, activeLogoSpinProgress(0f), 0f)
        assertEquals(0f, activeLogoSpinProgress(0.30f), 0.0001f)
        assertEquals(0.5f, activeLogoSpinProgress(0.39f), 0.0001f)
        assertEquals(1f, activeLogoSpinProgress(0.48f), 0.0001f)
        assertEquals(1f, activeLogoSpinProgress(0.9f), 0f)
        assertEquals(0f, activeLogoFloatOffsetDp(0f), 0.0001f)
        assertEquals(2.5f, activeLogoFloatOffsetDp(0.25f), 0.0001f)
        assertEquals(-2.5f, activeLogoFloatOffsetDp(0.75f), 0.0001f)
    }

    @Test
    fun tvGameDetailsInitiallyFocusPlayWhileTouchLayoutsKeepArtworkFocus() {
        assertTrue(shouldInitiallyFocusGameDetailsPlay(tvProfile = true))
        assertFalse(shouldInitiallyFocusGameDetailsPlay(tvProfile = false))
    }

    @Test
    fun focusedGameDetailsPlayButtonGetsAnUnmistakableLiftAndEdge() {
        assertEquals(1.06f, gameDetailsPlayFocusScale(focused = true), 0f)
        assertEquals(1f, gameDetailsPlayFocusScale(focused = false), 0f)
        assertEquals(4f, gameDetailsPlayFocusBorderWidthDp(focused = true), 0f)
        assertEquals(0f, gameDetailsPlayFocusBorderWidthDp(focused = false), 0f)
    }

    @Test
    fun whiteButtonFocusTreatmentRequiresTvOrPhysicalController() {
        assertTrue(
            shouldShowControllerFocus(
                focused = true,
                tvProfile = true,
                physicalControllerConnected = false,
            ),
        )
        assertTrue(
            shouldShowControllerFocus(
                focused = true,
                tvProfile = false,
                physicalControllerConnected = true,
            ),
        )
        assertFalse(
            shouldShowControllerFocus(
                focused = true,
                tvProfile = false,
                physicalControllerConnected = false,
            ),
        )
        assertFalse(
            shouldShowControllerFocus(
                focused = false,
                tvProfile = true,
                physicalControllerConnected = true,
            ),
        )
    }

    @Test
    fun controllerCatalogCardsAreArtworkOnly() {
        assertTrue(shouldUseArtworkOnlyCatalogCards(tvProfile = true, controllerActionMode = false))
        assertTrue(shouldUseArtworkOnlyCatalogCards(tvProfile = false, controllerActionMode = true))
        assertFalse(shouldUseArtworkOnlyCatalogCards(tvProfile = false, controllerActionMode = false))
    }

    @Test
    fun tvNeverShowsTouchControlsWhileMobileBehaviorIsPreserved() {
        assertFalse(
            shouldShowAndroidTouchControls(
                tvProfile = true,
                touchInputEnabled = true,
                touchControlsEnabled = true,
                suppressedByPhysicalController = false,
            ),
        )
        assertTrue(
            shouldShowAndroidTouchControls(
                tvProfile = false,
                touchInputEnabled = true,
                touchControlsEnabled = true,
                suppressedByPhysicalController = false,
            ),
        )
        assertFalse(
            shouldShowAndroidTouchControls(
                tvProfile = false,
                touchInputEnabled = true,
                touchControlsEnabled = true,
                suppressedByPhysicalController = true,
            ),
        )
        assertFalse(
            shouldShowAndroidTouchControls(
                tvProfile = false,
                touchInputEnabled = true,
                touchControlsEnabled = true,
                suppressedByPhysicalController = false,
                physicalMouseConnected = true,
            ),
        )
        assertTrue(
            shouldShowAndroidTouchControls(
                tvProfile = false,
                touchInputEnabled = true,
                touchControlsEnabled = true,
                suppressedByPhysicalController = false,
                physicalMouseConnected = true,
                allowWithPhysicalMouse = true,
            ),
        )
    }

    @Test
    fun allAndroidDevicesUseStableAudioBuffering() {
        assertFalse(shouldUseLowLatencyStreamAudio(androidTvProfile = true))
        assertFalse(shouldUseLowLatencyStreamAudio(androidTvProfile = false))
    }

    @Test
    fun tvSafeAreaStartsInset() {
        assertEquals(16f, AppSettings().tvSafeAreaPaddingDp, 0f)
    }

    @Test
    fun screenEdgePaddingOnlyAppliesToTvOutsideStream() {
        val settings = AppSettings(tvSafeAreaPaddingDp = 20f)

        assertEquals(20f, appContentEdgePaddingDp(settings, inStream = false, tvProfile = true), 0f)
        assertEquals(0f, appContentEdgePaddingDp(settings, inStream = true, tvProfile = true), 0f)
        assertEquals(0f, appContentEdgePaddingDp(settings, inStream = false, tvProfile = false), 0f)
    }

    @Test
    fun gameCardScaleChangesStoreRailWidthContinuously() {
        assertEquals(72f, scaledCatalogCardWidthDp(96f, 0.75f), 0f)
        assertEquals(96f, scaledCatalogCardWidthDp(96f, 1f), 0f)
        assertEquals(134.4f, scaledCatalogCardWidthDp(96f, 1.4f), 0.001f)
    }

    @Test
    fun storeRailSkeletonOnlyIncludesWholeCards() {
        assertEquals(3, storeRailVisibleCardCount(360f, 96f, 10f))
        assertEquals(2, storeRailVisibleCardCount(360f, 140f, 10f))
        assertEquals(3, storeRailVisibleCardCount(308f, 96f, 10f))
    }

    @Test
    fun storeSearchAndFiltersHideDiscoverySections() {
        assertTrue(shouldShowStoreDiscoverySections(searchActive = false, filterActive = false))
        assertFalse(shouldShowStoreDiscoverySections(searchActive = true, filterActive = false))
        assertFalse(shouldShowStoreDiscoverySections(searchActive = false, filterActive = true))
        assertFalse(shouldShowStoreDiscoverySections(searchActive = true, filterActive = true))
    }

    @Test
    fun unresolvedCatalogQueryReplacesOldCardsWithShimmerButCachedResultsStayVisible() {
        assertTrue(
            shouldShowCatalogLoadingPlaceholder(
                queryLoading = true,
                loadingGames = true,
                hasVisibleGames = true,
            ),
        )
        assertFalse(
            shouldShowCatalogLoadingPlaceholder(
                queryLoading = false,
                loadingGames = false,
                hasVisibleGames = true,
            ),
        )
        assertFalse(
            shouldShowCatalogLoadingPlaceholder(
                queryLoading = false,
                loadingGames = true,
                hasVisibleGames = true,
            ),
        )
        assertTrue(
            shouldShowCatalogLoadingPlaceholder(
                queryLoading = false,
                loadingGames = true,
                hasVisibleGames = false,
            ),
        )
    }

    @Test
    fun storeKeepsTopControlsMountedWhileControllerIsConnected() {
        assertTrue(
            shouldHideStoreChromeOnScroll(
                hideChromeWhenScrolled = true,
                scrolledAwayFromTop = true,
                physicalControllerConnected = false,
            ),
        )
        assertFalse(
            shouldHideStoreChromeOnScroll(
                hideChromeWhenScrolled = true,
                scrolledAwayFromTop = true,
                physicalControllerConnected = true,
            ),
        )
    }

    @Test
    fun catalogCardTitlesAreCaptionedOnTouchHandheldsOnly() {
        assertTrue(shouldShowCatalogCardTitles(tvProfile = false, enabled = true))
        assertFalse(shouldShowCatalogCardTitles(tvProfile = true, enabled = true))
        assertFalse(shouldShowCatalogCardTitles(tvProfile = false, enabled = false))
        assertFalse(shouldOverlayCatalogCardTitle(tvProfile = true))
    }

    @Test
    fun localTvRemoteRequiresExplicitOptIn() {
        assertFalse(AppSettings().localTvRemoteEnabled)
    }

    @Test
    fun localTvConnectionDotNeverLeaksIntoMobileChrome() {
        assertTrue(shouldShowLocalTvConnectionDot(tvProfile = true, pairedDeviceName = "Pixel"))
        assertFalse(shouldShowLocalTvConnectionDot(tvProfile = false, pairedDeviceName = "Pixel"))
        assertFalse(shouldShowLocalTvConnectionDot(tvProfile = true, pairedDeviceName = null))
    }

    @Test
    fun appIconGlideHonorsMotionPermissionAndKeepsTvChromeAlive() {
        val capableReport = RuntimeCodecReport(
            capabilities = emptyList(),
            nativeRuntimeSummary = "",
            androidTvProfile = false,
            lowPowerGpuProfile = false,
            constrainedRuntimeProfile = false,
        )
        assertTrue(shouldAnimateOpenNowAppIcon(capableReport, reduceMotion = false, absoluteCinemaEnabled = true))
        assertFalse(shouldAnimateOpenNowAppIcon(null, reduceMotion = false, absoluteCinemaEnabled = true))
        assertFalse(shouldAnimateOpenNowAppIcon(capableReport, reduceMotion = true, absoluteCinemaEnabled = true))
        assertFalse(shouldAnimateOpenNowAppIcon(capableReport, reduceMotion = false, absoluteCinemaEnabled = false))
        assertTrue(
            shouldAnimateOpenNowAppIcon(
                codecReport = null,
                reduceMotion = false,
                absoluteCinemaEnabled = false,
                androidTvProfile = true,
            ),
        )
        assertFalse(
            shouldAnimateOpenNowAppIcon(
                codecReport = null,
                reduceMotion = true,
                absoluteCinemaEnabled = true,
                androidTvProfile = true,
            ),
        )
        assertFalse(
            shouldAnimateOpenNowAppIcon(
                capableReport.copy(lowPowerGpuProfile = true),
                reduceMotion = false,
                absoluteCinemaEnabled = true,
            ),
        )
        assertFalse(
            shouldAnimateOpenNowAppIcon(
                capableReport.copy(constrainedRuntimeProfile = true),
                reduceMotion = false,
                absoluteCinemaEnabled = true,
            ),
        )
        assertTrue(shouldAnimateControllerFocusFrame(absoluteCinemaEnabled = true, reduceMotion = false))
        assertFalse(shouldAnimateControllerFocusFrame(absoluteCinemaEnabled = false, reduceMotion = false))
        assertFalse(shouldAnimateControllerFocusFrame(absoluteCinemaEnabled = true, reduceMotion = true))
    }

    @Test
    fun tvKeepsTopChromeAndRefreshAvailableOutsideTheStream() {
        assertTrue(
            shouldShowTopStatusBar(
                inStream = false,
                portraitChrome = false,
                phoneLandscapeChrome = false,
                phoneLandscapeScrollChromeHidden = false,
                tvProfile = true,
            ),
        )
        assertFalse(
            shouldShowTopStatusBar(
                inStream = true,
                portraitChrome = false,
                phoneLandscapeChrome = false,
                phoneLandscapeScrollChromeHidden = false,
                tvProfile = true,
            ),
        )
        assertTrue(shouldShowTvRefreshAction(tvProfile = true, inStream = false))
        assertFalse(shouldShowTvRefreshAction(tvProfile = true, inStream = true))
        assertFalse(shouldShowTvRefreshAction(tvProfile = false, inStream = false))
    }

    @Test
    fun tvControllerShortcutsRequireAPhysicalController() {
        assertTrue(
            catalogControllerActionMode(
                tvProfile = true,
                landscapeLayout = true,
                physicalControllerConnected = true,
            ),
        )
        assertFalse(
            catalogControllerActionMode(
                tvProfile = true,
                landscapeLayout = true,
                physicalControllerConnected = false,
            ),
        )
        assertFalse(
            catalogControllerActionMode(
                tvProfile = false,
                landscapeLayout = false,
                physicalControllerConnected = true,
            ),
        )
    }

    @Test
    fun tvSettingsNeverAddsASecondBackItemToTheRail() {
        assertFalse(
            shouldShowSettingsBackRail(
                tvProfile = true,
                settingsPageOpen = true,
                horizontalChrome = true,
                detailRouteOpen = true,
            ),
        )
        assertTrue(
            shouldShowSettingsBackRail(
                tvProfile = false,
                settingsPageOpen = true,
                horizontalChrome = true,
                detailRouteOpen = true,
            ),
        )
    }

    @Test
    fun batteryOptimizationIsHiddenOnlyWhenNoBatteryIsConfirmed() {
        assertFalse(shouldShowBatteryOptimization(explicitBatteryPresent = false))
        assertTrue(shouldShowBatteryOptimization(explicitBatteryPresent = true))
        assertTrue(shouldShowBatteryOptimization(explicitBatteryPresent = null))
    }

    @Test
    fun compactTvPairingHidesMainLoginContentOnlyWhenNeeded() {
        assertTrue(shouldUseDedicatedTvPairingLayout(true, true, 640f, 360f))
        assertTrue(shouldUseDedicatedTvPairingLayout(true, true, 960f, 480f))
        assertFalse(shouldUseDedicatedTvPairingLayout(true, true, 960f, 540f))
        assertFalse(shouldUseDedicatedTvPairingLayout(false, true, 400f, 720f))
        assertFalse(shouldUseDedicatedTvPairingLayout(true, false, 640f, 360f))
    }
}

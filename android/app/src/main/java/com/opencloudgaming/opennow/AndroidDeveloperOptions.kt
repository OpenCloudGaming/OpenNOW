package com.opencloudgaming.opennow

/**
 * Developer options: unlocking them, and the state each action resets.
 *
 * The Compose layer owns presentation only. Everything that decides *when* the section appears and
 * *what* an action writes back to [AppSettings] lives here so it can be unit tested without a
 * device, the same split `AndroidSetupFlow.kt` uses for first-run setup.
 *
 * Nothing here unlocks paid entitlements, bypasses the provider, or changes what OpenNOW reports
 * about the user. Every action either resets local state the user already controls elsewhere or
 * shows information already available in the diagnostics export.
 */

/** Taps on the build row needed to reveal developer options. Matches the Android platform gesture. */
internal const val DEVELOPER_OPTIONS_TAP_COUNT = 10

/**
 * Taps are only counted down out loud once the user is clearly not just double-tapping the row.
 * Android shows nothing for the first few, then counts the rest in — copying that avoids telling
 * every user who taps a version number twice that a hidden menu exists.
 */
internal const val DEVELOPER_OPTIONS_TAP_COUNTDOWN_FROM = 7

internal sealed interface DeveloperOptionsTapResult {
    /** Not far enough along to say anything. */
    data object Silent : DeveloperOptionsTapResult

    /** [remaining] more taps to go, and the user should be told. */
    data class Countdown(val remaining: Int) : DeveloperOptionsTapResult

    /** This tap crossed the threshold. */
    data object Unlocked : DeveloperOptionsTapResult

    /** Already unlocked before this tap. */
    data object AlreadyUnlocked : DeveloperOptionsTapResult
}

/**
 * Resolves one tap on the build row.
 *
 * [tapCount] is the running total *including* this tap.
 */
internal fun developerOptionsTapResult(
    tapCount: Int,
    alreadyUnlocked: Boolean,
): DeveloperOptionsTapResult {
    if (alreadyUnlocked) return DeveloperOptionsTapResult.AlreadyUnlocked
    if (tapCount >= DEVELOPER_OPTIONS_TAP_COUNT) return DeveloperOptionsTapResult.Unlocked
    val remaining = DEVELOPER_OPTIONS_TAP_COUNT - tapCount
    return if (tapCount >= DEVELOPER_OPTIONS_TAP_COUNTDOWN_FROM) {
        DeveloperOptionsTapResult.Countdown(remaining)
    } else {
        DeveloperOptionsTapResult.Silent
    }
}

internal fun AppSettings.unlockingDeveloperOptions(): AppSettings =
    if (developerOptionsUnlocked) this else copy(developerOptionsUnlocked = true)

internal fun AppSettings.lockingDeveloperOptions(): AppSettings =
    if (!developerOptionsUnlocked) this else copy(developerOptionsUnlocked = false)

// ---------------------------------------------------------------------------
// Flow resets
// ---------------------------------------------------------------------------

/** Shows the stream guide again on the next launch. */
internal fun AppSettings.resettingStreamGuide(): AppSettings =
    copy(androidStreamGuideDismissed = false)

/** Shows the "connect a controller" prompt again. */
internal fun AppSettings.resettingControllerPrompt(): AppSettings =
    copy(androidPhysicalControllerPromptDismissed = false)

/**
 * Replays the one-time migrations that run on upgrade.
 *
 * These versions gate the presentation and TV-layout defaults applied once per install, so zeroing
 * them is how a developer re-tests an upgrade path without wiping the whole profile.
 */
internal fun AppSettings.resettingProfileMigrations(): AppSettings =
    copy(streamPresentationProfileVersion = 0, tvLayoutProfileVersion = 0)

// ---------------------------------------------------------------------------
// Catalogue resets
// ---------------------------------------------------------------------------

/** Store and Library back to their default ordering with every filter cleared. */
internal fun AppSettings.resettingCatalogBrowsing(): AppSettings =
    copy(
        catalogSortId = AppSettings().catalogSortId,
        catalogFilterIds = emptyList(),
        librarySortId = AppSettings().librarySortId,
        libraryFilterIds = emptyList(),
    )

internal fun AppSettings.clearingFavorites(): AppSettings = copy(favoriteGameIds = emptyList())

/** Drops every remembered "always launch this game from this store" choice. */
internal fun AppSettings.clearingStorePreferences(): AppSettings = copy(defaultGameVariantIds = emptyMap())

internal fun AppSettings.clearingLocalAppShelf(): AppSettings = copy(localAppPackageNames = emptyList())

// ---------------------------------------------------------------------------
// Interface and input resets
// ---------------------------------------------------------------------------

/** Every appearance choice back to the shipped defaults, leaving account and stream alone. */
internal fun AppSettings.resettingInterface(): AppSettings {
    val defaults = AppSettings()
    return copy(
        uiAccent = defaults.uiAccent,
        dynamicColor = defaults.dynamicColor,
        expressiveUi = defaults.expressiveUi,
        absoluteCinemaEffects = defaults.absoluteCinemaEffects,
        absoluteCinemaEverywhere = defaults.absoluteCinemaEverywhere,
        liveSelectedOutlines = defaults.liveSelectedOutlines,
        controllerBackgroundAnimations = defaults.controllerBackgroundAnimations,
        nerdCatalogBackground = defaults.nerdCatalogBackground,
        ambientBackgroundEnabled = defaults.ambientBackgroundEnabled,
        systemWallpaperBackground = defaults.systemWallpaperBackground,
        catalogBackgroundPreset = defaults.catalogBackgroundPreset,
        nerdCatalogBackgroundUri = null,
        compactGameCards = defaults.compactGameCards,
        showCardTitles = defaults.showCardTitles,
        showFavoriteIconOnGameCards = defaults.showFavoriteIconOnGameCards,
        posterSizeScale = defaults.posterSizeScale,
        launchPage = defaults.launchPage,
        tvSafeAreaPaddingDp = defaults.tvSafeAreaPaddingDp,
    )
}

/** Touch overlay geometry back to the shipped layout, keeping the rest of the input settings. */
internal fun AppSettings.resettingTouchLayout(): AppSettings =
    copy(androidTouch = androidTouch.withResetOffsets())

/**
 * Every state a first-run install starts from, short of signing out.
 *
 * Used by the single "replay first launch" action so a developer does not have to press eight
 * separate resets to reproduce what a new user sees.
 */
internal fun AppSettings.replayingFirstLaunch(): AppSettings =
    restartingSetupFlow()
        .resettingStreamGuide()
        .resettingControllerPrompt()
        .resettingProfileMigrations()
        .resettingCatalogBrowsing()

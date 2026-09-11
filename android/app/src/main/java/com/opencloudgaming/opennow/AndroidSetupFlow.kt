package com.opencloudgaming.opennow

/**
 * First-run setup: the ordering, gating, and settings writes behind the intro screens.
 *
 * The Compose layer in `OpenNowSetupScreens.kt` owns presentation only. Everything that decides
 * *whether* the flow runs, *which* step comes next, and *what* finishing writes to [AppSettings]
 * lives here so it can be unit tested without a device.
 */

/**
 * Bump when a step is added that existing installs should be shown. Installs whose
 * [AppSettings.setupFlowCompletedVersion] is lower run the flow again.
 */
internal const val SETUP_FLOW_VERSION = 2

internal enum class SetupStep {
    /** What OpenNOW is, and what the next few screens will ask. */
    Welcome,

    /** Catalog backdrop and accent, previewed live. */
    Appearance,

    /** Quality preset for this specific device. */
    Streaming,

    /** Touch-mouse behavior and the small status line shown over a stream. */
    Play,

    /** Recap of the choices, and where to change them later. */
    Ready,
}

internal enum class SetupStreamingChoice {
    /** Whatever `recommendedAndroidStreamProfile` measured for this device. */
    Recommended,

    /** The highest profile the membership allows, whatever the device measured. */
    Best,

    /** 720p30 at 12 Mbps — mobile data, hotel Wi-Fi, capped connections. */
    DataSaver,

    /** Resolution, frame rate, and bitrate set by hand, on this screen. */
    Custom,
}

internal enum class SetupTouchMouseChoice {
    /** A tap moves the cursor to that point and clicks it in one gesture. */
    Direct,

    /** The screen behaves like a laptop trackpad: swipe to move, then tap to click. */
    Trackpad,

    /** Finger input does not drive the host cursor. */
    Off,
}

internal fun setupSteps(): List<SetupStep> = SetupStep.entries.toList()

internal fun shouldShowSetupFlow(settings: AppSettings): Boolean =
    settings.setupFlowCompletedVersion < SETUP_FLOW_VERSION

internal fun setupStepIndex(step: SetupStep): Int = setupSteps().indexOf(step)

internal fun setupStepAfter(step: SetupStep): SetupStep? =
    setupSteps().getOrNull(setupStepIndex(step) + 1)

internal fun setupStepBefore(step: SetupStep): SetupStep? =
    setupSteps().getOrNull(setupStepIndex(step) - 1)

internal fun isFinalSetupStep(step: SetupStep): Boolean = setupStepAfter(step) == null

/** Marks setup as done without changing any choices. */
internal fun AppSettings.completingSetupFlow(): AppSettings =
    copy(setupFlowCompletedVersion = SETUP_FLOW_VERSION)

/** Sends the user back through setup from Settings without touching any of their choices. */
internal fun AppSettings.restartingSetupFlow(): AppSettings = copy(setupFlowCompletedVersion = 0)

internal fun setupStreamingChoiceFor(settings: AppSettings): SetupStreamingChoice =
    when (settings.streamPreset) {
        StreamPreset.Recommended -> SetupStreamingChoice.Recommended
        StreamPreset.High -> SetupStreamingChoice.Best
        StreamPreset.LowDataSaver -> SetupStreamingChoice.DataSaver
        StreamPreset.Custom,
        StreamPreset.Medium,
        -> SetupStreamingChoice.Custom
    }

/**
 * The preset a streaming choice writes.
 *
 * Every choice now writes one, including [SetupStreamingChoice.Custom]. Custom used to leave the
 * settings untouched because the user had to go and find Settings > Stream; the step edits the
 * profile in place instead, so selecting it has to put the app into the custom preset for those
 * edits to survive.
 */
internal fun setupStreamingPresetFor(choice: SetupStreamingChoice): StreamPreset = when (choice) {
    SetupStreamingChoice.Recommended -> StreamPreset.Recommended
    SetupStreamingChoice.Best -> StreamPreset.High
    SetupStreamingChoice.DataSaver -> StreamPreset.LowDataSaver
    SetupStreamingChoice.Custom -> StreamPreset.Custom
}

/**
 * Whether the step should expose the resolution/FPS/bitrate controls under the choices.
 *
 * Only for [SetupStreamingChoice.Custom]: showing live controls beside a preset would let the user
 * edit values the next preset write silently discards.
 */
internal fun setupStreamingCustomControlsVisible(choice: SetupStreamingChoice): Boolean =
    choice == SetupStreamingChoice.Custom

internal fun setupTouchMouseChoiceFor(settings: AppSettings): SetupTouchMouseChoice = when {
    !settings.androidTouch.mousePad -> SetupTouchMouseChoice.Off
    settings.androidTouch.mouseDirectClick -> SetupTouchMouseChoice.Direct
    else -> SetupTouchMouseChoice.Trackpad
}

/** Writes both persisted switches together so setup cannot leave an impossible half-selected mode. */
internal fun AppSettings.withSetupTouchMouseChoice(choice: SetupTouchMouseChoice): AppSettings =
    copy(
        androidTouch = androidTouch.copy(
            mousePad = choice != SetupTouchMouseChoice.Off,
            mouseDirectClick = choice == SetupTouchMouseChoice.Direct,
        ),
    )

internal enum class AppBackgroundChoice {
    Default,
    Nothing,
    Wallpaper,
    SystemWallpaper,
}

internal fun appBackgroundChoiceFor(settings: AppSettings): AppBackgroundChoice = when {
    settings.systemWallpaperBackground -> AppBackgroundChoice.SystemWallpaper
    settings.nerdCatalogBackground -> AppBackgroundChoice.Wallpaper
    settings.ambientBackgroundEnabled -> AppBackgroundChoice.Default
    else -> AppBackgroundChoice.Nothing
}

internal fun AppSettings.withAppBackgroundChoice(choice: AppBackgroundChoice): AppSettings =
    when (choice) {
        AppBackgroundChoice.Default -> copy(
            nerdCatalogBackground = false,
            ambientBackgroundEnabled = true,
            systemWallpaperBackground = false,
        )
        AppBackgroundChoice.Nothing -> copy(
            nerdCatalogBackground = false,
            ambientBackgroundEnabled = false,
            systemWallpaperBackground = false,
        )
        AppBackgroundChoice.Wallpaper -> copy(
            nerdCatalogBackground = true,
            systemWallpaperBackground = false,
        )
        AppBackgroundChoice.SystemWallpaper -> copy(
            nerdCatalogBackground = false,
            ambientBackgroundEnabled = false,
            systemWallpaperBackground = true,
        )
    }

/** System wallpaper is an app-shell treatment; a stream surface must always stay opaque. */
internal fun shouldShowSystemWallpaperBackground(settings: AppSettings, inStream: Boolean): Boolean =
    settings.systemWallpaperBackground && !inStream

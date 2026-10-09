package com.opencloudgaming.opennow

/**
 * Master settings for a new Android installation. Edit [commonFirstInstallSettings] for both
 * distributions, or the two profiles at the bottom for one distribution only.
 *
 * These values are read only when no settings have been saved. AppSettings and StreamSettings still
 * provide serialization defaults for older saved files that do not contain newer fields; changing
 * those model defaults would silently change existing users' settings.
 *
 * The types are intentional: the compiler catches renamed fields and invalid enum choices.
 * Fields not listed here retain the constructor defaults in AppSettings, StreamSettings, and
 * AndroidTouchSettings. Add any of those fields here when a fresh-install override is needed.
 */
internal fun firstInstallSettings(playStoreRelease: Boolean): AppSettings =
    if (playStoreRelease) playStoreFirstInstallSettings() else apkFirstInstallSettings()

// Also resolves "Default" for older saved settings which have no video-output choice yet.
internal fun defaultStreamVideoOutput(playStoreRelease: Boolean): StreamVideoOutput =
    if (playStoreRelease) PLAY_STORE_VIDEO_OUTPUT else APK_VIDEO_OUTPUT

private val APK_VIDEO_OUTPUT = StreamVideoOutput.MediaCodecSurface
private val PLAY_STORE_VIDEO_OUTPUT = StreamVideoOutput.WebRtcTexture

private fun commonFirstInstallSettings(): AppSettings = AppSettings(
    stream = StreamSettings(
        resolution = "1920x1080", // Examples: "1280x720", "1920x1080", "2560x1440".
        aspectRatio = "16:9", // Use an aspect ratio offered by the Stream settings screen.
        fps = 60, // Normalization accepts 30..360; device/provider limits still apply.
        maxBitrateMbps = 75, // 1..150 after normalization.
        codec = VideoCodec.H264, // H264, H265, AV1.
        colorQuality = ColorQuality.TenBit420, // EightBit420, EightBit444, TenBit420, TenBit444.
        hdrEnabled = false,
        keyboardLayout = "en-US", // Use an offered keyboard-layout code, such as "en-GB".
        gameLanguage = "en_US", // Use an offered game-language code, such as "en_GB".
        enableL4S = false,
        experimentalNvst = false,
        microphoneMode = MicrophoneMode.Disabled, // Disabled, PushToTalk, VoiceActivity.
        recordingBitrateMbps = 0, // 0 = automatic; explicit values normalize to 2..50 Mbps.
        recordingFps = 0, // 0 = source; explicit choices: 24, 30, 60.
    ),
    streamPreset = StreamPreset.Recommended, // Recommended, Custom, LowDataSaver, Medium, High.
    launchPage = AppLaunchPage.Store, // Store or Library.
    catalogBackgroundPreset = CatalogBackgroundPreset.ColorfulAbstract, // ColorfulAbstract, Original, AbsoluteCinema.
    ambientBackgroundEnabled = true,
    systemWallpaperBackground = false, // If true, catalog and ambient backgrounds are disabled.
    uiAccent = UiAccent.OpenNow, // OpenNow, Pixel, HotPink, Lime, Coral, Violet, AbsoluteCinema, Switch.
    dynamicColor = false,
    expressiveUi = true,
    posterSizeScale = 1f, // 0.75..1.4 after normalization.
    compactGameCards = false,
    showCardTitles = false,
    showFavoriteIconOnGameCards = false,
    showPremiumMarker = true,
    landscapeNewGamesHero = true,
    liveSelectedOutlines = false,
    absoluteCinemaEffects = false,
    absoluteCinemaEverywhere = false, // Requires absoluteCinemaEffects.
    localAppsEnabled = false,
    hideServerSelector = false,
    showStatsOnLaunch = true,
    streamStatsStyle = StreamStatsStyle.Compact, // Compact or Detailed.
    streamStatsPosition = StreamStatsPosition.Right, // Left, Center, Right.
    streamStatsBackgroundEnabled = true,
    streamStatsBackgroundOpacity = DEFAULT_STREAM_STATS_BACKGROUND_OPACITY, // 0..1.
    hideStreamButtons = false,
    stretchStreamToFit = false,
    controllerUiSounds = true,
    vibrationEnabled = true,
    hapticsOutput = HapticsOutputPreference.Auto, // Auto, Controller, Device.
    externalMousePointerLock = true,
    streamMenuShortcut = DEFAULT_ANDROID_STREAM_MENU_SHORTCUT, // Physical-keyboard shortcut string.
    streamIntroMusic = false,
    streamIntroStartMode = IntroMusicStartMode.Muted, // Muted or Playing.
    queueReadyMusic = false,
    showSessionReportAfterStream = true,
    clipboardPaste = true,
    nativeLowLatencyDecoder = false,
    lowLatencyGameAudio = true,
    androidTouch = AndroidTouchSettings(
        enabled = true,
        mousePad = true,
        keyboardModeEnabled = false,
        touchControllerStyle = TouchControllerStyle.V1, // V1, V2, Neon, Frost, Contrast, Retro, Arcade.
        opacity = 0.82f, // 0..1.
        scale = 1f,
        joystickMode = TouchJoystickMode.Fixed, // Fixed or Dynamic.
        aimMode = TouchAimMode.LockJoystick, // LockJoystick or LockZone.
        joystickDeadZone = DEFAULT_TOUCH_JOYSTICK_DEAD_ZONE, // Fraction, 0..1.
        nativeTouchMode = NativeTouchMode.Auto, // Auto, Off, Always.
        gyroscopeEnabled = false,
    ),
    // Mark these values as intentional fresh-install choices so old-settings migrations do not
    // overwrite this file's choices (for example, session report, borders, and NVST).
    nvstOptInVersion = NVST_OPT_IN_VERSION,
    streamPresentationProfileVersion = STREAM_PRESENTATION_PROFILE_VERSION,
    sessionReportDefaultVersion = SESSION_REPORT_DEFAULT_VERSION,
    gameBordersDefaultVersion = GAME_BORDERS_DEFAULT_VERSION,
    catalogSortDefaultVersion = CATALOG_SORT_DEFAULT_VERSION,
    touchJoystickDefaultVersion = 1,
)

// Distribution-specific first-install choices; existing saved choices are preserved.
private fun apkFirstInstallSettings(): AppSettings = commonFirstInstallSettings().let { common ->
    common.copy(
        stream = common.stream.copy(
            videoOutput = APK_VIDEO_OUTPUT, // Direct MediaCodec SurfaceView; unsupported features use textures.
            colorQuality = ColorQuality.EightBit420, // Direct SDR output requires an eight-bit stream.
        ),
        autoCheckForUpdates = true, // true checks the APK update source; false leaves checks manual.
        localAppsEnabled = false, // APK builds can enable the local-app shelf here.
    )
}

private fun playStoreFirstInstallSettings(): AppSettings = commonFirstInstallSettings().let { common ->
    common.copy(
        stream = common.stream.copy(videoOutput = PLAY_STORE_VIDEO_OUTPUT), // WebRTC MediaCodec + EGL textures.
        autoCheckForUpdates = true, // true checks Google Play; APK downloads stay disabled by build policy.
        localAppsEnabled = false, // Play builds do not ship the local-app launcher.
    )
}

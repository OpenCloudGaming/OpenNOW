package com.opencloudgaming.opennow

import kotlinx.serialization.SerialName
import kotlinx.serialization.Serializable
import androidx.compose.runtime.Immutable
import java.util.Locale
import kotlin.math.abs
import kotlin.math.max
import kotlin.math.roundToInt


// Hoisted out of the per-game/per-store helpers below: these run across the whole catalogue, and
// compiling the pattern on each call was measurable on low-end devices.
private val NON_ALNUM_UPPER_RUN = Regex("[^A-Z0-9]+")
private val STORE_SEPARATOR_RUN = Regex("[\\s-]+")
private val WHITESPACE_RUN = Regex("\\s+")

@Serializable
enum class VideoCodec {
    H264,
    H265,
    AV1,
}

@Serializable
enum class ColorQuality {
    @kotlinx.serialization.SerialName("8bit_420")
    EightBit420,

    @kotlinx.serialization.SerialName("8bit_444")
    EightBit444,

    @kotlinx.serialization.SerialName("10bit_420")
    TenBit420,

    @kotlinx.serialization.SerialName("10bit_444")
    TenBit444,
}

@Serializable
enum class StreamPreset {
    @kotlinx.serialization.SerialName("recommended")
    Recommended,

    @kotlinx.serialization.SerialName("custom")
    Custom,

    @kotlinx.serialization.SerialName("low_data_saver")
    LowDataSaver,

    @kotlinx.serialization.SerialName("medium")
    Medium,

    @kotlinx.serialization.SerialName("high")
    High,
}

@Serializable
enum class MicrophoneMode {
    @kotlinx.serialization.SerialName("disabled")
    Disabled,

    @kotlinx.serialization.SerialName("push-to-talk")
    PushToTalk,

    @kotlinx.serialization.SerialName("voice-activity")
    VoiceActivity,
}

@Serializable
enum class UiAccent {
    OpenNow,
    Pixel,
    HotPink,
    Lime,
    Coral,
    Violet,
    /** Reads the removed Orange preference so existing settings can migrate without being reset. */
    @SerialName("Orange")
    LegacyOrange,
    /** Magenta/blue Absolute Cinema accent, selectable independently from its optional effects. */
    AbsoluteCinema,
    Switch,
}

@Serializable
enum class StreamStatsStyle {
    Compact,
    Detailed,
}

@Serializable
enum class StreamStatsPosition {
    Left,
    Center,
    Right,
}

@Serializable
enum class CatalogBackgroundPreset {
    @kotlinx.serialization.SerialName("colorful-abstract")
    ColorfulAbstract,

    @kotlinx.serialization.SerialName("original")
    Original,

    @kotlinx.serialization.SerialName("absolute-cinema")
    AbsoluteCinema,
}

@Serializable
data class StreamStatsMetrics(
    val fps: Boolean = true,
    val ping: Boolean = true,
    val bitrate: Boolean = false,
    val battery: Boolean = true,
    val connection: Boolean = true,
    val resolution: Boolean = false,
    val codec: Boolean = false,
    val location: Boolean = false,
    val latency: Boolean = false,
    val packetLoss: Boolean = false,
) {
    fun enabledCount(): Int = listOf(fps, ping, bitrate, battery, connection, resolution, codec, location, latency, packetLoss).count { it }
}

@Serializable
data class StreamKeyboardButtonPosition(
    val horizontalFraction: Float = 1f,
    val verticalFraction: Float = 0.5f,
) {
    internal fun normalized(): StreamKeyboardButtonPosition = StreamKeyboardButtonPosition(
        horizontalFraction = horizontalFraction.unitFractionOr(1f),
        verticalFraction = verticalFraction.unitFractionOr(0.5f),
    )
}

private fun Float.unitFractionOr(fallback: Float): Float =
    if (isFinite()) coerceIn(0f, 1f) else fallback

@Serializable
enum class IntroMusicStartMode {
    @kotlinx.serialization.SerialName("muted")
    Muted,

    @kotlinx.serialization.SerialName("playing")
    Playing,
}

@Serializable
enum class AppLaunchPage {
    @kotlinx.serialization.SerialName("store")
    Store,

    @kotlinx.serialization.SerialName("library")
    Library,
}

enum class SessionTimerMode {
    Countdown,
    Stopwatch,
}

data class SmartSessionLimit(
    val tierLabel: String,
    val limitHours: Int,
    val mode: SessionTimerMode,
)

internal val SESSION_WARNING_THRESHOLDS_SECONDS = listOf(
    30 * 60,
    10 * 60,
    5 * 60,
    3 * 60,
    60,
)

internal fun sessionElapsedSeconds(startedAtMs: Long, nowMs: Long): Int =
    ((nowMs - startedAtMs).coerceAtLeast(0L) / 1000L).toInt()

internal fun sessionRemainingSeconds(limit: SmartSessionLimit, startedAtMs: Long, nowMs: Long): Int {
    val limitSeconds = limit.limitHours * 60 * 60
    return (limitSeconds - sessionElapsedSeconds(startedAtMs, nowMs)).coerceAtLeast(0)
}

internal fun sessionWarningThresholdCrossed(previousRemainingSeconds: Int?, remainingSeconds: Int): Int? {
    val previous = previousRemainingSeconds ?: return null
    return SESSION_WARNING_THRESHOLDS_SECONDS
        .filter { threshold -> previous > threshold && remainingSeconds <= threshold }
        .minOrNull()
}

@Serializable
data class StreamSettings(
    val resolution: String = "1920x1080",
    val aspectRatio: String = "16:9",
    val fps: Int = 60,
    val maxBitrateMbps: Int = 75,
    val codec: VideoCodec = VideoCodec.H264,
    val colorQuality: ColorQuality = ColorQuality.TenBit420,
    val hdrEnabled: Boolean = false,
    @kotlinx.serialization.Transient val hdrDisplay: HdrDisplayProfile? = null,
    val region: String = "",
    val keyboardLayout: String = "en-US",
    val gameLanguage: String = "en_US",
    val sessionProxyEnabled: Boolean = false,
    val sessionProxyUrl: String = "",
    val enableL4S: Boolean = false,
    val mouseSensitivity: Float = 1f,
    val mouseAcceleration: Int = 1,
    val streamSharpeningEnabled: Boolean = false,
    val streamSharpeningAmount: Float = 0.25f,
    val microphoneMode: MicrophoneMode = MicrophoneMode.Disabled,
    val microphoneDeviceId: String = "",
    val mouseScrollSensitivity: Int = 30,
)

internal fun StreamSettings.withMicrophoneSettingsFrom(source: StreamSettings): StreamSettings =
    copy(
        microphoneMode = source.microphoneMode,
        microphoneDeviceId = source.microphoneDeviceId,
    )

/**
 * Where in-stream rumble is sent.
 *
 * [Auto] prefers the controller and falls back to the device, which is right for a phone plus a
 * separate pad. It is wrong on handhelds whose built-in pad advertises a vibrator that drives
 * nothing — the motor is wired to the *device* vibrator there — and no amount of probing
 * distinguishes "advertised and silent" from "advertised and working", so this is a choice.
 */
@Serializable
enum class HapticsOutputPreference {
    Auto,
    Controller,
    Device,
}

/**
 * A skin for the on-screen controller.
 *
 * A skin is a silhouette as well as a palette: the shapes come from `touchSkinForm` and the paint
 * from `touchSkinColors`, so two skins never differ by colour alone. Adding one is an entry here
 * plus a branch in each of those two functions.
 *
 * [V1] and [V2] keep their original names because they are already on disk in saved settings; the
 * labels people see come from `touchControllerStyleLabel`.
 */
@Serializable
enum class TouchControllerStyle {
    /** Classic: round filled caps, a one-piece d-pad cross, a ring stick. */
    V1,

    /** A heads-up display — unfilled caps, a grooved round pad, a crosshair stick. */
    V2,

    /** Hexagonal caps, blade d-pad and a hex gate, all bloomed in the accent colour. */
    Neon,

    /** Frosted squircle caps, a soft round pad and a stick sunk into a bowl. */
    Frost,

    /** Rimmed blocks and separated d-pad keys, for bright or busy games. */
    Contrast,

    /** A pre-analogue handheld: square keys, a restrictor gate, a monospaced legend. */
    Retro,

    /** A cabinet panel: domed plungers on chrome rims, round keys, a ball top on a shaft. */
    Arcade,
}

@Serializable
enum class TouchJoystickMode {
    Fixed,
    Dynamic,
}

@Serializable
enum class TouchAimMode {
    LockJoystick,
    LockZone,
}

/** Independently hideable pieces of the on-screen controller. */
@Serializable
enum class TouchControlGroup {
    FaceButtons,
    Dpad,
    LeftStick,
    RightStick,
    ShoulderButtons,
    ThumbButtons,
    MenuButtons,
}

/**
 * Actions available to the four movable accessibility buttons.
 *
 * These are deliberately single, momentary gamepad actions rather than macros. A duplicate action
 * is useful when a player cannot comfortably reach the original control, while avoiding recorded
 * sequences keeps releases predictable when a stream reconnects or the overlay is removed.
 */
@Serializable
enum class TouchExtraButtonAction {
    None,
    Guide,
    A,
    B,
    X,
    Y,
    DpadUp,
    DpadDown,
    DpadLeft,
    DpadRight,
    LeftBumper,
    RightBumper,
    LeftTrigger,
    RightTrigger,
    LeftStickClick,
    RightStickClick,
    Start,
    Select,
}

@Serializable
data class TouchOffset(val x: Float = 0f, val y: Float = 0f)

/**
 * Whether fingers are forwarded to the host as real touch rather than turned into a cursor.
 *
 * [Auto] limits it to games known to react to a touch device, which is the whole feature for most
 * people. The other two exist because that list is maintained by hand and will lag reality.
 */
@Serializable
enum class NativeTouchMode {
    Auto,
    Off,
    Always,
}

@Serializable
data class AndroidTouchSettings(
    val enabled: Boolean = true,
    val mousePad: Boolean = true,
    val opacity: Float = 0.82f,
    val scale: Float = 1f,
    val buttonScale: Float = 1.102468f,
    val stickScale: Float = 1f,
    /** Fine-grained multipliers applied after the two global size controls above. */
    val faceButtonScale: Float = 1f,
    val dpadScale: Float = 1f,
    val shoulderButtonScale: Float = 1f,
    val centerButtonScale: Float = 1f,
    val leftStickScale: Float = 1f,
    val rightStickScale: Float = 1f,
    /** Diameter of the movable stick cap as a fraction of its track. */
    val stickKnobScale: Float = 0.44f,
    /** Built-in clusters can be removed without giving up the rest of the overlay. */
    val visibleControlGroups: Set<TouchControlGroup> = TouchControlGroup.entries.toSet(),
    /** Up to four independently mapped and positioned accessibility buttons. */
    val extraButtonActions: List<TouchExtraButtonAction> = listOf(
        TouchExtraButtonAction.Guide,
        TouchExtraButtonAction.None,
        TouchExtraButtonAction.None,
        TouchExtraButtonAction.None,
    ),
    val extraButtonScale: Float = 1f,
    val joystickMode: TouchJoystickMode = TouchJoystickMode.Fixed,
    val aimMode: TouchAimMode = TouchAimMode.LockJoystick,
    /** Multiplies the visible and interactive Lock Zone footprint without changing its response. */
    val aimZoneScale: Float = 1f,
    /** Multiplies Lock Zone right-stick response without changing the zone footprint. */
    val aimZoneSensitivity: Float = 1f,
    val joystickDeadZone: Float = 0f,
    /** Opt-in motion aiming. Kept off for compatibility and to avoid unexpected camera motion. */
    val gyroscopeEnabled: Boolean = false,
    val gyroscopeSensitivity: Float = 1f,
    val gyroscopeDeadZone: Float = 0.035f,
    val gyroscopeSmoothing: Float = 0.35f,
    val gyroscopeInvertHorizontal: Boolean = false,
    val gyroscopeInvertVertical: Boolean = false,
    val edgePaddingDp: Float = 14f,
    val bottomPaddingDp: Float = 10f,
    val leftOffsetXDp: Float = 0f,
    val leftOffsetYDp: Float = 0f,
    val rightOffsetXDp: Float = 0f,
    val rightOffsetYDp: Float = 0f,
    val mouseDirectClick: Boolean = false,
    val nativeTouchMode: NativeTouchMode = NativeTouchMode.Auto,
    /**
     * Records an explicit Native Touch choice for compatibility with older saved settings. Auto is
     * the default and only activates for catalog variants that advertise TOUCHSCREEN support.
     */
    val nativeTouchOptedIn: Boolean = true,
    /**
     * Scales the velocity of touch movement in native touch mode. Values below 1.0 slow down
     * scroll/swipe gestures; values above 1.0 speed them up. Default 1.0 = no scaling.
     */
    val nativeTouchScrollScale: Float = 1.0f,
    /**
     * Minimum movement in dp before a MOVE event is forwarded in native touch mode.
     * Suppresses small sensor jitter that can look like a micro-swipe instead of a tap.
     * Default 8dp matches ViewConfiguration.getScaledTouchSlop() on most devices.
     */
    val nativeTouchJitterThresholdDp: Float = 8f,
    val offsets: Map<String, TouchOffset> = mapOf(
        "lstick_landscape" to TouchOffset(-67.02336f, 1.4236208f),
        "l3_landscape" to TouchOffset(-159.65048f, 119.79623f),
        "lt_landscape" to TouchOffset(32.63997f, -52.50644f),
        "dpad_landscape" to TouchOffset(47.38254f, -131.70163f),
        "rb_landscape" to TouchOffset(-124.94213f, -107.18774f),
        "lb_landscape" to TouchOffset(119.051155f, -100.54266f),
        "face_landscape" to TouchOffset(-20.225464f, -132.01855f),
        "rstick_landscape" to TouchOffset(96.44574f, -7.9870353f),
        "r3_landscape" to TouchOffset(191.65938f, 125.07891f),
        "rt_landscape" to TouchOffset(-30.344517f, -57.420998f),
        "extra1_landscape" to TouchOffset(-84f, 0f),
        "extra2_landscape" to TouchOffset(-28f, 0f),
        "extra3_landscape" to TouchOffset(28f, 0f),
        "extra4_landscape" to TouchOffset(84f, 0f),
        "extra1_portrait" to TouchOffset(-84f, 0f),
        "extra2_portrait" to TouchOffset(-28f, 0f),
        "extra3_portrait" to TouchOffset(28f, 0f),
        "extra4_portrait" to TouchOffset(84f, 0f),
    ),
    val touchControllerStyle: TouchControllerStyle = TouchControllerStyle.V1,
    /** Overrides the skin's own accent. Null keeps whatever the chosen skin ships with. */
    val touchSkinTint: ControllerThemeRgb? = null,
    /** Off leaves the caps blank — the layout is muscle memory once it is learned. */
    val touchButtonLabels: Boolean = true,
) {
    fun getOffset(key: String): TouchOffset = offsets[key] ?: TouchOffset()

    fun withOffset(key: String, x: Float, y: Float): AndroidTouchSettings {
        val newOffsets = offsets.toMutableMap()
        newOffsets[key] = TouchOffset(x, y)
        return this.copy(offsets = newOffsets)
    }

    fun isControlVisible(group: TouchControlGroup): Boolean = group in visibleControlGroups

    fun withControlVisible(group: TouchControlGroup, visible: Boolean): AndroidTouchSettings = copy(
        visibleControlGroups = if (visible) visibleControlGroups + group else visibleControlGroups - group,
    )

    fun extraButtonAction(index: Int): TouchExtraButtonAction =
        extraButtonActions.getOrNull(index) ?: TouchExtraButtonAction.None

    fun withExtraButtonAction(index: Int, action: TouchExtraButtonAction): AndroidTouchSettings {
        if (index !in 0 until TOUCH_EXTRA_BUTTON_COUNT) return this
        val actions = List(TOUCH_EXTRA_BUTTON_COUNT) { extraButtonAction(it) }.toMutableList()
        actions[index] = action
        return copy(extraButtonActions = actions)
    }

    fun withResetOffsets(): AndroidTouchSettings {
        val defaultSettings = AndroidTouchSettings()
        return this.copy(
            opacity = defaultSettings.opacity,
            scale = defaultSettings.scale,
            buttonScale = defaultSettings.buttonScale,
            stickScale = defaultSettings.stickScale,
            faceButtonScale = defaultSettings.faceButtonScale,
            dpadScale = defaultSettings.dpadScale,
            shoulderButtonScale = defaultSettings.shoulderButtonScale,
            centerButtonScale = defaultSettings.centerButtonScale,
            leftStickScale = defaultSettings.leftStickScale,
            rightStickScale = defaultSettings.rightStickScale,
            stickKnobScale = defaultSettings.stickKnobScale,
            extraButtonScale = defaultSettings.extraButtonScale,
            edgePaddingDp = defaultSettings.edgePaddingDp,
            bottomPaddingDp = defaultSettings.bottomPaddingDp,
            leftOffsetXDp = defaultSettings.leftOffsetXDp,
            leftOffsetYDp = defaultSettings.leftOffsetYDp,
            rightOffsetXDp = defaultSettings.rightOffsetXDp,
            rightOffsetYDp = defaultSettings.rightOffsetYDp,
            offsets = defaultSettings.offsets
        )
    }
}

internal const val TOUCH_EXTRA_BUTTON_COUNT = 4

internal const val DEFAULT_CATALOG_SORT_ID = "most_popular"
internal const val NEWLY_ADDED_CATALOG_SORT_ID = "last_added"
internal const val CATALOG_SORT_DEFAULT_VERSION = 1

@Serializable
data class AppSettings(
    val stream: StreamSettings = StreamSettings(),
    val streamPreset: StreamPreset = StreamPreset.Recommended,
    val posterSizeScale: Float = 1f,
    val compactGameCards: Boolean = true,
    val handheldLandscapeFourColumnGrid: Boolean = false,
    val handheldLandscapeSquareCards: Boolean = false,
    val nerdCatalogBackground: Boolean = false,
    /** The subtle accent-colour wash used when no catalogue wallpaper is selected. */
    val ambientBackgroundEnabled: Boolean = true,
    val catalogBackgroundPreset: CatalogBackgroundPreset = CatalogBackgroundPreset.ColorfulAbstract,
    val nerdCatalogBackgroundUri: String? = null,
    val tvSafeAreaPaddingDp: Float = 16f,
    val tvLayoutProfileVersion: Int = 0,
    val localTvRemoteEnabled: Boolean = false,
    /** Game titles under the poster in the catalog grid. Off makes the grid pure box art. */
    val showCardTitles: Boolean = true,
    /** Optional favorite affordance over catalogue artwork on mobile, handheld, and TV layouts. */
    val showFavoriteIconOnGameCards: Boolean = false,
    val expressiveUi: Boolean = true,
    val uselessMascotEnabled: Boolean = false,
    val uselessMascotDelaySeconds: Int = 5,
    /** Static outlines around game artwork, independent from optional animated border effects. */
    val liveSelectedOutlines: Boolean = false,
    /** Animated focus energy using the selected interface accent; never changes the accent itself. */
    val absoluteCinemaEffects: Boolean = false,
    /** Extends Absolute Cinema to pointer hover and non-controller focus surfaces throughout the UI. */
    val absoluteCinemaEverywhere: Boolean = false,
    val dynamicColor: Boolean = false,
    val uiAccent: UiAccent = UiAccent.OpenNow,
    /** Shows a user-managed shelf of installed Android apps above the cloud Library. */
    val localAppsEnabled: Boolean = false,
    /** Package names are stable across app updates and avoid persisting labels or icons. */
    val localAppPackageNames: List<String> = emptyList(),
    /**
     * Whether the "My own apps" shelf is folded away.
     *
     * Persisted rather than screen state: someone who keeps a dozen Android apps pinned but browses
     * the cloud catalogue most of the time wants that fold to survive leaving the Library.
     */
    val localAppsCollapsed: Boolean = false,
    /** The New games added hero in handheld landscape; dismissible from the hero header. */
    val landscapeNewGamesHero: Boolean = true,
    /** Catalogue presentation is user preference, not disposable screen state. */
    val catalogSortId: String = DEFAULT_CATALOG_SORT_ID,
    /** Lets the old relevance default migrate once without overriding a later explicit choice. */
    val catalogSortDefaultVersion: Int = 0,
    val catalogFilterIds: List<String> = emptyList(),
    val librarySortId: String = "library",
    val libraryFilterIds: List<String> = emptyList(),
    val launchPage: AppLaunchPage = AppLaunchPage.Store,
    val nerdMode: Boolean = false,
    val hideStreamButtons: Boolean = false,
    val streamKeyboardClearConfirmationDisabled: Boolean = false,
    val streamKeyboardButtonPosition: StreamKeyboardButtonPosition = StreamKeyboardButtonPosition(),
    val showAntiAfkIndicator: Boolean = true,
    val showStatsOnLaunch: Boolean = true,
    val streamStatsStyle: StreamStatsStyle = StreamStatsStyle.Compact,
    val streamStatsPosition: StreamStatsPosition = StreamStatsPosition.Right,
    val streamStatsMetrics: StreamStatsMetrics = StreamStatsMetrics(),
    /** Controller rumble when available, with device haptics as the fallback output. */
    @SerialName("phoneRumbleFallback")
    val vibrationEnabled: Boolean = true,
    val hapticsOutput: HapticsOutputPreference = HapticsOutputPreference.Auto,
    val hideServerSelector: Boolean = false,
    val controllerMode: Boolean = false,
    val controllerUiSounds: Boolean = true,
    val controllerMouseEmulation: Boolean = false,
    /** Capture an external mouse during gameplay so Android system edges cannot steal it. */
    val externalMousePointerLock: Boolean = true,
    val controllerBackgroundAnimations: Boolean = true,
    val controllerThemeStyle: String = "aurora",
    val controllerThemeColor: ControllerThemeRgb = ControllerThemeRgb(),
    val controllerLibraryGameBackdrop: Boolean = true,
    val autoLoadControllerLibrary: Boolean = false,
    val autoFullScreen: Boolean = true,
    val streamIntroMusic: Boolean = false,
    val streamIntroStartMode: IntroMusicStartMode = IntroMusicStartMode.Muted,
    val queueReadyMusic: Boolean = false,
    @SerialName("stretchStreamToFill")
    val legacyCropStreamToFill: Boolean = false,
    /**
     * Fills the display instead of letterboxing.
     *
     * Off by default so streams keep their exact geometry. Enabling this fills a mismatched display
     * by stretching only the mismatched axis; it never crops the stream.
     */
    @SerialName("stretchStreamToZoom")
    val stretchStreamToFit: Boolean = false,
    val streamPresentationProfileVersion: Int = 0,
    val favoriteGameIds: List<String> = emptyList(),
    val defaultGameVariantIds: Map<String, String> = emptyMap(),
    val sessionCounterEnabled: Boolean = true,
    val showSessionReportAfterStream: Boolean = false,
    /** One-time migration that makes post-stream reports opt-in instead of upgrade-persistent. */
    val sessionReportDefaultVersion: Int = 0,
    val sessionClockShowEveryMinutes: Int = 60,
    val sessionClockShowDurationSeconds: Int = 30,
    val clipboardPaste: Boolean = true,
    val androidTouch: AndroidTouchSettings = AndroidTouchSettings(),
    val androidStreamGuideDismissed: Boolean = false,
    val androidPhysicalControllerPromptDismissed: Boolean = false,
    val discordRichPresence: Boolean = false,
    val autoCheckForUpdates: Boolean = true,
    val analyticsOptOut: Boolean = true,
    val analyticsConsentAsked: Boolean = false,
    val allowEscapeToExitFullscreen: Boolean = false,
    val nativeLowLatencyDecoder: Boolean = false,
    /**
     * Highest [SETUP_FLOW_VERSION] whose first-run setup this install has been through. Versioned
     * rather than a boolean so a later release that adds a step can show the flow again, and so
     * "run setup again" is expressible as resetting the field.
     */
    val setupFlowCompletedVersion: Int = 0,
    /**
     * Whether the About > build-number gesture has revealed Settings > Developer options.
     *
     * Persisted so the section survives a restart, and clearable from inside it. See
     * `AndroidDeveloperOptions.kt`.
     */
    val developerOptionsUnlocked: Boolean = false,
)

internal const val MIN_GAME_CARD_SCALE = 0.75f
internal const val MAX_GAME_CARD_SCALE = 1.4f
internal const val STREAM_PRESENTATION_PROFILE_VERSION = 3
internal const val SESSION_REPORT_DEFAULT_VERSION = 1

/**
 * Re-asserts the stream presentation defaults once per profile version.
 *
 * Version 3 makes exact geometry the default for new profiles. Existing profiles keep their saved
 * stretch preference, including the version 2 phone default, so an app update does not silently
 * override a user's current presentation.
 */
internal fun AppSettings.withCurrentStreamPresentationDefaults(): AppSettings {
    if (streamPresentationProfileVersion >= STREAM_PRESENTATION_PROFILE_VERSION) return this
    return copy(
        legacyCropStreamToFill = false,
        streamPresentationProfileVersion = STREAM_PRESENTATION_PROFILE_VERSION,
    )
}

internal val AppSettings.analyticsSharingEnabled: Boolean
    get() = analyticsConsentAsked && !analyticsOptOut

internal fun streamResolutionPixels(settings: StreamSettings): Pair<Int, Int> {
    if (!isKnownStreamResolution(settings.resolution)) {
        parseResolutionPixelsOrNull(settings.resolution)?.let { return it }
    }
    return parseResolutionPixels(normalizeStreamResolutionForAspect(settings.resolution, settings.aspectRatio))
}

internal fun StreamSettings.requiresNativeDesktopCloudMatchMode(): Boolean {
    val (width, height) = streamResolutionPixels(this)
    // CloudMatch's browser allocation rejects HDR even at 1080p and caps the high-resolution
    // matrix. The caller selects a platform-appropriate native identity (including Android TV).
    return hdrEnabled || fps > 60 || width > 1920 || height > 1200
}

internal data class StreamResolutionMismatch(
    val actualResolution: String,
    val expectedResolution: String,
    val serverNegotiatedResolution: String? = null,
)

internal enum class StreamResolutionChangeSource {
    ServerNegotiatedFallback,
    ProviderOrGameModeChange,
}

internal data class ActiveStreamTransportProfile(
    val resolution: String,
    val aspectRatio: String,
    val fps: Int,
    val maxBitrateMbps: Int,
    val codec: VideoCodec,
    val colorQuality: ColorQuality,
    val hdrEnabled: Boolean,
    val enableL4S: Boolean,
    val streamSharpeningEnabled: Boolean,
)

internal fun StreamSettings.toActiveStreamTransportProfile(): ActiveStreamTransportProfile =
    ActiveStreamTransportProfile(
        resolution = resolution,
        aspectRatio = aspectRatio,
        fps = fps,
        maxBitrateMbps = maxBitrateMbps,
        codec = codec,
        colorQuality = colorQuality,
        hdrEnabled = hdrEnabled,
        enableL4S = enableL4S,
        streamSharpeningEnabled = streamSharpeningEnabled,
    )

internal data class ActiveStreamModeStatus(
    val requestedResolution: String,
    val displayedResolution: String,
    val serverNegotiatedResolution: String? = null,
    val serverFinalSelectedResolution: String? = null,
    val resolutionSource: StreamResolutionChangeSource? = null,
    val safeVideoRecoveryActive: Boolean = false,
    val requestedProfile: ActiveStreamTransportProfile,
    val transportProfile: ActiveStreamTransportProfile,
) {
    val transportCodec: VideoCodec
        get() = transportProfile.codec
}

internal val StreamResolutionMismatch.isServerNegotiatedFallback: Boolean
    get() = serverNegotiatedResolution == actualResolution

internal fun streamRuntimeResolutionMismatch(
    settings: StreamSettings,
    actualResolution: String?,
    serverNegotiatedResolution: String? = null,
): StreamResolutionMismatch? {
    val actualPixels = parseResolutionPixelsOrNull(actualResolution)
        ?.takeIf { (width, height) -> width >= 320 && height >= 180 }
        ?: return null
    val expectedPixels = streamResolutionPixels(settings)
    if (actualPixels == expectedPixels) return null
    val negotiatedPixels = parseResolutionPixelsOrNull(serverNegotiatedResolution)
    return StreamResolutionMismatch(
        actualResolution = "${actualPixels.first}x${actualPixels.second}",
        expectedResolution = "${expectedPixels.first}x${expectedPixels.second}",
        serverNegotiatedResolution = negotiatedPixels
            ?.takeIf { it == actualPixels }
            ?.let { "${it.first}x${it.second}" },
    )
}

internal fun activeStreamModeStatus(
    requestedSettings: StreamSettings,
    transportSettings: StreamSettings,
    decodedResolution: String?,
    serverNegotiatedResolution: String? = null,
    serverFinalSelectedResolution: String? = null,
): ActiveStreamModeStatus? {
    val requestedPixels = streamResolutionPixels(requestedSettings)
    val requestedResolution = "${requestedPixels.first}x${requestedPixels.second}"
    val decodedPixels = parseResolutionPixelsOrNull(decodedResolution)
        ?.takeIf { (width, height) -> width >= 320 && height >= 180 }
    val negotiatedPixels = parseResolutionPixelsOrNull(serverNegotiatedResolution)
        ?.takeIf { (width, height) -> width >= 320 && height >= 180 }
    val finalSelectedPixels = parseResolutionPixelsOrNull(serverFinalSelectedResolution)
        ?.takeIf { (width, height) -> width >= 320 && height >= 180 }
    // CloudMatch can publish an intermediate monitor profile before video arrives, then the actual
    // encoder emits the requested geometry. Do not turn that provisional snapshot into a user
    // notification or bug report. A decoded frame (or explicit final selection) is authoritative.
    val displayedPixels = decodedPixels ?: finalSelectedPixels ?: requestedPixels
    val resolutionSource = when {
        displayedPixels == requestedPixels -> null
        finalSelectedPixels == displayedPixels ||
            (decodedPixels != null && negotiatedPixels == displayedPixels) ->
            StreamResolutionChangeSource.ServerNegotiatedFallback
        decodedPixels != null -> StreamResolutionChangeSource.ProviderOrGameModeChange
        else -> null
    }
    val requestedProfile = requestedSettings.toActiveStreamTransportProfile()
    val transportProfile = transportSettings.toActiveStreamTransportProfile()
    val safeVideoRecoveryActive = requestedProfile != transportProfile
    if (resolutionSource == null && !safeVideoRecoveryActive) return null
    return ActiveStreamModeStatus(
        requestedResolution = requestedResolution,
        displayedResolution = "${displayedPixels.first}x${displayedPixels.second}",
        serverNegotiatedResolution = negotiatedPixels?.let { "${it.first}x${it.second}" },
        serverFinalSelectedResolution = finalSelectedPixels?.let { "${it.first}x${it.second}" },
        resolutionSource = resolutionSource,
        safeVideoRecoveryActive = safeVideoRecoveryActive,
        requestedProfile = requestedProfile,
        transportProfile = transportProfile,
    )
}

internal fun streamResolutionOptionsForAspect(aspectRatio: String): List<String> =
    STREAM_RESOLUTION_OPTIONS.filter { it.aspectRatio == aspectRatio }.map { it.value }

internal fun streamResolutionChoicesForAspect(aspectRatio: String): List<StreamResolutionChoice> =
    STREAM_RESOLUTION_OPTIONS.filter { it.aspectRatio == aspectRatio }.map { it.toChoice() }

internal fun streamAspectRatioOptions(): List<String> =
    STREAM_RESOLUTION_OPTIONS.map { it.aspectRatio }.distinct()

internal fun streamAspectRatioForResolution(resolution: String): String? =
    STREAM_RESOLUTION_OPTIONS.firstOrNull { it.value == resolution }?.aspectRatio

internal fun normalizeStreamResolutionForAspect(resolution: String, aspectRatio: String): String {
    val normalizedAspect = aspectRatio.trim()
    val options = streamResolutionOptionsForAspect(normalizedAspect)
    if (options.isEmpty()) return resolution
    if (STREAM_RESOLUTION_OPTIONS.any { it.value == resolution && it.aspectRatio == normalizedAspect }) {
        return resolution
    }

    val tier = STREAM_RESOLUTION_OPTIONS.firstOrNull { it.value == resolution }?.tier
        ?: resolutionTierForHeight(parseResolutionPixels(resolution).second)
    PREFERRED_RESOLUTION_BY_TIER_AND_ASPECT[tier]?.get(normalizedAspect)?.let { preferred ->
        if (preferred in options) return preferred
    }

    val requestedPixels = parseResolutionPixels(resolution).let { it.first * it.second }
    return options.minWithOrNull(
        compareBy<String> { option ->
            val pixels = parseResolutionPixels(option).let { it.first * it.second }
            abs(pixels - requestedPixels)
        }.thenBy { option ->
            parseResolutionPixels(option).first * parseResolutionPixels(option).second
        },
    ) ?: options.first()
}

internal fun normalizeStreamResolutionForAspectAndPlan(
    resolution: String,
    aspectRatio: String,
    subscriptionInfo: SubscriptionInfo?,
    fallbackMembershipTier: String?,
): String {
    val customResolution = customStreamResolutionOrNull(resolution)
    if (customResolution != null && customResolutionAllowedForPlan(customResolution, subscriptionInfo, fallbackMembershipTier)) {
        return "${customResolution.first}x${customResolution.second}"
    }

    val normalized = normalizeStreamResolutionForAspect(resolution, aspectRatio)
    val choices = streamResolutionChoicesForAspect(aspectRatio)
    val current = choices.firstOrNull { it.value == normalized }
    if (current?.isAvailableFor(subscriptionInfo, fallbackMembershipTier) == true) return normalized

    val availableChoices = choices.filter { it.isAvailableFor(subscriptionInfo, fallbackMembershipTier) }
        .ifEmpty {
            streamResolutionChoicesForAspect("16:9").filter { it.isAvailableFor(subscriptionInfo, fallbackMembershipTier) }
        }
    if (availableChoices.isEmpty()) return normalized

    val requestedPixels = parseResolutionPixels(normalized).let { it.first * it.second }
    return availableChoices
        .filter { it.width * it.height <= requestedPixels }
        .maxByOrNull { it.width * it.height }
        ?.value
        ?: availableChoices.minByOrNull { it.width * it.height }?.value
        ?: normalized
}

internal fun parseResolutionPixels(value: String): Pair<Int, Int> {
    val parts = value.split("x")
    val width = parts.getOrNull(0)?.toIntOrNull()
    val height = parts.getOrNull(1)?.toIntOrNull()
    return if (width != null && height != null && width > 0 && height > 0) width to height else 1920 to 1080
}

internal fun streamSettingsSessionSignature(settings: StreamSettings): String {
    val compatible = settings.withCodecColorCompatibility()
    val (width, height) = streamResolutionPixels(compatible)
    return listOf(
        "opennow-android-stream-v1",
        "res=${width}x$height",
        "fps=${compatible.fps}",
        "bitrate=${compatible.maxBitrateMbps}",
        "codec=${compatible.codec.name}",
        "color=${compatible.colorQuality.name}",
        "hdr=${if (compatible.hdrEnabled) 1 else 0}",
        "l4s=${if (compatible.enableL4S) 1 else 0}",
        "keyboard=${compatible.keyboardLayout.trim()}",
        "language=${compatible.gameLanguage.trim()}",
    ).joinToString(";")
}

internal data class StreamResolutionOption(
    val value: String,
    val aspectRatio: String,
    val tier: String,
    val requiredPlan: StreamResolutionPlan = StreamResolutionPlan.Free,
) {
    fun toChoice(): StreamResolutionChoice {
        val (width, height) = parseResolutionPixels(value)
        return StreamResolutionChoice(
            value = value,
            width = width,
            height = height,
            aspectRatio = aspectRatio,
            requiredPlan = requiredPlan,
        )
    }
}

internal enum class StreamResolutionPlan {
    Free,
    Priority,
    Ultimate,
}

internal data class StreamResolutionChoice(
    val value: String,
    val width: Int,
    val height: Int,
    val aspectRatio: String,
    val requiredPlan: StreamResolutionPlan,
) {
    val label: String
        get() = "$width x $height"

    val requiredPlanLabel: String?
        get() = streamResolutionPlanLabel(requiredPlan).takeIf { requiredPlan != StreamResolutionPlan.Free }

    fun isAvailableFor(subscriptionInfo: SubscriptionInfo?, fallbackMembershipTier: String?): Boolean {
        if (requiredPlan == StreamResolutionPlan.Free) return true
        return streamResolutionPlanRank(effectiveStreamingPlan(subscriptionInfo, fallbackMembershipTier)) >= streamResolutionPlanRank(requiredPlan)
    }
}

internal fun hasUltimateStreamingPlan(subscriptionInfo: SubscriptionInfo?, fallbackMembershipTier: String?): Boolean =
    streamResolutionPlanRank(effectiveStreamingPlan(subscriptionInfo, fallbackMembershipTier)) >=
        streamResolutionPlanRank(StreamResolutionPlan.Ultimate)

internal fun hasHdrStreamingPlan(subscriptionInfo: SubscriptionInfo?, fallbackMembershipTier: String?): Boolean =
    streamResolutionPlanRank(effectiveStreamingPlan(subscriptionInfo, fallbackMembershipTier)) >=
        streamResolutionPlanRank(StreamResolutionPlan.Priority)

internal fun maxStreamFpsFor(subscriptionInfo: SubscriptionInfo?, fallbackMembershipTier: String?): Int =
    if (hasUltimateStreamingPlan(subscriptionInfo, fallbackMembershipTier)) MAX_ULTIMATE_STREAM_FPS else MAX_STANDARD_STREAM_FPS

internal fun StreamSettings.withFpsAllowed(subscriptionInfo: SubscriptionInfo?, fallbackMembershipTier: String?): StreamSettings {
    val maxFps = maxStreamFpsFor(subscriptionInfo, fallbackMembershipTier)
    val allowedFps = fps.coerceIn(30, maxFps)
    return if (allowedFps == fps) this else copy(fps = allowedFps)
}

/**
 * What the signed-in plan actually allows, in the same terms the stream settings use.
 *
 * First-run setup shows this beside the quality choices. Without it, a Free account being offered
 * 1080p60 at most reads as OpenNOW deciding the device cannot manage more, when the cap is the
 * membership tier — so the tier, and the ceiling it buys, are stated outright.
 */
internal data class StreamPlanEntitlements(
    val plan: StreamResolutionPlan,
    val planLabel: String,
    val maxResolutionLabel: String,
    val maxFps: Int,
    val hdrAllowed: Boolean,
) {
    /** True when a higher tier would unlock resolutions or frame rates this one cannot reach. */
    val cappedBelowTopTier: Boolean get() = plan != StreamResolutionPlan.Ultimate
}

internal fun streamPlanEntitlements(
    subscriptionInfo: SubscriptionInfo?,
    fallbackMembershipTier: String?,
): StreamPlanEntitlements {
    val plan = effectiveStreamingPlan(subscriptionInfo, fallbackMembershipTier)
    val bestResolution = STREAM_RESOLUTION_OPTIONS
        .map { it.toChoice() }
        .filter { it.isAvailableFor(subscriptionInfo, fallbackMembershipTier) }
        .maxByOrNull { it.width.toLong() * it.height }
    return StreamPlanEntitlements(
        plan = plan,
        planLabel = streamResolutionPlanLabel(plan),
        maxResolutionLabel = bestResolution?.label ?: "1280 x 720",
        maxFps = maxStreamFpsFor(subscriptionInfo, fallbackMembershipTier),
        hdrAllowed = hasHdrStreamingPlan(subscriptionInfo, fallbackMembershipTier),
    )
}

internal fun streamResolutionPlanLabel(plan: StreamResolutionPlan): String = when (plan) {
    StreamResolutionPlan.Free -> "Free"
    StreamResolutionPlan.Priority -> "Performance"
    StreamResolutionPlan.Ultimate -> "Ultimate"
}

internal fun smartSessionLimitFor(subscriptionInfo: SubscriptionInfo?, fallbackMembershipTier: String?): SmartSessionLimit {
    val plan = effectiveStreamingPlan(subscriptionInfo, fallbackMembershipTier)
    val label = streamResolutionPlanLabel(plan)
    return when (plan) {
        StreamResolutionPlan.Ultimate -> SmartSessionLimit(label, 8, SessionTimerMode.Stopwatch)
        StreamResolutionPlan.Priority -> SmartSessionLimit(label, 6, SessionTimerMode.Stopwatch)
        StreamResolutionPlan.Free -> SmartSessionLimit(label, 1, SessionTimerMode.Countdown)
    }
}

internal fun monthlyHourLimitFor(subscriptionInfo: SubscriptionInfo?, fallbackMembershipTier: String?): Double? {
    val reported = subscriptionInfo?.totalHours?.takeIf { it > 0.0 }
    if (reported != null) return reported
    return when (effectiveStreamingPlan(subscriptionInfo, fallbackMembershipTier)) {
        StreamResolutionPlan.Free -> null
        StreamResolutionPlan.Priority,
        StreamResolutionPlan.Ultimate,
        -> 100.0
    }
}

internal fun monthlyHoursRemainingFor(subscriptionInfo: SubscriptionInfo?, fallbackMembershipTier: String?): Double? {
    val reported = subscriptionInfo?.remainingHours?.takeIf { it > 0.0 }
    if (reported != null) return reported
    val limit = monthlyHourLimitFor(subscriptionInfo, fallbackMembershipTier) ?: return null
    return (limit - (subscriptionInfo?.usedHours ?: 0.0)).coerceAtLeast(0.0)
}

internal fun StreamSettings.withHdrAllowed(subscriptionInfo: SubscriptionInfo?, fallbackMembershipTier: String?): StreamSettings =
    if (hdrEnabled && !hasHdrStreamingPlan(subscriptionInfo, fallbackMembershipTier)) copy(hdrEnabled = false).withCodecColorCompatibility() else withCodecColorCompatibility()

/** Transport envelope; the actual HDR10 display and decoder are checked at launch. */
@Suppress("UNUSED_PARAMETER")
internal fun StreamSettings.hdrAvailableForAndroid(androidTvProfile: Boolean): Boolean {
    val (width, height) = streamResolutionPixels(this)
    return codec == VideoCodec.H265 && fps <= 60 && width <= 3840 && height <= 2160
}

internal fun StreamSettings.withAndroidHdrCompatibility(androidTvProfile: Boolean): StreamSettings =
    if (hdrEnabled && !hdrAvailableForAndroid(androidTvProfile)) {
        copy(hdrEnabled = false).withCodecColorCompatibility()
    } else {
        withCodecColorCompatibility()
    }

internal fun VideoCodec.availableForAndroidSettings(): Boolean =
    true

internal fun ColorQuality.availableForAndroidSettings(): Boolean =
    !isChroma444()

internal fun ColorQuality.availableForCodec(codec: VideoCodec): Boolean =
    availableForAndroidSettings() &&
        codec.availableForAndroidSettings() &&
        (codec != VideoCodec.AV1 || !isTenBit())

internal fun StreamSettings.withAndroidSettingsAvailability(): StreamSettings {
    val providerCompatible = withProviderCompatibleUltrawideGeometry()
    val availableCodec = if (providerCompatible.codec.availableForAndroidSettings()) providerCompatible.codec else VideoCodec.H264
    val normalized = if (availableCodec == providerCompatible.codec) providerCompatible else providerCompatible.copy(codec = availableCodec)
    return normalized.withCodecColorCompatibility()
}

/**
 * The old Portal-sized option used the panel's 1376x640 dimensions, but GFN does not expose that
 * low 19.5:9 mode. CloudMatch selected 1680x720 and the cloud streamer then cropped it to 1376x590.
 * Treat the observed 21:9 mode as the user's requested geometry so launch, negotiation, decoding,
 * input mapping, and profile-change reporting all describe the same stream.
 */
private fun StreamSettings.withProviderCompatibleUltrawideGeometry(): StreamSettings =
    if (resolution == LEGACY_PORTAL_STREAM_RESOLUTION && aspectRatio == LEGACY_PORTAL_STREAM_ASPECT) {
        copy(resolution = LOW_ULTRAWIDE_STREAM_RESOLUTION, aspectRatio = "21:9")
    } else {
        this
    }

internal fun StreamSettings.withCodecColorCompatibility(): StreamSettings {
    val compatibleHdr = hdrEnabled && codec != VideoCodec.AV1
    val compatibleColor = when {
        codec == VideoCodec.AV1 -> ColorQuality.EightBit420
        colorQuality.isChroma444() -> colorQuality.asChroma420()
        compatibleHdr && !colorQuality.isTenBit() -> ColorQuality.TenBit420
        else -> colorQuality
    }
    return if (compatibleColor == colorQuality && compatibleHdr == hdrEnabled) {
        this
    } else {
        copy(colorQuality = compatibleColor, hdrEnabled = compatibleHdr)
    }
}

internal fun StreamSettings.usesTenBitStreamProfile(): Boolean =
    hdrEnabled || colorQuality.isTenBit()

internal fun StreamSettings.applyingStreamPreset(preset: StreamPreset): StreamSettings {
    if (preset == StreamPreset.Custom) return this
    val target = streamPresetTargetForAspect(preset, aspectRatio)
    return copy(
        resolution = target.resolution,
        aspectRatio = target.aspectRatio,
        fps = target.fps,
        maxBitrateMbps = target.maxBitrateMbps,
        colorQuality = ColorQuality.EightBit420,
        hdrEnabled = false,
    ).withoutExperimentalTransportRequests()
        .withAndroidSettingsAvailability()
}

internal fun StreamSettings.withoutExperimentalTransportRequests(): StreamSettings =
    if (!enableL4S) this else copy(enableL4S = false)

internal fun StreamSettings.withResolutionAllowed(subscriptionInfo: SubscriptionInfo?, fallbackMembershipTier: String?): StreamSettings {
    val providerCompatible = withProviderCompatibleUltrawideGeometry()
    if (providerCompatible != this) {
        return providerCompatible.withResolutionAllowed(subscriptionInfo, fallbackMembershipTier)
    }
    val customResolution = customStreamResolutionOrNull(resolution)
    if (customResolution != null && customResolutionAllowedForPlan(customResolution, subscriptionInfo, fallbackMembershipTier)) {
        val normalizedResolution = "${customResolution.first}x${customResolution.second}"
        return if (normalizedResolution == resolution) this else copy(resolution = normalizedResolution)
    }

    val allowedAspectRatio = if (streamResolutionChoicesForAspect(aspectRatio).any { it.isAvailableFor(subscriptionInfo, fallbackMembershipTier) }) {
        aspectRatio
    } else {
        "16:9"
    }
    val allowedResolution = normalizeStreamResolutionForAspectAndPlan(resolution, allowedAspectRatio, subscriptionInfo, fallbackMembershipTier)
    return if (allowedResolution == resolution && allowedAspectRatio == aspectRatio) this else copy(resolution = allowedResolution, aspectRatio = allowedAspectRatio)
}

internal fun StreamSettings.eligibleForAndroidLaunch(
    subscriptionInfo: SubscriptionInfo?,
    fallbackMembershipTier: String?,
    androidTvProfile: Boolean,
): StreamSettings =
    withResolutionAllowed(subscriptionInfo, fallbackMembershipTier)
        .withFpsAllowed(subscriptionInfo, fallbackMembershipTier)
        .withHdrAllowed(subscriptionInfo, fallbackMembershipTier)
        .withAndroidSettingsAvailability()
        .withAndroidHdrCompatibility(androidTvProfile)
        .withCodecColorCompatibility()

private fun isKnownStreamResolution(resolution: String): Boolean =
    STREAM_RESOLUTION_OPTIONS.any { it.value == resolution }

private fun customStreamResolutionOrNull(resolution: String): Pair<Int, Int>? =
    parseResolutionPixelsOrNull(resolution)?.takeUnless {
        isKnownStreamResolution(resolution) || resolution in UNSUPPORTED_LEGACY_STREAM_RESOLUTIONS
    }

private val UNSUPPORTED_LEGACY_STREAM_RESOLUTIONS = setOf(
    "1376x640",
    "1600x720",
    "2400x1080",
    "3200x1440",
    "4800x2160",
)

private fun customResolutionAllowedForPlan(
    resolution: Pair<Int, Int>,
    subscriptionInfo: SubscriptionInfo?,
    fallbackMembershipTier: String?,
): Boolean {
    val availableChoices = STREAM_RESOLUTION_OPTIONS
        .map { it.toChoice() }
        .filter { it.isAvailableFor(subscriptionInfo, fallbackMembershipTier) }
    if (availableChoices.isEmpty()) return false

    val (width, height) = resolution
    val pixels = width * height
    return width <= availableChoices.maxOf { it.width } &&
        height <= availableChoices.maxOf { it.height } &&
        pixels <= availableChoices.maxOf { it.width * it.height }
}

internal val STREAM_RESOLUTION_OPTIONS = listOf(
    StreamResolutionOption("1280x720", "16:9", "720"),
    StreamResolutionOption("1366x768", "16:9", "768"),
    StreamResolutionOption("1600x900", "16:9", "900"),
    StreamResolutionOption("1280x800", "16:10", "720"),
    StreamResolutionOption("1440x900", "16:10", "900"),
    StreamResolutionOption("1680x1050", "16:10", "1050"),
    StreamResolutionOption("1920x1080", "16:9", "1080"),
    StreamResolutionOption("1920x1200", "16:10", "1080"),
    StreamResolutionOption("1024x768", "4:3", "768"),
    StreamResolutionOption("1112x834", "4:3", "834"),
    StreamResolutionOption("1600x1200", "4:3", "1080"),
    StreamResolutionOption("1280x1024", "5:4", "1050"),
    StreamResolutionOption("1376x590", "21:9", "720"),
    StreamResolutionOption("1680x720", "21:9", "720"),
    StreamResolutionOption("2340x1080", "19.5:9", "1080", StreamResolutionPlan.Priority),
    StreamResolutionOption("2560x1080", "21:9", "1080", StreamResolutionPlan.Priority),
    StreamResolutionOption("3840x1080", "32:9", "1080", StreamResolutionPlan.Priority),
    StreamResolutionOption("2560x1440", "16:9", "1440", StreamResolutionPlan.Priority),
    StreamResolutionOption("2560x1600", "16:10", "1440", StreamResolutionPlan.Priority),
    StreamResolutionOption("3440x1440", "21:9", "1440", StreamResolutionPlan.Priority),
    StreamResolutionOption("5120x1440", "32:9", "1440", StreamResolutionPlan.Priority),
    StreamResolutionOption("3840x1600", "24:10", "1440", StreamResolutionPlan.Priority),
    StreamResolutionOption("3840x2160", "16:9", "2160", StreamResolutionPlan.Ultimate),
    StreamResolutionOption("3456x2160", "16:10", "2160", StreamResolutionPlan.Ultimate),
    StreamResolutionOption("5120x2160", "21:9", "2160", StreamResolutionPlan.Ultimate),
    StreamResolutionOption("5120x2880", "16:9", "2880", StreamResolutionPlan.Ultimate),
)

private val PREFERRED_RESOLUTION_BY_TIER_AND_ASPECT = mapOf(
    "720" to mapOf("16:9" to "1280x720", "16:10" to "1280x800", "4:3" to "1024x768", "21:9" to "1680x720"),
    "768" to mapOf("16:9" to "1366x768", "4:3" to "1024x768"),
    "834" to mapOf("4:3" to "1112x834"),
    "900" to mapOf("16:9" to "1600x900", "16:10" to "1440x900"),
    "1050" to mapOf("16:10" to "1680x1050", "5:4" to "1280x1024"),
    "1080" to mapOf("16:9" to "1920x1080", "16:10" to "1920x1200", "4:3" to "1600x1200", "19.5:9" to "2340x1080", "21:9" to "2560x1080", "32:9" to "3840x1080"),
    "1440" to mapOf("16:9" to "2560x1440", "16:10" to "2560x1600", "21:9" to "3440x1440", "24:10" to "3840x1600", "32:9" to "5120x1440"),
    "2160" to mapOf("16:9" to "3840x2160", "16:10" to "3456x2160", "21:9" to "5120x2160"),
    "2880" to mapOf("16:9" to "5120x2880"),
)

private data class StreamPresetTarget(
    val resolution: String,
    val aspectRatio: String,
    val fps: Int,
    val maxBitrateMbps: Int,
)

private fun streamPresetTargetForAspect(preset: StreamPreset, aspectRatio: String): StreamPresetTarget {
    val normalizedAspect = aspectRatio.takeIf { streamResolutionOptionsForAspect(it).isNotEmpty() } ?: "16:9"
    val maxHeight = when (preset) {
        StreamPreset.Custom -> Int.MAX_VALUE
        StreamPreset.Recommended -> 1200
        StreamPreset.LowDataSaver -> 800
        StreamPreset.Medium -> 1200
        StreamPreset.High -> 1600
    }
    val options = STREAM_RESOLUTION_OPTIONS
        .filter { it.aspectRatio == normalizedAspect }
        .sortedBy { it.pixelCount() }
    val resolution = options
        .filter { parseResolutionPixels(it.value).second <= maxHeight }
        .maxByOrNull { it.pixelCount() }
        ?: options.firstOrNull()
        ?: StreamResolutionOption("1280x720", "16:9", "720")

    return when (preset) {
        StreamPreset.Custom -> StreamPresetTarget(resolution.value, resolution.aspectRatio, 60, 75)
        StreamPreset.Recommended -> StreamPresetTarget(resolution.value, resolution.aspectRatio, 60, 35)
        StreamPreset.LowDataSaver -> StreamPresetTarget(resolution.value, resolution.aspectRatio, 30, 12)
        StreamPreset.Medium -> StreamPresetTarget(resolution.value, resolution.aspectRatio, 60, 35)
        StreamPreset.High -> StreamPresetTarget(resolution.value, resolution.aspectRatio, MAX_ULTIMATE_STREAM_FPS, 75)
    }
}

private fun ColorQuality.isChroma444(): Boolean =
    this == ColorQuality.EightBit444 || this == ColorQuality.TenBit444

internal fun ColorQuality.isTenBit(): Boolean =
    this == ColorQuality.TenBit420 || this == ColorQuality.TenBit444

private fun ColorQuality.asChroma420(): ColorQuality =
    when (this) {
        ColorQuality.TenBit444 -> ColorQuality.TenBit420
        ColorQuality.EightBit444 -> ColorQuality.EightBit420
        else -> this
    }

private fun resolutionTierForHeight(height: Int): String =
    when {
        height >= 2600 -> "2880"
        height >= 2000 -> "2160"
        height >= 1320 -> "1440"
        height >= 1120 -> "1080"
        height >= 975 -> "1050"
        height >= 850 -> "900"
        height >= 800 -> "834"
        height >= 740 -> "768"
        else -> "720"
    }

/**
 * What a game's `minimumMembershipTierLabel` demands, when the signed-in plan cannot meet it.
 *
 * Null whenever the game names no tier, names one this account already has, or names something the
 * normalizer does not recognise. Guessing here would be worse than staying quiet: a spurious
 * "requires Ultimate" in front of the Play button reads as the app refusing to launch a game the
 * player owns.
 */
data class GameMembershipRequirement(
    val requiredPlanLabel: String,
    val currentPlanLabel: String,
)

/** The launch a membership warning is holding, so continuing resumes exactly what was asked for. */
data class PendingMembershipNotice(
    val game: GameInfo,
    val requirement: GameMembershipRequirement,
    val streamingBaseUrlOverride: String?,
    val skipPrintedWaste: Boolean,
    val skipStoreChoice: Boolean,
)

internal fun gameMembershipRequirement(
    game: GameInfo,
    subscriptionInfo: SubscriptionInfo?,
    fallbackMembershipTier: String?,
): GameMembershipRequirement? {
    val label = game.membershipTierLabel?.trim()?.takeIf { it.isNotEmpty() } ?: return null
    val requiredPlan = planForMembershipTier(label)
    if (requiredPlan == StreamResolutionPlan.Free) return null
    val currentPlan = effectiveStreamingPlan(subscriptionInfo, fallbackMembershipTier)
    if (streamResolutionPlanRank(currentPlan) >= streamResolutionPlanRank(requiredPlan)) return null
    return GameMembershipRequirement(
        requiredPlanLabel = streamResolutionPlanLabel(requiredPlan),
        currentPlanLabel = streamResolutionPlanLabel(currentPlan),
    )
}

private fun planForMembershipTier(membershipTier: String?): StreamResolutionPlan {
    val normalized = membershipTier.orEmpty().uppercase(Locale.US).replace(NON_ALNUM_UPPER_RUN, "")
    return when {
        normalized.contains("ULTIMATE") || normalized.contains("RTX3080") -> StreamResolutionPlan.Ultimate
        normalized.contains("PRIORITY") || normalized.contains("PERFORMANCE") || normalized.contains("FOUNDERS") -> StreamResolutionPlan.Priority
        else -> StreamResolutionPlan.Free
    }
}

private fun effectiveStreamingPlan(
    subscriptionInfo: SubscriptionInfo?,
    fallbackMembershipTier: String?,
): StreamResolutionPlan =
    listOf(
        planForMembershipTier(subscriptionInfo?.membershipTier),
        planForMembershipTier(fallbackMembershipTier),
    ).maxBy { streamResolutionPlanRank(it) }

private fun streamResolutionPlanRank(plan: StreamResolutionPlan): Int =
    when (plan) {
        StreamResolutionPlan.Free -> 0
        StreamResolutionPlan.Priority -> 1
        StreamResolutionPlan.Ultimate -> 2
    }

@Serializable
data class ControllerThemeRgb(
    val r: Int = 124,
    val g: Int = 241,
    val b: Int = 177,
)

@Serializable
data class LoginProvider(
    val idpId: String,
    val code: String,
    val displayName: String,
    val streamingServiceUrl: String,
    val priority: Int = 0,
)

val LoginProvider.supportsDeviceCodeLogin: Boolean
    get() = code.equals("NVIDIA", ignoreCase = true)

@Serializable
data class AuthTokens(
    val accessToken: String,
    val refreshToken: String? = null,
    val idToken: String? = null,
    val expiresAt: Long,
    val clientToken: String? = null,
    val clientTokenExpiresAt: Long? = null,
    val authClientId: String? = null,
)

@Serializable
data class AuthUser(
    val userId: String,
    val displayName: String,
    val email: String? = null,
    val avatarUrl: String? = null,
    val membershipTier: String = "FREE",
)

@Serializable
data class AuthSession(
    val provider: LoginProvider,
    val tokens: AuthTokens,
    val user: AuthUser,
)

data class DeviceLoginPrompt(
    val userCode: String,
    val verificationUri: String,
    val verificationUriComplete: String? = null,
    val expiresAt: Long,
)

@Serializable
data class SavedAccount(
    val userId: String,
    val displayName: String,
    val email: String? = null,
    val avatarUrl: String? = null,
    val membershipTier: String = "FREE",
    val providerCode: String = "NVIDIA",
)

@Serializable
data class PersistedAuthState(
    val sessions: List<AuthSession> = emptyList(),
    val activeUserId: String? = null,
    val selectedProvider: LoginProvider? = null,
)

@Serializable
data class StreamRegion(
    val name: String,
    val url: String,
    val pingMs: Long? = null,
)

@Serializable
data class EntitledResolution(
    val width: Int,
    val height: Int,
    val fps: Int,
)

@Serializable
data class StorageAddon(
    val type: String = "PERMANENT_STORAGE",
    val sizeGb: Double? = null,
    val usedGb: Double? = null,
    val regionName: String? = null,
    val regionCode: String? = null,
    val status: String? = null,
    val subType: String? = null,
    val autoPayEnabled: Boolean? = null,
)

@Serializable
data class SubscriptionInfo(
    val membershipTier: String = "FREE",
    val subscriptionType: String? = null,
    val subscriptionSubType: String? = null,
    val allottedHours: Double = 0.0,
    val purchasedHours: Double = 0.0,
    val rolledOverHours: Double = 0.0,
    val usedHours: Double = 0.0,
    val remainingHours: Double = 0.0,
    val totalHours: Double = 0.0,
    val state: String? = null,
    val isGamePlayAllowed: Boolean? = null,
    val isUnlimited: Boolean = false,
    val storageAddon: StorageAddon? = null,
    val entitledResolutions: List<EntitledResolution> = emptyList(),
)

@Serializable
data class AccountConnector(
    val store: String,
    val label: String,
    val supported: Boolean = true,
    val required: Boolean = false,
    val userDisplayName: String? = null,
    val userIdentifier: String? = null,
    val expiresInSeconds: Long? = null,
    val syncedGameCount: Int? = null,
    val syncState: String? = null,
    val syncDate: String? = null,
)

val AccountConnector.isLinked: Boolean
    get() = !userDisplayName.isNullOrBlank() ||
        !userIdentifier.isNullOrBlank() ||
        expiresInSeconds != null ||
        syncedGameCount != null ||
        !syncState.isNullOrBlank() ||
        !syncDate.isNullOrBlank()

@Immutable
@Serializable
data class GameVariant(
    val id: String,
    val store: String,
    val storeUrl: String? = null,
    val supportedControls: List<String> = emptyList(),
    val librarySelected: Boolean? = null,
    val libraryStatus: String? = null,
    val lastPlayedDate: String? = null,
    val gfnStatus: String? = null,
    val isFreeToPlay: Boolean = false,
)

@Immutable
@Serializable
data class GameInfo(
    val id: String,
    val uuid: String? = null,
    val launchAppId: String? = null,
    val title: String,
    val catalogSectionId: String? = null,
    val catalogSectionTitle: String? = null,
    val description: String? = null,
    val longDescription: String? = null,
    val featureLabels: List<String> = emptyList(),
    val genres: List<String> = emptyList(),
    val imageUrl: String? = null,
    val tvCardImageUrl: String? = null,
    val screenshotUrl: String? = null,
    val screenshotUrls: List<String> = emptyList(),
    val tvBannerUrl: String? = null,
    val playType: String? = null,
    val membershipTierLabel: String? = null,
    val publisherName: String? = null,
    val contentRatings: List<String> = emptyList(),
    val playabilityState: String? = null,
    val availableStores: List<String> = emptyList(),
    val searchText: String? = null,
    val lastPlayed: String? = null,
    val isInLibrary: Boolean = false,
    val selectedVariantIndex: Int = 0,
    val variants: List<GameVariant> = emptyList(),
)

private val primaryCatalogStoreKeys = setOf(
    "STEAM",
    "EPIC",
    "EPIC_GAMES_STORE",
    "EGS",
    "XBOX",
    "XBOX_GAME_PASS",
    "MICROSOFT",
    "MICROSOFT_STORE",
)

private val ownedLibraryStatuses = setOf("MANUAL", "PLATFORM_SYNC", "IN_LIBRARY")

internal fun isOwnedLibraryStatus(status: String?): Boolean =
    status in ownedLibraryStatuses

internal fun isOwnedGameVariant(variant: GameVariant): Boolean =
    isOwnedLibraryStatus(variant.libraryStatus)

internal fun isGameInLibrary(game: GameInfo): Boolean =
    game.isInLibrary || game.variants.any(::isOwnedGameVariant)

internal fun gameTrackingKey(game: GameInfo): String =
    game.uuid?.takeIf { it.isNotBlank() }
        ?: game.launchAppId?.takeIf { it.isNotBlank() }
        ?: game.id

internal fun shouldLaunchWithAccountLinked(game: GameInfo, selectedVariant: GameVariant?): Boolean {
    if (game.playType == "INSTALL_TO_PLAY") return false
    if (selectedVariant?.let(::isOwnedGameVariant) == true) return true
    return isGameInLibrary(game)
}

internal fun shouldMarkVariantOwnedBeforeLaunch(selectedVariant: GameVariant?): Boolean {
    if (selectedVariant == null || selectedVariant.id.isBlank()) return false
    return selectedVariant.libraryStatus == "NOT_OWNED"
}

internal fun GameInfo.withManuallyOwnedVariant(variantId: String): GameInfo {
    val selectedIndex = variants.indexOfFirst { it.id == variantId }
    if (selectedIndex < 0) return this
    return copy(
        isInLibrary = true,
        selectedVariantIndex = selectedIndex,
        variants = variants.mapIndexed { index, variant ->
            if (index == selectedIndex) {
                variant.copy(libraryStatus = "MANUAL")
            } else {
                variant
            }
        },
    )
}

internal fun mergeKnownLibraryGames(vararg groups: List<GameInfo>): List<GameInfo> {
    val byKey = linkedMapOf<String, GameInfo>()
    for (game in groups.flatMap { it }) {
        if (!isGameInLibrary(game)) continue
        val key = game.uuid ?: game.id
        val existing = byKey[key]
        byKey[key] = if (existing == null) game.copy(isInLibrary = true) else mergeGameInfo(existing, game).copy(isInLibrary = true)
    }
    return byKey.values.toList()
}

internal fun mergePanelGameWithMetadata(panelGame: GameInfo, metadataGame: GameInfo): GameInfo =
    mergeGameInfo(panelGame, metadataGame).copy(
        imageUrl = metadataGame.imageUrl ?: panelGame.imageUrl,
        tvCardImageUrl = metadataGame.tvCardImageUrl ?: panelGame.tvCardImageUrl,
        screenshotUrl = metadataGame.screenshotUrl ?: panelGame.screenshotUrl,
        screenshotUrls = (metadataGame.screenshotUrls + panelGame.screenshotUrls).distinct(),
        tvBannerUrl = metadataGame.tvBannerUrl ?: panelGame.tvBannerUrl,
    )

internal fun normalizeGameStore(store: String): String =
    store.uppercase(Locale.US).replace(STORE_SEPARATOR_RUN, "_")

internal fun splitGameStoreKeys(store: String): List<String> =
    store.split(",")
        .map { normalizeGameStore(it.trim()) }
        .filter { it.isNotBlank() }

internal fun isPrimaryCatalogStoreValue(store: String): Boolean {
    val storeKeys = splitGameStoreKeys(store)
    return storeKeys.isNotEmpty() && storeKeys.all { it in primaryCatalogStoreKeys }
}

internal fun gameStoreDisplayName(store: String): String {
    val parts = store.split(",")
        .map { it.trim() }
        .filter { it.isNotBlank() }
        .ifEmpty { listOf(store.trim()) }
    return parts.map { part ->
        when (normalizeGameStore(part)) {
            "EPIC", "EGS", "EPIC_GAMES_STORE" -> "Epic"
            "STEAM" -> "Steam"
            "XBOX", "XBOX_GAME_PASS" -> "Xbox"
            "MICROSOFT", "MICROSOFT_STORE" -> "Microsoft Store"
            else -> part.replace('_', ' ').lowercase(Locale.US)
                .split(WHITESPACE_RUN)
                .filter { it.isNotBlank() }
                .joinToString(" ") { word -> word.replaceFirstChar { char -> char.titlecase(Locale.US) } }
                .ifBlank { "Unknown" }
        }
    }.distinct().joinToString(" / ")
}

internal fun launchableGameVariants(variants: List<GameVariant>): List<GameVariant> {
    val uniqueVariants = variants.distinctBy { it.id }
    val individualPrimaryStores = uniqueVariants
        .map { splitGameStoreKeys(it.store) }
        .filter { it.size == 1 && it.first() in primaryCatalogStoreKeys }
        .flatten()
        .toSet()
    val filtered = uniqueVariants.filterNot { variant ->
        val storeKeys = splitGameStoreKeys(variant.store)
        storeKeys.size > 1 && storeKeys.all { it in individualPrimaryStores }
    }
    val byStore = linkedMapOf<String, GameVariant>()
    for (variant in filtered) {
        val storeKey = splitGameStoreKeys(variant.store).joinToString(",").ifBlank { normalizeGameStore(variant.store) }
        val existing = byStore[storeKey]
        if (existing == null || variantLaunchRank(variant) > variantLaunchRank(existing)) {
            byStore[storeKey] = variant
        }
    }
    return byStore.values.toList()
}

internal fun displayStoresForVariants(variants: List<GameVariant>): List<String> =
    launchableGameVariants(variants)
        .flatMap { variant -> gameStoreDisplayName(variant.store).split(" / ") }
        .map { it.trim() }
        .filter { it.isNotBlank() }
        .distinctBy { normalizeGameStore(it) }

internal fun libraryStoreDisplayNames(game: GameInfo): List<String> {
    val variants = when {
        game.variants.any(::isOwnedGameVariant) -> game.variants.filter(::isOwnedGameVariant)
        game.isInLibrary -> listOfNotNull(game.variants.firstOrNull { it.librarySelected == true })
            .ifEmpty { listOfNotNull(game.variants.getOrNull(game.selectedVariantIndex)) }
            .ifEmpty { game.variants.take(1) }
        else -> emptyList()
    }
    val variantStores = variants
        .flatMap { variant -> gameStoreDisplayName(variant.store).split(" / ") }
        .map { it.trim() }
        .filter { it.isNotBlank() }
        .distinctBy { normalizeGameStore(it) }
    if (variantStores.isNotEmpty()) return variantStores
    if (!game.isInLibrary) return emptyList()
    return game.availableStores
        .map(::gameStoreDisplayName)
        .map { it.trim() }
        .filter { it.isNotBlank() }
        .distinctBy { normalizeGameStore(it) }
}

internal fun mergeGameInfo(left: GameInfo, right: GameInfo): GameInfo {
    val variants = linkedMapOf<String, GameVariant>()
    for (variant in left.variants + right.variants) {
        val existing = variants[variant.id]
        variants[variant.id] = if (existing == null) variant else mergeGameVariant(existing, variant)
    }
    val mergedVariants = variants.values.toList()
    val selectedVariantId = left.variants.getOrNull(left.selectedVariantIndex)?.id
        ?: right.variants.getOrNull(right.selectedVariantIndex)?.id
    val selectedIndex = selectedVariantId?.let { id -> mergedVariants.indexOfFirst { it.id == id } } ?: -1
    return left.copy(
        uuid = left.uuid ?: right.uuid,
        launchAppId = left.launchAppId ?: right.launchAppId,
        description = left.description ?: right.description,
        longDescription = left.longDescription ?: right.longDescription,
        imageUrl = left.imageUrl ?: right.imageUrl,
        tvCardImageUrl = left.tvCardImageUrl ?: right.tvCardImageUrl,
        screenshotUrl = left.screenshotUrl ?: right.screenshotUrl,
        screenshotUrls = (left.screenshotUrls + right.screenshotUrls).distinct(),
        tvBannerUrl = left.tvBannerUrl ?: right.tvBannerUrl,
        playType = left.playType ?: right.playType,
        membershipTierLabel = left.membershipTierLabel ?: right.membershipTierLabel,
        publisherName = left.publisherName ?: right.publisherName,
        contentRatings = (left.contentRatings + right.contentRatings).distinct(),
        playabilityState = left.playabilityState ?: right.playabilityState,
        variants = mergedVariants,
        availableStores = displayStoresForVariants(mergedVariants),
        genres = (left.genres + right.genres).distinct(),
        featureLabels = (left.featureLabels + right.featureLabels).distinct(),
        searchText = listOfNotNull(left.searchText, right.searchText).joinToString(" ").ifBlank { null },
        lastPlayed = left.lastPlayed ?: right.lastPlayed,
        isInLibrary = left.isInLibrary || right.isInLibrary,
        selectedVariantIndex = if (selectedIndex >= 0) selectedIndex else left.selectedVariantIndex.coerceAtMost(max(mergedVariants.size - 1, 0)),
    )
}

private fun mergeGameVariant(left: GameVariant, right: GameVariant): GameVariant =
    left.copy(
        store = left.store.takeUnless { it.isBlank() || it.equals("Unknown", ignoreCase = true) } ?: right.store,
        storeUrl = left.storeUrl?.takeIf { it.isNotBlank() } ?: right.storeUrl,
        supportedControls = (left.supportedControls + right.supportedControls).distinct(),
        librarySelected = when {
            left.librarySelected == true || right.librarySelected == true -> true
            else -> left.librarySelected ?: right.librarySelected
        },
        libraryStatus = when {
            isOwnedLibraryStatus(left.libraryStatus) -> left.libraryStatus
            isOwnedLibraryStatus(right.libraryStatus) -> right.libraryStatus
            else -> left.libraryStatus ?: right.libraryStatus
        },
        lastPlayedDate = left.lastPlayedDate ?: right.lastPlayedDate,
        gfnStatus = left.gfnStatus ?: right.gfnStatus,
        isFreeToPlay = left.isFreeToPlay || right.isFreeToPlay,
    )

private fun variantLaunchRank(variant: GameVariant): Int =
    when {
        variant.librarySelected == true && isOwnedGameVariant(variant) -> 4
        isOwnedGameVariant(variant) -> 3
        variant.id.all(Char::isDigit) -> 2
        else -> 1
    }

@Serializable
data class CatalogFilterOption(
    val id: String,
    val rawId: String,
    val label: String,
    val groupId: String,
    val groupLabel: String,
)

@Serializable
data class CatalogFilterGroup(
    val id: String,
    val label: String,
    val options: List<CatalogFilterOption>,
)

@Serializable
data class CatalogSortOption(
    val id: String,
    val label: String,
    val orderBy: String,
)

@Serializable
data class CatalogBrowseResult(
    val games: List<GameInfo>,
    val numberReturned: Int = games.size,
    val numberSupported: Int = games.size,
    val totalCount: Int = games.size,
    val hasNextPage: Boolean = false,
    val endCursor: String? = null,
    val searchQuery: String = "",
    val selectedSortId: String = "relevance",
    val selectedFilterIds: List<String> = emptyList(),
    val filterGroups: List<CatalogFilterGroup> = emptyList(),
    val sortOptions: List<CatalogSortOption> = emptyList(),
)

@Serializable
data class PrintedWasteZone(
    val QueuePosition: Int,
    val LastUpdated: Long = 0,
    val Region: String,
    val eta: Long? = null,
)

@Serializable
data class PrintedWasteServerMappingEntry(
    val title: String? = null,
    val region: String? = null,
    val is4080Server: Boolean? = null,
    val is5080Server: Boolean? = null,
    val nuked: Boolean? = null,
)

@Serializable
data class PingResult(
    val url: String,
    val pingMs: Long? = null,
    val error: String? = null,
)

@Serializable
data class IceServer(
    val urls: List<String>,
    val username: String? = null,
    val credential: String? = null,
)

@Serializable
data class MediaConnectionInfo(
    val ip: String,
    val port: Int,
)

@Serializable
data class NegotiatedStreamProfile(
    val resolution: String? = null,
    val fps: Int? = null,
    val codec: VideoCodec? = null,
    val colorQuality: ColorQuality? = null,
    val enableL4S: Boolean? = null,
    val enableReflex: Boolean? = null,
)

@Serializable
data class SessionMonitorSnapshot(
    val requestedResolution: String? = null,
    val requestedFps: Int? = null,
    val returnedResolution: String? = null,
    val returnedFps: Int? = null,
    val finalSelectedResolution: String? = null,
)

@Serializable
data class SessionAdMediaFile(
    val mediaFileUrl: String? = null,
    val encodingProfile: String? = null,
)

@Serializable
data class SessionAdInfo(
    val adId: String,
    val state: Int? = null,
    val adState: Int? = null,
    val adUrl: String? = null,
    val mediaUrl: String? = null,
    val adMediaFiles: List<SessionAdMediaFile> = emptyList(),
    val clickThroughUrl: String? = null,
    val adLengthInSeconds: Double? = null,
    val durationMs: Long? = null,
    val title: String? = null,
    val description: String? = null,
)

@Serializable
data class SessionOpportunityInfo(
    val state: String? = null,
    val queuePaused: Boolean? = null,
    val gracePeriodSeconds: Int? = null,
    val message: String? = null,
    val title: String? = null,
    val description: String? = null,
)

@Serializable
data class SessionAdState(
    val isAdsRequired: Boolean = false,
    val sessionAdsRequired: Boolean? = null,
    val isQueuePaused: Boolean? = null,
    val gracePeriodSeconds: Int? = null,
    val message: String? = null,
    val sessionAds: List<SessionAdInfo> = emptyList(),
    val ads: List<SessionAdInfo> = emptyList(),
    val opportunity: SessionOpportunityInfo? = null,
    val serverSentEmptyAds: Boolean = false,
)

@Serializable
data class StreamingFeatures(
    val reflex: Boolean? = null,
    val bitDepth: Int? = null,
    val chromaFormat: Int? = null,
    val enabledL4S: Boolean? = null,
    val trueHdr: Boolean? = null,
)

@Serializable
data class SessionInfo(
    val sessionId: String,
    val status: Int,
    val timerStartedAtMs: Long? = null,
    val queuePosition: Int? = null,
    val seatSetupStep: Int? = null,
    val adState: SessionAdState? = null,
    val zone: String = "",
    val assignedZone: String? = null,
    val streamingBaseUrl: String? = null,
    val serverIp: String,
    val signalingServer: String,
    val signalingUrl: String,
    val gpuType: String? = null,
    val iceServers: List<IceServer> = emptyList(),
    val mediaConnectionInfo: MediaConnectionInfo? = null,
    val negotiatedStreamProfile: NegotiatedStreamProfile? = null,
    val monitorSnapshot: SessionMonitorSnapshot? = null,
    val requestedStreamingFeatures: StreamingFeatures? = null,
    val finalizedStreamingFeatures: StreamingFeatures? = null,
    val clientId: String? = null,
    val deviceId: String? = null,
)

/**
 * The subset of a cloud-session snapshot that actually defines the native media transport.
 *
 * Queue/status and negotiated-profile fields are refreshed while a stream is connected. They are
 * useful diagnostics, but treating the whole [SessionInfo] as a Compose effect key tears down a
 * healthy WebRTC transport whenever one of those fields changes.
 */
internal data class NativeStreamTransportIdentity(
    val sessionId: String,
    val serverIp: String,
    val signalingServer: String,
    val signalingUrl: String,
    val iceServers: List<IceServer>,
    val mediaConnectionInfo: MediaConnectionInfo?,
)

internal fun SessionInfo.nativeStreamTransportIdentity(): NativeStreamTransportIdentity =
    NativeStreamTransportIdentity(
        sessionId = sessionId,
        serverIp = serverIp,
        signalingServer = signalingServer,
        signalingUrl = signalingUrl,
        iceServers = iceServers,
        mediaConnectionInfo = mediaConnectionInfo,
    )

@Serializable
data class ActiveSessionInfo(
    val sessionId: String,
    val appId: Int,
    val gpuType: String? = null,
    val status: Int,
    val queuePosition: Int? = null,
    val seatSetupStep: Int? = null,
    val streamingBaseUrl: String? = null,
    val serverIp: String? = null,
    val signalingUrl: String? = null,
    val resolution: String? = null,
    val fps: Int? = null,
    val settingsSignature: String? = null,
)

internal fun SessionInfo.isReadyForStream(): Boolean =
    status in setOf(2, 3) &&
        serverIp.isNotBlank() &&
        signalingServer.isNotBlank() &&
        signalingUrl.isNotBlank()

/**
 * CloudMatch status 6 is a transient cleanup state, but the other statuses above 3 are terminal.
 * In particular, a stale recovered session remains at status 7 forever and must not be treated as
 * ordinary rig setup.
 */
internal fun isTerminalSessionStatus(status: Int): Boolean = status > 3 && status != 6

internal fun ActiveSessionInfo.isReadyForClaim(): Boolean =
    status in setOf(2, 3) && !serverIp.isNullOrBlank()

internal fun ActiveSessionInfo.matchesStreamGeometry(settings: StreamSettings): Boolean {
    val activeResolution = parseResolutionPixelsOrNull(resolution)
    val expectedResolution = streamResolutionPixels(settings)
    val activeFps = fps?.takeIf { it > 0 }
    return activeResolution == expectedResolution && activeFps == settings.fps
}

internal fun ActiveSessionInfo.matchesStreamSettings(settings: StreamSettings): Boolean =
    settingsSignature == streamSettingsSessionSignature(settings) && matchesStreamGeometry(settings)

internal fun activeSessionRecoveryCandidate(
    sessions: List<ActiveSessionInfo>,
    previousSessionId: String,
    launchAppId: Int?,
    settings: StreamSettings,
): ActiveSessionInfo? {
    val readySessions = sessions.filter { it.isReadyForClaim() }
    return readySessions.firstOrNull {
        it.sessionId == previousSessionId && it.matchesStreamGeometry(settings)
    } ?: launchAppId?.let { appId ->
        readySessions.firstOrNull {
            it.appId == appId && it.matchesStreamSettings(settings)
        }
    }
}

internal fun activeSessionLaunchConflict(
    sessions: List<ActiveSessionInfo>,
    launchAppId: Int?,
    settings: StreamSettings,
): ActiveSessionInfo? =
    sessions
        .filter { it.status in setOf(1, 2, 3) }
        .sortedWith(
            compareByDescending<ActiveSessionInfo> { launchAppId != null && it.appId == launchAppId }
                .thenByDescending { it.matchesStreamSettings(settings) }
                .thenByDescending { it.isReadyForClaim() }
                .thenBy { it.queuePosition ?: Int.MAX_VALUE },
        )
        .firstOrNull()

internal fun parseResolutionPixelsOrNull(value: String?): Pair<Int, Int>? {
    val parts = value?.split("x") ?: return null
    val width = parts.getOrNull(0)?.toIntOrNull()
    val height = parts.getOrNull(1)?.toIntOrNull()
    return if (width != null && height != null && width > 0 && height > 0) width to height else null
}

data class CodecCapability(
    val codec: VideoCodec,
    val decoderAvailable: Boolean,
    val encoderAvailable: Boolean,
    val hardwareDecoder: Boolean,
    val hardwareEncoder: Boolean,
    val decoderName: String? = null,
    val encoderName: String? = null,
    val realtimeSafe: Boolean = hardwareDecoder,
    val nativeDecoderAvailable: Boolean? = null,
    val webRtcDecoderAvailable: Boolean? = null,
    val webRtcHardwareDecoderAvailable: Boolean? = null,
    val webRtcDecoderName: String? = null,
    val webRtcCodecProfiles: List<String> = emptyList(),
    val maxSupportedWidth: Int? = null,
    val maxSupportedHeight: Int? = null,
)

data class RuntimeCodecReport(
    val capabilities: List<CodecCapability>,
    val nativeRuntimeSummary: String,
    val androidTvProfile: Boolean,
    val lowPowerGpuProfile: Boolean,
    val constrainedRuntimeProfile: Boolean = false,
)

data class StreamRuntimeStats(
    val bitrateKbps: Int? = null,
    val availableIncomingBitrateKbps: Int? = null,
    val pingMs: Int? = null,
    val fps: Int? = null,
    val gameFps: Int? = null,
    val receivedFps: Int? = null,
    val decodedFps: Int? = null,
    val resolution: String? = null,
    val codec: String? = null,
    val decodeMs: Double? = null,
    val jitterMs: Double? = null,
    val packetLossPct: Double? = null,
    val packetsLostDelta: Long? = null,
    val packetsReceivedDelta: Long? = null,
    val processCpuPercent: Double? = null,
    val deviceCpuCapacityPercent: Double? = null,
    val cpuLogicalCoreCount: Int? = null,
)

internal fun CodecCapability.streamingDecoderAvailable(): Boolean =
    webRtcDecoderAvailable ?: decoderAvailable

internal fun CodecCapability.streamingHardwareDecoderAvailable(): Boolean =
    webRtcHardwareDecoderAvailable ?: (nativeDecoderAvailable?.let { it && hardwareDecoder } ?: hardwareDecoder)

internal fun CodecCapability.streamingDecoderName(): String? =
    webRtcDecoderName ?: decoderName

internal fun CodecCapability.streamingRealtimeSafe(): Boolean =
    streamingDecoderUsableForLaunch()

internal fun CodecCapability.hasKnownHighResolutionAv1Failure(settings: StreamSettings): Boolean {
    if (codec != VideoCodec.AV1) return false
    val decoder = streamingDecoderName()?.lowercase(Locale.US).orEmpty()
    if (decoder != KNOWN_AMLOGIC_AV1_DECODER) return false
    val (width, height) = streamResolutionPixels(settings)
    return width.toLong() * height.toLong() >= ANDROID_1440P_PIXEL_BUDGET.toLong()
}

internal fun CodecCapability.streamingDecoderUsableForLaunch(): Boolean {
    if (codec == VideoCodec.H264) return webRtcDecoderAvailable ?: decoderAvailable

    // The stream is decoded by the WebRTC decoder factory, so its successful hardware
    // probe is authoritative. The Media NDK probe is only a secondary diagnostic and can
    // legitimately disagree on devices whose codec is exposed through WebRTC's factory.
    if (webRtcDecoderAvailable != null) {
        return webRtcDecoderAvailable && webRtcHardwareDecoderAvailable == true
    }

    return nativeDecoderAvailable == true && hardwareDecoder && realtimeSafe
}

private fun RuntimeCodecReport.bestStreamingFallbackCodec(): VideoCodec =
    listOf(VideoCodec.H264, VideoCodec.H265, VideoCodec.AV1)
        .firstOrNull { codec -> capabilities.firstOrNull { it.codec == codec }?.streamingDecoderUsableForLaunch() == true }
        ?: VideoCodec.H264

internal fun StreamSettings.adjustedForDevice(report: RuntimeCodecReport?): StreamSettings {
    val availableSettings = withAndroidSettingsAvailability()
    if (availableSettings != this) return availableSettings.adjustedForDevice(report)

    if (
        report?.androidTvProfile == true &&
        report.lowPowerGpuProfile &&
        !report.constrainedRuntimeProfile
    ) {
        val requestedCapability = report.capabilities.firstOrNull { it.codec == codec }
        val requestedCodecUsable = requestedCapability?.streamingDecoderUsableForLaunch() ?: (codec == VideoCodec.H264)
        val knownAv1Failure = requestedCapability?.hasKnownHighResolutionAv1Failure(this) == true
        val usableCodec = when {
            knownAv1Failure -> report.bestCodecForKnownHighResolutionAv1Failure(this)
            requestedCodecUsable -> codec
            else -> report.bestStreamingFallbackCodec()
        }
        val effectiveCodec = if (report.capabilities.firstOrNull { it.codec == usableCodec }.supportsStreamResolution(this) != false) {
            usableCodec
        } else {
            report.bestStreamingCodecForResolution(copy(codec = usableCodec)) ?: usableCodec
        }
        val lowPowerProfile = copy(
            codec = effectiveCodec,
            colorQuality = ColorQuality.EightBit420,
            fps = minOf(fps, LOW_POWER_TV_FPS_CAP),
            hdrEnabled = false,
        ).withStableAndroidCloudMatchProfile()
            .withoutAndroidTvSharpening(report)
        // A codec probe may be incomplete or conservative, especially on Android TV. It can
        // choose a safer codec/FPS, but it must not silently replace the user's geometry or
        // selected bitrate ceiling.
        // The server-negotiated and decoded dimensions are reported separately at runtime.
        return lowPowerProfile.copy(
            resolution = normalizeStreamResolutionForAspect(resolution, aspectRatio),
        )
    }

    val capability = report?.capabilities?.firstOrNull { it.codec == codec }
    val codecSupported = capability?.streamingDecoderUsableForLaunch() ?: true
    val knownAv1Failure = report?.androidTvProfile == true &&
        capability?.hasKnownHighResolutionAv1Failure(this) == true
    val effectiveCodec = when {
        knownAv1Failure -> requireNotNull(report).bestCodecForKnownHighResolutionAv1Failure(this)
        !codecSupported -> requireNotNull(report).bestStreamingFallbackCodec()
        capability.supportsStreamResolution(this) != false -> codec
        else -> report?.bestStreamingCodecForResolution(this) ?: codec
    }

    val adjusted = (if (effectiveCodec == codec) this else copy(codec = effectiveCodec)).withCodecColorCompatibility()
    val compatible = when (effectiveCodec) {
        VideoCodec.H264 -> adjusted.copy(colorQuality = ColorQuality.EightBit420)
        VideoCodec.H265,
        VideoCodec.AV1 -> adjusted.copy(
            colorQuality = adjusted.androidWebRtcColorQuality(),
        )
    }.withStableAndroidCloudMatchProfile()
        .withoutAndroidTvSharpening(report)
    return compatible.copy(
        resolution = normalizeStreamResolutionForAspect(compatible.resolution, compatible.aspectRatio),
    )
}

private fun RuntimeCodecReport.bestCodecForKnownHighResolutionAv1Failure(settings: StreamSettings): VideoCodec =
    listOf(VideoCodec.H265, VideoCodec.H264)
        .firstOrNull { codec ->
            val capability = capabilities.firstOrNull { it.codec == codec }
            capability != null &&
                capability.streamingDecoderUsableForLaunch() &&
                capability.launchResolutionSupport(settings.copy(codec = codec)) != false
        }
        ?: VideoCodec.H264

private fun RuntimeCodecReport.bestStreamingCodecForResolution(settings: StreamSettings): VideoCodec? =
    listOf(VideoCodec.H265, VideoCodec.AV1, VideoCodec.H264)
        .asSequence()
        .filter { it != settings.codec }
        .mapNotNull { candidate -> capabilities.firstOrNull { it.codec == candidate } }
        .firstOrNull { capability ->
            capability.streamingDecoderUsableForLaunch() && capability.supportsStreamResolution(settings) == true
        }
        ?.codec

private fun CodecCapability?.launchResolutionSupport(settings: StreamSettings): Boolean? {
    this ?: return null
    val probedSupport = supportsStreamResolution(settings)
    if (probedSupport == true) return true

    val normalized = normalizeStreamResolutionForAspect(settings.resolution, settings.aspectRatio)
    val (width, height) = parseResolutionPixels(normalized)
    // Some Android TV codec implementations omit or underreport VideoCapabilities even
    // though WebRTC successfully opens their hardware decoder. Honor the selected 1440p
    // profile in that confirmed path; keep higher unknown profiles on the safety cap.
    val confirmedHardware1440pPath = streamingDecoderUsableForLaunch() &&
        streamingHardwareDecoderAvailable() &&
        width * height <= ANDROID_1440P_PIXEL_BUDGET
    return if (confirmedHardware1440pPath) true else probedSupport
}

private fun CodecCapability?.supportsStreamResolution(settings: StreamSettings): Boolean? {
    this ?: return null
    val maxWidth = maxSupportedWidth ?: return null
    val maxHeight = maxSupportedHeight ?: return null
    val normalized = normalizeStreamResolutionForAspect(settings.resolution, settings.aspectRatio)
    val (width, height) = parseResolutionPixels(normalized)
    val maxPixelCount = (maxWidth * maxHeight * DECODER_RESOLUTION_HEADROOM).roundToInt()
    return width <= maxWidth * 2 &&
        height <= maxHeight * 2 &&
        width * height <= maxPixelCount
}

internal fun StreamSettings.androidSafeVideoFallback(): StreamSettings =
    copy(
        fps = minOf(fps, 60),
        codec = VideoCodec.H264,
        colorQuality = ColorQuality.EightBit420,
        hdrEnabled = false,
        streamSharpeningEnabled = false,
    )

private fun StreamSettings.androidWebRtcColorQuality(): ColorQuality {
    val compatible = withCodecColorCompatibility()
    if (compatible.hdrEnabled) return when (compatible.colorQuality) {
        ColorQuality.TenBit420,
        ColorQuality.TenBit444,
        -> compatible.colorQuality
        else -> ColorQuality.TenBit420
    }
    return when (compatible.colorQuality) {
        ColorQuality.EightBit420,
        ColorQuality.EightBit444,
        -> compatible.colorQuality
        else -> ColorQuality.EightBit420
    }
}

private fun StreamSettings.withStableAndroidCloudMatchProfile(): StreamSettings {
    val normalizedResolution = normalizeStreamResolutionForAspect(resolution, aspectRatio)
    // The provider's low 21:9 mode was observed at 60 FPS. Requesting the retired 1376x640 panel
    // geometry at high refresh made CloudMatch select 1680x720 before the streamer cropped it.
    val geometryCompatibleFps = if (normalizedResolution == LOW_ULTRAWIDE_STREAM_RESOLUTION) {
        LOW_ULTRAWIDE_STREAM_MAX_FPS
    } else {
        MAX_ULTIMATE_STREAM_FPS
    }
    return copy(
        resolution = normalizedResolution,
        fps = minOf(fps, geometryCompatibleFps),
        hdrEnabled = hdrEnabled && codec != VideoCodec.H264,
    )
}

internal fun StreamSettings.lowPowerPerformanceWarningReasons(report: RuntimeCodecReport?): List<String> {
    if (report?.lowPowerGpuProfile != true && report?.constrainedRuntimeProfile != true) return emptyList()

    val normalizedResolution = normalizeStreamResolutionForAspect(resolution, aspectRatio)
    val (width, height) = parseResolutionPixels(normalizedResolution)
    return buildList {
        if (width * height > LOW_POWER_RECOMMENDED_PIXEL_COUNT) add("$normalizedResolution resolution")
        if (fps > LOW_POWER_RECOMMENDED_FPS) add("$fps FPS")
        if (maxBitrateMbps > LOW_POWER_RECOMMENDED_BITRATE_MBPS) add("$maxBitrateMbps Mbps bitrate")
        if (hdrEnabled) add("HDR")
        if (streamSharpeningEnabled) add("stream sharpening")
    }
}

private fun StreamSettings.withoutAndroidTvSharpening(report: RuntimeCodecReport?): StreamSettings =
    if (
        report?.androidTvProfile == true &&
        report.constrainedRuntimeProfile == false &&
        streamSharpeningEnabled
    ) {
        copy(streamSharpeningEnabled = false)
    } else {
        this
    }

private fun StreamResolutionOption.pixelCount(): Int {
    val (width, height) = parseResolutionPixels(value)
    return width * height
}

private const val LOW_POWER_TV_FPS_CAP = 60
private const val LEGACY_PORTAL_STREAM_RESOLUTION = "1376x640"
private const val LEGACY_PORTAL_STREAM_ASPECT = "19.5:9"
private const val LOW_ULTRAWIDE_STREAM_RESOLUTION = "1376x590"
private const val LOW_ULTRAWIDE_STREAM_MAX_FPS = 60
private const val MAX_STANDARD_STREAM_FPS = 60
private const val MAX_ULTIMATE_STREAM_FPS = 360
private const val MIN_STREAM_FPS = 30
private const val LOW_POWER_RECOMMENDED_PIXEL_COUNT = 1280 * 720
private const val LOW_POWER_RECOMMENDED_FPS = 30
private const val LOW_POWER_RECOMMENDED_BITRATE_MBPS = 12
private const val ANDROID_1440P_PIXEL_BUDGET = 2560 * 1440
private const val KNOWN_AMLOGIC_AV1_DECODER = "omx.amlogic.av1.decoder.awesome"
private const val DECODER_RESOLUTION_HEADROOM = 1.4f

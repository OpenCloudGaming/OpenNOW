package com.opencloudgaming.opennow

import kotlinx.serialization.Serializable
import java.util.Locale
import kotlin.math.abs
import kotlin.math.max

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
}

@Serializable
enum class StreamStatsStyle {
    Compact,
    Detailed,
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
    val region: String = "",
    val keyboardLayout: String = "en-US",
    val gameLanguage: String = "en_US",
    val sessionProxyEnabled: Boolean = false,
    val sessionProxyUrl: String = "",
    val enableL4S: Boolean = false,
    val enableCloudGsync: Boolean = false,
    val mouseSensitivity: Float = 1f,
    val mouseAcceleration: Int = 1,
    val streamSharpeningEnabled: Boolean = false,
    val streamSharpeningAmount: Float = 0.25f,
    val microphoneMode: MicrophoneMode = MicrophoneMode.Disabled,
    val microphoneDeviceId: String = "",
)

@Serializable
data class AndroidTouchSettings(
    val enabled: Boolean = true,
    val mousePad: Boolean = true,
    val opacity: Float = 0.82f,
    val scale: Float = 1f,
    val buttonScale: Float = 1f,
    val stickScale: Float = 1f,
    val edgePaddingDp: Float = 14f,
    val bottomPaddingDp: Float = 10f,
    val leftOffsetXDp: Float = 0f,
    val leftOffsetYDp: Float = 0f,
    val rightOffsetXDp: Float = 0f,
    val rightOffsetYDp: Float = 0f,
)

@Serializable
data class AppSettings(
    val stream: StreamSettings = StreamSettings(),
    val posterSizeScale: Float = 1f,
    val compactGameCards: Boolean = true,
    val showGameStoreLabels: Boolean = true,
    val expressiveUi: Boolean = true,
    val dynamicColor: Boolean = false,
    val uiAccent: UiAccent = UiAccent.OpenNow,
    val nerdMode: Boolean = false,
    val hideStreamButtons: Boolean = false,
    val showAntiAfkIndicator: Boolean = true,
    val showStatsOnLaunch: Boolean = false,
    val streamStatsStyle: StreamStatsStyle = StreamStatsStyle.Compact,
    val phoneRumbleFallback: Boolean = true,
    val hideServerSelector: Boolean = false,
    val controllerMode: Boolean = false,
    val controllerUiSounds: Boolean = false,
    val controllerBackgroundAnimations: Boolean = true,
    val controllerThemeStyle: String = "aurora",
    val controllerThemeColor: ControllerThemeRgb = ControllerThemeRgb(),
    val controllerLibraryGameBackdrop: Boolean = true,
    val autoLoadControllerLibrary: Boolean = false,
    val autoFullScreen: Boolean = true,
    val streamIntroMusic: Boolean = false,
    val stretchStreamToFill: Boolean = false,
    val favoriteGameIds: List<String> = emptyList(),
    val defaultGameVariantIds: Map<String, String> = emptyMap(),
    val sessionCounterEnabled: Boolean = false,
    val sessionClockShowEveryMinutes: Int = 60,
    val sessionClockShowDurationSeconds: Int = 30,
    val clipboardPaste: Boolean = true,
    val androidTouch: AndroidTouchSettings = AndroidTouchSettings(),
    val discordRichPresence: Boolean = false,
    val autoCheckForUpdates: Boolean = true,
    val analyticsOptOut: Boolean = false,
    val allowEscapeToExitFullscreen: Boolean = false,
)

internal fun streamResolutionPixels(settings: StreamSettings): Pair<Int, Int> {
    return parseResolutionPixels(normalizeStreamResolutionForAspect(settings.resolution, settings.aspectRatio))
}

internal fun streamResolutionOptionsForAspect(aspectRatio: String): List<String> =
    STREAM_RESOLUTION_OPTIONS.filter { it.aspectRatio == aspectRatio }.map { it.value }

internal fun streamResolutionChoicesForAspect(aspectRatio: String): List<StreamResolutionChoice> =
    STREAM_RESOLUTION_OPTIONS.filter { it.aspectRatio == aspectRatio }.map { it.toChoice() }

internal fun streamAspectRatioOptions(): List<String> =
    STREAM_RESOLUTION_OPTIONS.map { it.aspectRatio }.distinct()

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
    val (width, height) = streamResolutionPixels(settings)
    return listOf(
        "opennow-android-stream-v1",
        "res=${width}x$height",
        "fps=${settings.fps}",
        "bitrate=${settings.maxBitrateMbps}",
        "codec=${settings.codec.name}",
        "color=${settings.colorQuality.name}",
        "hdr=${if (settings.hdrEnabled) 1 else 0}",
        "l4s=${if (settings.enableL4S) 1 else 0}",
        "gsync=${if (settings.enableCloudGsync) 1 else 0}",
        "keyboard=${settings.keyboardLayout.trim()}",
        "language=${settings.gameLanguage.trim()}",
    ).joinToString(";")
}

private data class StreamResolutionOption(
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
        get() = when (requiredPlan) {
            StreamResolutionPlan.Free -> null
            StreamResolutionPlan.Priority -> "Priority"
            StreamResolutionPlan.Ultimate -> "Ultimate"
        }

    fun isAvailableFor(subscriptionInfo: SubscriptionInfo?, fallbackMembershipTier: String?): Boolean {
        if (requiredPlan == StreamResolutionPlan.Free) return true
        return streamResolutionPlanRank(planForMembershipTier(subscriptionInfo?.membershipTier ?: fallbackMembershipTier)) >= streamResolutionPlanRank(requiredPlan)
    }
}

internal fun hasUltimateStreamingPlan(subscriptionInfo: SubscriptionInfo?, fallbackMembershipTier: String?): Boolean =
    streamResolutionPlanRank(planForMembershipTier(subscriptionInfo?.membershipTier?.takeIf { it.isNotBlank() } ?: fallbackMembershipTier)) >=
        streamResolutionPlanRank(StreamResolutionPlan.Ultimate)

internal fun StreamSettings.withHdrAllowed(subscriptionInfo: SubscriptionInfo?, fallbackMembershipTier: String?): StreamSettings =
    if (hdrEnabled && !hasUltimateStreamingPlan(subscriptionInfo, fallbackMembershipTier)) copy(hdrEnabled = false) else this

internal fun StreamSettings.withResolutionAllowed(subscriptionInfo: SubscriptionInfo?, fallbackMembershipTier: String?): StreamSettings {
    val allowedAspectRatio = if (streamResolutionChoicesForAspect(aspectRatio).any { it.isAvailableFor(subscriptionInfo, fallbackMembershipTier) }) {
        aspectRatio
    } else {
        "16:9"
    }
    val allowedResolution = normalizeStreamResolutionForAspectAndPlan(resolution, allowedAspectRatio, subscriptionInfo, fallbackMembershipTier)
    return if (allowedResolution == resolution && allowedAspectRatio == aspectRatio) this else copy(resolution = allowedResolution, aspectRatio = allowedAspectRatio)
}

private val STREAM_RESOLUTION_OPTIONS = listOf(
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
    StreamResolutionOption("1680x720", "21:9", "720"),
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
    "1080" to mapOf("16:9" to "1920x1080", "16:10" to "1920x1200", "4:3" to "1600x1200", "21:9" to "2560x1080", "32:9" to "3840x1080"),
    "1440" to mapOf("16:9" to "2560x1440", "16:10" to "2560x1600", "21:9" to "3440x1440", "24:10" to "3840x1600", "32:9" to "5120x1440"),
    "2160" to mapOf("16:9" to "3840x2160", "16:10" to "3456x2160", "21:9" to "5120x2160"),
    "2880" to mapOf("16:9" to "5120x2880"),
)

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

private fun planForMembershipTier(membershipTier: String?): StreamResolutionPlan {
    val normalized = membershipTier.orEmpty().uppercase(Locale.US).replace(Regex("[^A-Z0-9]+"), "")
    return when {
        normalized.contains("ULTIMATE") || normalized.contains("RTX3080") -> StreamResolutionPlan.Ultimate
        normalized.contains("PRIORITY") || normalized.contains("PERFORMANCE") || normalized.contains("FOUNDERS") -> StreamResolutionPlan.Priority
        else -> StreamResolutionPlan.Free
    }
}

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

@Serializable
data class GameVariant(
    val id: String,
    val store: String,
    val supportedControls: List<String> = emptyList(),
    val librarySelected: Boolean? = null,
    val libraryStatus: String? = null,
    val lastPlayedDate: String? = null,
    val gfnStatus: String? = null,
)

@Serializable
data class GameInfo(
    val id: String,
    val uuid: String? = null,
    val launchAppId: String? = null,
    val title: String,
    val description: String? = null,
    val longDescription: String? = null,
    val featureLabels: List<String> = emptyList(),
    val genres: List<String> = emptyList(),
    val imageUrl: String? = null,
    val screenshotUrl: String? = null,
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

internal fun shouldLaunchWithAccountLinked(game: GameInfo, selectedVariant: GameVariant?): Boolean {
    if (game.playType == "INSTALL_TO_PLAY") return false
    if (selectedVariant?.let(::isOwnedGameVariant) == true) return true
    return isGameInLibrary(game)
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

internal fun normalizeGameStore(store: String): String =
    store.uppercase(Locale.US).replace(Regex("[\\s-]+"), "_")

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
                .split(Regex("\\s+"))
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

private fun mergeGameInfo(left: GameInfo, right: GameInfo): GameInfo {
    val variants = (left.variants + right.variants).distinctBy { it.id }
    val selectedVariantId = left.variants.getOrNull(left.selectedVariantIndex)?.id
        ?: right.variants.getOrNull(right.selectedVariantIndex)?.id
    val selectedIndex = selectedVariantId?.let { id -> variants.indexOfFirst { it.id == id } } ?: -1
    return left.copy(
        uuid = left.uuid ?: right.uuid,
        launchAppId = left.launchAppId ?: right.launchAppId,
        description = left.description ?: right.description,
        longDescription = left.longDescription ?: right.longDescription,
        imageUrl = left.imageUrl ?: right.imageUrl,
        screenshotUrl = left.screenshotUrl ?: right.screenshotUrl,
        playType = left.playType ?: right.playType,
        membershipTierLabel = left.membershipTierLabel ?: right.membershipTierLabel,
        publisherName = left.publisherName ?: right.publisherName,
        contentRatings = (left.contentRatings + right.contentRatings).distinct(),
        playabilityState = left.playabilityState ?: right.playabilityState,
        variants = variants,
        availableStores = displayStoresForVariants(variants),
        genres = (left.genres + right.genres).distinct(),
        featureLabels = (left.featureLabels + right.featureLabels).distinct(),
        searchText = listOfNotNull(left.searchText, right.searchText).joinToString(" ").ifBlank { null },
        lastPlayed = left.lastPlayed ?: right.lastPlayed,
        isInLibrary = left.isInLibrary || right.isInLibrary,
        selectedVariantIndex = if (selectedIndex >= 0) selectedIndex else left.selectedVariantIndex.coerceAtMost(max(variants.size - 1, 0)),
    )
}

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
    val enableCloudGsync: Boolean? = null,
    val enableReflex: Boolean? = null,
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
    val cloudGsync: Boolean? = null,
    val chromaFormat: Int? = null,
    val enabledL4S: Boolean? = null,
    val trueHdr: Boolean? = null,
)

@Serializable
data class SessionInfo(
    val sessionId: String,
    val status: Int,
    val queuePosition: Int? = null,
    val seatSetupStep: Int? = null,
    val adState: SessionAdState? = null,
    val zone: String = "",
    val streamingBaseUrl: String? = null,
    val serverIp: String,
    val signalingServer: String,
    val signalingUrl: String,
    val gpuType: String? = null,
    val iceServers: List<IceServer> = emptyList(),
    val mediaConnectionInfo: MediaConnectionInfo? = null,
    val negotiatedStreamProfile: NegotiatedStreamProfile? = null,
    val requestedStreamingFeatures: StreamingFeatures? = null,
    val finalizedStreamingFeatures: StreamingFeatures? = null,
    val clientId: String? = null,
    val deviceId: String? = null,
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

internal fun ActiveSessionInfo.isReadyForClaim(): Boolean =
    status in setOf(2, 3) && !serverIp.isNullOrBlank()

internal fun ActiveSessionInfo.matchesStreamSettings(settings: StreamSettings): Boolean {
    if (settingsSignature != streamSettingsSessionSignature(settings)) return false
    val activeResolution = parseResolutionPixelsOrNull(resolution)
    val expectedResolution = streamResolutionPixels(settings)
    val activeFps = fps?.takeIf { it > 0 }
    return activeResolution == expectedResolution && activeFps == settings.fps
}

private fun parseResolutionPixelsOrNull(value: String?): Pair<Int, Int>? {
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
    val webRtcDecoderAvailable: Boolean? = null,
    val webRtcHardwareDecoderAvailable: Boolean? = null,
    val webRtcDecoderName: String? = null,
    val webRtcCodecProfiles: List<String> = emptyList(),
)

data class RuntimeCodecReport(
    val capabilities: List<CodecCapability>,
    val nativeRuntimeSummary: String,
    val androidTvProfile: Boolean,
    val lowPowerGpuProfile: Boolean,
)

data class StreamRuntimeStats(
    val bitrateKbps: Int? = null,
    val pingMs: Int? = null,
    val fps: Int? = null,
    val resolution: String? = null,
    val codec: String? = null,
)

internal fun CodecCapability.streamingDecoderAvailable(): Boolean =
    webRtcDecoderAvailable ?: decoderAvailable

internal fun CodecCapability.streamingHardwareDecoderAvailable(): Boolean =
    webRtcHardwareDecoderAvailable ?: hardwareDecoder

internal fun CodecCapability.streamingDecoderName(): String? =
    webRtcDecoderName ?: decoderName

internal fun CodecCapability.streamingRealtimeSafe(): Boolean =
    streamingDecoderAvailable() && (codec == VideoCodec.H264 || streamingHardwareDecoderAvailable() || webRtcDecoderAvailable == true)

internal fun CodecCapability.streamingDecoderUsableForLaunch(): Boolean {
    if (webRtcDecoderAvailable == false) return false
    if (codec == VideoCodec.H264) return webRtcDecoderAvailable ?: decoderAvailable
    if (webRtcDecoderAvailable != true || webRtcHardwareDecoderAvailable != true) return false
    return !decoderAvailable || (hardwareDecoder && realtimeSafe)
}

private fun RuntimeCodecReport.bestStreamingFallbackCodec(): VideoCodec =
    listOf(VideoCodec.H264, VideoCodec.AV1, VideoCodec.H265)
        .firstOrNull { codec -> capabilities.firstOrNull { it.codec == codec }?.streamingDecoderUsableForLaunch() == true }
        ?: VideoCodec.H264

internal fun StreamSettings.adjustedForDevice(report: RuntimeCodecReport?): StreamSettings {
    if (report?.androidTvProfile == true && report.lowPowerGpuProfile) {
        return copy(
            codec = VideoCodec.H264,
            colorQuality = ColorQuality.EightBit420,
            maxBitrateMbps = minOf(maxBitrateMbps, LOW_POWER_TV_BITRATE_CAP_MBPS),
            fps = minOf(fps, LOW_POWER_TV_FPS_CAP),
        ).cappedResolution(LOW_POWER_TV_MAX_WIDTH, LOW_POWER_TV_MAX_HEIGHT)
    }

    val capability = report?.capabilities?.firstOrNull { it.codec == codec }
    val codecSupported = capability?.streamingDecoderUsableForLaunch() ?: true
    val effectiveCodec = if (codecSupported) codec else report?.bestStreamingFallbackCodec() ?: VideoCodec.H264

    val profileBitrateCap = when {
        !codecSupported -> 35
        report?.lowPowerGpuProfile == true -> 25
        report?.androidTvProfile == true -> 35
        effectiveCodec == VideoCodec.H264 -> 75
        else -> 75
    }

    val adjusted = if (effectiveCodec == codec) this else copy(codec = effectiveCodec)
    return when (effectiveCodec) {
        VideoCodec.H264 -> adjusted.copy(colorQuality = ColorQuality.EightBit420, maxBitrateMbps = minOf(adjusted.maxBitrateMbps, profileBitrateCap))
        VideoCodec.H265,
        VideoCodec.AV1 -> adjusted.copy(
            colorQuality = adjusted.androidWebRtcColorQuality(),
            maxBitrateMbps = minOf(adjusted.maxBitrateMbps, profileBitrateCap),
        )
    }
}

internal fun StreamSettings.androidSafeVideoFallback(): StreamSettings =
    copy(
        resolution = "1920x1080",
        aspectRatio = "16:9",
        fps = minOf(fps, 60),
        maxBitrateMbps = minOf(maxBitrateMbps, 75),
        codec = VideoCodec.H264,
        colorQuality = ColorQuality.EightBit420,
        hdrEnabled = false,
        enableCloudGsync = false,
    )

private fun StreamSettings.androidWebRtcColorQuality(): ColorQuality {
    if (hdrEnabled) return when (colorQuality) {
        ColorQuality.TenBit420,
        ColorQuality.TenBit444,
        -> colorQuality
        else -> ColorQuality.TenBit420
    }
    return when (colorQuality) {
        ColorQuality.EightBit420,
        ColorQuality.EightBit444,
        -> colorQuality
        else -> ColorQuality.EightBit420
    }
}

private fun StreamSettings.cappedResolution(maxWidth: Int, maxHeight: Int): StreamSettings {
    val normalized = normalizeStreamResolutionForAspect(resolution, aspectRatio)
    val (width, height) = parseResolutionPixels(normalized)
    if (width <= maxWidth && height <= maxHeight) return copy(resolution = normalized)

    val sameAspect = STREAM_RESOLUTION_OPTIONS
        .filter { it.aspectRatio == aspectRatio && it.fitsWithin(maxWidth, maxHeight) }
        .maxByOrNull { it.pixelCount() }
    val fallback = STREAM_RESOLUTION_OPTIONS
        .filter { it.fitsWithin(maxWidth, maxHeight) }
        .maxWithOrNull(compareBy<StreamResolutionOption> { it.pixelCount() }.thenBy { if (it.aspectRatio == "16:9") 1 else 0 })
    val capped = sameAspect ?: fallback ?: StreamResolutionOption("1280x720", "16:9", "720")
    return copy(resolution = capped.value, aspectRatio = capped.aspectRatio)
}

private fun StreamResolutionOption.fitsWithin(maxWidth: Int, maxHeight: Int): Boolean {
    val (width, height) = parseResolutionPixels(value)
    return width <= maxWidth && height <= maxHeight
}

private fun StreamResolutionOption.pixelCount(): Int {
    val (width, height) = parseResolutionPixels(value)
    return width * height
}

private const val LOW_POWER_TV_MAX_WIDTH = 1920
private const val LOW_POWER_TV_MAX_HEIGHT = 1080
private const val LOW_POWER_TV_BITRATE_CAP_MBPS = 25
private const val LOW_POWER_TV_FPS_CAP = 60

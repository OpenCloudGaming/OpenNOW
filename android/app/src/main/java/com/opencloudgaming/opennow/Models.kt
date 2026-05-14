package com.opencloudgaming.opennow

import kotlinx.serialization.Serializable

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
data class StreamSettings(
    val resolution: String = "1920x1080",
    val aspectRatio: String = "16:9",
    val fps: Int = 60,
    val maxBitrateMbps: Int = 75,
    val codec: VideoCodec = VideoCodec.H264,
    val colorQuality: ColorQuality = ColorQuality.TenBit420,
    val region: String = "",
    val keyboardLayout: String = "en-US",
    val gameLanguage: String = "en_US",
    val enableL4S: Boolean = false,
    val enableCloudGsync: Boolean = false,
    val mouseSensitivity: Float = 1f,
    val mouseAcceleration: Int = 1,
    val microphoneMode: MicrophoneMode = MicrophoneMode.Disabled,
    val microphoneDeviceId: String = "",
)

@Serializable
data class AndroidTouchSettings(
    val enabled: Boolean = false,
    val mousePad: Boolean = false,
    val opacity: Float = 0.82f,
    val scale: Float = 1f,
    val buttonScale: Float = 1f,
    val stickScale: Float = 1f,
)

@Serializable
data class AppSettings(
    val stream: StreamSettings = StreamSettings(),
    val posterSizeScale: Float = 1f,
    val hideStreamButtons: Boolean = false,
    val showAntiAfkIndicator: Boolean = true,
    val showStatsOnLaunch: Boolean = false,
    val hideServerSelector: Boolean = false,
    val controllerMode: Boolean = false,
    val controllerUiSounds: Boolean = false,
    val controllerBackgroundAnimations: Boolean = false,
    val controllerThemeStyle: String = "aurora",
    val controllerThemeColor: ControllerThemeRgb = ControllerThemeRgb(),
    val controllerLibraryGameBackdrop: Boolean = true,
    val autoLoadControllerLibrary: Boolean = false,
    val autoFullScreen: Boolean = false,
    val favoriteGameIds: List<String> = emptyList(),
    val sessionCounterEnabled: Boolean = false,
    val sessionClockShowEveryMinutes: Int = 60,
    val sessionClockShowDurationSeconds: Int = 30,
    val clipboardPaste: Boolean = false,
    val mouseCapture: Boolean = false,
    val androidTouch: AndroidTouchSettings = AndroidTouchSettings(),
    val discordRichPresence: Boolean = false,
    val autoCheckForUpdates: Boolean = true,
    val allowEscapeToExitFullscreen: Boolean = false,
)

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
    val streamingBaseUrl: String? = null,
    val serverIp: String? = null,
    val signalingUrl: String? = null,
    val resolution: String? = null,
    val fps: Int? = null,
)

data class CodecCapability(
    val codec: VideoCodec,
    val decoderAvailable: Boolean,
    val encoderAvailable: Boolean,
    val hardwareDecoder: Boolean,
    val hardwareEncoder: Boolean,
    val decoderName: String? = null,
    val encoderName: String? = null,
)

data class RuntimeCodecReport(
    val capabilities: List<CodecCapability>,
    val nativeRuntimeSummary: String,
    val androidTvProfile: Boolean,
    val lowPowerGpuProfile: Boolean,
)

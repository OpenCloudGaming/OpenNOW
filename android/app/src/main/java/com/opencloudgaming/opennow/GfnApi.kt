package com.opencloudgaming.opennow

import android.content.Context
import android.content.Intent
import android.net.Uri
import android.os.Build
import android.os.SystemClock
import android.util.Base64
import androidx.browser.customtabs.CustomTabsIntent
import kotlinx.coroutines.CancellationException
import kotlinx.coroutines.Dispatchers
import kotlinx.coroutines.async
import kotlinx.coroutines.channels.Channel
import kotlinx.coroutines.coroutineScope
import kotlinx.coroutines.delay
import kotlinx.coroutines.sync.Mutex
import kotlinx.coroutines.sync.withLock
import kotlinx.coroutines.withContext
import kotlinx.serialization.json.Json
import kotlinx.serialization.json.JsonArray
import kotlinx.serialization.json.JsonElement
import kotlinx.serialization.json.JsonNull
import kotlinx.serialization.json.JsonObject
import kotlinx.serialization.json.JsonObjectBuilder
import kotlinx.serialization.json.JsonPrimitive
import kotlinx.serialization.json.booleanOrNull
import kotlinx.serialization.json.buildJsonArray
import kotlinx.serialization.json.buildJsonObject
import kotlinx.serialization.json.contentOrNull
import kotlinx.serialization.json.doubleOrNull
import kotlinx.serialization.json.intOrNull
import kotlinx.serialization.json.jsonArray
import kotlinx.serialization.json.jsonObject
import kotlinx.serialization.json.jsonPrimitive
import kotlinx.serialization.json.longOrNull
import kotlinx.serialization.json.put
import kotlinx.serialization.json.putJsonArray
import kotlinx.serialization.json.putJsonObject
import okhttp3.Dns
import okhttp3.FormBody
import okhttp3.Headers
import okhttp3.HttpUrl
import okhttp3.HttpUrl.Companion.toHttpUrl
import okhttp3.HttpUrl.Companion.toHttpUrlOrNull
import okhttp3.MediaType.Companion.toMediaType
import okhttp3.OkHttpClient
import okhttp3.Request
import okhttp3.RequestBody.Companion.toRequestBody
import okhttp3.Credentials
import okhttp3.dnsoverhttps.DnsOverHttps
import java.io.Closeable
import java.io.BufferedReader
import java.io.InputStreamReader
import java.io.OutputStreamWriter
import java.net.BindException
import java.net.InetSocketAddress
import java.net.InetAddress
import java.net.Proxy
import java.net.ServerSocket
import java.net.Socket
import java.net.SocketException
import java.net.SocketTimeoutException
import java.net.URI
import java.net.UnknownHostException
import java.net.URLDecoder
import java.net.URLEncoder
import java.security.MessageDigest
import java.security.SecureRandom
import java.util.Locale
import java.util.UUID
import java.util.concurrent.TimeUnit
import kotlin.math.max
import kotlin.math.min

private const val GFN_USER_AGENT =
    "GFN-PC/22.0 (Android 14) PGC/3.8 (6.36.38319306) okhttp/4.12.0"
// User-Agent used by the official GeForce NOW Android client for touch sessions.
private const val GFN_ANDROID_TOUCH_USER_AGENT =
    "GFN-PC/22.0 (Android-Generic-Touch 14) PGC/3.8 (6.36.38319306) okhttp/4.12.0"
// User-Agent used by the official GeForce NOW Android client for TV sessions.
private const val GFN_ANDROID_TV_USER_AGENT =
    "GFN-PC/22.0 (Android-Generic-TV 14) PGC/3.8 (6.36.38319306) okhttp/4.12.0"
private const val GFN_CLIENT_VERSION = "2.0.80.173"
private const val GFN_BROWSER_USER_AGENT =
    "Mozilla/5.0 (Linux; Android 10; K) AppleWebKit/537.36 (KHTML, like Gecko) Chrome/150.0.0.0 Mobile Safari/537.36"
private const val GFN_BROWSER_CLIENT_VERSION = "2.0.86.124"
private const val LCARS_CLIENT_ID = "ec7e38d4-03af-4b58-b131-cfb0495903ab"
private const val GFN_PLAY_ORIGIN = "https://play.geforcenow.com"
private const val GFN_PLAY_REFERER = "https://play.geforcenow.com/"
private const val NVIDIA_FILE_ORIGIN = "https://nvfile"
private const val NVIDIA_FILE_REFERER = "https://nvfile/"
private const val SERVICE_URLS_ENDPOINT = "https://pcs.geforcenow.com/v1/serviceUrls"
private const val TOKEN_ENDPOINT = "https://login.nvidia.com/token"
private const val CLIENT_TOKEN_ENDPOINT = "https://login.nvidia.com/client_token"
private const val USERINFO_ENDPOINT = "https://login.nvidia.com/userinfo"
private const val AUTH_ENDPOINT = "https://login.nvidia.com/authorize"
private const val DEVICE_AUTHORIZATION_ENDPOINT = "https://login.nvidia.com/device/authorize"
private const val GAMES_GRAPHQL_URL = "https://games.geforce.com/graphql"
internal const val GFN_APPS_GRAPHQL_URL = "https://apps.gxn.nvidia.com/graphql"
private const val MES_URL = "https://mes.geforcenow.com/v4/subscriptions"
private const val PRINTEDWASTE_QUEUE_URL = "https://api.printedwaste.com/gfn/queue/"
private const val PRINTEDWASTE_SERVER_MAPPING_URL = "https://remote.printedwaste.com/config/GFN_SERVERID_TO_REGION_MAPPING"
private const val DEFAULT_STREAMING_SERVICE_URL = "https://prod.cloudmatchbeta.nvidiagrid.net/"
private const val CLIENT_ID = "ZU7sPN-miLujMD95LfOQ453IB0AtjM8sMyvgJ9wCXEQ"
private const val DEVICE_CODE_CLIENT_ID = "q61ddeJrVt7O90Nl-P-N7I36yctih4Ml6FyXLrb6j-U"
private const val DEFAULT_IDP_ID = "PDiAhv2kJTFeQ7WOPqiQ2tRZ7lGhR2X11dXvM4TZSxg"
private const val SCOPES = "openid consent email tk_client age"
private const val PANELS_QUERY_HASH = "46ec15f267a056e7d5e46e629efa929529e5e7542a4850faece90b9f8fa5f810"
internal const val GFN_APP_METADATA_QUERY_HASH = "cf8b620dfd03617017ba7c858cee65197e1ace5180e41be194b39227227ced63"

private data class CloudMatchClientIdentity(
    val platformName: String,
    val persistGameSettings: Boolean,
    val streamer: String,
    val clientType: String,
    val clientVersion: String,
    val deviceOs: String,
    val deviceType: String,
    val userAgent: String,
    val desktopMonitorDescriptor: Boolean,
)

private val NVIDIA_BROWSER_CLOUD_MATCH_IDENTITY = CloudMatchClientIdentity(
    platformName = "browser",
    persistGameSettings = false,
    streamer = "WEBRTC",
    clientType = "BROWSER",
    clientVersion = GFN_BROWSER_CLIENT_VERSION,
    deviceOs = "ANDROID",
    deviceType = "PHONE",
    userAgent = GFN_BROWSER_USER_AGENT,
    desktopMonitorDescriptor = false,
)

private val NVIDIA_NATIVE_CLOUD_MATCH_IDENTITY = CloudMatchClientIdentity(
    platformName = "windows",
    persistGameSettings = true,
    streamer = "NVIDIA-CLASSIC",
    clientType = "NATIVE",
    clientVersion = GFN_CLIENT_VERSION,
    deviceOs = "WINDOWS",
    deviceType = "DESKTOP",
    userAgent = GFN_BROWSER_USER_AGENT,
    desktopMonitorDescriptor = true,
)

// Touch-capable identity mirroring commit 160e439e: uses the desktop-native streamer/client
// (NVIDIA-CLASSIC / NATIVE) with ANDROID os + TABLET device type. This combination
// tells the server to allocate the full desktop resolution matrix (including ultrawide
// 2560x1080) while still enabling the native touch digitizer on the host.
// desktopMonitorDescriptor = true ensures monitorSettings emits the full desktop
// descriptor (monitorId/positionX/Y/dpi=100).
private val NVIDIA_NATIVE_TOUCH_CLOUD_MATCH_IDENTITY = CloudMatchClientIdentity(
    platformName = "browser",
    persistGameSettings = false,
    streamer = "NVIDIA-CLASSIC",
    clientType = "NATIVE",
    clientVersion = GFN_CLIENT_VERSION,
    deviceOs = "ANDROID",
    deviceType = "TABLET",
    userAgent = GFN_ANDROID_TOUCH_USER_AGENT,
    desktopMonitorDescriptor = true,
)

// Default high-quality allocation for Android TVs other than an explicitly detected SHIELD.
private val NVIDIA_NATIVE_TV_CLOUD_MATCH_IDENTITY = CloudMatchClientIdentity(
    platformName = "android",
    persistGameSettings = false,
    streamer = "NVIDIA-CLASSIC",
    clientType = "NATIVE",
    clientVersion = GFN_CLIENT_VERSION,
    deviceOs = "ANDROID",
    deviceType = "DESKTOP",
    userAgent = GFN_ANDROID_TV_USER_AGENT,
    desktopMonitorDescriptor = true,
)

private val ALLIANCE_CLOUD_MATCH_IDENTITY = CloudMatchClientIdentity(
    platformName = "windows",
    persistGameSettings = true,
    streamer = "NVIDIA-CLASSIC",
    clientType = "NATIVE",
    clientVersion = GFN_CLIENT_VERSION,
    deviceOs = "WINDOWS",
    deviceType = "DESKTOP",
    userAgent = GFN_USER_AGENT,
    desktopMonitorDescriptor = true,
)

// NVIDIA's Browser/WebRTC identity preserves the standard mobile allocation, but CloudMatch limits
// its mode matrix and rejects HDR requests from that client class. Use the internally consistent
// desktop-native identity only for explicit gamepad launches that need the high-quality mode
// matrix. SHIELD and the third-generation Fire TV Cube are the known TV exceptions whose
// Android/native allocations silently provisioned 1080p for higher-resolution requests. Other
// Android TVs retain the Android/native identity. Generic follow-up requests stay on the browser
// identity so they cannot change an existing allocation.
private fun cloudMatchClientIdentity(
    streamingBaseUrl: String?,
    appLaunchMode: Int? = null,
    preferNativeDesktopMode: Boolean = false,
    isAndroidTv: Boolean = false,
    useDesktopNativeTvIdentity: Boolean = false,
): CloudMatchClientIdentity {
    // Touch sessions use the desktop-native CloudMatch identity (NVIDIA-CLASSIC / NATIVE)
    // with Android os + TABLET device type, so the server allocates the full desktop
    // resolution matrix (including ultrawide 2560×1080) while still enabling the
    // native touch digitizer on the host.
    if (appLaunchMode == GfnAppLaunchMode.TOUCH_FRIENDLY) {
        return NVIDIA_NATIVE_TOUCH_CLOUD_MATCH_IDENTITY
    }
    val requestedNativeIdentity = when {
        appLaunchMode == null || !preferNativeDesktopMode -> null
        isAndroidTv && useDesktopNativeTvIdentity -> NVIDIA_NATIVE_CLOUD_MATCH_IDENTITY
        isAndroidTv -> NVIDIA_NATIVE_TV_CLOUD_MATCH_IDENTITY
        else -> NVIDIA_NATIVE_CLOUD_MATCH_IDENTITY
    }
    if (streamingBaseUrl.isNullOrBlank()) {
        return requestedNativeIdentity ?: NVIDIA_BROWSER_CLOUD_MATCH_IDENTITY
    }
    val host = streamingBaseUrl.toHttpUrlOrNull()?.host?.lowercase(Locale.US)
        ?: return ALLIANCE_CLOUD_MATCH_IDENTITY
    val isNvidiaCloudMatch = host == "cloudmatchbeta.nvidiagrid.net" ||
        host.endsWith(".cloudmatchbeta.nvidiagrid.net") ||
        host == "cloudmatch.nvidiagrid.net" ||
        host.endsWith(".cloudmatch.nvidiagrid.net")
    if (!isNvidiaCloudMatch) return ALLIANCE_CLOUD_MATCH_IDENTITY
    return requestedNativeIdentity ?: NVIDIA_BROWSER_CLOUD_MATCH_IDENTITY
}

internal fun isNvidiaShieldTvDevice(
    androidTvProfile: Boolean,
    manufacturer: String?,
    model: String?,
): Boolean =
    androidTvProfile &&
        manufacturer?.trim()?.equals("NVIDIA", ignoreCase = true) == true &&
        model?.contains("SHIELD", ignoreCase = true) == true

internal fun isThirdGenerationFireTvCubeDevice(
    androidTvProfile: Boolean,
    manufacturer: String?,
    model: String?,
): Boolean =
    androidTvProfile &&
        manufacturer?.trim()?.equals("Amazon", ignoreCase = true) == true &&
        model?.trim()?.equals("AFTGAZL", ignoreCase = true) == true

internal fun usesDesktopNativeTvCloudMatchIdentity(
    androidTvProfile: Boolean,
    manufacturer: String?,
    model: String?,
): Boolean =
    isNvidiaShieldTvDevice(androidTvProfile, manufacturer, model) ||
        isThirdGenerationFireTvCubeDevice(androidTvProfile, manufacturer, model)

/**
 * Server-side values, chosen when the session is created. They decide which virtual input devices
 * the host sets up, which is why the choice cannot be revisited once the game is running.
 *
 * [TOUCH_FRIENDLY] is what makes the host present a digitizer. The official client gates its whole
 * touch pipeline on it — `enableTouchInput: appLaunchMode === AppLaunchMode.TouchFriendly` — so a
 * session created as [GAMEPAD_FRIENDLY] will silently ignore perfectly well-formed touch packets.
 */
internal object GfnAppLaunchMode {
    const val DEFAULT = 1
    const val GAMEPAD_FRIENDLY = 2
    const val TOUCH_FRIENDLY = 3
}

private const val DEFAULT_REMOTE_CONTROLLERS_BITMAP = 1
private const val DEFAULT_SUPPORTED_CONTROLLER_TYPE = 2

private data class GfnControllerCapabilities(
    val remoteControllersBitmap: Int,
    val supportedControllerTypes: List<Int>,
)

/**
 * CloudMatch provisions input devices when the session is created. Keep controller advertising
 * exclusive to non-touch launches so virtual/physical gamepad packets have a host device without
 * changing touch-friendly sessions back into controller mode.
 */
private fun gfnControllerCapabilities(appLaunchMode: Int): GfnControllerCapabilities =
    if (appLaunchMode == GfnAppLaunchMode.TOUCH_FRIENDLY) {
        GfnControllerCapabilities(remoteControllersBitmap = 0, supportedControllerTypes = emptyList())
    } else {
        GfnControllerCapabilities(
            remoteControllersBitmap = DEFAULT_REMOTE_CONTROLLERS_BITMAP,
            supportedControllerTypes = listOf(DEFAULT_SUPPORTED_CONTROLLER_TYPE),
        )
    }

private const val LIBRARY_WITH_TIME_QUERY_HASH = "7f54d6bbbf3b1c09d0e5264dfa36f0f4aaf5e2678f2089f0cbf0d4dda18c3af9"
private const val DEFAULT_LOCALE = "en_US"

internal fun gfnLocaleForAndroidLanguageTag(languageTag: String): String {
    val locale = Locale.forLanguageTag(languageTag.trim().replace('_', '-'))
    return when (locale.language.lowercase(Locale.US)) {
        "ar" -> "ar_SA"
        "de" -> "de_DE"
        "es" -> "es_ES"
        "fr" -> "fr_FR"
        "ja" -> "ja_JP"
        "ko" -> "ko_KR"
        "nl" -> "nl_NL"
        "pl" -> "pl_PL"
        "pt" -> if (locale.country.equals("BR", ignoreCase = true)) "pt_BR" else "pt_PT"
        "ro" -> "ro_RO"
        "ru" -> "ru_RU"
        "tr" -> "tr_TR"
        "zh" -> if (
            locale.script.equals("Hant", ignoreCase = true) ||
            locale.country.uppercase(Locale.US) in setOf("TW", "HK", "MO")
        ) {
            "zh_TW"
        } else {
            "zh_CN"
        }
        else -> DEFAULT_LOCALE
    }
}
private const val DEFAULT_CATALOG_FETCH_COUNT = 120
private const val MAX_CATALOG_PAGES = 3
internal const val MAX_CATALOG_REQUEST_PAGES = 50
private const val DEFAULT_SORT_ID = DEFAULT_CATALOG_SORT_ID
private const val POPULAR_SORT_ORDER = "itemMetadata.relevance:DESC,sortName:ASC"
private const val LAST_PLAYED_SORT_ORDER = "variants.gfn.library.lastPlayedDate:DESC,sortName:ASC"
private const val GFN_THURSDAY_SECTION_TITLE = "GFN Thursday"
private const val GFN_THURSDAY_SECTION_ID_PREFIX = "section-cbc43218-6ad6-4ff3-8538-bc84f90c796c-"
private const val LIBRARY_APPS_FETCH_COUNT = 200
private const val MAX_LIBRARY_APPS_PAGES = 25
private const val LIBRARY_APPS_SORT_ORDER =
    "variants.gfn.library.lastPlayedDate:DESC,computedValues.libraryAddedDate:DESC,sortName:ASC"
private const val SESSION_MODIFY_ACTION_AD_UPDATE = 6
internal const val OPENNOW_STREAM_SETTINGS_METADATA_KEY = "OpenNOWStreamSettingsSignature"
private const val STORAGE_ADDON_TYPE = "STORAGE"
private const val TOTAL_STORAGE_SIZE_IN_GB = "TOTAL_STORAGE_SIZE_IN_GB"
private const val USED_STORAGE_SIZE_IN_GB = "USED_STORAGE_SIZE_IN_GB"
private const val STORAGE_METRO_REGION = "STORAGE_METRO_REGION"
private const val STORAGE_METRO_REGION_NAME = "STORAGE_METRO_REGION_NAME"
private const val ACCOUNT_LINKING_BASE_URL = "https://als.geforcenow.com/v1"
private const val ACCOUNT_LINKING_CLIENT_ID = "gfn-pc"
private const val ACCOUNT_LINKING_REDIRECT_URL = "http://localhost:2259/"

private val JSON_MEDIA_TYPE = "application/json; charset=utf-8".toMediaType()
private val GRAPHQL_MEDIA_TYPE = "application/graphql".toMediaType()
private val REDIRECT_PORTS = intArrayOf(2259, 6460, 7119, 8870, 9096)
private const val OAUTH_CALLBACK_TIMEOUT_MS = 120_000L
private const val OAUTH_CALLBACK_PROBE_TIMEOUT_MS = 2_000
private const val OAUTH_CALLBACK_PROBE_PATH = "/opennow-callback-probe"
private const val DEVICE_CODE_MIN_POLL_INTERVAL_SECONDS = 5
internal const val TOKEN_REFRESH_WINDOW_MS = 10 * 60 * 1000L
internal const val CLIENT_TOKEN_REFRESH_WINDOW_MS = 5 * 60 * 1000L
private val AUTH_RESTORE_MUTEX = Mutex()
private val READY_SESSION_STATUSES = setOf(2, 3)
internal fun shouldResumeClaimedSession(status: Int?, recoveryMode: Boolean): Boolean =
    status != 1 && !(recoveryMode && status != null && status in READY_SESSION_STATUSES)
private const val INVALID_SESSION_PROXY_MESSAGE =
    "Invalid session proxy URL. Use http://host:port, https://host:port, socks4://host:port, or socks5://host:port."

internal class SessionClaimNotReadyException(
    val latestSession: SessionInfo?,
) : IllegalStateException("Session did not become ready after claiming.")

internal class TerminalSessionStatusException(
    val status: Int,
    val latestSession: SessionInfo?,
) : IllegalStateException("Cloud session entered terminal status $status.")

val OpenNowJson: Json = Json {
    ignoreUnknownKeys = true
    explicitNulls = false
    isLenient = true
    encodeDefaults = true
}

fun defaultHttpClient(): OkHttpClient =
    OkHttpClient.Builder()
        .addInterceptor { chain ->
            val request = chain.request()
            val canonicalUrl = canonicalizeGfnRequestUrl(request.url)
            val canonicalRequest = if (canonicalUrl == request.url) {
                request
            } else {
                request.newBuilder().url(canonicalUrl).build()
            }
            chain.proceed(canonicalRequest)
        }
        .dns(OpenNowDns)
        .connectTimeout(15, TimeUnit.SECONDS)
        .readTimeout(30, TimeUnit.SECONDS)
        .writeTimeout(30, TimeUnit.SECONDS)
        .pingInterval(15, TimeUnit.SECONDS)
        .build()

internal fun canonicalizeGfnRequestUrl(url: HttpUrl): HttpUrl =
    when (url.host.lowercase(Locale.US)) {
        "games.geforcenow.com" -> url.newBuilder().host("games.geforce.com").build()
        else -> url
    }

private fun metadataEntry(key: String, value: String): JsonObject = buildJsonObject {
    put("key", key)
    put("value", value)
}

private fun hdrCapabilitiesJson(): JsonObject =
    buildJsonObject {
        put("version", 1)
        put("hdrEdrSupportedFlagsInUint32", 1)
        put("staticMetadataDescriptorId", 0)
    }

private fun hdrDisplayDataJson(display: HdrDisplayProfile?): JsonElement {
    if (display == null) return JsonNull
    return buildJsonObject {
        put("desiredContentMaxLuminance", display.maxLuminance)
        put("desiredContentMinLuminance", display.minLuminance)
        put("desiredContentMaxFrameAverageLuminance", display.maxAverageLuminance)
    }
}

private data class StreamRequestProfile(
    val width: Int,
    val height: Int,
    val hdrEnabled: Boolean,
    val hdrDisplay: HdrDisplayProfile?,
    val bitDepth: Int,
    val chroma: Int,
)

private fun StreamSettings.requestProfile(): StreamRequestProfile {
    val compatible = withCodecColorCompatibility()
    val (width, height) = streamResolutionPixels(compatible)
    val hdrEnabled = compatible.hdrEnabled
    return StreamRequestProfile(
        width = width,
        height = height,
        hdrEnabled = hdrEnabled,
        hdrDisplay = compatible.hdrDisplay,
        bitDepth = if (hdrEnabled || compatible.colorQuality.name.startsWith("TenBit")) 10 else 0,
        chroma = if (compatible.colorQuality == ColorQuality.EightBit444 || compatible.colorQuality == ColorQuality.TenBit444) 2 else 0,
    )
}

private fun monitorSettings(
    profile: StreamRequestProfile,
    fps: Int,
    identity: CloudMatchClientIdentity,
): JsonObject =
    buildJsonObject {
        // For touch sessions we MUST emit the full desktop descriptor
        // (monitorId=0, positionX=0, positionY=0, dpi=100) so the server
        // allocates the full resolution matrix including ultrawide.
        if (identity.desktopMonitorDescriptor) {
            put("monitorId", 0)
            put("positionX", 0)
            put("positionY", 0)
        }
        put("widthInPixels", profile.width)
        put("heightInPixels", profile.height)
        put("framesPerSecond", fps)
        put("sdrHdrMode", if (profile.hdrEnabled) 1 else 0)
        put("displayData", if (profile.hdrEnabled) hdrDisplayDataJson(profile.hdrDisplay) else JsonNull)
        put("hdr10PlusGamingData", JsonNull)
        put("dpi", if (identity.desktopMonitorDescriptor) 100 else 0)
    }

private fun requestedStreamingFeatures(settings: StreamSettings, profile: StreamRequestProfile): JsonObject =
    buildJsonObject {
        put("reflex", settings.fps >= 120)
        put("bitDepth", profile.bitDepth)
        // OpenNOW no longer requests cloud G-Sync. The key stays on the wire with its previous
        // default so the request shape CloudMatch validates against is unchanged.
        put("cloudGsync", false)
        put("enabledL4S", settings.enableL4S)
        put("trueHdr", profile.hdrEnabled)
        put("mouseMovementFlags", 0)
        put("supportedHidDevices", 0)
        put("profile", 0)
        put("fallbackToLogicalResolution", false)
        put("hidDevices", JsonNull)
        put("chromaFormat", profile.chroma)
        put("prefilterMode", 0)
        put("prefilterSharpness", 0)
        put("prefilterNoiseReduction", 0)
        put("hudStreamingMode", 0)
        put("sdrColorSpace", 2)
        put("hdrColorSpace", if (profile.hdrEnabled) 4 else 0)
        if (settings.experimentalNvst) {
            put("codec", when (settings.codec) {
                VideoCodec.H264 -> 1
                VideoCodec.H265 -> 2
                VideoCodec.AV1 -> 3
            })
            put("maxBitrateKbps", settings.maxBitrateMbps * 1000)
            put("vsync", false)
            put("audioChannelCount", 2)
            put("qosPolicy", 0)
            put("touchSupport", true)
            put("dynamicStreamingMode", StreamNetworkAdaptation.DYNAMIC_STREAMING_MODE)
        }
    }

/** CloudMatch's native allocation must agree with the subsequent NVST RTSP handshake. */
private fun JsonObjectBuilder.putStreamTransportRequest(settings: StreamSettings?) {
    val nvst = settings?.experimentalNvst == true
    if (nvst) put("streamerVersion", "14") else put("streamerVersion", 1)
    put("sdkVersion", if (nvst) "2.0" else "1.0")
    put("enhancedStreamMode", if (nvst) 0 else 1)
    put("secureRTSPSupported", nvst)
    if (nvst) {
        put("requestedAudioFormat", 0)
        put("transport", JsonNull)
    }
}

private fun baseWebRtcSessionMetadata(): JsonArray = buildJsonArray {
    add(metadataEntry("SubSessionId", UUID.randomUUID().toString()))
    add(metadataEntry("wssignaling", "1"))
    add(metadataEntry("GSStreamerType", "WebRTC"))
    add(metadataEntry("networkType", "Unknown"))
    add(metadataEntry("ClientImeSupport", "0"))
    add(metadataEntry("surroundAudioInfo", "2"))
}

private fun streamSessionMetadata(
    settings: StreamSettings,
    profile: StreamRequestProfile,
    physicalDisplayResolution: Pair<Int, Int>? = null,
): JsonArray = buildJsonArray {
    baseWebRtcSessionMetadata().forEach { entry ->
        if (!settings.experimentalNvst || entry.jsonObject["key"]?.jsonPrimitive?.content != "GSStreamerType") add(entry)
    }
    val requestedResolution = profile.width to profile.height
    val (physicalWidth, physicalHeight) = physicalDisplayResolution
        ?.takeIf { (width, height) ->
            width > 0 && height > 0 &&
                width >= requestedResolution.first && height >= requestedResolution.second
        }
        ?: requestedResolution
    if (physicalWidth > 0 && physicalHeight > 0) {
        add(
            metadataEntry(
                "clientPhysicalResolution",
                buildJsonObject {
                    put("horizontalPixels", physicalWidth)
                    put("verticalPixels", physicalHeight)
                }.toString(),
            ),
        )
    }
    add(metadataEntry(OPENNOW_STREAM_SETTINGS_METADATA_KEY, streamSettingsSessionSignature(settings)))
}

internal fun activeSessionMonitorSettings(session: JsonObject): JsonObject? =
    session.arr("monitorSettings")?.firstOrNull()?.asObject()
        ?: session.obj("sessionRequestData")?.arr("clientRequestMonitorSettings")?.firstOrNull()?.asObject()

private fun monitorResolution(monitor: JsonObject?): String? {
    val width = monitor?.int("widthInPixels")
        ?: monitor?.int("horizontalPixels")
        ?: monitor?.int("width")
    val height = monitor?.int("heightInPixels")
        ?: monitor?.int("verticalPixels")
        ?: monitor?.int("height")
    return if (width != null && height != null && width > 0 && height > 0) "${width}x$height" else null
}

private fun selectedResolution(value: JsonElement?): String? {
    val objectResolution = value.asObject()?.let(::monitorResolution)
    if (objectResolution != null) return objectResolution
    val arrayResolution = value.asArray()?.firstOrNull()?.asObject()?.let(::monitorResolution)
    if (arrayResolution != null) return arrayResolution
    val text = value.asString()?.trim().orEmpty()
    val match = Regex("""(\d{3,5})\s*[xX]\s*(\d{3,5})""").find(text) ?: return null
    return "${match.groupValues[1]}x${match.groupValues[2]}"
}

internal fun extractSessionMonitorSnapshot(session: JsonObject): SessionMonitorSnapshot? {
    val requested = session.obj("sessionRequestData")
        ?.arr("clientRequestMonitorSettings")
        ?.firstOrNull()
        ?.asObject()
    val returned = session.arr("monitorSettings")?.firstOrNull()?.asObject()
    val snapshot = SessionMonitorSnapshot(
        requestedResolution = monitorResolution(requested),
        requestedFps = requested?.int("framesPerSecond"),
        returnedResolution = monitorResolution(returned),
        returnedFps = returned?.int("framesPerSecond"),
        finalSelectedResolution = selectedResolution(session["finalSelectedScreenResolution"]),
    )
    return snapshot.takeIf {
        it.requestedResolution != null ||
            it.requestedFps != null ||
            it.returnedResolution != null ||
            it.returnedFps != null ||
            it.finalSelectedResolution != null
    }
}

internal fun activeSessionSettingsSignature(session: JsonObject): String? =
    session.obj("sessionRequestData")?.arr("metaData")?.metadataValue(OPENNOW_STREAM_SETTINGS_METADATA_KEY)
        ?: session.arr("metaData")?.metadataValue(OPENNOW_STREAM_SETTINGS_METADATA_KEY)

private fun JsonArray.metadataValue(key: String): String? =
    firstNotNullOfOrNull { item ->
        item.asObject()
            ?.takeIf { it.string("key") == key }
            ?.string("value")
            ?.takeIf(String::isNotBlank)
    }

internal fun buildMinimalClaimRequestBody(
    appId: String,
    deviceId: String,
    settings: StreamSettings? = null,
    physicalDisplayResolution: Pair<Int, Int>? = null,
    streamingBaseUrl: String? = null,
    appLaunchMode: Int = GfnAppLaunchMode.GAMEPAD_FRIENDLY,
    isAndroidTv: Boolean = false,
    useDesktopNativeTvIdentity: Boolean = false,
): JsonObject {
    val identity = cloudMatchClientIdentity(
        streamingBaseUrl = streamingBaseUrl,
        appLaunchMode = appLaunchMode,
        preferNativeDesktopMode = if (appLaunchMode == GfnAppLaunchMode.TOUCH_FRIENDLY) false else settings?.requiresNativeDesktopCloudMatchMode() == true,
        isAndroidTv = isAndroidTv,
        useDesktopNativeTvIdentity = useDesktopNativeTvIdentity,
    )
    val profile = settings?.requestProfile()
    val controllerCapabilities = gfnControllerCapabilities(appLaunchMode)
    return buildJsonObject {
        put("action", 2)
        put("data", "RESUME")
        putJsonObject("sessionRequestData") {
            put("audioMode", 2)
            put("remoteControllersBitmap", controllerCapabilities.remoteControllersBitmap)
            put("sdrHdrMode", if (profile?.hdrEnabled == true) 1 else 0)
            put("networkTestSessionId", JsonNull)
            putJsonArray("availableSupportedControllers") {
                controllerCapabilities.supportedControllerTypes.forEach { add(JsonPrimitive(it)) }
            }
            put("clientVersion", "30.0")
            putStreamTransportRequest(settings)
            put("deviceHashId", deviceId)
            put("internalTitle", JsonNull)
            put("clientPlatformName", if (appLaunchMode == GfnAppLaunchMode.TOUCH_FRIENDLY) "android" else identity.platformName)
            if (settings != null && profile != null) {
                putJsonArray("clientRequestMonitorSettings") {
                    add(monitorSettings(profile, settings.fps, identity))
                }
            }
            put(
                "metaData",
                if (settings != null && profile != null) {
                    streamSessionMetadata(settings, profile, physicalDisplayResolution)
                } else {
                    baseWebRtcSessionMetadata()
                },
            )
            put("surroundAudioInfo", 0)
            put("clientTimezoneOffset", java.util.TimeZone.getDefault().getOffset(System.currentTimeMillis()))
            put("clientIdentification", "GFN-PC")
            put("parentSessionId", JsonNull)
            put("appId", appId.toIntOrNull() ?: 0)
            put("appLaunchMode", appLaunchMode)
            put("useOps", true)
            put("clientDisplayHdrCapabilities", if (profile?.hdrEnabled == true) hdrCapabilitiesJson() else JsonNull)
            put("accountLinked", true)
            put("partnerCustomData", "")
            put("enablePersistingInGameSettings", identity.persistGameSettings)
            put("userAge", 26)
            if (settings != null && profile != null) {
                put("requestedStreamingFeatures", requestedStreamingFeatures(settings, profile))
            }
        }
        putJsonArray("metaData") {}
    }
}

private data class SessionProxyConfig(
    val normalizedUrl: String,
    val proxy: Proxy,
    val username: String,
    val password: String,
)

private val sessionProxyClients = mutableMapOf<String, OkHttpClient>()

private fun sessionProxyHttpClient(settings: StreamSettings, fallback: OkHttpClient): OkHttpClient {
    val proxyConfig = resolveSessionProxyConfig(settings) ?: return fallback
    return synchronized(sessionProxyClients) {
        sessionProxyClients.getOrPut(proxyConfig.normalizedUrl) {
            fallback.newBuilder()
                .proxy(proxyConfig.proxy)
                .apply {
                    if (proxyConfig.username.isNotBlank()) {
                        proxyAuthenticator { _, response ->
                            if (response.request.header("Proxy-Authorization") != null) {
                                return@proxyAuthenticator null
                            }
                            response.request.newBuilder()
                                .header("Proxy-Authorization", Credentials.basic(proxyConfig.username, proxyConfig.password))
                                .build()
                        }
                    }
                }
                .build()
        }
    }
}

private fun resolveSessionProxyConfig(settings: StreamSettings): SessionProxyConfig? {
    if (!settings.sessionProxyEnabled) return null
    val raw = settings.sessionProxyUrl.trim()
    if (raw.isBlank()) return null
    val candidate = if (Regex("^[a-z][a-z0-9+.-]*://", RegexOption.IGNORE_CASE).containsMatchIn(raw)) raw else "http://$raw"
    val uri = runCatching { URI(candidate) }.getOrNull() ?: error(INVALID_SESSION_PROXY_MESSAGE)
    val scheme = uri.scheme?.lowercase(Locale.US) ?: error(INVALID_SESSION_PROXY_MESSAGE)
    val host = uri.host?.takeIf { it.isNotBlank() } ?: error(INVALID_SESSION_PROXY_MESSAGE)
    val port = uri.port.takeIf { it in 1..65535 } ?: error(INVALID_SESSION_PROXY_MESSAGE)
    val proxyType = when (scheme) {
        "http", "https" -> Proxy.Type.HTTP
        "socks4", "socks5" -> Proxy.Type.SOCKS
        else -> error(INVALID_SESSION_PROXY_MESSAGE)
    }
    val username = uri.userInfo?.substringBefore(":")?.let(::urlDecode).orEmpty()
    val password = uri.userInfo?.substringAfter(":", "")?.let(::urlDecode).orEmpty()
    val credentials = if (username.isBlank()) "" else "${urlEncode(username)}${if (password.isNotEmpty()) ":${urlEncode(password)}" else ""}@"
    return SessionProxyConfig(
        normalizedUrl = "$scheme://$credentials$host:$port",
        proxy = Proxy(proxyType, InetSocketAddress.createUnresolved(host, port)),
        username = username,
        password = password,
    )
}

private data class NamedDnsResolver(val name: String, val dns: Dns)

private object OpenNowDns : Dns {
    private val dohResolvers: List<NamedDnsResolver> by lazy {
        val bootstrapClient = OkHttpClient.Builder()
            .connectTimeout(5, TimeUnit.SECONDS)
            .readTimeout(5, TimeUnit.SECONDS)
            .build()
        listOf(
            NamedDnsResolver(
                name = "cloudflare-doh",
                dns = DnsOverHttps.Builder()
                    .client(bootstrapClient)
                    .url("https://cloudflare-dns.com/dns-query".toHttpUrl())
                    .bootstrapDnsHosts(ipv4(1, 1, 1, 1), ipv4(1, 0, 0, 1))
                    .build(),
            ),
            NamedDnsResolver(
                name = "google-doh",
                dns = DnsOverHttps.Builder()
                    .client(bootstrapClient)
                    .url("https://dns.google/dns-query".toHttpUrl())
                    .bootstrapDnsHosts(ipv4(8, 8, 8, 8), ipv4(8, 8, 4, 4))
                    .build(),
            ),
            NamedDnsResolver(
                name = "quad9-doh",
                dns = DnsOverHttps.Builder()
                    .client(bootstrapClient)
                    .url("https://dns.quad9.net/dns-query".toHttpUrl())
                    .bootstrapDnsHosts(ipv4(9, 9, 9, 9), ipv4(149, 112, 112, 112))
                    .build(),
            ),
        )
    }

    override fun lookup(hostname: String): List<InetAddress> {
        val failures = mutableListOf<String>()
        val systemResult = runCatching { Dns.SYSTEM.lookup(hostname) }
            .onFailure { failures += "system=${it.message ?: it::class.java.simpleName}" }
            .getOrNull()
        if (!systemResult.isNullOrEmpty()) return systemResult

        for (resolver in dohResolvers) {
            val result = runCatching { resolver.dns.lookup(hostname) }
                .onFailure { failures += "${resolver.name}=${it.message ?: it::class.java.simpleName}" }
                .getOrNull()
            if (!result.isNullOrEmpty()) return result
        }

        throw UnknownHostException("$hostname: DNS lookup failed after system, Cloudflare, Google, and Quad9 (${failures.joinToString("; ")})")
    }

    private fun ipv4(a: Int, b: Int, c: Int, d: Int): InetAddress =
        InetAddress.getByAddress(byteArrayOf(a.toByte(), b.toByte(), c.toByte(), d.toByte()))
}

private fun JsonObject.string(key: String): String? = this[key]?.jsonPrimitive?.contentOrNull
private fun JsonObject.int(key: String): Int? = this[key]?.jsonPrimitive?.intOrNull
private fun JsonObject.long(key: String): Long? = this[key]?.jsonPrimitive?.longOrNull
private fun JsonObject.double(key: String): Double? = this[key]?.jsonPrimitive?.doubleOrNull
private fun JsonObject.boolean(key: String): Boolean? = this[key]?.jsonPrimitive?.booleanOrNull
private fun JsonObject.obj(key: String): JsonObject? = this[key] as? JsonObject
private fun JsonObject.arr(key: String): JsonArray? = this[key] as? JsonArray
private fun JsonElement?.asObject(): JsonObject? = this as? JsonObject
private fun JsonElement?.asArray(): JsonArray? = this as? JsonArray
private fun JsonElement?.asString(): String? = this?.jsonPrimitive?.contentOrNull
private fun JsonElement?.asInt(): Int? = this?.jsonPrimitive?.intOrNull
private fun JsonElement?.asDouble(): Double? = this?.jsonPrimitive?.doubleOrNull
private fun JsonElement?.asBoolean(): Boolean? = this?.jsonPrimitive?.booleanOrNull
private fun JsonObject.graphQlErrorMessage(): String? =
    arr("errors")
        ?.mapNotNull { it.asObject()?.string("message")?.takeIf(String::isNotBlank) }
        ?.takeIf { it.isNotEmpty() }
        ?.joinToString(", ")

private fun JsonObject.checkGraphQlErrors(label: String = "GFN GraphQL"): JsonObject {
    graphQlErrorMessage()?.let { message -> error("$label: $message") }
    return this
}

internal fun isAppStoreEnumSerializationError(error: Throwable): Boolean {
    var current: Throwable? = error
    while (current != null) {
        val message = current.message.orEmpty()
        if (message.contains("AppStoreEnum") && message.contains("cannot represent value", ignoreCase = true)) {
            return true
        }
        current = current.cause
    }
    return false
}

private suspend fun OkHttpClient.awaitText(request: Request): Pair<Int, String> =
    withContext(Dispatchers.IO) {
        val requestBody = OpenNowHttpDiagnostics.captureRequestBody(request)
        val startedAtMs = SystemClock.elapsedRealtime()
        try {
            newCall(request).execute().use { response ->
                val text = response.body?.string().orEmpty()
                OpenNowHttpDiagnostics.record(
                    request = request,
                    requestBody = requestBody,
                    statusCode = response.code,
                    responseBody = text,
                    elapsedMs = SystemClock.elapsedRealtime() - startedAtMs,
                )
                response.code to text
            }
        } catch (error: Throwable) {
            OpenNowHttpDiagnostics.record(
                request = request,
                requestBody = requestBody,
                statusCode = null,
                responseBody = "",
                elapsedMs = SystemClock.elapsedRealtime() - startedAtMs,
                error = error,
            )
            throw error
        }
    }

private fun bearerAuthorization(token: String): String = "Bearer $token"
private fun gfnJwtAuthorization(token: String): String = "GFNJWT $token"

private fun Headers.Builder.putDesktopLcars(
    token: String? = null,
    clientType: String = "NATIVE",
    clientStreamer: String = "NVIDIA-CLASSIC",
    accept: String = "application/json",
    includeUserAgent: Boolean = false,
    includeEmptyTokenAuthorization: Boolean = false,
): Headers.Builder {
    add("Accept", accept)
    if (token != null || includeEmptyTokenAuthorization) add("Authorization", gfnJwtAuthorization(token.orEmpty()))
    add("nv-client-id", LCARS_CLIENT_ID)
    add("nv-client-type", clientType)
    add("nv-client-version", GFN_CLIENT_VERSION)
    add("nv-client-streamer", clientStreamer)
    add("nv-device-os", "WINDOWS")
    add("nv-device-type", "DESKTOP")
    if (includeUserAgent) add("User-Agent", GFN_USER_AGENT)
    return this
}

private fun desktopGraphQlHeaders(token: String? = null): Headers =
    Headers.Builder()
        .add("Accept", "application/json, text/plain, */*")
        .add("Content-Type", "application/json")
        .add("Origin", GFN_PLAY_ORIGIN)
        .add("Referer", GFN_PLAY_REFERER)
        .apply {
            if (!token.isNullOrBlank()) add("Authorization", gfnJwtAuthorization(token))
        }
        .add("nv-client-id", LCARS_CLIENT_ID)
        .add("nv-client-type", "NATIVE")
        .add("nv-client-version", GFN_CLIENT_VERSION)
        .add("nv-client-streamer", "NVIDIA-CLASSIC")
        .add("nv-device-os", "WINDOWS")
        .add("nv-device-type", "DESKTOP")
        .add("nv-device-make", "UNKNOWN")
        .add("nv-device-model", "UNKNOWN")
        .add("nv-browser-type", "CHROME")
        .add("User-Agent", GFN_USER_AGENT)
        .build()

internal fun cloudMatchHeaders(
    token: String,
    clientId: String,
    deviceId: String,
    includeOrigin: Boolean,
    streamingBaseUrl: String? = null,
    appLaunchMode: Int? = null,
    preferNativeDesktopMode: Boolean = false,
    isAndroidTv: Boolean = false,
    useDesktopNativeTvIdentity: Boolean = false,
): Headers {
    val identity = cloudMatchClientIdentity(
        streamingBaseUrl = streamingBaseUrl,
        appLaunchMode = appLaunchMode,
        preferNativeDesktopMode = preferNativeDesktopMode,
        isAndroidTv = isAndroidTv,
        useDesktopNativeTvIdentity = useDesktopNativeTvIdentity,
    )
    val userAgent = when {
        identity == NVIDIA_NATIVE_CLOUD_MATCH_IDENTITY -> identity.userAgent
        isAndroidTv -> GFN_ANDROID_TV_USER_AGENT
        appLaunchMode == GfnAppLaunchMode.TOUCH_FRIENDLY -> GFN_ANDROID_TOUCH_USER_AGENT
        else -> identity.userAgent
    }
    val deviceType = when {
        isAndroidTv -> "DESKTOP"
        appLaunchMode == GfnAppLaunchMode.TOUCH_FRIENDLY -> "TABLET"
        else -> identity.deviceType
    }
    return Headers.Builder()
        .add("User-Agent", userAgent)
        .add("Authorization", gfnJwtAuthorization(token))
        .add("Content-Type", "application/json")
        .add("nv-browser-type", "CHROME")
        .add("nv-client-id", clientId)
        .add("nv-client-streamer", identity.streamer)
        .add("nv-client-type", identity.clientType)
        .add("nv-client-version", identity.clientVersion)
        .add("nv-device-make", "UNKNOWN")
        .add("nv-device-model", "UNKNOWN")
        .add("nv-device-os", identity.deviceOs)
        .add("nv-device-type", deviceType)
        .add("x-device-id", deviceId)
        .apply {
            if (includeOrigin) {
                add("Origin", GFN_PLAY_ORIGIN)
                add("Referer", GFN_PLAY_REFERER)
            }
        }
        .build()
}

private fun normalizeStreamingServiceUrl(value: String): String? {
    val url = value.trim().toHttpUrlOrNull() ?: return null
    if (url.scheme != "https") return null
    val host = url.host
    if (host.isBlank() || host.startsWith(".") || host.contains("..")) return null
    val port = if (url.port != 443) ":${url.port}" else ""
    return "https://$host$port/"
}

private fun normalizeProvider(provider: LoginProvider): LoginProvider =
    provider.copy(streamingServiceUrl = normalizeStreamingServiceUrl(provider.streamingServiceUrl) ?: DEFAULT_STREAMING_SERVICE_URL)

fun defaultProvider(): LoginProvider =
    LoginProvider(
        idpId = DEFAULT_IDP_ID,
        code = "NVIDIA",
        displayName = "NVIDIA",
        streamingServiceUrl = DEFAULT_STREAMING_SERVICE_URL,
        priority = 0,
    )

private fun nowMs(): Long = System.currentTimeMillis()
private fun expiresAt(seconds: Int?, defaultSeconds: Int = 86400): Long = nowMs() + ((seconds ?: defaultSeconds) * 1000L)
private fun isExpired(expiresAt: Long?): Boolean = expiresAt == null || expiresAt <= nowMs()
private fun isNearExpiry(expiresAt: Long?, windowMs: Long): Boolean = expiresAt == null || expiresAt - nowMs() < windowMs

private fun JsonObject.firstString(vararg keys: String): String? =
    keys.firstNotNullOfOrNull { key -> string(key)?.trim()?.takeIf(String::isNotEmpty) }

private fun JsonObject.firstLong(vararg keys: String): Long? =
    keys.firstNotNullOfOrNull(::long)

private fun epochMilliseconds(value: Long): Long =
    if (value in 1..9_999_999_999L) value * 1_000L else value

internal fun parseManualAuthTokens(input: String, currentTimeMs: Long = nowMs()): AuthTokens {
    val trimmed = input.trim()
    require(trimmed.isNotEmpty()) { "Paste an NVIDIA access token or token-response JSON." }
    require(trimmed.length <= 64_000) { "The pasted token data is too large." }

    val root = if (trimmed.startsWith('{')) {
        runCatching { OpenNowJson.parseToJsonElement(trimmed).jsonObject }
            .getOrElse { throw IllegalArgumentException("The pasted token JSON is invalid.", it) }
    } else {
        null
    }
    val tokenObject = root?.obj("tokens") ?: root
    val rawAccessToken = tokenObject?.firstString("access_token", "accessToken") ?: trimmed
    val accessToken = rawAccessToken.replaceFirst(Regex("^Bearer\\s+", RegexOption.IGNORE_CASE), "").trim()
    require(accessToken.isNotEmpty() && !accessToken.startsWith('{')) {
        "The pasted data does not contain an access token."
    }

    val absoluteExpiry = tokenObject?.firstLong("expires_at", "expiresAt")?.let(::epochMilliseconds)
    val expiresInSeconds = tokenObject?.firstLong("expires_in", "expiresIn")
    require(expiresInSeconds == null || expiresInSeconds > 0) { "The pasted token expiry is invalid." }
    val tokenExpiresAt = absoluteExpiry ?: currentTimeMs + (expiresInSeconds ?: 86_400L) * 1_000L
    val clientTokenExpiresAt = tokenObject
        ?.firstLong("client_token_expires_at", "clientTokenExpiresAt")
        ?.let(::epochMilliseconds)

    return AuthTokens(
        accessToken = accessToken,
        refreshToken = tokenObject?.firstString("refresh_token", "refreshToken"),
        idToken = tokenObject?.firstString("id_token", "idToken"),
        expiresAt = tokenExpiresAt,
        clientToken = tokenObject?.firstString("client_token", "clientToken"),
        clientTokenExpiresAt = clientTokenExpiresAt,
        authClientId = tokenObject?.firstString("auth_client_id", "authClientId"),
    )
}

private fun randomBase64Url(byteCount: Int): String {
    val bytes = ByteArray(byteCount)
    SecureRandom().nextBytes(bytes)
    return Base64.encodeToString(bytes, Base64.URL_SAFE or Base64.NO_WRAP or Base64.NO_PADDING)
}

private fun sha256Base64Url(input: String): String {
    val bytes = MessageDigest.getInstance("SHA-256").digest(input.toByteArray(Charsets.US_ASCII))
    return Base64.encodeToString(bytes, Base64.URL_SAFE or Base64.NO_WRAP or Base64.NO_PADDING)
}

private fun decodeJwtPayload(token: String): JsonObject? {
    val payload = token.split(".").getOrNull(1) ?: return null
    return runCatching {
        val json = String(Base64.decode(payload, Base64.URL_SAFE or Base64.NO_WRAP), Charsets.UTF_8)
        OpenNowJson.parseToJsonElement(json).jsonObject
    }.getOrNull()
}

private fun encoded(value: String): String = URLEncoder.encode(value, Charsets.UTF_8.name())
private fun urlEncode(value: String): String = URLEncoder.encode(value, Charsets.UTF_8.name())
private fun urlDecode(value: String): String = URLDecoder.decode(value, Charsets.UTF_8.name())

class GfnAuthRepository(
    private val context: Context,
    private val authStore: AuthStore,
    private val http: OkHttpClient = defaultHttpClient(),
) {
    private val externalOAuthRedirects = Channel<Map<String, String>>(capacity = 4)

    private data class OAuthCallbackServers(
        val port: Int,
        val sockets: List<ServerSocket>,
    ) : Closeable {
        override fun close() {
            sockets.forEach { socket -> runCatching { socket.close() } }
        }
    }

    suspend fun loginProviders(): List<LoginProvider> {
        val request = Request.Builder()
            .url(SERVICE_URLS_ENDPOINT)
            .headers(
                Headers.Builder()
                    .add("Accept", "application/json, text/plain, */*")
                    .add("Origin", NVIDIA_FILE_ORIGIN)
                    .add("Referer", NVIDIA_FILE_REFERER)
                    .add("User-Agent", GFN_USER_AGENT)
                    .build(),
            )
            .build()
        val (code, text) = http.awaitText(request)
        if (code !in 200..299) return listOf(defaultProvider())
        val root = runCatching { OpenNowJson.parseToJsonElement(text).jsonObject }.getOrNull() ?: return listOf(defaultProvider())
        val providers = root.obj("gfnServiceInfo")
            ?.arr("gfnServiceEndpoints")
            ?.mapNotNull { item ->
                val obj = item.asObject() ?: return@mapNotNull null
                val idp = obj.string("idpId") ?: return@mapNotNull null
                val codeValue = obj.string("loginProviderCode") ?: "NVIDIA"
                val display = obj.string("loginProviderDisplayName") ?: codeValue
                val url = obj.string("streamingServiceUrl") ?: return@mapNotNull null
                val streamingServiceUrl = normalizeStreamingServiceUrl(url) ?: return@mapNotNull null
                normalizeProvider(
                    LoginProvider(
                        idpId = idp,
                        code = codeValue,
                        displayName = display,
                        streamingServiceUrl = streamingServiceUrl,
                        priority = obj.int("loginProviderPriority") ?: 0,
                    ),
                )
            }
            ?.sortedWith(compareBy<LoginProvider> { it.priority }.thenBy { it.displayName })
            .orEmpty()
        return providers.ifEmpty { listOf(defaultProvider()) }
    }

    suspend fun restore(
        forceRefresh: Boolean = false,
        throwOnRefreshFailure: Boolean = forceRefresh,
        removeExpiredSessionOnFailure: Boolean = true,
    ): AuthSession? =
        AUTH_RESTORE_MUTEX.withLock {
            authStore.reload()
            val restored = authStore.activeSession() ?: return@withLock null
            var session = restored
            if (session.tokens.clientToken.isNullOrBlank() || isNearExpiry(session.tokens.clientTokenExpiresAt, CLIENT_TOKEN_REFRESH_WINDOW_MS)) {
                val withClientToken = runCatching { ensureClientToken(session.tokens) }.getOrElse { session.tokens }
                if (withClientToken != session.tokens) {
                    val updatedSession = session.copy(tokens = withClientToken)
                    if (!authStore.updateSessionIfUnchanged(session, updatedSession)) {
                        return@withLock authStore.activeSession()
                    }
                    session = updatedSession
                }
            }

            val refreshed = if (forceRefresh || isNearExpiry(session.tokens.expiresAt, TOKEN_REFRESH_WINDOW_MS)) {
                refreshSession(
                    session = session,
                    forceRefresh = forceRefresh || throwOnRefreshFailure,
                    removeExpiredSessionOnFailure = removeExpiredSessionOnFailure,
                )
            } else {
                session
            }

            if (refreshed != session && !authStore.updateSessionIfUnchanged(session, refreshed)) {
                return@withLock authStore.activeSession()
            }
            refreshed
        }

    suspend fun login(
        provider: LoginProvider,
        onAuthorizationCodeReceived: suspend () -> Unit = {},
    ): AuthSession {
        drainExternalOAuthRedirects()
        val callbackServers = openAvailableCallbackServers()
        val port = callbackServers.port
        val verifier = randomBase64Url(64).take(86)
        val challenge = sha256Base64Url(verifier)
        val authUrl = buildAuthUrl(provider, challenge, port)
        val code = coroutineScope {
            val codeDeferred = async(Dispatchers.IO) { waitForAuthorizationCode(callbackServers) }
            runCatching {
                verifyCallbackListenerReachable(port)
            }.onFailure { error ->
                codeDeferred.cancel()
                callbackServers.close()
                throw IllegalStateException(
                    "OAuth callback listener was not reachable on localhost:$port before opening the browser.",
                    error,
                )
            }
            val customTabs = CustomTabsIntent.Builder().build()
            customTabs.intent.addFlags(Intent.FLAG_ACTIVITY_NEW_TASK)
            runCatching {
                customTabs.launchUrl(context, Uri.parse(authUrl))
            }.onFailure {
                callbackServers.close()
                throw it
            }
            codeDeferred.await()
        }
        onAuthorizationCodeReceived()
        val tokens = ensureClientTokenBestEffort(exchangeAuthorizationCode(code, verifier, port))
        val session = buildSession(provider, tokens)
        authStore.upsertSession(session)
        return session
    }

    fun handleOAuthRedirect(uri: Uri?): Boolean {
        if (uri == null || !isLoopbackOAuthRedirect(uri)) return false
        val params = uri.queryParameterNames
            .associateWith { name -> uri.getQueryParameter(name).orEmpty() }
            .filterValues { it.isNotBlank() }
        if (!params.containsKey("code") && !params.containsKey("error")) return false
        externalOAuthRedirects.trySend(params)
        return true
    }

    suspend fun loginWithDeviceCode(provider: LoginProvider, onPrompt: suspend (DeviceLoginPrompt) -> Unit): AuthSession {
        check(provider.supportsDeviceCodeLogin) { "Code sign-in is only available for NVIDIA accounts." }
        val deviceCode = requestDeviceCode(provider)
        onPrompt(deviceCode.prompt)
        val tokens = ensureClientTokenBestEffort(pollDeviceCodeToken(deviceCode))
        val session = buildSession(provider, tokens)
        authStore.upsertSession(session)
        return session
    }

    suspend fun loginWithToken(provider: LoginProvider, tokenInput: String): AuthSession {
        val parsedTokens = parseManualAuthTokens(tokenInput)
        require(!isExpired(parsedTokens.expiresAt)) { "The pasted access token has expired." }
        val tokens = ensureClientTokenBestEffort(parsedTokens)
        val session = buildSession(provider, tokens, requireVerifiedIdentity = true)
        authStore.upsertSession(session)
        return session
    }

    suspend fun logout(userId: String? = null) {
        val activeId = userId ?: authStore.activeSession()?.user?.userId
        if (activeId != null) authStore.removeSession(activeId)
    }

    fun logoutAll() = authStore.clear()

    private data class ClientTokenResponse(val token: String, val expiresAt: Long)

    private suspend fun ensureClientTokenBestEffort(tokens: AuthTokens): AuthTokens =
        runCatching { ensureClientToken(tokens) }.getOrElse { tokens }

    private suspend fun ensureClientToken(tokens: AuthTokens): AuthTokens {
        val hasUsableClientToken =
            !tokens.clientToken.isNullOrBlank() &&
                !isNearExpiry(tokens.clientTokenExpiresAt, CLIENT_TOKEN_REFRESH_WINDOW_MS)
        if (hasUsableClientToken || isExpired(tokens.expiresAt)) return tokens

        val clientToken = requestClientToken(tokens.accessToken)
        return tokens.copy(
            clientToken = clientToken.token,
            clientTokenExpiresAt = clientToken.expiresAt,
        )
    }

    private suspend fun requestClientToken(accessToken: String): ClientTokenResponse {
        val request = Request.Builder()
            .url(CLIENT_TOKEN_ENDPOINT)
            .headers(nvidiaFileHeaders(bearerToken = accessToken, includeReferer = false))
            .build()
        val (code, text) = http.awaitText(request)
        check(code in 200..299) { "Client token request failed ($code): ${text.take(400)}" }
        val root = OpenNowJson.parseToJsonElement(text).jsonObject
        return ClientTokenResponse(
            token = requireNotNull(root.string("client_token")) { "Missing client token" },
            expiresAt = expiresAt(root.int("expires_in")),
        )
    }

    private suspend fun refreshSession(
        session: AuthSession,
        forceRefresh: Boolean,
        removeExpiredSessionOnFailure: Boolean,
    ): AuthSession {
        val tokens = session.tokens
        val refreshErrors = mutableListOf<String>()
        val refreshClientIds = authenticationRefreshClientIds(
            savedClientId = tokens.authClientId,
            browserClientId = CLIENT_ID,
            deviceClientId = DEVICE_CODE_CLIENT_ID,
        )

        if (!tokens.clientToken.isNullOrBlank()) {
            for (clientId in refreshClientIds) {
                try {
                    val refreshed = mergeTokenSnapshot(
                        base = tokens,
                        root = refreshWithClientToken(tokens.clientToken, session.user.userId, clientId),
                        authClientId = clientId,
                    )
                    return buildRefreshedSession(session, ensureClientTokenBestEffort(refreshed), source = "client token")
                } catch (error: CancellationException) {
                    throw error
                } catch (error: Exception) {
                    refreshErrors += "client_token(${authClientLabel(clientId)}): ${error.message ?: "Unknown refresh error"}"
                }
            }
        }

        val refresh = tokens.refreshToken
        if (!refresh.isNullOrBlank()) {
            for (clientId in refreshClientIds) {
                try {
                    val refreshed = refreshAuthTokens(refresh, tokens, clientId)
                    return buildRefreshedSession(session, ensureClientTokenBestEffort(refreshed), source = "refresh token")
                } catch (error: CancellationException) {
                    throw error
                } catch (error: Exception) {
                    refreshErrors += "refresh_token(${authClientLabel(clientId)}): ${error.message ?: "Unknown refresh error"}"
                }
            }
        }

        val hasRefreshMechanism = !tokens.clientToken.isNullOrBlank() || !tokens.refreshToken.isNullOrBlank()
        if (!hasRefreshMechanism) {
            if (isExpired(tokens.expiresAt)) {
                if (removeExpiredSessionOnFailure) {
                    authStore.removeSession(session.user.userId)
                }
                error("Saved session expired and has no refresh mechanism. Please log in again.")
            }
            return session
        }

        if (isExpired(tokens.expiresAt)) {
            if (removeExpiredSessionOnFailure) {
                authStore.removeSession(session.user.userId)
            }
            val detail = refreshErrors.takeIf { it.isNotEmpty() }?.joinToString(" | ")
            error("Token refresh failed and the saved session expired. Please log in again.${detail?.let { " $it" }.orEmpty()}")
        }

        if (forceRefresh && refreshErrors.isNotEmpty()) {
            error("Token refresh failed. Using saved session token. ${refreshErrors.joinToString(" | ")}")
        }
        return session
    }

    private suspend fun refreshWithClientToken(clientToken: String, userId: String, authClientId: String): JsonObject {
        val body = FormBody.Builder()
            .add("grant_type", "urn:ietf:params:oauth:grant-type:client_token")
            .add("client_token", clientToken)
            .add("client_id", authClientId)
            .add("sub", userId)
            .build()
        val request = Request.Builder()
            .url(TOKEN_ENDPOINT)
            .headers(nvidiaFileHeaders(includeReferer = false))
            .post(body)
            .build()
        val (code, text) = http.awaitText(request)
        check(code in 200..299) { "Client-token refresh failed ($code): ${text.take(400)}" }
        return OpenNowJson.parseToJsonElement(text).jsonObject
    }

    private suspend fun refreshAuthTokens(refresh: String, base: AuthTokens, authClientId: String): AuthTokens {
        val body = FormBody.Builder()
            .add("grant_type", "refresh_token")
            .add("refresh_token", refresh)
            .add("client_id", authClientId)
            .build()
        val request = Request.Builder()
            .url(TOKEN_ENDPOINT)
            .headers(nvidiaFileHeaders(includeReferer = false))
            .post(body)
            .build()
        val (code, text) = http.awaitText(request)
        check(code in 200..299) { "Token refresh failed ($code): ${text.take(400)}" }
        val root = OpenNowJson.parseToJsonElement(text).jsonObject
        return AuthTokens(
            accessToken = requireNotNull(root.string("access_token")) { "Missing access token" },
            refreshToken = root.string("refresh_token") ?: refresh,
            idToken = root.string("id_token") ?: base.idToken,
            expiresAt = expiresAt(root.int("expires_in")),
            clientToken = base.clientToken,
            clientTokenExpiresAt = base.clientTokenExpiresAt,
            authClientId = authClientId,
        )
    }

    private fun mergeTokenSnapshot(base: AuthTokens, root: JsonObject, authClientId: String): AuthTokens =
        AuthTokens(
            accessToken = requireNotNull(root.string("access_token")) { "Missing access token" },
            refreshToken = root.string("refresh_token") ?: base.refreshToken,
            idToken = root.string("id_token") ?: base.idToken,
            expiresAt = expiresAt(root.int("expires_in")),
            clientToken = root.string("client_token") ?: base.clientToken,
            clientTokenExpiresAt = base.clientTokenExpiresAt,
            authClientId = authClientId,
        )

    private fun authClientLabel(clientId: String): String =
        when (clientId) {
            CLIENT_ID -> "browser"
            DEVICE_CODE_CLIENT_ID -> "device"
            else -> "saved"
        }

    private suspend fun buildRefreshedSession(session: AuthSession, tokens: AuthTokens, source: String): AuthSession {
        val refreshed = buildSession(session.provider, tokens, fallbackUser = session.user)
        check(refreshed.user.userId == session.user.userId) {
            "Token refresh via $source returned a different account than expected."
        }
        return refreshed
    }

    private suspend fun exchangeAuthorizationCode(code: String, verifier: String, port: Int): AuthTokens {
        val body = FormBody.Builder()
            .add("grant_type", "authorization_code")
            .add("code", code)
            .add("redirect_uri", "http://localhost:$port")
            .add("code_verifier", verifier)
            .build()
        val request = Request.Builder()
            .url(TOKEN_ENDPOINT)
            .headers(nvidiaFileHeaders(includeReferer = true))
            .post(body)
            .build()
        val (status, text) = http.awaitText(request)
        check(status in 200..299) { "Token exchange failed ($status): ${text.take(400)}" }
        val root = OpenNowJson.parseToJsonElement(text).jsonObject
        return AuthTokens(
            accessToken = requireNotNull(root.string("access_token")) { "Missing access token" },
            refreshToken = root.string("refresh_token"),
            idToken = root.string("id_token"),
            expiresAt = expiresAt(root.int("expires_in")),
            clientToken = root.string("client_token"),
            authClientId = CLIENT_ID,
        )
    }

    private data class DeviceCodeChallenge(
        val deviceCode: String,
        val prompt: DeviceLoginPrompt,
        val intervalSeconds: Int,
    )

    private suspend fun requestDeviceCode(provider: LoginProvider): DeviceCodeChallenge {
        val body = FormBody.Builder()
            .add("client_id", DEVICE_CODE_CLIENT_ID)
            .add("scope", SCOPES)
            .add("device_id", authStore.stableDeviceId())
            .add("display_name", androidDeviceDisplayName())
            .add("idp_id", provider.idpId)
            .build()
        val request = Request.Builder()
            .url(DEVICE_AUTHORIZATION_ENDPOINT)
            .headers(starfleetFormHeaders())
            .post(body)
            .build()
        val (status, text) = http.awaitText(request)
        check(status in 200..299) { "Device sign-in failed ($status): ${text.take(400)}" }
        val root = OpenNowJson.parseToJsonElement(text).jsonObject
        val deviceCode = requireNotNull(root.string("device_code")) { "Missing device code" }
        val userCode = requireNotNull(root.string("user_code")) { "Missing user code" }
        val verificationUri = root.string("verification_uri")
            ?: root.string("verification_url")
            ?: "https://login.nvidia.com"
        val expiresIn = root.int("expires_in") ?: 600
        val interval = (root.int("interval") ?: DEVICE_CODE_MIN_POLL_INTERVAL_SECONDS)
            .coerceAtLeast(DEVICE_CODE_MIN_POLL_INTERVAL_SECONDS)
        return DeviceCodeChallenge(
            deviceCode = deviceCode,
            intervalSeconds = interval,
            prompt = DeviceLoginPrompt(
                userCode = userCode,
                verificationUri = verificationUri,
                verificationUriComplete = root.string("verification_uri_complete"),
                expiresAt = nowMs() + expiresIn * 1000L,
            ),
        )
    }

    private suspend fun pollDeviceCodeToken(challenge: DeviceCodeChallenge): AuthTokens {
        var intervalSeconds = challenge.intervalSeconds
        while (nowMs() < challenge.prompt.expiresAt) {
            val body = FormBody.Builder()
                .add("grant_type", "urn:ietf:params:oauth:grant-type:device_code")
                .add("device_code", challenge.deviceCode)
                .add("client_id", DEVICE_CODE_CLIENT_ID)
                .build()
            val request = Request.Builder()
                .url(TOKEN_ENDPOINT)
                .headers(starfleetFormHeaders())
                .post(body)
                .build()
            val (status, text) = awaitDeviceCodePollResponse(
                expiresAt = challenge.prompt.expiresAt,
                intervalSeconds = intervalSeconds,
            ) { http.awaitText(request) }
            val root = runCatching { OpenNowJson.parseToJsonElement(text).jsonObject }.getOrNull()
            if (status in 200..299 && root != null) {
                return AuthTokens(
                    accessToken = requireNotNull(root.string("access_token")) { "Missing access token" },
                    refreshToken = root.string("refresh_token"),
                    idToken = root.string("id_token"),
                    expiresAt = expiresAt(root.int("expires_in")),
                    clientToken = root.string("client_token"),
                    authClientId = DEVICE_CODE_CLIENT_ID,
                )
            }
            val error = root?.string("error").orEmpty()
            when (error) {
                "authorization_pending" -> Unit
                "slow_down" -> intervalSeconds += 5
                "access_denied" -> error("Device sign-in was cancelled.")
                "expired_token" -> error("Device sign-in code expired.")
                else -> check(status in 200..299) { "Device token exchange failed ($status): ${text.take(400)}" }
            }
        }
        error("Device sign-in code expired.")
    }

    private suspend fun buildSession(
        provider: LoginProvider,
        tokens: AuthTokens,
        fallbackUser: AuthUser? = null,
        requireVerifiedIdentity: Boolean = false,
    ): AuthSession {
        val userInfoResult = runCatching { fetchUserInfo(tokens.accessToken) }
        if (requireVerifiedIdentity && userInfoResult.isFailure) {
            throw IllegalArgumentException(
                "NVIDIA did not accept the pasted access token.",
                userInfoResult.exceptionOrNull(),
            )
        }
        val userInfo = userInfoResult.getOrDefault(JsonObject(emptyMap()))
        val jwt = tokens.idToken?.let(::decodeJwtPayload)
        val verifiedUserId = userInfo.string("sub") ?: userInfo.string("id")
        if (requireVerifiedIdentity) {
            require(!verifiedUserId.isNullOrBlank()) { "The pasted access token did not identify an NVIDIA account." }
        }
        val userId = verifiedUserId ?: jwt?.string("sub") ?: fallbackUser?.userId ?: "nvidia-user"
        val email = userInfo.string("email") ?: jwt?.string("email") ?: fallbackUser?.email
        val displayName = userInfo.string("name")
            ?: userInfo.string("preferred_username")
            ?: email
            ?: fallbackUser?.displayName
            ?: "NVIDIA Account"
        val tier = userInfo.string("membershipTier") ?: jwt?.string("membershipTier") ?: fallbackUser?.membershipTier ?: "FREE"
        return AuthSession(
            provider = normalizeProvider(provider),
            tokens = tokens,
            user = AuthUser(
                userId = userId,
                displayName = displayName,
                email = email,
                avatarUrl = userInfo.string("picture") ?: fallbackUser?.avatarUrl,
                membershipTier = tier,
            ),
        )
    }

    private suspend fun fetchUserInfo(accessToken: String): JsonObject {
        val request = Request.Builder()
            .url(USERINFO_ENDPOINT)
            .headers(nvidiaFileHeaders(bearerToken = accessToken, includeReferer = true))
            .build()
        val (code, text) = http.awaitText(request)
        return if (code in 200..299) {
            runCatching { OpenNowJson.parseToJsonElement(text).jsonObject }.getOrDefault(JsonObject(emptyMap()))
        } else {
            JsonObject(emptyMap())
        }
    }

    private fun nvidiaFileHeaders(bearerToken: String? = null, includeReferer: Boolean): Headers =
        Headers.Builder()
            .apply {
                if (bearerToken != null) add("Authorization", bearerAuthorization(bearerToken))
                add("Origin", NVIDIA_FILE_ORIGIN)
                if (includeReferer) add("Referer", NVIDIA_FILE_REFERER)
                add("Accept", "application/json, text/plain, */*")
                add("User-Agent", GFN_USER_AGENT)
            }
            .build()

    private fun starfleetFormHeaders(): Headers =
        Headers.Builder()
            .add("Accept", "application/json, text/plain, */*")
            .add("User-Agent", GFN_USER_AGENT)
            .build()

    private fun androidDeviceDisplayName(): String {
        val model = listOf(Build.MANUFACTURER, Build.MODEL)
            .map { it.trim() }
            .filter { it.isNotBlank() && !it.equals("unknown", ignoreCase = true) }
            .distinctBy { it.lowercase(Locale.US) }
            .joinToString(" ")
        return model.ifBlank { "OpenNOW Android" }
    }

    private fun buildAuthUrl(provider: LoginProvider, challenge: String, port: Int): String {
        val deviceId = authStore.stableDeviceId()
        val nonce = randomBase64Url(16)
        val params = linkedMapOf(
            "response_type" to "code",
            "device_id" to deviceId,
            "scope" to SCOPES,
            "client_id" to CLIENT_ID,
            "redirect_uri" to "http://localhost:$port",
            "ui_locales" to "en_US",
            "nonce" to nonce,
            "prompt" to "select_account",
            "code_challenge" to challenge,
            "code_challenge_method" to "S256",
            "idp_id" to provider.idpId,
        ).map { (key, value) -> "${encoded(key)}=${encoded(value)}" }.joinToString("&")
        return "$AUTH_ENDPOINT?$params"
    }

    private suspend fun openAvailableCallbackServers(): OAuthCallbackServers = withContext(Dispatchers.IO) {
        for (port in REDIRECT_PORTS) {
            val server = runCatching { openCallbackServerSockets(port) }.getOrNull()
            if (server != null) return@withContext server
        }
        error("No available OAuth callback ports")
    }

    private suspend fun waitForAuthorizationCode(callbackServers: OAuthCallbackServers): String = withContext(Dispatchers.IO) {
        callbackServers.use {
            val deadline = System.currentTimeMillis() + OAUTH_CALLBACK_TIMEOUT_MS
            while (System.currentTimeMillis() < deadline) {
                externalOAuthRedirects.tryReceive().getOrNull()?.let { params ->
                    authorizationCodeFromParams(params)?.let { code -> return@withContext code }
                }
                for (server in callbackServers.sockets) {
                    val remainingMs = deadline - System.currentTimeMillis()
                    if (remainingMs <= 0L) break
                    server.soTimeout = minOf(500, remainingMs.coerceAtLeast(1)).toInt()
                    val socket = try {
                        server.accept()
                    } catch (_: SocketTimeoutException) {
                        null
                    } catch (error: SocketException) {
                        if (server.isClosed) null else throw error
                    } ?: continue
                    socket.use { callbackSocket ->
                        val params = runCatching { readCallbackQueryParams(callbackSocket) }.getOrDefault(emptyMap())
                        params["error"]?.takeIf { it.isNotBlank() }?.let { error ->
                            writeCallbackResponse(callbackSocket, "Login failed or was cancelled.")
                            throw IllegalStateException(error)
                        }
                        val code = authorizationCodeFromParams(params)
                        if (code != null) {
                            writeCallbackResponse(callbackSocket, "Login complete. Return to OpenNOW.")
                            return@withContext code
                        }
                        writeCallbackResponse(callbackSocket, "Waiting for NVIDIA to finish sign-in.")
                    }
                }
            }
            throw IllegalStateException("Timed out waiting for OAuth callback")
        }
    }

    private fun authorizationCodeFromParams(params: Map<String, String>): String? {
        params["error"]?.takeIf { it.isNotBlank() }?.let { error ->
            throw IllegalStateException(error)
        }
        return params["code"]?.takeIf { code -> code.isNotBlank() }
    }

    private fun isLoopbackOAuthRedirect(uri: Uri): Boolean {
        if (uri.scheme != "http") return false
        val host = uri.host?.lowercase(Locale.US) ?: return false
        if (host != "localhost" && host != "127.0.0.1" && host != "::1") return false
        return uri.port in REDIRECT_PORTS
    }

    private fun drainExternalOAuthRedirects() {
        while (externalOAuthRedirects.tryReceive().isSuccess) {
            // discard stale browser callbacks from earlier attempts
        }
    }

    private suspend fun verifyCallbackListenerReachable(port: Int) = withContext(Dispatchers.IO) {
        val failures = mutableListOf<String>()
        val reachable = listOf("127.0.0.1", "::1").any { host ->
            runCatching {
                probeCallbackListener(host, port)
            }.onFailure { error ->
                failures += "$host=${error.message ?: error::class.java.simpleName}"
            }.getOrDefault(false)
        }
        check(reachable) {
            "OAuth callback listener probe failed (${failures.joinToString("; ")})"
        }
    }

    private fun probeCallbackListener(host: String, port: Int): Boolean {
        val address = InetAddress.getByName(host)
        Socket().use { socket ->
            socket.connect(InetSocketAddress(address, port), OAUTH_CALLBACK_PROBE_TIMEOUT_MS)
            socket.soTimeout = OAUTH_CALLBACK_PROBE_TIMEOUT_MS
            val hostHeader = if (host.contains(":")) "[$host]" else host
            val writer = OutputStreamWriter(socket.getOutputStream())
            writer.write("GET $OAUTH_CALLBACK_PROBE_PATH HTTP/1.1\r\n")
            writer.write("Host: $hostHeader:$port\r\n")
            writer.write("Connection: close\r\n\r\n")
            writer.flush()
            val status = BufferedReader(InputStreamReader(socket.getInputStream())).use { reader ->
                reader.readLine().orEmpty()
            }
            return status.startsWith("HTTP/1.1 200") || status.startsWith("HTTP/1.0 200")
        }
    }

    private fun openCallbackServerSockets(port: Int): OAuthCallbackServers {
        val sockets = mutableListOf<ServerSocket>()
        val failures = mutableListOf<String>()
        var portInUse = false
        for (host in listOf("127.0.0.1", "::1")) {
            runCatching { openCallbackServerSocket(port, host) }
                .onSuccess { sockets += it }
                .onFailure { error ->
                    failures += "$host=${error.message ?: error::class.java.simpleName}"
                    portInUse = portInUse || error is BindException
                }
        }
        if (portInUse) {
            sockets.forEach { socket -> runCatching { socket.close() } }
            error("OAuth callback port $port is already in use")
        }
        if (sockets.isEmpty()) {
            runCatching { openCallbackServerSocket(port, host = null) }
                .onSuccess { sockets += it }
                .onFailure { error -> failures += "wildcard=${error.message ?: error::class.java.simpleName}" }
        }
        if (sockets.isEmpty()) {
            error("OAuth callback port $port unavailable (${failures.joinToString("; ")})")
        }
        return OAuthCallbackServers(port, sockets)
    }

    private fun openCallbackServerSocket(port: Int, host: String?): ServerSocket {
        val socket = ServerSocket()
        return try {
            socket.reuseAddress = true
            val address = host?.let(InetAddress::getByName)
            socket.bind(if (address == null) InetSocketAddress(port) else InetSocketAddress(address, port))
            socket
        } catch (error: Throwable) {
            runCatching { socket.close() }
            throw error
        }
    }

    private fun readCallbackQueryParams(socket: Socket): Map<String, String> {
        socket.soTimeout = 2_000
        val reader = BufferedReader(InputStreamReader(socket.getInputStream()))
        val requestLine = reader.readLine().orEmpty()
        while (true) {
            val line = reader.readLine() ?: break
            if (line.isEmpty()) break
        }
        val target = requestLine.split(" ").getOrNull(1).orEmpty()
        val query = target.substringAfter("?", "")
        if (query.isBlank() || query == target) return emptyMap()
        return query.split("&").mapNotNull { pair ->
            val key = pair.substringBefore("=", "")
            val value = pair.substringAfter("=", "")
            if (key.isBlank()) null else key to Uri.decode(value)
        }.toMap()
    }

    private fun writeCallbackResponse(socket: Socket, message: String) {
        val html = """
            <!doctype html><html><head><meta charset="utf-8"><title>OpenNOW Login</title></head>
            <body style="font-family:sans-serif;background:#07100b;color:#dfffea;display:grid;place-items:center;height:100vh">
            <main style="max-width:480px;padding:24px;border:1px solid #245138;border-radius:12px">
            <h2>OpenNOW Login</h2><p>$message</p>
            </main></body></html>
        """.trimIndent()
        val bytes = html.toByteArray()
        val writer = OutputStreamWriter(socket.getOutputStream())
        writer.write("HTTP/1.1 200 OK\r\nContent-Type: text/html; charset=utf-8\r\nContent-Length: ${bytes.size}\r\nConnection: close\r\n\r\n")
        writer.write(html)
        writer.flush()
    }
}

internal data class CatalogCardArtwork(
    val mobileImageUrl: String?,
    val tvImageUrl: String?,
)

internal fun catalogCardArtwork(
    keyArt: String?,
    gameBoxArt: String?,
    heroImage: String?,
    tvBanner: String?,
): CatalogCardArtwork = CatalogCardArtwork(
    mobileImageUrl = gameBoxArt?.takeIf { it.isNotBlank() },
    tvImageUrl = listOf(gameBoxArt, keyArt, heroImage, tvBanner).firstOrNull { !it.isNullOrBlank() },
)

internal fun catalogScreenshotUrls(images: JsonObject?): List<String> =
    images?.arr("SCREENSHOTS")
        ?.mapNotNull { it.asString()?.trim()?.takeIf(String::isNotBlank) }
        ?.distinct()
        .orEmpty()

internal fun catalogGameDescription(app: JsonObject): String? =
    app.string("description") ?: app.string("shortDescription")

internal fun gameStoreFromVariant(variant: JsonObject): String {
    variant.string("appStore")?.trim()?.takeIf(String::isNotBlank)?.let { return it }

    val storeUrl = variant.string("storeUrl")?.trim().orEmpty()
    val host = storeUrl.toHttpUrlOrNull()?.host?.lowercase(Locale.US).orEmpty()
    val shortName = variant.string("shortName")?.lowercase(Locale.US).orEmpty().removeSuffix("_gfn_pc")
    val publisher = variant.string("publisherName")?.lowercase(Locale.US).orEmpty()
    return when {
        host == "store.steampowered.com" -> "STEAM"
        host == "epicgames.com" || host.endsWith(".epicgames.com") -> "EPIC"
        host == "gog.com" || host.endsWith(".gog.com") -> "GOG"
        host == "store.ubi.com" || host == "register.ubisoft.com" -> "UPLAY"
        host == "xbox.com" || host.endsWith(".xbox.com") -> "XBOX"
        host == "microsoft.com" || host.endsWith(".microsoft.com") -> "MICROSOFT_STORE"
        host == "battle.net" || host.endsWith(".battle.net") -> "BATTLENET"
        host == "ea.com" || host.endsWith(".ea.com") -> "EA"
        host == "rockstargames.com" || host.endsWith(".rockstargames.com") -> "ROCKSTAR"
        host == "play.google.com" -> "GOOGLE_PLAY"
        host == "guildwars2.com" || host.endsWith(".guildwars2.com") ||
            host == "ncsoft.com" || host.endsWith(".ncsoft.com") ||
            host == "plaync.com" || host.endsWith(".plaync.com") ||
            host == "purpleonplay.com" || host.endsWith(".purpleonplay.com") ||
            publisher.contains("ncsoft") -> "NCSOFT"
        shortName.endsWith("_steam") -> "STEAM"
        shortName.endsWith("_epic") || shortName.endsWith("_egs") || shortName.endsWith("_epic_games_store") -> "EPIC"
        shortName.endsWith("_uplay") || shortName.endsWith("_ubisoft") -> "UPLAY"
        shortName.endsWith("_gog") -> "GOG"
        shortName.endsWith("_xbox") || shortName.endsWith("_game_pass") -> "XBOX"
        shortName.endsWith("_origin") || shortName.endsWith("_ea_app") -> "EA"
        shortName.endsWith("_battlenet") || shortName.endsWith("_battle_net") -> "BATTLENET"
        shortName.endsWith("_ncsoft") || shortName.endsWith("_purple") -> "NCSOFT"
        else -> "Unknown"
    }
}

internal fun gfnVariantMetadataFields(includeAppStore: Boolean): String = """
    id
    ${if (includeAppStore) "appStore" else ""}
    shortName
    storeUrl
    publisherName
    supportedControls
    paymentModels { __typename }
    gfn { status library { status selected lastPlayedDate } }
""".trimIndent()

internal data class LibraryBrowseSpec(
    val filterIds: List<String>,
    val sortOrderId: String?,
)

internal fun libraryBrowseSpec(payload: JsonObject): LibraryBrowseSpec? =
    payload.obj("data")?.arr("panels")
        ?.flatMap { panel -> panel.asObject()?.arr("sections").orEmpty() }
        ?.mapNotNull { section -> section.asObject()?.obj("seeMoreInfo") }
        ?.firstNotNullOfOrNull { seeMore ->
            val filterIds = seeMore.arr("filterIds")?.mapNotNull { it.asString() }.orEmpty()
            filterIds.takeIf { it.isNotEmpty() }?.let {
                LibraryBrowseSpec(filterIds = it, sortOrderId = seeMore.string("sortOrderId"))
            }
        }

internal fun libraryAppsFilter(): JsonObject = buildJsonObject {
    putJsonObject("variants") {
        putJsonObject("gfn") {
            putJsonObject("library") {
                putJsonObject("status") {
                    put("notEquals", "NOT_OWNED")
                }
            }
        }
    }
}

internal fun hasFreeToPlayPaymentModel(paymentModels: JsonArray?): Boolean =
    paymentModels.orEmpty().any { model ->
        val name = model.asObject()?.string("__typename") ?: model.asString()
        name == "FreeToPlayPaymentModel"
    }

internal fun mergeSupplementalPublicGameVariants(
    games: List<GameInfo>,
    publicGames: List<GameInfo>,
): List<GameInfo> {
    val publicByTitle = publicGames
        .groupBy { it.title.normalizedTitleKey() }
        .mapValues { (_, bucket) -> bucket.reduce(::mergeGameInfo) }
    return games.map { game ->
        val publicGame = publicByTitle[game.title.normalizedTitleKey()] ?: return@map game
        val existingStores = game.variants.map { normalizeGameStore(it.store) }.toSet()
        val supplemental = publicGame.variants.filter { normalizeGameStore(it.store) !in existingStores }
        if (supplemental.isEmpty()) game else game.copy(
            launchAppId = game.launchAppId ?: publicGame.launchAppId,
            imageUrl = game.imageUrl ?: publicGame.imageUrl,
            tvCardImageUrl = game.tvCardImageUrl ?: publicGame.tvCardImageUrl,
            screenshotUrl = game.screenshotUrl ?: publicGame.screenshotUrl,
            screenshotUrls = (game.screenshotUrls + publicGame.screenshotUrls).distinct(),
            tvBannerUrl = game.tvBannerUrl ?: publicGame.tvBannerUrl,
            variants = game.variants + supplemental,
            availableStores = displayStoresForVariants(game.variants + supplemental),
            searchText = listOfNotNull(game.searchText, publicGame.searchText).joinToString(" "),
        )
    }
}

internal enum class CatalogSortKind {
    Relevance,
    Popular,
    NewlyAdded,
    LastPlayed,
    Other,
}

internal fun catalogSortKind(sortId: String): CatalogSortKind =
    when (sortId.trim().lowercase(Locale.US)) {
        "relevance" -> CatalogSortKind.Relevance
        "popular", "most_popular" -> CatalogSortKind.Popular
        "last_added", "latest", "new_games", "newly_added" -> CatalogSortKind.NewlyAdded
        "last_played", "recently_played" -> CatalogSortKind.LastPlayed
        else -> CatalogSortKind.Other
    }

internal fun resolveCatalogSort(
    options: List<CatalogSortOption>,
    requestedSortId: String,
): CatalogSortOption {
    val requestedKind = catalogSortKind(requestedSortId)
    return options.firstOrNull { it.id == requestedSortId }
        ?: requestedKind.takeUnless { it == CatalogSortKind.Other }?.let { kind ->
            options.firstOrNull { catalogSortKind(it.id) == kind }
        }
        ?: options.firstOrNull { catalogSortKind(it.id) == CatalogSortKind.Popular }
        ?: CatalogSortOption(DEFAULT_SORT_ID, "Most Popular", POPULAR_SORT_ORDER)
}

internal fun catalogSortOrder(option: CatalogSortOption): String =
    when (catalogSortKind(option.id)) {
        CatalogSortKind.LastPlayed -> LAST_PLAYED_SORT_ORDER
        CatalogSortKind.Popular -> option.orderBy.ifBlank { POPULAR_SORT_ORDER }
        else -> option.orderBy
    }

/**
 * The provider has returned identical order strings for Last added and Last played on some
 * catalogue versions. Last played has trustworthy per-game timestamps, so enforce that one
 * locally while preserving the provider order for Popular and New games.
 */
internal fun applyCatalogSortGuarantees(
    games: List<GameInfo>,
    sortId: String,
): List<GameInfo> =
    if (catalogSortKind(sortId) == CatalogSortKind.LastPlayed) {
        games.sortedWith(
            compareByDescending<GameInfo> { game ->
                game.lastPlayed?.takeIf(String::isNotBlank)
                    ?: game.variants.mapNotNull { it.lastPlayedDate?.takeIf(String::isNotBlank) }.maxOrNull()
            },
        )
    } else {
        games
    }

/** The MAIN panel is NVIDIA's authoritative weekly list; generic catalogue sort is only fallback. */
internal fun gfnThursdayCatalogGames(games: List<GameInfo>): List<GameInfo> =
    games.filter { game ->
        game.catalogSectionTitle?.trim()?.equals(GFN_THURSDAY_SECTION_TITLE, ignoreCase = true) == true ||
            game.catalogSectionId?.startsWith(GFN_THURSDAY_SECTION_ID_PREFIX) == true
    }

internal fun catalogResultWithGfnThursdayGames(
    fallback: CatalogBrowseResult,
    games: List<GameInfo>,
): CatalogBrowseResult {
    if (games.isEmpty()) return fallback
    return fallback.copy(
        games = games,
        numberReturned = games.size,
        numberSupported = games.size,
        totalCount = games.size,
        hasNextPage = false,
        endCursor = null,
        searchQuery = "",
        selectedSortId = NEWLY_ADDED_CATALOG_SORT_ID,
        selectedFilterIds = emptyList(),
    )
}

class GfnCatalogRepository(
    private val http: OkHttpClient = defaultHttpClient(),
    private val localeProvider: () -> String = { DEFAULT_LOCALE },
) {
    private data class CachedVpcId(val value: String, val expiresAtElapsedMs: Long)
    private data class CachedCatalogDefinitions(val value: CatalogDefinitions, val expiresAtElapsedMs: Long)
    private data class CachedPublicGames(val value: List<GameInfo>, val expiresAtElapsedMs: Long)

    private val vpcIdMutex = Mutex()
    private val vpcIdCache = mutableMapOf<String, CachedVpcId>()
    private val catalogDefinitionsMutex = Mutex()
    private val catalogDefinitionsCache = mutableMapOf<String, CachedCatalogDefinitions>()
    private val publicGamesMutex = Mutex()
    private var publicGamesCache: CachedPublicGames? = null

    private fun requestLocale(): String = localeProvider().takeIf {
        it.matches(Regex("^[a-z]{2}_[A-Z]{2}$"))
    } ?: DEFAULT_LOCALE

    suspend fun fetchMainGames(
        token: String,
        providerStreamingBaseUrl: String,
        includeSupplementalPublicVariants: Boolean = true,
    ): List<GameInfo> {
        val vpcId = getVpcId(token, providerStreamingBaseUrl)
        val panels = fetchPanels(token, listOf("MAIN"), vpcId, withLibraryTime = false)
        val games = enrichGamesWithMetadata(token, vpcId, flattenPanels(panels))
        return if (includeSupplementalPublicVariants) mergePublicGameVariants(games, fetchPublicGames()) else games
    }

    suspend fun fetchGfnThursdayGames(
        token: String,
        providerStreamingBaseUrl: String,
        includeSupplementalPublicVariants: Boolean = true,
    ): List<GameInfo> {
        val vpcId = getVpcId(token, providerStreamingBaseUrl)
        val panels = fetchPanels(token, listOf("MAIN"), vpcId, withLibraryTime = false)
        val games = enrichGamesWithMetadata(
            token = token,
            vpcId = vpcId,
            games = gfnThursdayCatalogGames(flattenPanels(panels)),
        )
        return if (includeSupplementalPublicVariants) mergePublicGameVariants(games, fetchPublicGames()) else games
    }

    suspend fun fetchLibraryGames(
        token: String,
        providerStreamingBaseUrl: String,
        includeSupplementalPublicVariants: Boolean = true,
    ): List<GameInfo> {
        val vpcId = getVpcId(token, providerStreamingBaseUrl)
        val paginatedLibrary = try {
            val page = fetchCatalogAppsPages(
                token = token,
                vpcId = vpcId,
                searchQuery = "",
                sortOrder = LIBRARY_APPS_SORT_ORDER,
                fetchCount = LIBRARY_APPS_FETCH_COUNT,
                filters = libraryAppsFilter(),
                maxPages = MAX_LIBRARY_APPS_PAGES,
            )
            enrichGamesWithMetadata(token, vpcId, dedupeGames(page.apps.map(::appToGame)))
        } catch (error: CancellationException) {
            throw error
        } catch (_: Exception) {
            null
        }
        if (paginatedLibrary != null) {
            return if (includeSupplementalPublicVariants) {
                mergePublicGameVariants(paginatedLibrary, fetchPublicGames())
            } else {
                paginatedLibrary
            }
        }

        val panels = runCatching { fetchPanels(token, listOf("LIBRARY"), vpcId, withLibraryTime = true) }
            .getOrElse { fetchPanels(token, listOf("LIBRARY"), vpcId, withLibraryTime = false) }
        val panelGames = enrichGamesWithMetadata(token, vpcId, flattenPanels(panels))
        val paginatedGames = libraryBrowseSpec(panels)?.let { spec ->
            runCatching {
                browseCatalog(
                    token = token,
                    providerStreamingBaseUrl = providerStreamingBaseUrl,
                    searchQuery = "",
                    sortId = spec.sortOrderId ?: DEFAULT_SORT_ID,
                    filterIds = spec.filterIds,
                    maxPages = MAX_CATALOG_REQUEST_PAGES,
                    includeSupplementalPublicVariants = false,
                ).games
            }.getOrDefault(emptyList())
        }.orEmpty()
        val games = mergeKnownLibraryGames(panelGames, paginatedGames)
        return if (includeSupplementalPublicVariants) mergePublicGameVariants(games, fetchPublicGames()) else games
    }

    /**
     * Browse pages intentionally stay lightweight. Hydrate the one game whose details were opened
     * so Store entries get genres and the rest of the metadata response without delaying the whole
     * catalogue behind hundreds of detail records.
     */
    suspend fun hydrateGameDetails(
        token: String,
        providerStreamingBaseUrl: String,
        game: GameInfo,
    ): GameInfo {
        val appId = game.uuid?.takeIf { it.isNotBlank() } ?: return game
        val vpcId = getVpcId(token, providerStreamingBaseUrl)
        val metadata = fetchAppMetaData(token, listOf(appId), vpcId)
            .firstOrNull { it.string("id") == appId }
            ?: return game
        return mergePanelGameWithMetadata(game, appToGame(metadata))
    }

    suspend fun browseCatalog(
        token: String,
        providerStreamingBaseUrl: String,
        searchQuery: String,
        sortId: String = DEFAULT_SORT_ID,
        filterIds: List<String> = emptyList(),
        maxPages: Int = MAX_CATALOG_PAGES,
        includeSupplementalPublicVariants: Boolean = true,
    ): CatalogBrowseResult {
        val vpcId = getVpcId(token, providerStreamingBaseUrl)
        val definitions = fetchFilterAndSortDefinitions(token)
        val selectedSort = resolveCatalogSort(definitions.sortOptions, sortId)
        val selectedFilters = filterIds.filter { definitions.filterPayloadById.containsKey(it) }
        val filters = selectedFilters.mapNotNull { definitions.filterPayloadById[it]?.asObject() }
            .fold(mutableMapOf<String, JsonElement>()) { acc, obj ->
                acc.putAll(obj)
                acc
            }
        val page = fetchCatalogAppsPages(
            token = token,
            vpcId = vpcId,
            searchQuery = searchQuery,
            sortOrder = catalogSortOrder(selectedSort),
            fetchCount = DEFAULT_CATALOG_FETCH_COUNT,
            filters = JsonObject(filters),
            maxPages = maxPages,
        )
        val publicGames = if (includeSupplementalPublicVariants) fetchPublicGames() else emptyList()
        val games = dedupeGames(page.apps.map(::appToGame))
        val withSearchFallbacks = if (searchQuery.isBlank() || publicGames.isEmpty()) {
            games
        } else {
            dedupeGames(games + publicGames.filter { it.matchesSearch(searchQuery) })
        }
        val merged = if (publicGames.isEmpty()) withSearchFallbacks else mergePublicGameVariants(withSearchFallbacks, publicGames)
        val ordered = applyCatalogSortGuarantees(merged, selectedSort.id)
        return CatalogBrowseResult(
            games = ordered,
            numberReturned = page.numberReturned,
            numberSupported = max(page.numberSupported, ordered.size),
            totalCount = max(page.totalCount, ordered.size),
            hasNextPage = page.hasNextPage,
            endCursor = page.endCursor?.takeIf { it.isNotBlank() },
            searchQuery = searchQuery,
            selectedSortId = selectedSort.id,
            selectedFilterIds = selectedFilters,
            filterGroups = definitions.filterGroups,
            sortOptions = definitions.sortOptions,
        )
    }

    private suspend fun fetchCatalogAppsPages(
        token: String,
        vpcId: String,
        searchQuery: String,
        sortOrder: String,
        fetchCount: Int,
        filters: JsonObject,
        maxPages: Int,
    ): CatalogAppsPage {
        val collectedApps = mutableListOf<JsonObject>()
        var numberReturned = 0
        var numberSupported = 0
        var totalCount = 0
        var hasNextPage = false
        var endCursor: String? = null
        var cursor = ""
        for (page in 0 until maxPages.coerceIn(1, MAX_CATALOG_REQUEST_PAGES)) {
            val payload = postGraphQlWithAppStoreFallback(
                query = { includeAppStore -> catalogQuery(searchQuery.isNotBlank(), includeAppStore) },
                variables = buildJsonObject {
                    put("vpcId", vpcId)
                    put("locale", requestLocale())
                    put("sortString", sortOrder)
                    put("fetchCount", fetchCount)
                    put("cursor", cursor)
                    put("filters", filters)
                    if (searchQuery.isNotBlank()) put("searchString", searchQuery.trim())
                },
                token = token,
            )
            val apps = payload.obj("data")?.obj("apps")
            val items = apps?.arr("items")?.mapNotNull { it.asObject() }.orEmpty()
            collectedApps += items
            numberReturned += apps?.int("numberReturned") ?: items.size
            numberSupported = apps?.int("numberSupported") ?: numberSupported
            totalCount = apps?.obj("pageInfo")?.int("totalCount") ?: totalCount
            hasNextPage = apps?.obj("pageInfo")?.boolean("hasNextPage") ?: false
            endCursor = apps?.obj("pageInfo")?.string("endCursor")
            if (!hasNextPage || endCursor.isNullOrBlank()) break
            cursor = endCursor.orEmpty()
        }
        return CatalogAppsPage(
            apps = collectedApps,
            numberReturned = numberReturned,
            numberSupported = numberSupported,
            totalCount = totalCount,
            hasNextPage = hasNextPage,
            endCursor = endCursor,
        )
    }

    suspend fun fetchPublicGames(): List<GameInfo> = publicGamesMutex.withLock {
        val now = SystemClock.elapsedRealtime()
        publicGamesCache
            ?.takeIf { it.expiresAtElapsedMs > now }
            ?.value
            ?.let { return@withLock it }

        requestPublicGames().also { games ->
            // The public list is static supplemental metadata. Cache successful responses so Store,
            // Library, search, and sort refreshes do not download and parse the same large JSON.
            if (games.isNotEmpty()) {
                publicGamesCache = CachedPublicGames(games, now + PUBLIC_GAMES_CACHE_TTL_MS)
            }
        }
    }

    private suspend fun requestPublicGames(): List<GameInfo> {
        val request = Request.Builder()
            .url("https://static.nvidiagrid.net/supported-public-game-list/locales/gfnpc-en-US.json")
            .header("User-Agent", GFN_USER_AGENT)
            .build()
        val (code, text) = http.awaitText(request)
        if (code !in 200..299) return emptyList()
        return dedupeGames(OpenNowJson.parseToJsonElement(text).jsonArray
            .mapNotNull { item ->
                val obj = item.asObject() ?: return@mapNotNull null
                if (obj.string("status") != "AVAILABLE") return@mapNotNull null
                val title = obj.string("title") ?: return@mapNotNull null
                val id = obj["id"]?.jsonPrimitive?.contentOrNull ?: title
                val steamAppId = obj.string("steamUrl")?.substringAfter("/app/", "")?.substringBefore("/")
                val store = obj.string("store") ?: if (obj.string("publisher")?.contains("ncsoft", true) == true) "NCSoft" else "Unknown"
                val posterUrl = steamAppId?.takeIf { it.isNotBlank() }?.let { "https://cdn.cloudflare.steamstatic.com/steam/apps/$it/library_600x900.jpg" }
                GameInfo(
                    id = id,
                    uuid = id,
                    launchAppId = id.takeIf { it.all(Char::isDigit) },
                    title = title,
                    imageUrl = posterUrl,
                    tvCardImageUrl = posterUrl,
                    screenshotUrl = steamAppId?.takeIf { it.isNotBlank() }?.let { "https://cdn.cloudflare.steamstatic.com/steam/apps/$it/library_hero.jpg" },
                    tvBannerUrl = steamAppId?.takeIf { it.isNotBlank() }?.let { "https://cdn.cloudflare.steamstatic.com/steam/apps/$it/library_hero.jpg" },
                    searchText = listOf(title, store, obj.string("publisher")).filterNotNull().joinToString(" ").lowercase(),
                    selectedVariantIndex = 0,
                    variants = listOf(GameVariant(id = id, store = store)),
                    availableStores = displayStoresForVariants(listOf(GameVariant(id = id, store = store))),
                )
            })
    }

    suspend fun hydrateGameForLaunch(
        token: String,
        providerStreamingBaseUrl: String,
        game: GameInfo,
        selectedVariant: GameVariant?,
    ): GameInfo {
        val vpcId = getVpcId(token, providerStreamingBaseUrl)
        val appId = game.uuid ?: game.id
        val hydrated = fetchAppMetaData(token, listOf(appId), vpcId)
            .firstOrNull { it.string("id") == appId }
            ?.let(::appToGame)
            ?: error("Launch metadata did not include ${game.title}")
        val merged = mergeGameInfo(game, hydrated)
        val requestedVariantId = selectedVariant?.id
        val selectedVariantIndex = requestedVariantId
            ?.let { id -> merged.variants.indexOfFirst { it.id == id } }
            ?.takeIf { it >= 0 }
            ?: merged.selectedVariantIndex
        return merged.copy(selectedVariantIndex = selectedVariantIndex)
    }

    suspend fun addOwnedVariant(token: String, variantId: String): String {
        val query = """
            mutation AddOwnedVariant(${'$'}cmsId: String!, ${'$'}locale: String!) {
              addOwnedVariant(language: ${'$'}locale, variantId: ${'$'}cmsId) { app { id } }
            }
        """.trimIndent()
        val payload = postGraphQl(
            query = query,
            variables = buildJsonObject {
                put("cmsId", variantId)
                put("locale", requestLocale())
            },
            token = token,
            endpoint = GFN_APPS_GRAPHQL_URL,
        ).checkGraphQlErrors("Mark game as owned")
        val confirmedVariantId = payload.obj("data")?.obj("addOwnedVariant")?.obj("app")?.string("id")
            ?: error("GFN did not confirm that the game was marked as owned")
        check(confirmedVariantId == variantId) {
            "GFN confirmed a different owned variant ($confirmedVariantId instead of $variantId)"
        }
        return confirmedVariantId
    }

    suspend fun resolveLaunchAppId(token: String, appIdOrUuid: String, providerStreamingBaseUrl: String): String? {
        if (appIdOrUuid.all(Char::isDigit)) return appIdOrUuid
        val vpcId = getVpcId(token, providerStreamingBaseUrl)
        val meta = fetchAppMetaData(token, listOf(appIdOrUuid), vpcId)
        return meta.firstOrNull()?.let(::resolveNumericAppId)
    }

    suspend fun getVpcId(token: String, providerStreamingBaseUrl: String): String {
        val base = normalizeStreamingServiceUrl(providerStreamingBaseUrl) ?: DEFAULT_STREAMING_SERVICE_URL
        val cacheKey = base.lowercase(Locale.US)
        return vpcIdMutex.withLock {
            val now = SystemClock.elapsedRealtime()
            vpcIdCache[cacheKey]
                ?.takeIf { it.expiresAtElapsedMs > now }
                ?.value
                ?.let { return@withLock it }

            val resolved = runCatching {
                val request = Request.Builder()
                    .url("${base}v2/serverInfo")
                    .headers(
                        Headers.Builder()
                            .putDesktopLcars(token, includeUserAgent = true, includeEmptyTokenAuthorization = true)
                            .build(),
                    )
                    .build()
                val (code, text) = http.awaitText(request)
                if (code !in 200..299) {
                    "GFN-PC"
                } else {
                    OpenNowJson.parseToJsonElement(text).jsonObject.obj("requestStatus")?.string("serverId") ?: "GFN-PC"
                }
            }.getOrDefault("GFN-PC")
            // Catalog, library, and subscription refreshes start together. Share their
            // server identity instead of issuing the same request several times. Do not
            // cache the fallback so a transient network failure can recover immediately.
            if (resolved != "GFN-PC") {
                vpcIdCache[cacheKey] = CachedVpcId(resolved, now + VPC_ID_CACHE_TTL_MS)
            }
            resolved
        }
    }

    private companion object {
        const val VPC_ID_CACHE_TTL_MS = 5 * 60 * 1_000L
        const val CATALOG_DEFINITIONS_CACHE_TTL_MS = 30 * 60 * 1_000L
        const val PUBLIC_GAMES_CACHE_TTL_MS = 6 * 60 * 60 * 1_000L
    }

    private suspend fun fetchPanels(token: String, panelNames: List<String>, vpcId: String, withLibraryTime: Boolean): JsonObject {
        val variables = buildJsonObject {
            put("vpcId", vpcId)
            put("locale", requestLocale())
            putJsonArray("panelNames") { panelNames.forEach { add(JsonPrimitive(it)) } }
        }.toString()
        val extensions = buildJsonObject {
            putJsonObject("persistedQuery") {
                put("sha256Hash", if (withLibraryTime) LIBRARY_WITH_TIME_QUERY_HASH else PANELS_QUERY_HASH)
            }
        }.toString()
        val requestType = if (panelNames.contains("LIBRARY")) "panels/Library" else "panels/MainV2"
        val url = "$GAMES_GRAPHQL_URL?requestType=${encoded(requestType)}&extensions=${encoded(extensions)}&huId=${randomHuId()}&variables=${encoded(variables)}"
        val request = Request.Builder()
            .url(url)
            .headers(desktopGraphQlHeaders(token).newBuilder().set("Content-Type", "application/graphql").build())
            .get()
            .build()
        val (code, text) = http.awaitText(request)
        check(code in 200..299) { "Games GraphQL failed ($code): ${text.take(400)}" }
        return OpenNowJson.parseToJsonElement(text).jsonObject
    }

    private suspend fun fetchAppMetaData(token: String, appIds: List<String>, vpcId: String): List<JsonObject> {
        if (appIds.isEmpty()) return emptyList()
        val variables = buildJsonObject {
            put("vpcId", vpcId)
            put("locale", requestLocale())
            putJsonArray("appIds") { appIds.distinct().forEach { add(JsonPrimitive(it)) } }
        }.toString()
        val extensions = buildJsonObject {
            putJsonObject("persistedQuery") { put("sha256Hash", GFN_APP_METADATA_QUERY_HASH) }
        }.toString()
        val url = "$GFN_APPS_GRAPHQL_URL?requestType=appMetaData&extensions=${encoded(extensions)}&huId=${randomHuId()}&variables=${encoded(variables)}"
        val request = Request.Builder()
            .url(url)
            .headers(desktopGraphQlHeaders(token).newBuilder().set("Content-Type", "application/graphql").build())
            .build()
        val (code, text) = http.awaitText(request)
        if (code !in 200..299) return emptyList()
        return OpenNowJson.parseToJsonElement(text).jsonObject.checkGraphQlErrors("App metadata")
            .obj("data")?.obj("apps")?.arr("items")?.mapNotNull { it.asObject() }.orEmpty()
    }

    private suspend fun enrichGamesWithMetadata(token: String, vpcId: String, games: List<GameInfo>): List<GameInfo> {
        val ids = games.mapNotNull { it.uuid }.distinct()
        if (ids.isEmpty()) return games
        val apps = ids.chunked(40).flatMap { fetchAppMetaData(token, it, vpcId) }
        val byId = apps.associateBy { it.string("id").orEmpty() }
        return dedupeGames(games.map { game ->
            val app = byId[game.uuid] ?: return@map game
            mergePanelGameWithMetadata(game, appToGame(app))
        })
    }

    private fun flattenPanels(payload: JsonObject): List<GameInfo> {
        val games = payload.checkGraphQlErrors("Games GraphQL").obj("data")?.arr("panels")?.flatMap { panel ->
            panel.asObject()?.arr("sections")?.flatMap { section ->
                section.asObject()?.arr("items")?.mapNotNull { item ->
                    val obj = item.asObject()
                    val app = obj?.obj("app")
                    if (obj?.string("__typename") == "GameItem" && app != null) {
                        appToGame(app).copy(
                            catalogSectionId = section.asObject()?.string("id"),
                            catalogSectionTitle = section.asObject()?.string("title"),
                        )
                    } else null
                }.orEmpty()
            }.orEmpty()
        }.orEmpty()
        return dedupeGames(games)
    }

    private fun appToGame(app: JsonObject): GameInfo {
        val appIsFreeToPlay = hasFreeToPlayPaymentModel(app.obj("computedValues")?.arr("paymentModels"))
        val variants = app.arr("variants")?.mapNotNull { raw ->
            val obj = raw.asObject() ?: return@mapNotNull null
            val library = obj.obj("gfn")?.obj("library")
            val variantPaymentModels = obj.arr("paymentModels")
            GameVariant(
                id = obj.string("id") ?: return@mapNotNull null,
                store = gameStoreFromVariant(obj),
                storeUrl = obj.string("storeUrl"),
                supportedControls = obj.arr("supportedControls")?.mapNotNull { it.asString() }.orEmpty(),
                librarySelected = library?.boolean("selected"),
                libraryStatus = library?.string("status"),
                lastPlayedDate = library?.string("lastPlayedDate"),
                gfnStatus = obj.obj("gfn")?.string("status"),
                isFreeToPlay = variantPaymentModels?.let(::hasFreeToPlayPaymentModel) ?: appIsFreeToPlay,
            )
        }.orEmpty()
        val numericAppId = resolveNumericAppId(app)
        val selectedVariantId = app.arr("variants")
            ?.mapNotNull { it.asObject() }
            ?.firstOrNull { it.obj("gfn")?.obj("library")?.boolean("selected") == true }
            ?.string("id")
        val selectedIndex = max(0, variants.indexOfFirst { it.id == (selectedVariantId ?: numericAppId) })
        val images = app.obj("images")
        val cardArtwork = catalogCardArtwork(
            keyArt = images?.string("KEY_ART"),
            gameBoxArt = images?.string("GAME_BOX_ART"),
            heroImage = images?.string("HERO_IMAGE"),
            tvBanner = images?.string("TV_BANNER"),
        )
        val screenshotUrl = listOf("HERO_IMAGE", "TV_BANNER", "KEY_ART", "GAME_BOX_ART")
            .firstNotNullOfOrNull { images?.string(it) }
        val screenshotUrls = catalogScreenshotUrls(images)
        val tvBannerUrl = listOf("TV_BANNER", "HERO_IMAGE", "KEY_ART", "GAME_BOX_ART")
            .firstNotNullOfOrNull { images?.string(it) }
        val genres = extractLabels(app.arr("genres"))
        val featureLabels = (extractLabels(app.arr("features")) + extractLabels(app.arr("gameFeatures")) + extractLabels(app.arr("appFeatures")) + genres).distinct()
        val title = app.string("title") ?: app.string("id") ?: "Unknown Game"
        val stores = displayStoresForVariants(variants)
        return GameInfo(
            id = app.string("id") ?: title,
            uuid = app.string("id"),
            launchAppId = numericAppId,
            title = title,
            description = catalogGameDescription(app),
            longDescription = app.string("longDescription"),
            featureLabels = featureLabels,
            genres = genres,
            imageUrl = cardArtwork.mobileImageUrl,
            tvCardImageUrl = cardArtwork.tvImageUrl,
            screenshotUrl = screenshotUrl,
            screenshotUrls = screenshotUrls,
            tvBannerUrl = tvBannerUrl,
            playType = app.obj("gfn")?.string("playType"),
            membershipTierLabel = app.obj("gfn")?.string("minimumMembershipTierLabel"),
            publisherName = app.string("publisherName"),
            contentRatings = extractLabels(app.arr("contentRatings")),
            playabilityState = app.obj("gfn")?.string("playabilityState"),
            availableStores = stores,
            searchText = (listOf(title, app.string("publisherName")) + stores + genres + featureLabels).filterNotNull().joinToString(" ").lowercase(),
            lastPlayed = variants.firstNotNullOfOrNull { it.lastPlayedDate },
            isInLibrary = variants.any(::isOwnedGameVariant),
            selectedVariantIndex = min(selectedIndex, max(variants.size - 1, 0)),
            variants = variants,
        )
    }

    private fun resolveNumericAppId(app: JsonObject): String? {
        val variants = app.arr("variants")?.mapNotNull { it.asObject() }.orEmpty()
        val selected = variants.firstOrNull { it.obj("gfn")?.obj("library")?.boolean("selected") == true }?.string("id")
        return selected?.takeIf { it.all(Char::isDigit) }
            ?: variants.firstNotNullOfOrNull { it.string("id")?.takeIf { value -> value.isNumeric() } }
            ?: app.string("id")?.takeIf { value -> value.isNumeric() }
    }

    private suspend fun fetchFilterAndSortDefinitions(token: String): CatalogDefinitions {
        val locale = requestLocale()
        return catalogDefinitionsMutex.withLock {
            val now = SystemClock.elapsedRealtime()
            catalogDefinitionsCache[locale]
                ?.takeIf { it.expiresAtElapsedMs > now }
                ?.value
                ?.let { return@withLock it }

            requestFilterAndSortDefinitions(token, locale).also { definitions ->
                catalogDefinitionsCache[locale] = CachedCatalogDefinitions(
                    definitions,
                    now + CATALOG_DEFINITIONS_CACHE_TTL_MS,
                )
            }
        }
    }

    private suspend fun requestFilterAndSortDefinitions(token: String, locale: String): CatalogDefinitions {
        val query = """
            query GetFilterGroupAndSortOrderDefinitions(${'$'}locale: String!) {
              filterGroupDefinitions(language: ${'$'}locale) { id label filters { id label filters } }
              sortOrderDefinitions(language: ${'$'}locale) { id label orderBy }
            }
        """.trimIndent()
        val payload = postGraphQl(query, buildJsonObject { put("locale", locale) }, token).checkGraphQlErrors()
        val data = payload.obj("data")
        val filterPayloadById = mutableMapOf<String, JsonElement>()
        val groups = data?.arr("filterGroupDefinitions")?.mapNotNull groupMap@ { raw ->
            val group = raw.asObject() ?: return@groupMap null
            val options = group.arr("filters")?.mapNotNull filterMap@ { filterRaw ->
                val filter = filterRaw.asObject() ?: return@filterMap null
                val filterJson = filter.arr("filters")?.firstOrNull()?.asString() ?: return@filterMap null
                val parsed = runCatching { OpenNowJson.parseToJsonElement(filterJson) }.getOrNull() ?: return@filterMap null
                val id = filter.string("id") ?: return@filterMap null
                filterPayloadById[id] = parsed
                CatalogFilterOption(
                    id = id,
                    rawId = id,
                    label = filter.string("label") ?: id,
                    groupId = group.string("id") ?: "",
                    groupLabel = group.string("label") ?: "",
                )
            }.orEmpty()
            if (options.isEmpty()) null else CatalogFilterGroup(
                id = group.string("id") ?: "",
                label = group.string("label") ?: "",
                options = options,
            )
        }.orEmpty()
        val sorts = data?.arr("sortOrderDefinitions")?.mapNotNull {
            val obj = it.asObject() ?: return@mapNotNull null
            CatalogSortOption(obj.string("id") ?: return@mapNotNull null, obj.string("label") ?: "", obj.string("orderBy") ?: "")
        }.orEmpty()
        return CatalogDefinitions(groups, sorts, filterPayloadById)
    }

    private suspend fun postGraphQl(
        query: String,
        variables: JsonObject,
        token: String,
        endpoint: String = GAMES_GRAPHQL_URL,
    ): JsonObject {
        val body = buildJsonObject {
            put("query", query)
            put("variables", variables)
        }.toString().toRequestBody(JSON_MEDIA_TYPE)
        val request = Request.Builder()
            .url(endpoint)
            .headers(desktopGraphQlHeaders(token))
            .post(body)
            .build()
        val (code, text) = http.awaitText(request)
        check(code in 200..299) { "GFN GraphQL failed ($code): ${text.take(400)}" }
        return OpenNowJson.parseToJsonElement(text).jsonObject
    }

    private suspend fun postGraphQlWithAppStoreFallback(
        query: (includeAppStore: Boolean) -> String,
        variables: JsonObject,
        token: String,
        errorLabel: String = "GFN GraphQL",
    ): JsonObject {
        try {
            return postGraphQl(query(true), variables, token).checkGraphQlErrors(errorLabel)
        } catch (error: CancellationException) {
            throw error
        } catch (error: Exception) {
            if (!isAppStoreEnumSerializationError(error)) throw error
        }
        return postGraphQl(query(false), variables, token).checkGraphQlErrors(errorLabel)
    }

    private fun catalogQuery(hasSearch: Boolean, includeAppStore: Boolean): String {
        val appFields = """
            numberReturned
            numberSupported
            pageInfo { hasNextPage endCursor totalCount }
            items {
              id
              title
              shortDescription
              longDescription
              publisherName
              images { KEY_ART GAME_BOX_ART TV_BANNER HERO_IMAGE SCREENSHOTS }
              computedValues { paymentModels { __typename } }
              variants { ${gfnVariantMetadataFields(includeAppStore)} }
              gfn { playType playabilityState minimumMembershipTierLabel catalogSkuStrings { SKU_BASED_TAG } }
              itemMetadata { campaignIds }
            }
        """.trimIndent()
        return if (hasSearch) {
            """
            query GetSearchFilterResults(${'$'}vpcId: String!, ${'$'}locale: String!, ${'$'}sortString: String!, ${'$'}fetchCount: Int!, ${'$'}cursor: String!, ${'$'}searchString: String!, ${'$'}filters: AppFilterFields!) {
              apps(vpcId: ${'$'}vpcId, language: ${'$'}locale, orderBy: ${'$'}sortString, first: ${'$'}fetchCount, after: ${'$'}cursor, searchQuery: ${'$'}searchString, filters: ${'$'}filters) {
                $appFields
              }
            }
            """.trimIndent()
        } else {
            """
            query GetFilterBrowseResults(${'$'}vpcId: String!, ${'$'}locale: String!, ${'$'}sortString: String!, ${'$'}fetchCount: Int!, ${'$'}cursor: String!, ${'$'}filters: AppFilterFields!) {
              apps(vpcId: ${'$'}vpcId, language: ${'$'}locale, orderBy: ${'$'}sortString, first: ${'$'}fetchCount, after: ${'$'}cursor, filters: ${'$'}filters) {
                $appFields
              }
            }
            """.trimIndent()
        }
    }

    private fun dedupeGames(games: List<GameInfo>): List<GameInfo> =
        games.groupBy { game -> game.title.normalizedTitleKey().ifBlank { game.id } }.map { (_, bucket) ->
            bucket.reduce(::mergeGameInfo)
        }

    private fun mergePublicGameVariants(games: List<GameInfo>, publicGames: List<GameInfo>): List<GameInfo> {
        return mergeSupplementalPublicGameVariants(games, publicGames)
    }

    private fun GameInfo.matchesSearch(query: String): Boolean {
        val normalized = query.trim().lowercase()
        if (normalized.isBlank()) return true
        return (listOf(title, searchText) + availableStores + variants.map { it.store })
            .filterNotNull()
            .any { it.lowercase().contains(normalized) }
    }

    private fun extractLabels(array: JsonArray?): List<String> = array?.mapNotNull { item ->
        when (item) {
            is JsonPrimitive -> item.contentOrNull
            is JsonObject -> listOf("name", "label", "title", "displayName").firstNotNullOfOrNull { item.string(it) }
            else -> null
        }?.trim()?.takeIf { it.isNotBlank() }
    }?.distinct().orEmpty()

    private fun randomHuId(): String = System.currentTimeMillis().toString(16) + UUID.randomUUID().toString().replace("-", "").take(8)

    private data class CatalogDefinitions(
        val filterGroups: List<CatalogFilterGroup>,
        val sortOptions: List<CatalogSortOption>,
        val filterPayloadById: Map<String, JsonElement>,
    )

    private data class CatalogAppsPage(
        val apps: List<JsonObject>,
        val numberReturned: Int,
        val numberSupported: Int,
        val totalCount: Int,
        val hasNextPage: Boolean,
        val endCursor: String?,
    )
}

class GfnSubscriptionRepository(
    private val http: OkHttpClient = defaultHttpClient(),
) {
    suspend fun fetchSubscription(token: String, userId: String, vpcId: String = "NP-AMS-08"): SubscriptionInfo {
        val url = "$MES_URL?serviceName=gfn_pc&languageCode=en_US&vpcId=${encoded(vpcId)}&userId=${encoded(userId)}"
        val request = Request.Builder()
            .url(url)
            .headers(Headers.Builder().putDesktopLcars(token).build())
            .build()
        val (code, text) = http.awaitText(request)
        if (code !in 200..299) return SubscriptionInfo()
        val data = OpenNowJson.parseToJsonElement(text).jsonObject
        val allotted = data.double("allottedTimeInMinutes") ?: 0.0
        val purchased = data.double("purchasedTimeInMinutes") ?: 0.0
        val rolled = data.double("rolledOverTimeInMinutes") ?: 0.0
        val total = data.double("totalTimeInMinutes") ?: (allotted + purchased + rolled)
        val remaining = data.double("remainingTimeInMinutes") ?: 0.0
        val resolutions = data.obj("features")?.arr("resolutions")?.mapNotNull { raw ->
            val obj = raw.asObject() ?: return@mapNotNull null
            EntitledResolution(
                width = obj.int("widthInPixels") ?: return@mapNotNull null,
                height = obj.int("heightInPixels") ?: return@mapNotNull null,
                fps = obj.int("framesPerSecond") ?: return@mapNotNull null,
            )
        }?.sortedWith(compareByDescending<EntitledResolution> { it.width }.thenByDescending { it.height }.thenByDescending { it.fps }).orEmpty()
        val subscription = data.obj("subscription") ?: data
        val storageAddon = subscription.arr("addons")
            ?.mapNotNull { it.asObject() }
            ?.firstOrNull(::isActivePersistentStorageAddon)
            ?.let(::parseStorageAddon)
        return SubscriptionInfo(
            membershipTier = data.string("membershipTier") ?: "FREE",
            subscriptionType = data.string("type"),
            subscriptionSubType = data.string("subType"),
            allottedHours = allotted / 60.0,
            purchasedHours = purchased / 60.0,
            rolledOverHours = rolled / 60.0,
            usedHours = max(total - remaining, 0.0) / 60.0,
            remainingHours = remaining / 60.0,
            totalHours = total / 60.0,
            state = data.obj("currentSubscriptionState")?.string("state"),
            isGamePlayAllowed = data.obj("currentSubscriptionState")?.boolean("isGamePlayAllowed"),
            isUnlimited = data.string("subType") == "UNLIMITED",
            storageAddon = storageAddon,
            entitledResolutions = resolutions,
        )
    }

    private fun parseStorageAddon(addon: JsonObject): StorageAddon {
        val attributes = addon.arr("attributes")
            ?.mapNotNull { it.asObject() }
            ?.associate { attribute ->
                attribute.string("key").orEmpty() to attribute.string("textValue").orEmpty()
            }
            .orEmpty()
        val total = attributes[TOTAL_STORAGE_SIZE_IN_GB]?.toDoubleOrNull()
        val used = attributes[USED_STORAGE_SIZE_IN_GB]?.toDoubleOrNull()
        return StorageAddon(
            type = addon.string("type") ?: STORAGE_ADDON_TYPE,
            sizeGb = total,
            usedGb = used,
            regionName = attributes[STORAGE_METRO_REGION_NAME],
            regionCode = attributes[STORAGE_METRO_REGION],
            status = addon.string("status"),
            subType = addon.string("subType"),
            autoPayEnabled = addon.boolean("autoPayEnabled"),
        )
    }

    private fun isActivePersistentStorageAddon(addon: JsonObject): Boolean =
        addon.string("type") == STORAGE_ADDON_TYPE &&
            addon.string("subType") == "PERMANENT_STORAGE" &&
            addon.string("status") == "OK"
}

class GfnAccountConnectorRepository(
    private val http: OkHttpClient = defaultHttpClient(),
) {
    suspend fun fetchConnectors(token: String): List<AccountConnector> {
        val query = """
            query GetAccountConnectors(${'$'}locale: String!, ${'$'}stringsKey: [String]!) {
              appStoreDefinitions(language: ${'$'}locale) {
                store
                label
                sortOrder
                features {
                  __typename
                  ... on AccountLinkingSso {
                    supported
                  }
                  ... on AccountGamesSyncing {
                    supported
                  }
                }
                accountLinkingMetadata {
                  isSupported
                  isRequired
                  label
                }
              }
              userAccount {
                storesData {
                  store
                  accountLinkingData {
                    userDisplayName
                    expiresIn
                    userIdentifier
                    accountSyncingData {
                      totalNumberOfSyncedGfnGames
                      syncState
                      syncDate
                    }
                  }
                }
              }
              clientStrings(language: ${'$'}locale, keys: ${'$'}stringsKey)
            }
        """.trimIndent()
        val payload = postGraphQl(
            query,
            buildJsonObject {
                put("locale", DEFAULT_LOCALE)
                putJsonArray("stringsKey") {}
            },
            token,
        ).checkGraphQlErrors("Account connectors")
        val data = payload.obj("data") ?: return emptyList()
        val userStores = data.obj("userAccount")?.arr("storesData")
            ?.mapNotNull { it.asObject() }
            ?.associateBy { normalizeGameStore(it.string("store").orEmpty()) }
            .orEmpty()
        val connectors = data.arr("appStoreDefinitions")
            ?.mapNotNull { raw ->
                val store = raw.asObject() ?: return@mapNotNull null
                val storeId = store.string("store")?.takeIf { it.isNotBlank() } ?: return@mapNotNull null
                val metadata = store.obj("accountLinkingMetadata")
                val featureSupported = store.arr("features")
                    ?.mapNotNull { it.asObject() }
                    ?.any { feature ->
                        feature.boolean("supported") == true &&
                            feature.string("__typename") in setOf("AccountLinkingSso", "AccountGamesSyncing")
                    } == true
                val supported = metadata?.boolean("isSupported") == true || featureSupported
                val normalizedStoreId = normalizeGameStore(storeId)
                if (!supported && normalizedStoreId !in userStores) return@mapNotNull null
                val linked = userStores[normalizedStoreId]?.obj("accountLinkingData")
                val sync = linked?.obj("accountSyncingData")
                AccountConnector(
                    store = storeId,
                    label = metadata?.string("label") ?: store.string("label") ?: gameStoreDisplayName(storeId),
                    supported = supported,
                    required = metadata?.boolean("isRequired") ?: false,
                    userDisplayName = linked?.string("userDisplayName"),
                    userIdentifier = linked?.string("userIdentifier"),
                    expiresInSeconds = linked?.long("expiresIn"),
                    syncedGameCount = sync?.int("totalNumberOfSyncedGfnGames"),
                    syncState = sync?.string("syncState"),
                    syncDate = sync?.string("syncDate"),
                )
            }
            .orEmpty()
            .ensureSteamConnector(userStores)
        return connectors.sortedWith(
            compareByDescending<AccountConnector> { it.isLinked }
                .thenBy { accountConnectorSortRank(it.store) }
                .thenBy { it.label.lowercase(Locale.US) },
        )
    }

    suspend fun loginUrl(store: String, accessToken: String): String {
        val platform = accountLinkingPlatform(store)
        val url = "$ACCOUNT_LINKING_BASE_URL/login_url"
            .toHttpUrl()
            .newBuilder()
            .addQueryParameter("platform", platform)
            .addQueryParameter("redirect_uri", ACCOUNT_LINKING_REDIRECT_URL)
            .addQueryParameter("client_id", ACCOUNT_LINKING_CLIENT_ID)
            .build()
        val request = Request.Builder()
            .url(url)
            .headers(accountLinkingHeaders(accessToken))
            .build()
        val (code, text) = http.awaitText(request)
        check(code in 200..299) { "Store connection failed ($code): ${text.take(240)}" }
        return OpenNowJson.parseToJsonElement(text).jsonObject.string("login_url")
            ?: error("Store connection did not return a login URL")
    }

    suspend fun disconnect(store: String, accessToken: String) {
        val platform = accountLinkingPlatform(store)
        val request = Request.Builder()
            .url("$ACCOUNT_LINKING_BASE_URL/linking/${encoded(platform)}")
            .headers(accountLinkingHeaders(accessToken))
            .delete()
            .build()
        val (code, text) = http.awaitText(request)
        check(code in 200..299) { "Store disconnect failed ($code): ${text.take(240)}" }
    }

    private suspend fun postGraphQl(query: String, variables: JsonObject, token: String): JsonObject {
        val body = buildJsonObject {
            put("query", query)
            put("variables", variables)
        }.toString().toRequestBody(JSON_MEDIA_TYPE)
        val request = Request.Builder()
            .url(GAMES_GRAPHQL_URL)
            .headers(desktopGraphQlHeaders(token))
            .post(body)
            .build()
        val (code, text) = http.awaitText(request)
        check(code in 200..299) { "Account connectors failed ($code): ${text.take(400)}" }
        return OpenNowJson.parseToJsonElement(text).jsonObject
    }

    private fun accountLinkingHeaders(accessToken: String): Headers =
        Headers.Builder()
            .add("Accept", "application/json, text/plain, */*")
            .add("Authorization", bearerAuthorization(accessToken))
            .add("Origin", GFN_PLAY_ORIGIN)
            .add("Referer", GFN_PLAY_REFERER)
            .add("User-Agent", GFN_USER_AGENT)
            .build()

    private fun List<AccountConnector>.ensureSteamConnector(userStores: Map<String, JsonObject>): List<AccountConnector> {
        if (any { normalizeGameStore(it.store) == "STEAM" }) return this
        val linked = userStores["STEAM"]?.obj("accountLinkingData")
        val sync = linked?.obj("accountSyncingData")
        return this + AccountConnector(
            store = "STEAM",
            label = "Steam",
            supported = true,
            required = false,
            userDisplayName = linked?.string("userDisplayName"),
            userIdentifier = linked?.string("userIdentifier"),
            expiresInSeconds = linked?.long("expiresIn"),
            syncedGameCount = sync?.int("totalNumberOfSyncedGfnGames"),
            syncState = sync?.string("syncState"),
            syncDate = sync?.string("syncDate"),
        )
    }

    private fun accountLinkingPlatform(store: String): String =
        when (val normalized = normalizeGameStore(store).ifBlank { store }.uppercase(Locale.US)) {
            "UBISOFT", "UBISOFT_CONNECT" -> "UPLAY"
            "BATTLE_NET", "BLIZZARD" -> "BATTLENET"
            "EPIC_GAMES", "EPIC_GAMES_STORE" -> "EPIC"
            else -> normalized
        }

    private fun accountConnectorSortRank(store: String): Int =
        when (normalizeGameStore(store)) {
            "STEAM" -> 0
            "EPIC", "EGS", "EPIC_GAMES_STORE" -> 1
            "XBOX", "XBOX_GAME_PASS", "GAME_PASS" -> 2
            "UBISOFT", "UBISOFT_CONNECT" -> 3
            else -> 10
        }
}

class PrintedWasteRepository(
    private val http: OkHttpClient = defaultHttpClient(),
) {
    suspend fun fetchQueue(): Map<String, PrintedWasteZone> {
        val request = Request.Builder()
            .url(PRINTEDWASTE_QUEUE_URL)
            .header("User-Agent", "opennow-android")
            .header("Accept", "application/json")
            .build()
        val (code, text) = http.awaitText(request)
        check(code in 200..299) { "PrintedWaste queue returned HTTP $code" }
        val payload = OpenNowJson.parseToJsonElement(text).jsonObject
        check(payload.boolean("status") == true) { "PrintedWaste queue returned status:false" }
        val data = payload.obj("data") ?: error("PrintedWaste queue missing data")
        return data.mapNotNull { (zoneId, raw) ->
            val zone = raw.asObject() ?: return@mapNotNull null
            val queue = zone.int("QueuePosition") ?: return@mapNotNull null
            val region = zone.string("Region")?.takeIf { it.isNotBlank() } ?: return@mapNotNull null
            zoneId to PrintedWasteZone(
                QueuePosition = queue,
                LastUpdated = zone.long("Last Updated") ?: 0L,
                Region = region,
                eta = zone.long("eta"),
            )
        }.toMap().also {
            check(it.isNotEmpty()) { "PrintedWaste queue returned no usable zones" }
        }
    }

    suspend fun fetchServerMapping(): Map<String, PrintedWasteServerMappingEntry> {
        val request = Request.Builder()
            .url(PRINTEDWASTE_SERVER_MAPPING_URL)
            .header("User-Agent", "opennow-android")
            .header("Accept", "application/json")
            .build()
        val (code, text) = http.awaitText(request)
        check(code in 200..299) { "PrintedWaste mapping returned HTTP $code" }
        val payload = OpenNowJson.parseToJsonElement(text).jsonObject
        check(payload.boolean("status") == true) { "PrintedWaste mapping returned status:false" }
        val data = payload.obj("data") ?: error("PrintedWaste mapping missing data")
        return data.mapNotNull { (zoneId, raw) ->
            val zone = raw.asObject() ?: return@mapNotNull null
            zoneId to PrintedWasteServerMappingEntry(
                title = zone.string("title"),
                region = zone.string("region"),
                is4080Server = zone.boolean("is4080Server"),
                is5080Server = zone.boolean("is5080Server"),
                nuked = zone.boolean("nuked"),
            )
        }.toMap()
    }

    suspend fun pingRegions(regions: List<StreamRegion>): List<PingResult> = coroutineScope {
        regions.map { region ->
            async(Dispatchers.IO) {
                val url = region.url.toHttpUrlOrNull()
                    ?: return@async PingResult(region.url, error = "Invalid URL")
                val hostname = url.host
                val port = if (url.isHttps) 443 else 80
                val validPings = mutableListOf<Long>()

                // The selector waits for the slowest region, so multi-second probes make
                // one unreachable edge look like a frozen queue screen. A short warm-up
                // plus two samples is enough to rank playable streaming regions while
                // bounding the entire parallel pass to roughly two seconds.
                tcpPing(hostname, port, timeoutMs = 750)
                repeat(2) { index ->
                    if (index > 0) delay(50)
                    tcpPing(hostname, port, timeoutMs = 750)?.let(validPings::add)
                }

                if (validPings.isEmpty()) {
                    PingResult(region.url, error = "All ping tests failed")
                } else {
                    PingResult(region.url, pingMs = validPings.average().toLong())
                }
            }
        }.map { it.await() }
    }

    private fun tcpPing(hostname: String, port: Int, timeoutMs: Int): Long? =
        runCatching {
            Socket().use { socket ->
                val start = System.nanoTime()
                socket.connect(InetSocketAddress(hostname, port), timeoutMs)
                TimeUnit.NANOSECONDS.toMillis(System.nanoTime() - start)
            }
        }.getOrNull()
}

data class GfnSessionDiagnosticResponse(
    val operation: String,
    val method: String,
    val url: String,
    val statusCode: Int,
    val requestBody: String,
    val responseBody: String,
)

class GfnSessionRepository(
    private val authStore: AuthStore,
    private val http: OkHttpClient = defaultHttpClient(),
    private val physicalDisplayResolutionProvider: () -> Pair<Int, Int>? = { null },
    private val diagnosticsSink: (GfnSessionDiagnosticResponse) -> Unit = {},
    private val isAndroidTv: Boolean = false,
    private val useDesktopNativeTvIdentity: Boolean = usesDesktopNativeTvCloudMatchIdentity(
        androidTvProfile = isAndroidTv,
        manufacturer = Build.MANUFACTURER,
        model = Build.MODEL,
    ),
) {
    suspend fun createSession(
        token: String,
        streamingBaseUrl: String?,
        appId: String,
        internalTitle: String,
        zone: String,
        settings: StreamSettings,
        accountLinked: Boolean = true,
        // Decided here and never again: the host provisions its virtual input devices from this,
        // so a session created without it cannot be given a touchscreen later.
        appLaunchMode: Int = GfnAppLaunchMode.GAMEPAD_FRIENDLY,
    ): SessionInfo {
        require(appId.all(Char::isDigit)) { "Invalid launch appId '$appId'." }
        val clientId = UUID.randomUUID().toString()
        val deviceId = authStore.stableDeviceId()
        val base = resolveLaunchSessionBaseUrl(token, resolveStreamingBaseUrl(zone, streamingBaseUrl))
        val body = buildSessionRequestBody(
            appId = appId,
            internalTitle = internalTitle,
            settings = settings,
            accountLinked = accountLinked,
            deviceId = deviceId,
            physicalDisplayResolution = physicalDisplayResolutionProvider(),
            streamingBaseUrl = base,
            appLaunchMode = appLaunchMode,
        )
        val url = cloudMatchSessionRequestUrl(base, settings)
        val host = Uri.parse(base).host.orEmpty()
        val requestHttp = if (isZoneHostname(host)) sessionProxyHttpClient(settings, http) else http
        val request = Request.Builder()
            .url(url)
            .headers(
                cloudMatchHeaders(
                    token = token,
                    clientId = clientId,
                    deviceId = deviceId,
                    includeOrigin = true,
                    streamingBaseUrl = base,
                    appLaunchMode = appLaunchMode,
                    preferNativeDesktopMode = if (appLaunchMode == GfnAppLaunchMode.TOUCH_FRIENDLY) false else settings.requiresNativeDesktopCloudMatchMode(),
                    isAndroidTv = isAndroidTv,
                    useDesktopNativeTvIdentity = useDesktopNativeTvIdentity,
                ),
            )
            .post(body.toString().toRequestBody(JSON_MEDIA_TYPE))
            .build()
        val (code, text) = requestHttp.awaitText(request)
        recordDiagnosticResponse("session.create", request, code, text)
        val payload = OpenNowJson.parseToJsonElement(text).jsonObject
        return toSessionInfo(zone, base, payload, clientId, deviceId)
    }

    suspend fun pollSession(
        token: String,
        streamingBaseUrl: String?,
        serverIp: String?,
        zone: String,
        sessionId: String,
        clientId: String?,
        deviceId: String?,
        settings: StreamSettings,
        diagnosticOperation: String = "session.poll",
    ): SessionInfo {
        val cid = clientId ?: UUID.randomUUID().toString()
        val did = deviceId ?: authStore.stableDeviceId()
        val base = resolvePollStopBase(zone, streamingBaseUrl, serverIp)
        val host = Uri.parse(base).host.orEmpty()
        val requestHttp = if (isZoneHostname(host)) sessionProxyHttpClient(settings, http) else http
        val request = Request.Builder()
            .url("$base/v2/session/$sessionId")
            .headers(cloudMatchHeaders(token, cid, did, includeOrigin = false, streamingBaseUrl = base, isAndroidTv = isAndroidTv))
            .build()
        val (code, text) = requestHttp.awaitText(request)
        recordDiagnosticResponse(diagnosticOperation, request, code, text)
        val payload = OpenNowJson.parseToJsonElement(text).jsonObject
        val realServer = streamingServerIp(payload)
        if (isZoneHostname(host) && realServer != null && !isZoneHostname(realServer) && READY_SESSION_STATUSES.contains(payload.obj("session")?.int("status"))) {
            val directBase = "https://$realServer"
            val directRequest = Request.Builder()
                .url("$directBase/v2/session/$sessionId")
                .headers(cloudMatchHeaders(token, cid, did, includeOrigin = false, streamingBaseUrl = directBase, isAndroidTv = isAndroidTv))
                .build()
            val (code, directText) = http.awaitText(directRequest)
            recordDiagnosticResponse("$diagnosticOperation.direct", directRequest, code, directText)
            if (code in 200..299) {
                val directPayload = OpenNowJson.parseToJsonElement(directText).jsonObject
                if (directPayload.obj("requestStatus")?.int("statusCode") == 1) {
                    return toSessionInfo(zone, directBase, directPayload, cid, did)
                }
            }
        }
        return toSessionInfo(zone, base, payload, cid, did)
    }

    suspend fun stopSession(token: String, input: SessionInfo, settings: StreamSettings) {
        val base = resolvePollStopBase(input.zone, input.streamingBaseUrl, input.serverIp)
        val host = Uri.parse(base).host.orEmpty()
        val requestHttp = if (isZoneHostname(host)) sessionProxyHttpClient(settings, http) else http
        val cid = input.clientId ?: UUID.randomUUID().toString()
        val did = input.deviceId ?: authStore.stableDeviceId()
        val request = Request.Builder()
            .url("$base/v2/session/${input.sessionId}")
            .headers(cloudMatchHeaders(token, cid, did, includeOrigin = false, streamingBaseUrl = base, isAndroidTv = isAndroidTv))
            .delete()
            .build()
        val (code, text) = requestHttp.awaitText(request)
        recordDiagnosticResponse("session.stop", request, code, text)
    }

    suspend fun getActiveSessions(token: String, streamingBaseUrl: String, settings: StreamSettings): List<ActiveSessionInfo> {
        val base = streamingBaseUrl.trim().trimEnd('/')
        val host = Uri.parse(base).host.orEmpty()
        val requestHttp = if (isZoneHostname(host)) sessionProxyHttpClient(settings, http) else http
        val request = Request.Builder()
            .url("$base/v2/session")
            .headers(
                cloudMatchHeaders(
                    token,
                    UUID.randomUUID().toString(),
                    authStore.stableDeviceId(),
                    includeOrigin = false,
                    streamingBaseUrl = base,
                    isAndroidTv = isAndroidTv,
                ),
            )
            .build()
        val (code, text) = requestHttp.awaitText(request)
        recordDiagnosticResponse("session.active", request, code, text)
        if (code !in 200..299) return emptyList()
        val payload = runCatching { OpenNowJson.parseToJsonElement(text).jsonObject }.getOrNull() ?: return emptyList()
        if (payload.obj("requestStatus")?.int("statusCode") != 1) return emptyList()
        return payload.arr("sessions")?.mapNotNull { raw ->
            val s = raw.asObject() ?: return@mapNotNull null
            val status = s.int("status") ?: return@mapNotNull null
            if (status !in setOf(1, 2, 3)) return@mapNotNull null
            val connIp = streamingServerIpFromSession(s)
            val controlIp = s.obj("sessionControlInfo")?.string("ip")
            val monitor = activeSessionMonitorSettings(s)
            ActiveSessionInfo(
                sessionId = s.string("sessionId") ?: return@mapNotNull null,
                appId = s.obj("sessionRequestData")?.string("appId")?.toIntOrNull() ?: 0,
                gpuType = s.string("gpuType"),
                status = status,
                queuePosition = extractQueuePosition(s),
                seatSetupStep = s.obj("seatSetupInfo")?.int("seatSetupStep"),
                streamingBaseUrl = base,
                serverIp = connIp ?: controlIp,
                signalingUrl = s.arr("connectionInfo")
                    ?.mapNotNull { it.asObject() }
                    ?.firstOrNull { it.int("usage") == 14 }
                    ?.let { connection ->
                        val serverIp = connIp ?: controlIp
                        serverIp?.let { buildSignalingUrl(connection.string("resourcePath") ?: "/nvst/", it).first }
                    }
                    ?: (connIp ?: controlIp)?.let { "wss://$it:443/nvst/" },
                resolution = monitor?.let { "${it.int("widthInPixels") ?: 0}x${it.int("heightInPixels") ?: 0}" },
                fps = monitor?.int("framesPerSecond"),
                settingsSignature = activeSessionSettingsSignature(s),
            )
        }.orEmpty()
    }

    suspend fun claimSession(
        token: String,
        active: ActiveSessionInfo,
        settings: StreamSettings,
        appLaunchMode: Int = GfnAppLaunchMode.GAMEPAD_FRIENDLY,
        recoveryMode: Boolean = false,
    ): SessionInfo {
        val deviceId = authStore.stableDeviceId()
        val clientId = UUID.randomUUID().toString()
        val providerBase = normalizeStreamingServiceUrl(active.streamingBaseUrl.orEmpty())?.trimEnd('/')
        val providerHost = providerBase?.let { Uri.parse(it).host.orEmpty() }.orEmpty()
        val useProviderBaseForSessionOps = providerBase != null && !isZoneHostname(providerHost)
        var effectiveServerIp = active.serverIp.orEmpty()
        if (!useProviderBaseForSessionOps && effectiveServerIp.isBlank()) {
            error("Missing server IP for session claim")
        }
        if (!useProviderBaseForSessionOps && isZoneHostname(effectiveServerIp)) {
            val requestHttp = sessionProxyHttpClient(settings, http)
            val prefetch = Request.Builder()
                .url("https://$effectiveServerIp/v2/session/${active.sessionId}")
                .headers(cloudMatchHeaders(token, clientId, deviceId, includeOrigin = false, streamingBaseUrl = active.streamingBaseUrl, isAndroidTv = isAndroidTv))
                .build()
            val (code, text) = requestHttp.awaitText(prefetch)
            recordDiagnosticResponse("session.claim.prefetch", prefetch, code, text)
            if (code in 200..299) {
                streamingServerIp(OpenNowJson.parseToJsonElement(text).jsonObject)?.let { effectiveServerIp = it }
            }
        }
        val sessionBase = if (useProviderBaseForSessionOps) requireNotNull(providerBase) else "https://$effectiveServerIp"
        val validationUrl = "$sessionBase/v2/session/${active.sessionId}"
        val validationRequest = Request.Builder()
            .url(validationUrl)
            .headers(cloudMatchHeaders(token, clientId, deviceId, includeOrigin = false, streamingBaseUrl = active.streamingBaseUrl, isAndroidTv = isAndroidTv))
            .build()
        val (validationCode, validationText) = http.awaitText(validationRequest)
        recordDiagnosticResponse("session.claim.validation", validationRequest, validationCode, validationText)
        val validation = runCatching { OpenNowJson.parseToJsonElement(validationText).jsonObject }.getOrNull()
        val status = validation?.obj("session")?.int("status")
        if (status != null && isTerminalSessionStatus(status)) {
            val latestSession = runCatching {
                toSessionInfo("", sessionBase, requireNotNull(validation), clientId, deviceId)
            }.getOrNull()
            throw TerminalSessionStatusException(status, latestSession)
        }
        // A recovery GET can already return a stream-ready session. Repeating RESUME in that case
        // can rotate signaling hosts and move a healthy session back through transient setup.
        if (shouldResumeClaimedSession(status, recoveryMode)) {
            val claimBody = buildClaimRequestBody(
                appId = active.appId.toString(),
                deviceId = deviceId,
                settings = settings,
                physicalDisplayResolution = physicalDisplayResolutionProvider(),
                streamingBaseUrl = active.streamingBaseUrl,
                appLaunchMode = appLaunchMode,
            )
            val claimRequest = Request.Builder()
                .url(cloudMatchSessionRequestUrl(sessionBase, settings, active.sessionId))
                .headers(
                    cloudMatchHeaders(
                        token = token,
                        clientId = clientId,
                        deviceId = deviceId,
                        includeOrigin = true,
                        streamingBaseUrl = active.streamingBaseUrl,
                        appLaunchMode = appLaunchMode,
                        preferNativeDesktopMode = if (appLaunchMode == GfnAppLaunchMode.TOUCH_FRIENDLY) false else settings.requiresNativeDesktopCloudMatchMode(),
                        isAndroidTv = isAndroidTv,
                        useDesktopNativeTvIdentity = useDesktopNativeTvIdentity,
                    ),
                )
                .put(claimBody.toString().toRequestBody(JSON_MEDIA_TYPE))
                .build()
            val (claimCode, claimText) = http.awaitText(claimRequest)
            recordDiagnosticResponse("session.claim.put", claimRequest, claimCode, claimText)
        }
        var latestSession: SessionInfo? = null
        repeat(60) { attempt ->
            if (attempt > 0) delay(1000)
            val poll = Request.Builder()
                .url(validationUrl)
                .headers(cloudMatchHeaders(token, clientId, deviceId, includeOrigin = false, streamingBaseUrl = active.streamingBaseUrl, isAndroidTv = isAndroidTv))
                .build()
            val (code, text) = http.awaitText(poll)
            recordDiagnosticResponse("session.claim.poll", poll, code, text)
            if (code in 200..299) {
                val payload = OpenNowJson.parseToJsonElement(text).jsonObject
                val pollStatus = payload.obj("session")?.int("status")
                val polledSession = toSessionInfo("", sessionBase, payload, clientId, deviceId)
                latestSession = polledSession
                if (pollStatus in READY_SESSION_STATUSES) return polledSession
                if (pollStatus != null && isTerminalSessionStatus(pollStatus)) {
                    throw TerminalSessionStatusException(pollStatus, polledSession)
                }
            }
        }
        throw SessionClaimNotReadyException(latestSession)
    }

    suspend fun stopActiveSession(token: String, active: ActiveSessionInfo, settings: StreamSettings) {
        stopSession(
            token = token,
            input = SessionInfo(
                sessionId = active.sessionId,
                status = active.status,
                queuePosition = active.queuePosition,
                seatSetupStep = active.seatSetupStep,
                streamingBaseUrl = active.streamingBaseUrl,
                serverIp = active.serverIp.orEmpty(),
                signalingServer = active.serverIp.orEmpty(),
                signalingUrl = active.signalingUrl.orEmpty(),
                gpuType = active.gpuType,
                clientId = UUID.randomUUID().toString(),
                deviceId = authStore.stableDeviceId(),
            ),
            settings = settings,
        )
    }

    suspend fun reportSessionAd(
        token: String,
        session: SessionInfo,
        adId: String,
        action: String,
        settings: StreamSettings,
        watchedTimeInMs: Long? = null,
        pausedTimeInMs: Long? = null,
        cancelReason: String? = null,
        errorInfo: String? = null,
    ): SessionInfo {
        val base = resolvePollStopBase(session.zone, session.streamingBaseUrl, session.serverIp)
        val host = Uri.parse(base).host.orEmpty()
        val requestHttp = if (isZoneHostname(host)) sessionProxyHttpClient(settings, http) else http
        val cid = session.clientId ?: UUID.randomUUID().toString()
        val did = session.deviceId ?: authStore.stableDeviceId()
        val actionCode = mapOf("start" to 1, "pause" to 2, "resume" to 3, "finish" to 4, "cancel" to 5)[action] ?: 5
        val body = buildJsonObject {
            put("action", SESSION_MODIFY_ACTION_AD_UPDATE)
            putJsonArray("adUpdates") {
                add(buildJsonObject {
                    put("adId", adId)
                    put("adAction", actionCode)
                    put("clientTimestamp", System.currentTimeMillis() / 1000)
                    if (watchedTimeInMs != null) {
                        put("watchedTimeInMs", max(0L, watchedTimeInMs))
                    }
                    if (pausedTimeInMs != null) {
                        put("pausedTimeInMs", max(0L, pausedTimeInMs))
                    }
                    if (!cancelReason.isNullOrBlank()) {
                        put("cancelReason", cancelReason)
                    }
                    if (!errorInfo.isNullOrBlank()) {
                        put("errorInfo", errorInfo)
                    }
                })
            }
        }
        val request = Request.Builder()
            .url("$base/v2/session/${session.sessionId}")
            .headers(cloudMatchHeaders(token, cid, did, includeOrigin = true, streamingBaseUrl = session.streamingBaseUrl, isAndroidTv = isAndroidTv))
            .put(body.toString().toRequestBody(JSON_MEDIA_TYPE))
            .build()
        val (code, text) = requestHttp.awaitText(request)
        recordDiagnosticResponse("session.adUpdate", request, code, text)
        check(code in 200..299) { "Queue ad update failed ($code): ${text.take(400)}" }
        return toSessionInfo(session.zone, base, OpenNowJson.parseToJsonElement(text).jsonObject, cid, did)
    }

    private fun recordDiagnosticResponse(operation: String, request: Request, statusCode: Int, responseBody: String) {
        runCatching {
            diagnosticsSink(
                GfnSessionDiagnosticResponse(
                    operation = operation,
                    method = request.method,
                    url = request.url.toString(),
                    statusCode = statusCode,
                    requestBody = OpenNowHttpDiagnostics.captureRequestBody(request),
                    responseBody = responseBody,
                ),
            )
        }
    }

    private fun buildSessionRequestBody(
        appId: String,
        internalTitle: String,
        settings: StreamSettings,
        accountLinked: Boolean,
        deviceId: String,
        physicalDisplayResolution: Pair<Int, Int>?,
        streamingBaseUrl: String?,
        appLaunchMode: Int,
    ): JsonObject {
        val identity = cloudMatchClientIdentity(
            streamingBaseUrl = streamingBaseUrl,
            appLaunchMode = appLaunchMode,
            preferNativeDesktopMode = if (appLaunchMode == GfnAppLaunchMode.TOUCH_FRIENDLY) false else settings.requiresNativeDesktopCloudMatchMode(),
            isAndroidTv = isAndroidTv,
            useDesktopNativeTvIdentity = useDesktopNativeTvIdentity,
        )
        val profile = settings.requestProfile()
        val controllerCapabilities = gfnControllerCapabilities(appLaunchMode)
        return buildJsonObject {
            putJsonObject("sessionRequestData") {
                put("appId", appId)
                if (internalTitle.isBlank()) put("internalTitle", JsonNull) else put("internalTitle", internalTitle)
                putJsonArray("availableSupportedControllers") {
                    controllerCapabilities.supportedControllerTypes.forEach { add(JsonPrimitive(it)) }
                }
                put("networkTestSessionId", JsonNull)
                put("parentSessionId", JsonNull)
                put("clientIdentification", "GFN-PC")
                put("deviceHashId", deviceId)
                put("clientVersion", "30.0")
                putStreamTransportRequest(settings)
                put("clientPlatformName", if (appLaunchMode == GfnAppLaunchMode.TOUCH_FRIENDLY) "android" else identity.platformName)
                putJsonArray("clientRequestMonitorSettings") {
                    add(monitorSettings(profile, settings.fps, identity))
                }
                put("useOps", true)
                put("audioMode", 2)
                put("metaData", streamSessionMetadata(settings, profile, physicalDisplayResolution))
                put("sdrHdrMode", if (profile.hdrEnabled) 1 else 0)
                put("clientDisplayHdrCapabilities", if (profile.hdrEnabled) hdrCapabilitiesJson() else JsonNull)
                put("surroundAudioInfo", 0)
                put("remoteControllersBitmap", controllerCapabilities.remoteControllersBitmap)
                put("clientTimezoneOffset", java.util.TimeZone.getDefault().getOffset(System.currentTimeMillis()))
                put("appLaunchMode", appLaunchMode)
                put("partnerCustomData", "")
                put("accountLinked", accountLinked)
                put("enablePersistingInGameSettings", identity.persistGameSettings)
                put("userAge", 26)
                put("requestedStreamingFeatures", requestedStreamingFeatures(settings, profile))
            }
        }
    }

    private fun buildClaimRequestBody(
        appId: String,
        deviceId: String,
        settings: StreamSettings,
        physicalDisplayResolution: Pair<Int, Int>?,
        streamingBaseUrl: String?,
        appLaunchMode: Int,
    ): JsonObject =
        buildMinimalClaimRequestBody(
            appId = appId,
            deviceId = deviceId,
            settings = settings,
            physicalDisplayResolution = physicalDisplayResolution,
            streamingBaseUrl = streamingBaseUrl,
            appLaunchMode = appLaunchMode,
            isAndroidTv = isAndroidTv,
            useDesktopNativeTvIdentity = useDesktopNativeTvIdentity,
        )

    private suspend fun toSessionInfo(zone: String, base: String, payload: JsonObject, clientId: String, deviceId: String): SessionInfo {
        val requestStatus = payload.obj("requestStatus")
        val status = requestStatus?.int("statusCode")
        if (status != 1) {
            throw CloudMatchRequestStatusException(
                statusCode = status,
                statusDescription = requestStatus?.string("statusDescription"),
                unifiedErrorCode = requestStatus?.string("unifiedErrorCode"),
            )
        }
        val session = payload.obj("session") ?: error("CloudMatch response missing session")
        val sessionStatus = session.int("status") ?: 0
        val signaling = runCatching { resolveSignaling(payload) }.getOrElse { error ->
            if (sessionStatus in READY_SESSION_STATUSES) {
                throw error
            }
            null
        }
        return SessionInfo(
            sessionId = session.string("sessionId") ?: error("Missing session id"),
            status = sessionStatus,
            queuePosition = extractQueuePosition(session),
            seatSetupStep = session.obj("seatSetupInfo")?.int("seatSetupStep"),
            adState = extractAdState(session),
            zone = payload.obj("requestStatus")?.string("serverId")?.takeIf { it.isNotBlank() } ?: zone,
            assignedZone = assignedSessionZoneFromControlHost(session.obj("sessionControlInfo")?.string("ip")),
            streamingBaseUrl = base,
            serverIp = signaling?.serverIp.orEmpty(),
            signalingServer = signaling?.signalingServer.orEmpty(),
            signalingUrl = signaling?.signalingUrl.orEmpty(),
            gpuType = session.string("gpuType"),
            iceServers = normalizeIceServers(payload),
            mediaConnectionInfo = signaling?.mediaConnectionInfo,
            rtspsEndpoints = nvstRtspEndpoints(session.arr("connectionInfo")),
            negotiatedStreamProfile = extractNegotiatedStreamProfile(session),
            monitorSnapshot = extractSessionMonitorSnapshot(session),
            requestedStreamingFeatures = normalizeStreamingFeatures(session.obj("sessionRequestData")?.obj("requestedStreamingFeatures")),
            finalizedStreamingFeatures = normalizeStreamingFeatures(session.obj("finalizedStreamingFeatures")),
            clientId = clientId,
            deviceId = deviceId,
        )
    }

    private fun extractAdState(session: JsonObject): SessionAdState? {
        val required = session.boolean("sessionAdsRequired")
            ?: session.boolean("isAdsRequired")
            ?: session.obj("sessionProgress")?.boolean("isAdsRequired")
            ?: session.obj("progressInfo")?.boolean("isAdsRequired")
        val ads = session.arr("sessionAds")?.mapIndexedNotNull { index, raw ->
            val ad = raw.asObject() ?: return@mapIndexedNotNull null
            val media = ad.arr("adMediaFiles")?.mapNotNull {
                val m = it.asObject() ?: return@mapNotNull null
                SessionAdMediaFile(m.string("mediaFileUrl"), m.string("encodingProfile"))
            }?.sortedBy {
                when (it.encodingProfile) {
                    "mp4deinterlaced720p" -> 0
                    "webm" -> 1
                    "hlsadaptive" -> 2
                    else -> 99
                }
            }.orEmpty()
            val id = ad.string("adId") ?: "ad-${index + 1}"
            if (media.isEmpty() && ad.string("adUrl") == null && ad.string("mediaUrl") == null && ad.string("title") == null) null else {
                SessionAdInfo(
                    adId = id,
                    state = ad.int("adState"),
                    adState = ad.int("adState"),
                    adUrl = ad.string("adUrl"),
                    mediaUrl = ad.string("mediaUrl") ?: ad.string("videoUrl") ?: ad.string("url"),
                    adMediaFiles = media,
                    clickThroughUrl = ad.string("clickThroughUrl"),
                    adLengthInSeconds = ad.double("adLengthInSeconds"),
                    durationMs = ad.int("durationMs")?.toLong() ?: ad.int("durationInMs")?.toLong(),
                    title = ad.string("title"),
                    description = ad.string("description"),
                )
            }
        }.orEmpty()
        val opportunityRaw = session.obj("opportunity")
        val opportunity = opportunityRaw?.let {
            SessionOpportunityInfo(
                state = it.string("state"),
                queuePaused = it.boolean("queuePaused"),
                gracePeriodSeconds = it.int("gracePeriodSeconds"),
                message = it.string("message"),
                title = it.string("title"),
                description = it.string("description"),
            )
        }
        val queuePaused = opportunity?.queuePaused ?: (opportunity?.state?.equals("graceperiodstart", true) == true)
        val effectiveRequired = required ?: ads.isNotEmpty()
        val message = opportunity?.message ?: opportunity?.description ?: if (queuePaused) "Resume ads to stay in queue." else if (effectiveRequired) "Finish ads to stay in queue." else null
        if (!effectiveRequired && ads.isEmpty() && !queuePaused && message == null) return null
        return SessionAdState(
            isAdsRequired = effectiveRequired,
            sessionAdsRequired = required,
            isQueuePaused = queuePaused,
            gracePeriodSeconds = opportunity?.gracePeriodSeconds,
            message = message,
            sessionAds = ads,
            ads = ads,
            opportunity = opportunity,
            serverSentEmptyAds = session["sessionAds"] == null || session["sessionAds"] is JsonNull,
        )
    }

    private fun extractQueuePosition(session: JsonObject): Int? =
        session.int("queuePosition")
            ?: session.obj("seatSetupInfo")?.int("queuePosition")
            ?: session.obj("sessionProgress")?.int("queuePosition")
            ?: session.obj("progressInfo")?.int("queuePosition")

    private fun normalizeStreamingFeatures(features: JsonObject?): StreamingFeatures? {
        if (features == null) return null
        val normalized = StreamingFeatures(
            reflex = features.boolean("reflex"),
            bitDepth = features.int("bitDepth"),
            chromaFormat = features.int("chromaFormat"),
            enabledL4S = features.boolean("enabledL4S"),
            trueHdr = features.boolean("trueHdr"),
        )
        return if (listOf(normalized.reflex, normalized.bitDepth, normalized.chromaFormat, normalized.enabledL4S, normalized.trueHdr).all { it == null }) null else normalized
    }

    private fun extractNegotiatedStreamProfile(session: JsonObject): NegotiatedStreamProfile? {
        val monitorSnapshot = extractSessionMonitorSnapshot(session)
        val finalized = session.obj("finalizedStreamingFeatures")
        val requested = session.obj("sessionRequestData")?.obj("requestedStreamingFeatures")
        val resolution = monitorSnapshot?.returnedResolution
            ?: monitorSnapshot?.finalSelectedResolution
            ?: monitorSnapshot?.requestedResolution
        val fps = monitorSnapshot?.returnedFps ?: monitorSnapshot?.requestedFps
        val bitDepth = finalized?.int("bitDepth") ?: requested?.int("bitDepth")
        val chroma = finalized?.int("chromaFormat") ?: requested?.int("chromaFormat")
        val cq = when {
            bitDepth == 10 && chroma == 2 -> ColorQuality.TenBit444
            bitDepth == 10 -> ColorQuality.TenBit420
            chroma == 2 -> ColorQuality.EightBit444
            bitDepth == 0 -> ColorQuality.EightBit420
            else -> null
        }
        return NegotiatedStreamProfile(
            resolution = resolution,
            fps = fps,
            colorQuality = cq,
            enableL4S = finalized?.boolean("enabledL4S") ?: requested?.boolean("enabledL4S"),
            enableReflex = finalized?.boolean("reflex") ?: requested?.boolean("reflex"),
        ).takeIf {
            it.resolution != null || it.fps != null || it.colorQuality != null || it.enableL4S != null || it.enableReflex != null
        }
    }

    private fun normalizeIceServers(payload: JsonObject): List<IceServer> {
        val servers = payload.obj("session")
            ?.obj("iceServerConfiguration")
            ?.arr("iceServers")
            ?.mapNotNull { raw ->
                val obj = raw.asObject() ?: return@mapNotNull null
                val urlsElement = obj["urls"]
                val urls = when (urlsElement) {
                    is JsonArray -> urlsElement.mapNotNull { it.asString() }
                    is JsonPrimitive -> listOfNotNull(urlsElement.contentOrNull)
                    else -> emptyList()
                }
                if (urls.isEmpty()) null else IceServer(urls, obj.string("username"), obj.string("credential"))
            }
            .orEmpty()
        return servers.ifEmpty {
            listOf(
                IceServer(listOf("stun:s1.stun.gamestream.nvidia.com:19308")),
                IceServer(listOf("stun:stun.l.google.com:19302")),
                IceServer(listOf("stun:stun1.l.google.com:19302")),
            )
        }
    }

    private data class SignalingResolution(
        val serverIp: String,
        val signalingServer: String,
        val signalingUrl: String,
        val mediaConnectionInfo: MediaConnectionInfo?,
    )

    private fun resolveSignaling(payload: JsonObject): SignalingResolution {
        val session = payload.obj("session") ?: error("Missing session")
        val connections = session.arr("connectionInfo")?.mapNotNull { it.asObject() }.orEmpty()
        val serverIp = streamingServerIp(payload) ?: error("CloudMatch response did not include a signaling host")
        val signalingConnection = connections.firstOrNull { it.int("usage") == 14 && it.string("ip") != null } ?: connections.firstOrNull { it.string("ip") != null }
        val resourcePath = signalingConnection?.string("resourcePath") ?: "/nvst/"
        val (url, host) = buildSignalingUrl(resourcePath, serverIp)
        val effectiveHost = host ?: serverIp
        return SignalingResolution(
            serverIp = serverIp,
            signalingServer = if (effectiveHost.contains(":")) effectiveHost else "$effectiveHost:443",
            signalingUrl = url,
            mediaConnectionInfo = resolveMediaConnectionInfo(connections, serverIp),
        )
    }

    private fun resolveMediaConnectionInfo(connections: List<JsonObject>, serverIp: String): MediaConnectionInfo? {
        fun extractIp(conn: JsonObject): String? = conn.string("ip")?.let(::usableSessionHost) ?: conn.string("resourcePath")?.let(::extractHostFromUrl)
        fun extractPort(conn: JsonObject): Int = conn.int("port") ?: conn.string("resourcePath")?.let { Uri.parse(it.replace("rtsps://", "https://").replace("rtsp://", "http://")).port } ?: 0
        listOf(2, 17).forEach { usage ->
            connections.firstOrNull { it.int("usage") == usage }?.let {
                val ip = extractIp(it)
                val port = extractPort(it)
                if (ip != null && port > 0) return MediaConnectionInfo(ip, port)
            }
        }
        webRtcMediaFallbackConnections(connections).forEach {
            val port = extractPort(it)
            if (port > 0) return MediaConnectionInfo(extractIp(it) ?: serverIp, port)
        }
        return null
    }

    private fun streamingServerIp(payload: JsonObject): String? {
        val session = payload.obj("session") ?: return null
        return streamingServerIpFromSession(session)
    }

    private fun streamingServerIpFromSession(session: JsonObject): String? {
        val conn = session.arr("connectionInfo")?.mapNotNull { it.asObject() }?.firstOrNull { it.int("usage") == 14 }
        conn?.string("ip")?.let(::usableSessionHost)?.let { return it }
        conn?.string("resourcePath")?.let(::extractHostFromUrl)?.let { return it }
        return session.obj("sessionControlInfo")?.string("ip")?.let(::usableSessionHost)
    }

    private fun buildSignalingUrl(raw: String, serverIp: String): Pair<String, String?> =
        when {
            raw.startsWith("rtsps://") || raw.startsWith("rtsp://") -> {
                val host = raw.substringAfter("://").substringBefore(":").substringBefore("/")
                usableSessionHost(host)?.let { "wss://$it/nvst/" to it } ?: ("wss://$serverIp:443/nvst/" to null)
            }
            raw.startsWith("wss://") -> extractHostFromUrl(raw)?.let { raw to it } ?: ("wss://$serverIp:443/nvst/" to null)
            raw.startsWith("/") -> "wss://$serverIp:443$raw" to null
            else -> "wss://$serverIp:443/nvst/" to null
        }

    private fun extractHostFromUrl(raw: String): String? {
        val after = listOf("rtsps://", "rtsp://", "wss://", "https://").firstOrNull { raw.startsWith(it) }?.let { raw.removePrefix(it) } ?: return null
        val host = after.substringBefore(":").substringBefore("/")
        return usableSessionHost(host)
    }

    private fun isZoneHostname(value: String): Boolean =
        value.contains("cloudmatchbeta.nvidiagrid.net") || value.contains("cloudmatch.nvidiagrid.net")

    private suspend fun resolveLaunchSessionBaseUrl(token: String, base: String): String {
        if (!isProviderRootStreamingBase(base)) return base
        val regions = fetchDynamicRegions(http, token, base).first
        return providerLaunchBaseUrl(base, regions)
    }

    private fun resolveStreamingBaseUrl(zone: String, provided: String?): String {
        normalizeStreamingServiceUrl(provided.orEmpty())?.let { return it.trimEnd('/') }
        val safeZone = zone.trim().takeIf { it.isNotBlank() && !it.startsWith(".") && !it.contains("/") && !it.contains(":") }
        return if (safeZone != null) "https://$safeZone.cloudmatchbeta.nvidiagrid.net" else DEFAULT_STREAMING_SERVICE_URL.trimEnd('/')
    }

    private fun resolvePollStopBase(zone: String, provided: String?, serverIp: String?): String {
        val base = resolveStreamingBaseUrl(zone, provided)
        val host = serverIp?.takeIf { it.isNotBlank() }
        return if (host != null && base.contains("cloudmatchbeta.nvidiagrid.net") && !isZoneHostname(host)) "https://$host" else base
    }

}

internal fun cloudMatchSessionRequestUrl(
    base: String,
    settings: StreamSettings,
    sessionId: String? = null,
): String {
    val path = if (sessionId.isNullOrBlank()) {
        "${base.trimEnd('/')}/v2/session"
    } else {
        "${base.trimEnd('/')}/v2/session/${encoded(sessionId)}"
    }
    return "$path?keyboardLayout=${encoded(settings.keyboardLayout)}&languageCode=${encoded(settings.gameLanguage)}"
}

internal fun usableSessionHost(value: String?): String? {
    val host = value?.trim().orEmpty()
    return host.takeIf {
        it.isNotBlank() &&
            !it.startsWith(".") &&
            !it.endsWith(".") &&
            !it.contains("..")
    }
}

private fun isProviderRootStreamingBase(base: String): Boolean {
    val url = base.toHttpUrlOrNull() ?: return false
    val host = url.host.lowercase(Locale.US)
    return host.startsWith("prod.") && host.endsWith(".geforcenow.nvidiagrid.net")
}

internal fun providerLaunchBaseUrl(providerBase: String, regions: List<StreamRegion>): String {
    val normalizedBase = normalizeStreamingServiceUrl(providerBase)?.trimEnd('/') ?: providerBase.trim().trimEnd('/')
    if (!isProviderRootStreamingBase(normalizedBase)) return normalizedBase
    val providerHost = normalizedBase.toHttpUrlOrNull()?.host?.lowercase(Locale.US) ?: return normalizedBase
    val regionUrls = regions
        .mapNotNull { normalizeStreamingServiceUrl(it.url)?.trimEnd('/') }
        .distinct()
        .filter { regionUrl ->
            val regionHost = regionUrl.toHttpUrlOrNull()?.host?.lowercase(Locale.US)
            regionHost != null && regionHost != providerHost
        }
    return if (regionUrls.size == 1) regionUrls.first() else normalizedBase
}

suspend fun fetchDynamicRegions(
    http: OkHttpClient,
    token: String?,
    streamingBaseUrl: String,
): Pair<List<StreamRegion>, String?> {
    val base = normalizeStreamingServiceUrl(streamingBaseUrl) ?: return emptyList<StreamRegion>() to null
    return runCatching {
        val request = Request.Builder()
            .url("${base}v2/serverInfo")
            .headers(
                Headers.Builder()
                    .putDesktopLcars(token, clientType = "BROWSER", clientStreamer = "WEBRTC")
                    .build(),
            )
            .build()
        val (code, text) = http.awaitText(request)
        if (code !in 200..299) return@runCatching emptyList<StreamRegion>() to null
        val data = OpenNowJson.parseToJsonElement(text).jsonObject
        val vpcId = data.obj("requestStatus")?.string("serverId")
        val regions = data.arr("metaData")?.mapNotNull {
            val obj = it.asObject() ?: return@mapNotNull null
            val key = obj.string("key") ?: return@mapNotNull null
            val value = obj.string("value") ?: return@mapNotNull null
            val regionUrl = normalizeStreamingServiceUrl(value) ?: return@mapNotNull null
            if (key == "gfn-regions" || key.startsWith("gfn-")) null else StreamRegion(key, regionUrl)
        }?.sortedBy { it.name }.orEmpty()
        regions to vpcId
    }.getOrDefault(emptyList<StreamRegion>() to null)
}

private val NON_ALNUM_RUN = Regex("[^a-z0-9]+")

// Runs once per catalogue entry during the merge; compiling the pattern per call showed up on
// large libraries.
private fun String.normalizedTitleKey(): String =
    trim().lowercase(Locale.US).replace(NON_ALNUM_RUN, " ").trim()
private fun String.isNumeric(): Boolean = all(Char::isDigit)

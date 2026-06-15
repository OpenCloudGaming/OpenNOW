package com.opencloudgaming.opennow

import android.content.Context
import android.content.Intent
import android.net.Uri
import android.os.Build
import android.util.Base64
import androidx.browser.customtabs.CustomTabsIntent
import kotlinx.coroutines.Dispatchers
import kotlinx.coroutines.async
import kotlinx.coroutines.channels.Channel
import kotlinx.coroutines.coroutineScope
import kotlinx.coroutines.delay
import kotlinx.coroutines.withContext
import kotlinx.serialization.json.Json
import kotlinx.serialization.json.JsonArray
import kotlinx.serialization.json.JsonElement
import kotlinx.serialization.json.JsonNull
import kotlinx.serialization.json.JsonObject
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
import okhttp3.HttpUrl.Companion.toHttpUrl
import okhttp3.HttpUrl.Companion.toHttpUrlOrNull
import okhttp3.MediaType.Companion.toMediaType
import okhttp3.OkHttpClient
import okhttp3.Request
import okhttp3.RequestBody.Companion.toRequestBody
import okhttp3.Credentials
import okhttp3.dnsoverhttps.DnsOverHttps
import java.io.BufferedReader
import java.io.InputStreamReader
import java.io.OutputStreamWriter
import java.net.InetSocketAddress
import java.net.InetAddress
import java.net.Proxy
import java.net.ServerSocket
import java.net.Socket
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
    "Mozilla/5.0 (Windows NT 10.0; Win64; x64) AppleWebKit/537.36 (KHTML, like Gecko) Chrome/128.0.0.0 Safari/537.36 NVIDIACEFClient/HEAD/debb5919f6 GFN-PC/2.0.80.173"
private const val GFN_CLIENT_VERSION = "2.0.80.173"
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
private const val MES_URL = "https://mes.geforcenow.com/v4/subscriptions"
private const val PRINTEDWASTE_QUEUE_URL = "https://api.printedwaste.com/gfn/queue/"
private const val PRINTEDWASTE_SERVER_MAPPING_URL = "https://remote.printedwaste.com/config/GFN_SERVERID_TO_REGION_MAPPING"
private const val DEFAULT_STREAMING_SERVICE_URL = "https://prod.cloudmatchbeta.nvidiagrid.net/"
private const val CLIENT_ID = "ZU7sPN-miLujMD95LfOQ453IB0AtjM8sMyvgJ9wCXEQ"
private const val DEVICE_CODE_CLIENT_ID = "q61ddeJrVt7O90Nl-P-N7I36yctih4Ml6FyXLrb6j-U"
private const val DEFAULT_IDP_ID = "PDiAhv2kJTFeQ7WOPqiQ2tRZ7lGhR2X11dXvM4TZSxg"
private const val SCOPES = "openid consent email tk_client age"
private const val PANELS_QUERY_HASH = "f8e26265a5db5c20e1334a6872cf04b6e3970507697f6ae55a6ddefa5420daf0"
private const val APP_METADATA_QUERY_HASH = "39187e85b6dcf60b7279a5f233288b0a8b69a8b1dbcfb5b25555afdcb988f0d7"
private const val LIBRARY_WITH_TIME_QUERY_HASH = "039e8c0d553972975485fee56e59f2549d2fdb518e247a42ab5022056a74406f"
private const val DEFAULT_LOCALE = "en_US"
private const val DEFAULT_CATALOG_FETCH_COUNT = 120
private const val MAX_CATALOG_PAGES = 3
private const val DEFAULT_SORT_ID = "relevance"
private const val SESSION_MODIFY_ACTION_AD_UPDATE = 6

private val JSON_MEDIA_TYPE = "application/json; charset=utf-8".toMediaType()
private val GRAPHQL_MEDIA_TYPE = "application/graphql".toMediaType()
private val REDIRECT_PORTS = intArrayOf(2259, 6460, 7119, 8870, 9096)
private const val OAUTH_CALLBACK_TIMEOUT_MS = 120_000L
private const val DEVICE_CODE_MIN_POLL_INTERVAL_SECONDS = 5
private const val TOKEN_REFRESH_WINDOW_MS = 10 * 60 * 1000L
private const val CLIENT_TOKEN_REFRESH_WINDOW_MS = 5 * 60 * 1000L
private val READY_SESSION_STATUSES = setOf(2, 3)
private const val INVALID_SESSION_PROXY_MESSAGE =
    "Invalid session proxy URL. Use http://host:port, https://host:port, socks4://host:port, or socks5://host:port."

val OpenNowJson: Json = Json {
    ignoreUnknownKeys = true
    explicitNulls = false
    isLenient = true
    encodeDefaults = true
}

fun defaultHttpClient(): OkHttpClient =
    OkHttpClient.Builder()
        .dns(OpenNowDns)
        .connectTimeout(15, TimeUnit.SECONDS)
        .readTimeout(30, TimeUnit.SECONDS)
        .writeTimeout(30, TimeUnit.SECONDS)
        .pingInterval(15, TimeUnit.SECONDS)
        .build()

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

private object OpenNowDns : Dns {
    private val doh: Dns by lazy {
        val bootstrapClient = OkHttpClient.Builder().build()
        DnsOverHttps.Builder()
            .client(bootstrapClient)
            .url("https://cloudflare-dns.com/dns-query".toHttpUrl())
            .bootstrapDnsHosts(ipv4(1, 1, 1, 1), ipv4(1, 0, 0, 1))
            .build()
    }

    override fun lookup(hostname: String): List<InetAddress> {
        val systemResult = runCatching { Dns.SYSTEM.lookup(hostname) }.getOrNull()
        if (!systemResult.isNullOrEmpty()) return systemResult
        val dohResult = runCatching { doh.lookup(hostname) }.getOrNull()
        if (!dohResult.isNullOrEmpty()) return dohResult
        throw UnknownHostException("$hostname: system DNS and DNS-over-HTTPS fallback failed")
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

private suspend fun OkHttpClient.awaitText(request: Request): Pair<Int, String> =
    withContext(Dispatchers.IO) {
        newCall(request).execute().use { response ->
            response.code to (response.body?.string().orEmpty())
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

private fun cloudMatchHeaders(
    token: String,
    clientId: String,
    deviceId: String,
    includeOrigin: Boolean,
): Headers =
    Headers.Builder()
        .add("User-Agent", GFN_USER_AGENT)
        .add("Authorization", gfnJwtAuthorization(token))
        .add("Content-Type", "application/json")
        .add("nv-browser-type", "CHROME")
        .add("nv-client-id", clientId)
        .add("nv-client-streamer", "NVIDIA-CLASSIC")
        .add("nv-client-type", "NATIVE")
        .add("nv-client-version", GFN_CLIENT_VERSION)
        .add("nv-device-make", "UNKNOWN")
        .add("nv-device-model", "UNKNOWN")
        .add("nv-device-os", "WINDOWS")
        .add("nv-device-type", "DESKTOP")
        .add("x-device-id", deviceId)
        .apply {
            if (includeOrigin) {
                add("Origin", GFN_PLAY_ORIGIN)
                add("Referer", GFN_PLAY_REFERER)
            }
        }
        .build()

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

    suspend fun restore(forceRefresh: Boolean = false): AuthSession? {
        val restored = authStore.activeSession() ?: return null
        var session = restored
        if (session.tokens.clientToken.isNullOrBlank() || isNearExpiry(session.tokens.clientTokenExpiresAt, CLIENT_TOKEN_REFRESH_WINDOW_MS)) {
            val withClientToken = runCatching { ensureClientToken(session.tokens) }.getOrElse { session.tokens }
            if (withClientToken != session.tokens) {
                session = session.copy(tokens = withClientToken)
                authStore.upsertSession(session)
            }
        }

        val refreshed = if (forceRefresh || isNearExpiry(session.tokens.expiresAt, TOKEN_REFRESH_WINDOW_MS)) {
            refreshSession(session, forceRefresh)
        } else {
            session
        }

        authStore.upsertSession(refreshed)
        return refreshed
    }

    suspend fun login(provider: LoginProvider): AuthSession {
        drainExternalOAuthRedirects()
        val server = openAvailableCallbackServer()
        val port = server.localPort
        val verifier = randomBase64Url(64).take(86)
        val challenge = sha256Base64Url(verifier)
        val authUrl = buildAuthUrl(provider, challenge, port)
        val code = coroutineScope {
            val codeDeferred = async(Dispatchers.IO) { waitForAuthorizationCode(server) }
            val customTabs = CustomTabsIntent.Builder().build()
            customTabs.intent.addFlags(Intent.FLAG_ACTIVITY_NEW_TASK)
            runCatching {
                customTabs.launchUrl(context, Uri.parse(authUrl))
            }.onFailure {
                server.close()
                throw it
            }
            codeDeferred.await()
        }
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

    private suspend fun refreshSession(session: AuthSession, forceRefresh: Boolean): AuthSession {
        val tokens = session.tokens
        val refreshErrors = mutableListOf<String>()

        if (!tokens.clientToken.isNullOrBlank()) {
            runCatching {
                val refreshed = mergeTokenSnapshot(tokens, refreshWithClientToken(tokens.clientToken, session.user.userId))
                return buildRefreshedSession(session, ensureClientTokenBestEffort(refreshed), source = "client token")
            }.onFailure { error ->
                refreshErrors += "client_token: ${error.message ?: "Unknown refresh error"}"
            }
        }

        val refresh = tokens.refreshToken
        if (!refresh.isNullOrBlank()) {
            runCatching {
                val refreshed = refreshAuthTokens(refresh, tokens)
                return buildRefreshedSession(session, ensureClientTokenBestEffort(refreshed), source = "refresh token")
            }.onFailure { error ->
                refreshErrors += "refresh_token: ${error.message ?: "Unknown refresh error"}"
            }
        }

        val hasRefreshMechanism = !tokens.clientToken.isNullOrBlank() || !tokens.refreshToken.isNullOrBlank()
        if (!hasRefreshMechanism) {
            if (isExpired(tokens.expiresAt)) {
                authStore.removeSession(session.user.userId)
                error("Saved session expired and has no refresh mechanism. Please log in again.")
            }
            return session
        }

        if (isExpired(tokens.expiresAt)) {
            authStore.removeSession(session.user.userId)
            val detail = refreshErrors.takeIf { it.isNotEmpty() }?.joinToString(" | ")
            error("Token refresh failed and the saved session expired. Please log in again.${detail?.let { " $it" }.orEmpty()}")
        }

        if (forceRefresh && refreshErrors.isNotEmpty()) {
            error("Token refresh failed. Using saved session token. ${refreshErrors.joinToString(" | ")}")
        }
        return session
    }

    private suspend fun refreshWithClientToken(clientToken: String, userId: String): JsonObject {
        val body = FormBody.Builder()
            .add("grant_type", "urn:ietf:params:oauth:grant-type:client_token")
            .add("client_token", clientToken)
            .add("client_id", CLIENT_ID)
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

    private suspend fun refreshAuthTokens(refresh: String, base: AuthTokens): AuthTokens {
        val body = FormBody.Builder()
            .add("grant_type", "refresh_token")
            .add("refresh_token", refresh)
            .add("client_id", CLIENT_ID)
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
        )
    }

    private fun mergeTokenSnapshot(base: AuthTokens, root: JsonObject): AuthTokens =
        AuthTokens(
            accessToken = requireNotNull(root.string("access_token")) { "Missing access token" },
            refreshToken = root.string("refresh_token") ?: base.refreshToken,
            idToken = root.string("id_token") ?: base.idToken,
            expiresAt = expiresAt(root.int("expires_in")),
            clientToken = root.string("client_token") ?: base.clientToken,
            clientTokenExpiresAt = base.clientTokenExpiresAt,
        )

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
            delay(intervalSeconds * 1000L)
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
            val (status, text) = http.awaitText(request)
            val root = runCatching { OpenNowJson.parseToJsonElement(text).jsonObject }.getOrNull()
            if (status in 200..299 && root != null) {
                return AuthTokens(
                    accessToken = requireNotNull(root.string("access_token")) { "Missing access token" },
                    refreshToken = root.string("refresh_token"),
                    idToken = root.string("id_token"),
                    expiresAt = expiresAt(root.int("expires_in")),
                    clientToken = root.string("client_token"),
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

    private suspend fun buildSession(provider: LoginProvider, tokens: AuthTokens, fallbackUser: AuthUser? = null): AuthSession {
        val userInfo = runCatching { fetchUserInfo(tokens.accessToken) }.getOrDefault(JsonObject(emptyMap()))
        val jwt = tokens.idToken?.let(::decodeJwtPayload)
        val userId = userInfo.string("sub") ?: jwt?.string("sub") ?: fallbackUser?.userId ?: userInfo.string("id") ?: "nvidia-user"
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

    private suspend fun openAvailableCallbackServer(): ServerSocket = withContext(Dispatchers.IO) {
        for (port in REDIRECT_PORTS) {
            val server = runCatching { openCallbackServerSocket(port) }.getOrNull()
            if (server != null) return@withContext server
        }
        error("No available OAuth callback ports")
    }

    private suspend fun waitForAuthorizationCode(server: ServerSocket): String = withContext(Dispatchers.IO) {
        server.use {
            val deadline = System.currentTimeMillis() + OAUTH_CALLBACK_TIMEOUT_MS
            while (System.currentTimeMillis() < deadline) {
                externalOAuthRedirects.tryReceive().getOrNull()?.let { params ->
                    authorizationCodeFromParams(params)?.let { code -> return@withContext code }
                }
                server.soTimeout = minOf(5_000, (deadline - System.currentTimeMillis()).coerceAtLeast(1)).toInt()
                val socket = try {
                    server.accept()
                } catch (_: SocketTimeoutException) {
                    continue
                }
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

    private fun openCallbackServerSocket(port: Int): ServerSocket =
        ServerSocket().apply {
            reuseAddress = true
            bind(InetSocketAddress(port))
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

class GfnCatalogRepository(
    private val http: OkHttpClient = defaultHttpClient(),
) {
    suspend fun fetchMainGames(token: String, providerStreamingBaseUrl: String): List<GameInfo> {
        val vpcId = getVpcId(token, providerStreamingBaseUrl)
        val panels = fetchPanels(token, listOf("MAIN"), vpcId, withLibraryTime = false)
        return mergePublicGameVariants(enrichGamesWithMetadata(token, vpcId, flattenPanels(panels)), fetchPublicGames())
    }

    suspend fun fetchLibraryGames(token: String, providerStreamingBaseUrl: String): List<GameInfo> {
        val vpcId = getVpcId(token, providerStreamingBaseUrl)
        val panels = runCatching { fetchPanels(token, listOf("LIBRARY"), vpcId, withLibraryTime = true) }
            .getOrElse { fetchPanels(token, listOf("LIBRARY"), vpcId, withLibraryTime = false) }
        return mergePublicGameVariants(enrichGamesWithMetadata(token, vpcId, flattenPanels(panels)), fetchPublicGames())
    }

    suspend fun browseCatalog(
        token: String,
        providerStreamingBaseUrl: String,
        searchQuery: String,
        sortId: String = DEFAULT_SORT_ID,
        filterIds: List<String> = emptyList(),
    ): CatalogBrowseResult {
        val vpcId = getVpcId(token, providerStreamingBaseUrl)
        val definitions = fetchFilterAndSortDefinitions(token)
        val selectedSort = definitions.sortOptions.firstOrNull { it.id == sortId }
            ?: definitions.sortOptions.firstOrNull { it.id == DEFAULT_SORT_ID }
            ?: CatalogSortOption(DEFAULT_SORT_ID, "Relevance", "itemMetadata.relevance:DESC,sortName:ASC")
        val selectedFilters = filterIds.filter { definitions.filterPayloadById.containsKey(it) }
        val filters = selectedFilters.mapNotNull { definitions.filterPayloadById[it]?.asObject() }
            .fold(mutableMapOf<String, JsonElement>()) { acc, obj ->
                acc.putAll(obj)
                acc
            }
        val query = catalogQuery(searchQuery.isNotBlank())
        val collectedApps = mutableListOf<JsonObject>()
        var numberReturned = 0
        var numberSupported = 0
        var totalCount = 0
        var hasNextPage = false
        var endCursor: String? = null
        var cursor = ""
        for (page in 0 until MAX_CATALOG_PAGES) {
            val payload = postGraphQl(
                query = query,
                variables = buildJsonObject {
                    put("vpcId", vpcId)
                    put("locale", DEFAULT_LOCALE)
                    put("sortString", selectedSort.orderBy)
                    put("fetchCount", DEFAULT_CATALOG_FETCH_COUNT)
                    put("cursor", cursor)
                    put("filters", JsonObject(filters))
                    if (searchQuery.isNotBlank()) put("searchString", searchQuery.trim())
                },
                token = token,
            ).checkGraphQlErrors()
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
        val publicGames = fetchPublicGames()
        val games = dedupeGames(collectedApps.map(::appToGame))
        val withSearchFallbacks = if (searchQuery.isBlank()) {
            games
        } else {
            dedupeGames(games + publicGames.filter { it.matchesSearch(searchQuery) })
        }
        val merged = mergePublicGameVariants(withSearchFallbacks, publicGames)
        return CatalogBrowseResult(
            games = merged,
            numberReturned = numberReturned,
            numberSupported = max(numberSupported, merged.size),
            totalCount = max(totalCount, merged.size),
            hasNextPage = hasNextPage,
            endCursor = endCursor?.takeIf { it.isNotBlank() },
            searchQuery = searchQuery,
            selectedSortId = selectedSort.id,
            selectedFilterIds = selectedFilters,
            filterGroups = definitions.filterGroups,
            sortOptions = definitions.sortOptions,
        )
    }

    suspend fun fetchPublicGames(): List<GameInfo> {
        val request = Request.Builder()
            .url("https://static.nvidiagrid.net/supported-public-game-list/locales/gfnpc-en-US.json")
            .header("User-Agent", GFN_USER_AGENT)
            .build()
        val (code, text) = http.awaitText(request)
        if (code !in 200..299) return emptyList()
        return OpenNowJson.parseToJsonElement(text).jsonArray
            .mapNotNull { item ->
                val obj = item.asObject() ?: return@mapNotNull null
                if (obj.string("status") != "AVAILABLE") return@mapNotNull null
                val title = obj.string("title") ?: return@mapNotNull null
                val id = obj["id"]?.jsonPrimitive?.contentOrNull ?: title
                val steamAppId = obj.string("steamUrl")?.substringAfter("/app/", "")?.substringBefore("/")
                val store = obj.string("store") ?: if (obj.string("publisher")?.contains("ncsoft", true) == true) "NCSoft" else "Unknown"
                GameInfo(
                    id = id,
                    uuid = id,
                    launchAppId = id.takeIf { it.all(Char::isDigit) },
                    title = title,
                    imageUrl = steamAppId?.takeIf { it.isNotBlank() }?.let { "https://cdn.cloudflare.steamstatic.com/steam/apps/$it/library_600x900.jpg" },
                    searchText = listOf(title, store, obj.string("publisher")).filterNotNull().joinToString(" ").lowercase(),
                    selectedVariantIndex = 0,
                    variants = listOf(GameVariant(id = id, store = store)),
                    availableStores = displayStoresForVariants(listOf(GameVariant(id = id, store = store))),
                )
            }
    }

    suspend fun resolveLaunchAppId(token: String, appIdOrUuid: String, providerStreamingBaseUrl: String): String? {
        if (appIdOrUuid.all(Char::isDigit)) return appIdOrUuid
        val vpcId = getVpcId(token, providerStreamingBaseUrl)
        val meta = fetchAppMetaData(token, listOf(appIdOrUuid), vpcId)
        return meta.firstOrNull()?.let(::resolveNumericAppId)
    }

    suspend fun getVpcId(token: String, providerStreamingBaseUrl: String): String {
        val base = normalizeStreamingServiceUrl(providerStreamingBaseUrl) ?: DEFAULT_STREAMING_SERVICE_URL
        return runCatching {
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
    }

    private suspend fun fetchPanels(token: String, panelNames: List<String>, vpcId: String, withLibraryTime: Boolean): JsonObject {
        val variables = buildJsonObject {
            put("vpcId", vpcId)
            put("locale", DEFAULT_LOCALE)
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
            put("locale", DEFAULT_LOCALE)
            putJsonArray("appIds") { appIds.distinct().forEach { add(JsonPrimitive(it)) } }
        }.toString()
        val extensions = buildJsonObject {
            putJsonObject("persistedQuery") { put("sha256Hash", APP_METADATA_QUERY_HASH) }
        }.toString()
        val url = "$GAMES_GRAPHQL_URL?requestType=appMetaData&extensions=${encoded(extensions)}&huId=${randomHuId()}&variables=${encoded(variables)}"
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
            val merged = appToGame(app)
            val selectedId = game.variants.getOrNull(game.selectedVariantIndex)?.id
            val selectedIndex = selectedId?.let { id -> merged.variants.indexOfFirst { it.id == id } } ?: -1
            merged.copy(
                id = game.id,
                selectedVariantIndex = if (selectedIndex >= 0) selectedIndex else merged.selectedVariantIndex,
            )
        })
    }

    private fun flattenPanels(payload: JsonObject): List<GameInfo> {
        val games = payload.checkGraphQlErrors("Games GraphQL").obj("data")?.arr("panels")?.flatMap { panel ->
            panel.asObject()?.arr("sections")?.flatMap { section ->
                section.asObject()?.arr("items")?.mapNotNull { item ->
                    val obj = item.asObject()
                    val app = obj?.obj("app")
                    if (obj?.string("__typename") == "GameItem" && app != null) appToGame(app) else null
                }.orEmpty()
            }.orEmpty()
        }.orEmpty()
        return dedupeGames(games)
    }

    private fun appToGame(app: JsonObject): GameInfo {
        val variants = app.arr("variants")?.mapNotNull { raw ->
            val obj = raw.asObject() ?: return@mapNotNull null
            val library = obj.obj("gfn")?.obj("library")
            GameVariant(
                id = obj.string("id") ?: return@mapNotNull null,
                store = obj.string("appStore") ?: "Unknown",
                supportedControls = obj.arr("supportedControls")?.mapNotNull { it.asString() }.orEmpty(),
                librarySelected = library?.boolean("selected"),
                libraryStatus = library?.string("status"),
                lastPlayedDate = library?.string("lastPlayedDate"),
                gfnStatus = obj.obj("gfn")?.string("status"),
            )
        }.orEmpty()
        val numericAppId = resolveNumericAppId(app)
        val selectedVariantId = app.arr("variants")
            ?.mapNotNull { it.asObject() }
            ?.firstOrNull { it.obj("gfn")?.obj("library")?.boolean("selected") == true }
            ?.string("id")
        val selectedIndex = max(0, variants.indexOfFirst { it.id == (selectedVariantId ?: numericAppId) })
        val images = app.obj("images")
        val imageUrl = listOf("KEY_ART", "GAME_BOX_ART", "TV_BANNER", "HERO_IMAGE")
            .firstNotNullOfOrNull { images?.string(it) }
            ?.let(::optimizeImage)
        val genres = extractLabels(app.arr("genres"))
        val featureLabels = (extractLabels(app.arr("features")) + extractLabels(app.arr("gameFeatures")) + extractLabels(app.arr("appFeatures")) + genres).distinct()
        val title = app.string("title") ?: app.string("id") ?: "Unknown Game"
        val stores = displayStoresForVariants(variants)
        return GameInfo(
            id = app.string("id") ?: title,
            uuid = app.string("id"),
            launchAppId = numericAppId,
            title = title,
            description = app.string("description"),
            longDescription = app.string("longDescription"),
            featureLabels = featureLabels,
            genres = genres,
            imageUrl = imageUrl,
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
        val query = """
            query GetFilterGroupAndSortOrderDefinitions(${'$'}locale: String!) {
              filterGroupDefinitions(language: ${'$'}locale) { id label filters { id label filters } }
              sortOrderDefinitions(language: ${'$'}locale) { id label orderBy }
            }
        """.trimIndent()
        val payload = postGraphQl(query, buildJsonObject { put("locale", DEFAULT_LOCALE) }, token).checkGraphQlErrors()
        val data = payload.obj("data")
        val filterPayloadById = mutableMapOf<String, JsonElement>()
        val groups = data?.arr("filterGroupDefinitions")?.mapNotNull { raw ->
            val group = raw.asObject() ?: return@mapNotNull null
            val options = group.arr("filters")?.mapNotNull { filterRaw ->
                val filter = filterRaw.asObject() ?: return@mapNotNull null
                val filterJson = filter.arr("filters")?.firstOrNull()?.asString() ?: return@mapNotNull null
                val parsed = runCatching { OpenNowJson.parseToJsonElement(filterJson) }.getOrNull() ?: return@mapNotNull null
                val id = filter.string("id") ?: return@mapNotNull null
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
        check(code in 200..299) { "GFN GraphQL failed ($code): ${text.take(400)}" }
        return OpenNowJson.parseToJsonElement(text).jsonObject
    }

    private fun catalogQuery(hasSearch: Boolean): String {
        val appFields = """
            numberReturned
            numberSupported
            pageInfo { hasNextPage endCursor totalCount }
            items {
              id
              title
              images { KEY_ART GAME_BOX_ART TV_BANNER HERO_IMAGE }
              variants { id appStore supportedControls gfn { status library { status selected } } }
              gfn { playabilityState minimumMembershipTierLabel catalogSkuStrings { SKU_BASED_TAG } }
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
            bucket.reduce { left, right ->
                val variants = (left.variants + right.variants).distinctBy { it.id }
                val selectedVariantId = left.variants.getOrNull(left.selectedVariantIndex)?.id
                    ?: right.variants.getOrNull(right.selectedVariantIndex)?.id
                val selectedIndex = selectedVariantId?.let { id -> variants.indexOfFirst { it.id == id } } ?: -1
                left.copy(
                    launchAppId = left.launchAppId ?: right.launchAppId,
                    uuid = left.uuid ?: right.uuid,
                    description = left.description ?: right.description,
                    longDescription = left.longDescription ?: right.longDescription,
                    imageUrl = left.imageUrl ?: right.imageUrl,
                    variants = variants,
                    availableStores = displayStoresForVariants(variants),
                    genres = (left.genres + right.genres).distinct(),
                    featureLabels = (left.featureLabels + right.featureLabels).distinct(),
                    isInLibrary = left.isInLibrary || right.isInLibrary,
                    searchText = listOfNotNull(left.searchText, right.searchText).joinToString(" ").ifBlank { null },
                    selectedVariantIndex = if (selectedIndex >= 0) selectedIndex else left.selectedVariantIndex.coerceAtMost(max(variants.size - 1, 0)),
                )
            }
        }

    private fun mergePublicGameVariants(games: List<GameInfo>, publicGames: List<GameInfo>): List<GameInfo> {
        val publicByTitle = publicGames.associateBy { it.title.normalizedTitleKey() }
        return games.map { game ->
            val publicGame = publicByTitle[game.title.normalizedTitleKey()] ?: return@map game
            val existingStores = game.variants.map { normalizeGameStore(it.store) }.toSet()
            val supplemental = publicGame.variants.filter { !isPrimaryCatalogStoreValue(it.store) && normalizeGameStore(it.store) !in existingStores }
            if (supplemental.isEmpty()) game else game.copy(
                launchAppId = game.launchAppId ?: publicGame.launchAppId,
                imageUrl = game.imageUrl ?: publicGame.imageUrl,
                variants = game.variants + supplemental,
                availableStores = displayStoresForVariants(game.variants + supplemental),
                searchText = listOfNotNull(game.searchText, publicGame.searchText).joinToString(" "),
            )
        }
    }

    private fun optimizeImage(url: String): String = if (url.contains("img.nvidiagrid.net")) "$url;f=webp;w=272" else url

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
            entitledResolutions = resolutions,
        )
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

                tcpPing(hostname, port, timeoutMs = 3_000)
                repeat(3) { index ->
                    if (index > 0) delay(100)
                    tcpPing(hostname, port, timeoutMs = 3_000)?.let(validPings::add)
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

class GfnSessionRepository(
    private val authStore: AuthStore,
    private val http: OkHttpClient = defaultHttpClient(),
) {
    suspend fun createSession(
        token: String,
        streamingBaseUrl: String?,
        appId: String,
        internalTitle: String,
        zone: String,
        settings: StreamSettings,
        accountLinked: Boolean = true,
    ): SessionInfo {
        require(appId.all(Char::isDigit)) { "Invalid launch appId '$appId'." }
        val clientId = UUID.randomUUID().toString()
        val deviceId = authStore.stableDeviceId()
        val base = resolveStreamingBaseUrl(zone, streamingBaseUrl)
        val body = buildSessionRequestBody(appId, internalTitle, settings, accountLinked, deviceId)
        val url = "$base/v2/session?keyboardLayout=${encoded(settings.keyboardLayout)}&languageCode=${encoded(settings.gameLanguage)}"
        val host = Uri.parse(base).host.orEmpty()
        val requestHttp = if (isZoneHostname(host)) sessionProxyHttpClient(settings, http) else http
        val request = Request.Builder()
            .url(url)
            .headers(cloudMatchHeaders(token, clientId, deviceId, includeOrigin = true))
            .post(body.toString().toRequestBody(JSON_MEDIA_TYPE))
            .build()
        val (_, text) = requestHttp.awaitText(request)
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
    ): SessionInfo {
        val cid = clientId ?: UUID.randomUUID().toString()
        val did = deviceId ?: authStore.stableDeviceId()
        val base = resolvePollStopBase(zone, streamingBaseUrl, serverIp)
        val host = Uri.parse(base).host.orEmpty()
        val requestHttp = if (isZoneHostname(host)) sessionProxyHttpClient(settings, http) else http
        val request = Request.Builder()
            .url("$base/v2/session/$sessionId")
            .headers(cloudMatchHeaders(token, cid, did, includeOrigin = false))
            .build()
        val (_, text) = requestHttp.awaitText(request)
        val payload = OpenNowJson.parseToJsonElement(text).jsonObject
        val realServer = streamingServerIp(payload)
        if (isZoneHostname(host) && realServer != null && !isZoneHostname(realServer) && READY_SESSION_STATUSES.contains(payload.obj("session")?.int("status"))) {
            val directBase = "https://$realServer"
            val directRequest = Request.Builder()
                .url("$directBase/v2/session/$sessionId")
                .headers(cloudMatchHeaders(token, cid, did, includeOrigin = false))
                .build()
            val (code, directText) = http.awaitText(directRequest)
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
            .headers(cloudMatchHeaders(token, cid, did, includeOrigin = false))
            .delete()
            .build()
        requestHttp.awaitText(request)
    }

    suspend fun getActiveSessions(token: String, streamingBaseUrl: String, settings: StreamSettings): List<ActiveSessionInfo> {
        val base = streamingBaseUrl.trim().trimEnd('/')
        val host = Uri.parse(base).host.orEmpty()
        val requestHttp = if (isZoneHostname(host)) sessionProxyHttpClient(settings, http) else http
        val request = Request.Builder()
            .url("$base/v2/session")
            .headers(cloudMatchHeaders(token, UUID.randomUUID().toString(), authStore.stableDeviceId(), includeOrigin = false))
            .build()
        val (code, text) = requestHttp.awaitText(request)
        if (code !in 200..299) return emptyList()
        val payload = runCatching { OpenNowJson.parseToJsonElement(text).jsonObject }.getOrNull() ?: return emptyList()
        if (payload.obj("requestStatus")?.int("statusCode") != 1) return emptyList()
        return payload.arr("sessions")?.mapNotNull { raw ->
            val s = raw.asObject() ?: return@mapNotNull null
            val status = s.int("status") ?: return@mapNotNull null
            if (status !in setOf(1, 2, 3)) return@mapNotNull null
            val connIp = streamingServerIpFromSession(s)
            val controlIp = s.obj("sessionControlInfo")?.string("ip")
            val monitor = s.arr("monitorSettings")?.firstOrNull()?.asObject()
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
            )
        }.orEmpty()
    }

    suspend fun claimSession(token: String, active: ActiveSessionInfo, settings: StreamSettings): SessionInfo {
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
                .headers(cloudMatchHeaders(token, clientId, deviceId, includeOrigin = false))
                .build()
            val (code, text) = requestHttp.awaitText(prefetch)
            if (code in 200..299) {
                streamingServerIp(OpenNowJson.parseToJsonElement(text).jsonObject)?.let { effectiveServerIp = it }
            }
        }
        val sessionBase = if (useProviderBaseForSessionOps) providerBase else "https://$effectiveServerIp"
        val validationUrl = "$sessionBase/v2/session/${active.sessionId}"
        val validationRequest = Request.Builder()
            .url(validationUrl)
            .headers(cloudMatchHeaders(token, clientId, deviceId, includeOrigin = false))
            .build()
        val (_, validationText) = http.awaitText(validationRequest)
        val validation = runCatching { OpenNowJson.parseToJsonElement(validationText).jsonObject }.getOrNull()
        val status = validation?.obj("session")?.int("status")
        if (status != 1) {
            val claimBody = buildClaimRequestBody(active.sessionId, active.appId.toString(), settings, deviceId)
            val claimRequest = Request.Builder()
                .url("$sessionBase/v2/session/${active.sessionId}?keyboardLayout=${encoded(settings.keyboardLayout)}&languageCode=${encoded(settings.gameLanguage)}")
                .headers(cloudMatchHeaders(token, clientId, deviceId, includeOrigin = true))
                .put(claimBody.toString().toRequestBody(JSON_MEDIA_TYPE))
                .build()
            http.awaitText(claimRequest)
        }
        repeat(60) { attempt ->
            if (attempt > 0) delay(1000)
            val poll = Request.Builder()
                .url(validationUrl)
                .headers(cloudMatchHeaders(token, clientId, deviceId, includeOrigin = false))
                .build()
            val (code, text) = http.awaitText(poll)
            if (code in 200..299) {
                val payload = OpenNowJson.parseToJsonElement(text).jsonObject
                val pollStatus = payload.obj("session")?.int("status")
                if (pollStatus in READY_SESSION_STATUSES) {
                    return toSessionInfo("", sessionBase, payload, clientId, deviceId)
                }
            }
        }
        error("Session did not become ready after claiming.")
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
            .headers(cloudMatchHeaders(token, cid, did, includeOrigin = true))
            .put(body.toString().toRequestBody(JSON_MEDIA_TYPE))
            .build()
        val (code, text) = requestHttp.awaitText(request)
        check(code in 200..299) { "Queue ad update failed ($code): ${text.take(400)}" }
        return toSessionInfo(session.zone, base, OpenNowJson.parseToJsonElement(text).jsonObject, cid, did)
    }

    private fun buildSessionRequestBody(
        appId: String,
        internalTitle: String,
        settings: StreamSettings,
        accountLinked: Boolean,
        deviceId: String,
    ): JsonObject {
        val (width, height) = streamResolutionPixels(settings)
        val hdrEnabled = settings.hdrEnabled
        val bitDepth = if (hdrEnabled || settings.colorQuality.name.startsWith("TenBit")) 10 else 0
        val chroma = if (settings.colorQuality == ColorQuality.EightBit444 || settings.colorQuality == ColorQuality.TenBit444) 2 else 0
        return buildJsonObject {
            putJsonObject("sessionRequestData") {
                put("appId", appId)
                if (internalTitle.isBlank()) put("internalTitle", JsonNull) else put("internalTitle", internalTitle)
                putJsonArray("availableSupportedControllers") {}
                put("networkTestSessionId", JsonNull)
                put("parentSessionId", JsonNull)
                put("clientIdentification", "GFN-PC")
                put("deviceHashId", deviceId)
                put("clientVersion", "30.0")
                put("sdkVersion", "1.0")
                put("streamerVersion", 1)
                put("clientPlatformName", "windows")
                putJsonArray("clientRequestMonitorSettings") {
                    add(buildJsonObject {
                        put("monitorId", 0)
                        put("positionX", 0)
                        put("positionY", 0)
                        put("widthInPixels", width)
                        put("heightInPixels", height)
                        put("framesPerSecond", settings.fps)
                        put("sdrHdrMode", if (hdrEnabled) 1 else 0)
                        put("displayData", if (hdrEnabled) hdrDisplayData() else JsonNull)
                        put("hdr10PlusGamingData", JsonNull)
                        put("dpi", 100)
                    })
                }
                put("useOps", true)
                put("audioMode", 2)
                put("metaData", webRtcSessionMetadata(width, height))
                put("sdrHdrMode", if (hdrEnabled) 1 else 0)
                put("clientDisplayHdrCapabilities", if (hdrEnabled) hdrCapabilities() else JsonNull)
                put("surroundAudioInfo", 0)
                put("remoteControllersBitmap", 0)
                put("clientTimezoneOffset", java.util.TimeZone.getDefault().getOffset(System.currentTimeMillis()))
                put("enhancedStreamMode", 1)
                put("appLaunchMode", 1)
                put("secureRTSPSupported", false)
                put("partnerCustomData", "")
                put("accountLinked", accountLinked)
                put("enablePersistingInGameSettings", true)
                put("userAge", 26)
                put("requestedStreamingFeatures", requestedStreamingFeatures(settings, bitDepth, chroma, hdrEnabled))
            }
        }
    }

    private fun buildClaimRequestBody(sessionId: String, appId: String, settings: StreamSettings, deviceId: String): JsonObject {
        val (width, height) = streamResolutionPixels(settings)
        val hdrEnabled = settings.hdrEnabled
        val bitDepth = if (hdrEnabled || settings.colorQuality.name.startsWith("TenBit")) 10 else 0
        val chroma = if (settings.colorQuality == ColorQuality.EightBit444 || settings.colorQuality == ColorQuality.TenBit444) 2 else 0
        return buildJsonObject {
            put("action", 2)
            put("data", "RESUME")
            putJsonObject("sessionRequestData") {
                put("audioMode", 2)
                put("remoteControllersBitmap", 0)
                put("sdrHdrMode", if (hdrEnabled) 1 else 0)
                put("networkTestSessionId", JsonNull)
                putJsonArray("availableSupportedControllers") {}
                put("clientVersion", "30.0")
                put("deviceHashId", deviceId)
                put("internalTitle", JsonNull)
                put("clientPlatformName", "windows")
                putJsonArray("clientRequestMonitorSettings") {
                    add(buildJsonObject {
                        put("monitorId", 0)
                        put("positionX", 0)
                        put("positionY", 0)
                        put("widthInPixels", width)
                        put("heightInPixels", height)
                        put("framesPerSecond", settings.fps)
                        put("sdrHdrMode", if (hdrEnabled) 1 else 0)
                        put("displayData", if (hdrEnabled) hdrDisplayData() else JsonNull)
                        put("hdr10PlusGamingData", JsonNull)
                        put("dpi", 100)
                    })
                }
                put("metaData", webRtcSessionMetadata(width, height))
                put("surroundAudioInfo", 0)
                put("clientTimezoneOffset", java.util.TimeZone.getDefault().getOffset(System.currentTimeMillis()))
                put("clientIdentification", "GFN-PC")
                put("parentSessionId", JsonNull)
                put("appId", appId.toIntOrNull() ?: 0)
                put("streamerVersion", 1)
                put("appLaunchMode", 1)
                put("sdkVersion", "1.0")
                put("enhancedStreamMode", 1)
                put("useOps", true)
                put("clientDisplayHdrCapabilities", if (hdrEnabled) hdrCapabilities() else JsonNull)
                put("accountLinked", true)
                put("partnerCustomData", "")
                put("enablePersistingInGameSettings", true)
                put("secureRTSPSupported", false)
                put("userAge", 26)
                put("requestedStreamingFeatures", requestedStreamingFeatures(settings, bitDepth, chroma, hdrEnabled))
            }
            putJsonArray("metaData") {}
        }
    }

    private fun requestedStreamingFeatures(settings: StreamSettings, bitDepth: Int, chroma: Int, hdrEnabled: Boolean): JsonObject =
        buildJsonObject {
            put("reflex", settings.enableCloudGsync || settings.fps >= 60)
            put("bitDepth", bitDepth)
            put("cloudGsync", settings.enableCloudGsync)
            put("enabledL4S", settings.enableL4S)
            put("trueHdr", hdrEnabled)
            put("mouseMovementFlags", 0)
            put("supportedHidDevices", 0)
            put("profile", 0)
            put("fallbackToLogicalResolution", false)
            put("hidDevices", JsonNull)
            put("chromaFormat", chroma)
            put("prefilterMode", 0)
            put("prefilterSharpness", 0)
            put("prefilterNoiseReduction", 0)
            put("hudStreamingMode", 0)
            put("sdrColorSpace", 2)
            put("hdrColorSpace", if (hdrEnabled) 4 else 0)
        }

    private fun hdrDisplayData(): JsonObject =
        buildJsonObject {
            put("desiredContentMaxLuminance", 1000)
            put("desiredContentMinLuminance", 0)
            put("desiredContentMaxFrameAverageLuminance", 500)
        }

    private fun hdrCapabilities(): JsonObject =
        buildJsonObject {
            put("version", 1)
            put("hdrEdrSupportedFlagsInUint32", 1)
            put("staticMetadataDescriptorId", 0)
        }

    private fun webRtcSessionMetadata(width: Int, height: Int): JsonArray = buildJsonArray {
        add(kv("SubSessionId", UUID.randomUUID().toString()))
        add(kv("wssignaling", "1"))
        add(kv("GSStreamerType", "WebRTC"))
        add(kv("networkType", "Unknown"))
        add(kv("ClientImeSupport", "0"))
        if (width > 0 && height > 0) {
            add(kv("clientPhysicalResolution", buildJsonObject { put("horizontalPixels", width); put("verticalPixels", height) }.toString()))
        }
        add(kv("surroundAudioInfo", "2"))
    }

    private fun kv(key: String, value: String): JsonObject = buildJsonObject {
        put("key", key)
        put("value", value)
    }

    private suspend fun toSessionInfo(zone: String, base: String, payload: JsonObject, clientId: String, deviceId: String): SessionInfo {
        val status = payload.obj("requestStatus")?.int("statusCode")
        check(status == 1) { "CloudMatch returned status $status: ${payload.obj("requestStatus")?.string("statusDescription")}" }
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
            zone = zone,
            streamingBaseUrl = base,
            serverIp = signaling?.serverIp.orEmpty(),
            signalingServer = signaling?.signalingServer.orEmpty(),
            signalingUrl = signaling?.signalingUrl.orEmpty(),
            gpuType = session.string("gpuType"),
            iceServers = normalizeIceServers(payload),
            mediaConnectionInfo = signaling?.mediaConnectionInfo,
            negotiatedStreamProfile = extractNegotiatedStreamProfile(session),
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
            cloudGsync = features.boolean("cloudGsync"),
            chromaFormat = features.int("chromaFormat"),
            enabledL4S = features.boolean("enabledL4S"),
            trueHdr = features.boolean("trueHdr"),
        )
        return if (listOf(normalized.reflex, normalized.bitDepth, normalized.cloudGsync, normalized.chromaFormat, normalized.enabledL4S, normalized.trueHdr).all { it == null }) null else normalized
    }

    private fun extractNegotiatedStreamProfile(session: JsonObject): NegotiatedStreamProfile? {
        val monitor = session.obj("sessionRequestData")?.arr("clientRequestMonitorSettings")?.firstOrNull()?.asObject()
            ?: session.arr("monitorSettings")?.firstOrNull()?.asObject()
        val finalized = session.obj("finalizedStreamingFeatures")
        val requested = session.obj("sessionRequestData")?.obj("requestedStreamingFeatures")
        val width = monitor?.int("widthInPixels")
        val height = monitor?.int("heightInPixels")
        val fps = monitor?.int("framesPerSecond")
        val bitDepth = finalized?.int("bitDepth") ?: requested?.int("bitDepth")
        val chroma = finalized?.int("chromaFormat") ?: requested?.int("chromaFormat")
        val cq = when {
            bitDepth == 10 && chroma == 2 -> ColorQuality.TenBit444
            bitDepth == 10 -> ColorQuality.TenBit420
            chroma == 2 -> ColorQuality.EightBit444
            bitDepth == 0 -> ColorQuality.EightBit420
            else -> null
        }
        val resolution = if (width != null && height != null && width > 0 && height > 0) "${width}x$height" else null
        return NegotiatedStreamProfile(
            resolution = resolution,
            fps = fps,
            colorQuality = cq,
            enableL4S = finalized?.boolean("enabledL4S") ?: requested?.boolean("enabledL4S"),
            enableCloudGsync = finalized?.boolean("cloudGsync") ?: requested?.boolean("cloudGsync"),
            enableReflex = finalized?.boolean("reflex") ?: requested?.boolean("reflex"),
        ).takeIf {
            it.resolution != null || it.fps != null || it.colorQuality != null || it.enableL4S != null || it.enableCloudGsync != null || it.enableReflex != null
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
        fun extractIp(conn: JsonObject): String? = conn.string("ip") ?: conn.string("resourcePath")?.let(::extractHostFromUrl)
        fun extractPort(conn: JsonObject): Int = conn.int("port") ?: conn.string("resourcePath")?.let { Uri.parse(it.replace("rtsps://", "https://").replace("rtsp://", "http://")).port } ?: 0
        listOf(2, 17).forEach { usage ->
            connections.firstOrNull { it.int("usage") == usage }?.let {
                val ip = extractIp(it)
                val port = extractPort(it)
                if (ip != null && port > 0) return MediaConnectionInfo(ip, port)
            }
        }
        connections.filter { it.int("usage") == 14 }.sortedByDescending { it.int("port") ?: 0 }.forEach {
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
        conn?.string("ip")?.takeIf { it.isNotBlank() }?.let { return it }
        conn?.string("resourcePath")?.let(::extractHostFromUrl)?.let { return it }
        return session.obj("sessionControlInfo")?.string("ip")
    }

    private fun buildSignalingUrl(raw: String, serverIp: String): Pair<String, String?> =
        when {
            raw.startsWith("rtsps://") || raw.startsWith("rtsp://") -> {
                val host = raw.substringAfter("://").substringBefore(":").substringBefore("/")
                if (host.isNotBlank() && !host.startsWith(".")) "wss://$host/nvst/" to host else "wss://$serverIp:443/nvst/" to null
            }
            raw.startsWith("wss://") -> raw to raw.removePrefix("wss://").substringBefore("/")
            raw.startsWith("/") -> "wss://$serverIp:443$raw" to null
            else -> "wss://$serverIp:443/nvst/" to null
        }

    private fun extractHostFromUrl(raw: String): String? {
        val after = listOf("rtsps://", "rtsp://", "wss://", "https://").firstOrNull { raw.startsWith(it) }?.let { raw.removePrefix(it) } ?: return null
        val host = after.substringBefore(":").substringBefore("/")
        return host.takeIf { it.isNotBlank() && !it.startsWith(".") }
    }

    private fun isZoneHostname(value: String): Boolean =
        value.contains("cloudmatchbeta.nvidiagrid.net") || value.contains("cloudmatch.nvidiagrid.net")

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

private fun String.normalizedTitleKey(): String = trim().lowercase(Locale.US).replace(Regex("[^a-z0-9]+"), " ").trim()
private fun String.isNumeric(): Boolean = all(Char::isDigit)

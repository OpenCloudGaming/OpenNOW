// Adapted from OpenStroid Android. See android/app/src/sideload/assets/licenses/OpenStroid-NOTICE.txt.
package com.opencloudgaming.opennow.boosteroid

import com.opencloudgaming.opennow.AuthUser
import com.opencloudgaming.opennow.BOOSTEROID_GAME_PREFIX
import com.opencloudgaming.opennow.BuildConfig
import com.opencloudgaming.opennow.DeviceLoginPrompt
import com.opencloudgaming.opennow.GameInfo
import com.opencloudgaming.opennow.GameVariant
import com.opencloudgaming.opennow.OpenNowJson
import com.opencloudgaming.opennow.StreamSettings
import com.opencloudgaming.opennow.SubscriptionInfo
import com.opencloudgaming.opennow.VideoCodec
import com.opencloudgaming.opennow.streamResolutionPixels

import android.os.Build
import android.util.Base64
import kotlinx.coroutines.CompletableDeferred
import kotlinx.coroutines.CoroutineScope
import kotlinx.coroutines.Dispatchers
import kotlinx.coroutines.Job
import kotlinx.coroutines.SupervisorJob
import kotlinx.coroutines.async
import kotlinx.coroutines.cancel
import kotlinx.coroutines.coroutineScope
import kotlinx.coroutines.currentCoroutineContext
import kotlinx.coroutines.delay
import kotlinx.coroutines.isActive
import kotlinx.coroutines.launch
import kotlinx.coroutines.sync.Mutex
import kotlinx.coroutines.sync.withLock
import kotlinx.coroutines.withTimeoutOrNull
import kotlinx.serialization.json.JsonArray
import kotlinx.serialization.json.JsonElement
import kotlinx.serialization.json.JsonNull
import kotlinx.serialization.json.JsonObject
import kotlinx.serialization.json.JsonPrimitive
import kotlinx.serialization.json.booleanOrNull
import kotlinx.serialization.json.buildJsonObject
import kotlinx.serialization.json.contentOrNull
import kotlinx.serialization.json.doubleOrNull
import kotlinx.serialization.json.intOrNull
import kotlinx.serialization.json.longOrNull
import kotlinx.serialization.json.put
import okhttp3.Headers
import okhttp3.HttpUrl
import okhttp3.HttpUrl.Companion.toHttpUrl
import okhttp3.HttpUrl.Companion.toHttpUrlOrNull
import okhttp3.MediaType.Companion.toMediaType
import okhttp3.OkHttpClient
import okhttp3.Request
import okhttp3.RequestBody.Companion.toRequestBody
import okhttp3.Response
import okhttp3.WebSocket
import okhttp3.WebSocketListener
import okio.ByteString
import java.net.URLEncoder
import java.util.Locale
import java.util.UUID
import java.util.concurrent.ConcurrentHashMap
import java.util.concurrent.TimeUnit
import kotlin.math.max

private const val BOOSTEROID_BASE_URL = "https://cloud.boosteroid.com"
private const val BOOSTEROID_CLIENT_ID = 6
private const val BOOSTEROID_ENTRYPOINT_COOKIE =
    "boosteroid_entrypoint_source=1;boosteroid_entrypoint_page=1"
private const val BOOSTEROID_QR_SYNC_PATH = "/api/v1/auth/login/qr-code/sync"
private const val BOOSTEROID_QR_VALIDATE_PATH = "/api/v1/auth/login/qr-code/validate"
private const val BOOSTEROID_REFRESH_PATH = "/api/v1/auth/refresh-token"
private const val BOOSTEROID_LOGOUT_PATH = "/api/v2/auth/logout"
private const val BOOSTEROID_USER_PATH = "/api/v1/user"
private const val BOOSTEROID_QR_POLL_INTERVAL_MS = 3_000L
private const val BOOSTEROID_QR_TIMEOUT_MS = 5L * 60L * 1_000L
internal const val BOOSTEROID_DEFAULT_PAGE_SIZE = 100
private const val BOOSTEROID_QUEUE_POLL_INTERVAL_MS = 2_000L
private const val BOOSTEROID_QUEUE_INITIAL_DELAY_MS = 5_000L
private const val BOOSTEROID_QUEUE_TIMEOUT_MS = 180_000L
private const val BOOSTEROID_DETAILS_TIMEOUT_MS = 60_000L
private val JSON_MEDIA_TYPE = "application/json; charset=utf-8".toMediaType()
private val UUID_PATTERN = Regex(
    "[0-9a-f]{8}-[0-9a-f]{4}-[1-5][0-9a-f]{3}-[89ab][0-9a-f]{3}-[0-9a-f]{12}",
    RegexOption.IGNORE_CASE,
)

internal fun boosteroidHttpClient(): OkHttpClient =
    OkHttpClient.Builder()
        .connectTimeout(15, TimeUnit.SECONDS)
        .readTimeout(30, TimeUnit.SECONDS)
        .writeTimeout(30, TimeUnit.SECONDS)
        .pingInterval(15, TimeUnit.SECONDS)
        .followRedirects(false)
        .build()

private fun nowMs(): Long = System.currentTimeMillis()

private fun encoded(value: String): String =
    URLEncoder.encode(value, Charsets.UTF_8.name())

internal fun JsonElement?.asObject(): JsonObject? = this as? JsonObject
private fun JsonElement?.asArray(): JsonArray? = this as? JsonArray
internal fun JsonElement?.asString(): String? =
    (this as? JsonPrimitive)?.contentOrNull?.trim()?.takeIf(String::isNotBlank)

private fun JsonElement?.asInt(): Int? {
    val primitive = this as? JsonPrimitive ?: return null
    return primitive.intOrNull ?: primitive.contentOrNull?.toIntOrNull()
}

private fun JsonElement?.asLong(): Long? {
    val primitive = this as? JsonPrimitive ?: return null
    return primitive.longOrNull ?: primitive.contentOrNull?.toLongOrNull()
}

private fun JsonElement?.asDouble(): Double? {
    val primitive = this as? JsonPrimitive ?: return null
    return (primitive.doubleOrNull ?: primitive.contentOrNull?.toDoubleOrNull())?.takeIf(Double::isFinite)
}

private fun JsonElement?.asBoolean(): Boolean? {
    val primitive = this as? JsonPrimitive ?: return null
    return primitive.booleanOrNull ?: when (primitive.contentOrNull?.trim()?.lowercase(Locale.US)) {
        "1", "true", "yes", "active", "enabled" -> true
        "0", "false", "no", "inactive", "disabled" -> false
        else -> null
    }
}

internal fun JsonObject.string(vararg keys: String): String? =
    keys.firstNotNullOfOrNull { key -> this[key].asString() }

internal fun JsonObject.int(vararg keys: String): Int? =
    keys.firstNotNullOfOrNull { key -> this[key].asInt() }

private fun JsonObject.long(vararg keys: String): Long? =
    keys.firstNotNullOfOrNull { key -> this[key].asLong() }

internal fun JsonObject.double(vararg keys: String): Double? =
    keys.firstNotNullOfOrNull { key -> this[key].asDouble() }

internal fun JsonObject.boolean(vararg keys: String): Boolean? =
    keys.firstNotNullOfOrNull { key -> this[key].asBoolean() }

internal fun JsonObject.obj(vararg keys: String): JsonObject? =
    keys.firstNotNullOfOrNull { key -> this[key].asObject() }

private fun JsonObject.arr(vararg keys: String): JsonArray? =
    keys.firstNotNullOfOrNull { key -> this[key].asArray() }

private fun unwrapObject(element: JsonElement?): JsonObject {
    var current = element.asObject() ?: return JsonObject(emptyMap())
    repeat(4) {
        val nested = current["data"].asObject() ?: return current
        current = nested
    }
    return current
}

private fun unwrapArray(element: JsonElement?, extraKeys: List<String> = emptyList()): List<JsonElement> {
    if (element is JsonArray) return element
    val root = element.asObject() ?: return emptyList()
    val keys = listOf("data", "applications", "items", "results", "games") + extraKeys
    for (key in keys) {
        val value = root[key]
        if (value is JsonArray) return value
        if (value is JsonObject) {
            val nested = unwrapArray(value, extraKeys)
            if (nested.isNotEmpty()) return nested
        }
    }
    return emptyList()
}

private fun findNestedString(
    element: JsonElement?,
    keys: Set<String>,
    depth: Int = 0,
): String? {
    if (depth > 6) return null
    val obj = element.asObject() ?: return null
    for ((key, value) in obj) {
        if (key in keys) {
            value.asString()?.let { return it }
        }
    }
    for (value in obj.values) {
        findNestedString(value, keys, depth + 1)?.let { return it }
        if (value is JsonArray) {
            value.forEach { child ->
                findNestedString(child, keys, depth + 1)?.let { return it }
            }
        }
    }
    return null
}

private fun findNestedInt(
    element: JsonElement?,
    keys: Set<String>,
    depth: Int = 0,
): Int? {
    if (depth > 6) return null
    val obj = element.asObject() ?: return null
    for ((key, value) in obj) {
        if (key in keys) {
            value.asInt()?.let { return it }
        }
    }
    for (value in obj.values) {
        findNestedInt(value, keys, depth + 1)?.let { return it }
        if (value is JsonArray) {
            value.forEach { child ->
                findNestedInt(child, keys, depth + 1)?.let { return it }
            }
        }
    }
    return null
}

private fun walkJson(
    element: JsonElement?,
    key: String = "",
    visit: (String, JsonElement) -> Unit,
) {
    when (element) {
        is JsonObject -> element.forEach { (childKey, child) ->
            visit(childKey, child)
            walkJson(child, childKey, visit)
        }
        is JsonArray -> element.forEach { child -> walkJson(child, key, visit) }
        null -> Unit
        else -> visit(key, element)
    }
}

private fun androidTvHeaders(
    token: String? = null,
    authDataToken: String? = null,
): Headers {
    val model = listOf(Build.MANUFACTURER, Build.MODEL)
        .map(String::trim)
        .filter(String::isNotBlank)
        .distinct()
        .joinToString(" ")
        .ifBlank { "OpenNOW Android" }
    return Headers.Builder()
        .add(
            "User-Agent",
            "BoosteroidAndroidTVClient tv.1.2.9; Android " +
                Build.VERSION.RELEASE.orEmpty().ifBlank { "0" } + "; " + model,
        )
        .add("Device-Name", "OpenNOW " + model + " " + Build.VERSION.SDK_INT)
        .add("Device-Uniq-Id", "")
        .add("Content-Type", "application/json")
        .add("Accept", "application/json")
        .add("Nonce", "0")
        .apply {
            token?.takeIf(String::isNotBlank)?.let {
                add("Authorization", normalizeAuthorization(it))
            }
            authDataToken?.takeIf(String::isNotBlank)?.let {
                add("Authorization-Data", it)
            }
            add("Cookie", BOOSTEROID_ENTRYPOINT_COOKIE)
        }
        .build()
}

private fun rawAccessToken(value: String): String {
    val trimmed = if ('%' in value) {
        boosteroidResult { java.net.URLDecoder.decode(value, Charsets.UTF_8.name()) }.getOrDefault(value)
    } else {
        value
    }.trim()
    return trimmed.replace(Regex("^Bearer\\s+", RegexOption.IGNORE_CASE), "").trim()
}

private fun normalizeAuthorization(value: String): String = "Bearer " + rawAccessToken(value)

private fun sameAccessToken(left: String, right: String): Boolean =
    rawAccessToken(left) == rawAccessToken(right)

internal data class BoosteroidHttpCall(
    val statusCode: Int,
    val body: String,
) {
    fun json(): JsonElement =
        body.takeIf(String::isNotBlank)
            ?.let { raw -> boosteroidResult { OpenNowJson.parseToJsonElement(raw) }.getOrNull() }
            ?: JsonObject(emptyMap())
}

class BoosteroidApiException(
    val statusCode: Int,
    val errorCode: Int?,
    message: String,
) : IllegalStateException(message)

private suspend fun executeBoosteroidRequest(
    http: OkHttpClient,
    method: String,
    path: String,
    tokens: BoosteroidTokens? = null,
    body: JsonElement? = null,
): BoosteroidHttpCall {
    val url = requireNotNull(BOOSTEROID_BASE_URL.toHttpUrl().resolve(path))
    check(url.host == BOOSTEROID_BASE_URL.toHttpUrl().host && url.isHttps)
    val requestBody = body?.toString()?.toRequestBody(JSON_MEDIA_TYPE)
        ?: if (method in setOf("POST", "PATCH")) "{}".toRequestBody(JSON_MEDIA_TYPE) else null
    val request = Request.Builder()
        .url(url)
        .headers(androidTvHeaders(tokens?.accessToken, tokens?.authDataToken))
        .method(method, requestBody)
        .build()
    return http.awaitBoosteroidResponse(request) { response ->
        BoosteroidHttpCall(response.code, response.body.string())
    }
}

private fun BoosteroidHttpCall.requireSuccess(): JsonElement {
    if (statusCode in 200..299) return json()
    val payload = json()
    val message = findNestedString(
        payload,
        setOf("message", "error_message", "errorMessage", "description"),
    ) ?: "Boosteroid request failed (HTTP " + statusCode + ")"
    throw BoosteroidApiException(
        statusCode = statusCode,
        errorCode = extractErrorCode(payload, body),
        message = message,
    )
}

private fun extractErrorCode(payload: JsonElement?, rawBody: String = ""): Int? {
    var found: Int? = null
    walkJson(payload) { key, value ->
        if (found == null && key.lowercase(Locale.US).filter(Char::isLetterOrDigit) == "errorcode") {
            found = value.asInt()
        }
    }
    if (found != null) return found
    return Regex("\"?error[_-]?code\"?\\s*:?\\s*(\\d{4,})", RegexOption.IGNORE_CASE)
        .find(rawBody)
        ?.groupValues
        ?.getOrNull(1)
        ?.toIntOrNull()
}

private fun extractAuthDataToken(element: JsonElement?): String? {
    if (element is JsonPrimitive) return element.contentOrNull?.takeIf(String::isNotBlank)
    return findNestedString(
        element,
        setOf("boosteroid_auth", "boosteroidAuth", "authorization_data", "authorizationData"),
    )
}

private fun refreshedBoosteroidTokens(
    current: BoosteroidTokens,
    response: JsonElement,
): BoosteroidTokens {
    val envelope = unwrapObject(response)
    val accessToken = envelope.string("access_token", "accessToken")
        ?: error("Boosteroid refresh did not return an access token.")
    return current.copy(
        accessToken = accessToken,
        refreshToken = envelope.string("refresh_token", "refreshToken") ?: current.refreshToken,
        authDataToken = extractAuthDataToken(envelope["user_data"] ?: envelope["userData"])
            ?: current.authDataToken,
    )
}

private fun decodeJwtPayload(token: String): JsonObject? {
    val normalized = token.removePrefix("Bearer ").trim()
    val payload = normalized.split(".").getOrNull(1) ?: return null
    return boosteroidResult {
        val bytes = Base64.decode(payload, Base64.URL_SAFE or Base64.NO_WRAP or Base64.NO_PADDING)
        OpenNowJson.parseToJsonElement(String(bytes, Charsets.UTF_8)).asObject()
    }.getOrNull()
}

private fun parseBoosteroidUser(
    element: JsonElement?,
    accessToken: String,
    fallback: AuthUser? = null,
): AuthUser {
    val root = unwrapObject(element)
    val user = root.obj("user", "profile", "account")
        ?: root.obj("user_data", "userData")
        ?: root
    val jwt = decodeJwtPayload(accessToken)
    val id = user.string("id", "userId", "user_id")
        ?: jwt?.string("sub", "id")
        ?: fallback?.userId
        ?: "boosteroid-user"
    val email = user.string("email", "mail") ?: jwt?.string("email") ?: fallback?.email
    val displayName = user.string("name", "displayName", "username", "nickname")
        ?: jwt?.string("name", "preferred_username")
        ?: email
        ?: fallback?.displayName
        ?: "Account"
    val avatar = user.string("avatar", "avatarUrl", "picture", "photo") ?: fallback?.avatarUrl
    val membership = user.string("membershipTier", "plan", "subscription", "tariff")
        ?: fallback?.membershipTier
        ?: "ACTIVE"
    return AuthUser(
        userId = id,
        displayName = displayName,
        email = email,
        avatarUrl = avatar,
        membershipTier = membership,
    )
}

internal class BoosteroidHttp(private val store: BoosteroidAccountStore) {
    val client = boosteroidHttpClient()
    private val refreshMutex = Mutex()
    private val tokenAliases = ConcurrentHashMap.newKeySet<String>()

    fun currentTokens(token: String): BoosteroidTokens? = store.account?.tokens?.takeIf { current ->
        sameAccessToken(current.accessToken, token) || rawAccessToken(token) in tokenAliases
    }

    suspend fun login(onPrompt: (DeviceLoginPrompt) -> Unit): BoosteroidAccount {
        val authCode = UUID.randomUUID().toString().lowercase(Locale.US)
        val validationUrl = BOOSTEROID_BASE_URL + BOOSTEROID_QR_VALIDATE_PATH +
            "?auth-code=" + encoded(authCode)
        val expiresAt = nowMs() + BOOSTEROID_QR_TIMEOUT_MS
        onPrompt(
            DeviceLoginPrompt(
                userCode = authCode.take(8).uppercase(Locale.US),
                verificationUri = validationUrl,
                verificationUriComplete = validationUrl,
                expiresAt = expiresAt,
            ),
        )

        while (nowMs() < expiresAt) {
            val body = buildJsonObject {
                put("auth-code", authCode)
                put("clientId", BOOSTEROID_CLIENT_ID)
            }
            val call = executeBoosteroidRequest(
                http = client,
                method = "POST",
                path = BOOSTEROID_QR_SYNC_PATH,
                body = body,
            )
            if (call.statusCode == 403) call.requireSuccess()
            if (call.statusCode == 200) {
                val envelope = unwrapObject(call.json())
                val accessToken = envelope.string("access_token", "accessToken")
                val refreshToken = envelope.string("refresh_token", "refreshToken")
                if (!accessToken.isNullOrBlank() && !refreshToken.isNullOrBlank()) {
                    val tokens = BoosteroidTokens(
                        accessToken = accessToken,
                        refreshToken = refreshToken,
                        authDataToken = extractAuthDataToken(envelope["user_data"] ?: envelope["userData"]),
                    )
                    val upstreamUser = boosteroidResult { fetchUser(tokens) }.getOrNull()
                    val userPayload = upstreamUser
                        ?: envelope["user"]
                        ?: envelope["user_data"]
                    val session = BoosteroidAccount(
                        tokens = tokens,
                        user = parseBoosteroidUser(userPayload, accessToken),
                    )
                    store.save(session)
                    return session
                }
            }
            delay(BOOSTEROID_QR_POLL_INTERVAL_MS)
        }
        error("Boosteroid QR sign-in timed out before the code was verified.")
    }

    suspend fun call(
        method: String,
        path: String,
        token: String,
        body: JsonElement? = null,
    ): BoosteroidHttpCall {
        val account = store.account ?: error("Sign in to Boosteroid again.")
        val current = currentTokens(token) ?: error("The Boosteroid account changed.")
        val response = executeBoosteroidRequest(client, method, path, current, body)
        if (response.statusCode != 401 || path == BOOSTEROID_LOGOUT_PATH) return response
        val fresh = refresh(account) ?: return response
        return executeBoosteroidRequest(client, method, path, fresh, body)
    }

    private suspend fun refresh(expected: BoosteroidAccount): BoosteroidTokens? = refreshMutex.withLock {
        val current = store.account?.takeIf { it.user.userId == expected.user.userId } ?: return@withLock null
        if (current != expected) return@withLock current.tokens
        val refreshToken = current.tokens.refreshToken?.takeIf(String::isNotBlank) ?: return@withLock null
        val response = executeBoosteroidRequest(
            http = client,
            method = "POST",
            path = BOOSTEROID_REFRESH_PATH,
            body = buildJsonObject { put("refresh_token", refreshToken) },
        )
        if (response.statusCode !in 200..299) return@withLock null
        val tokens = refreshedBoosteroidTokens(current.tokens, response.json())
        tokenAliases.add(rawAccessToken(current.tokens.accessToken))
        if (store.replaceIfCurrent(current, current.copy(tokens = tokens))) tokens else null
    }

    suspend fun logout() {
        val account = refreshMutex.withLock {
            store.account?.also {
                store.replaceIfCurrent(it, null)
                tokenAliases.clear()
            }
        } ?: return
        boosteroidResult {
            executeBoosteroidRequest(client, "POST", BOOSTEROID_LOGOUT_PATH, account.tokens)
        }
    }

    private suspend fun fetchUser(tokens: BoosteroidTokens): JsonElement =
        executeBoosteroidRequest(client, "GET", BOOSTEROID_USER_PATH, tokens).requireSuccess()
}

private fun appendQuery(path: String, parameters: Map<String, String?>): String {
    val values = parameters
        .filterValues { !it.isNullOrBlank() }
        .entries
        .joinToString("&") { entry -> encoded(entry.key) + "=" + encoded(entry.value.orEmpty()) }
    return if (values.isBlank()) path else path + "?" + values
}

private fun nestedGameObject(element: JsonElement): JsonObject {
    val root = element.asObject() ?: return JsonObject(emptyMap())
    return root.obj("application", "app", "game", "item") ?: root
}

private fun absoluteBoosteroidUrl(value: String?): String? {
    val raw = value?.trim()?.takeIf(String::isNotBlank) ?: return null
    if (raw.startsWith("http://") || raw.startsWith("https://")) return raw
    return BOOSTEROID_BASE_URL.toHttpUrl().resolve(raw)?.toString()
}

private fun imageFromGame(obj: JsonObject, vararg preferredKeys: String): String? {
    obj.string(*preferredKeys)?.let { return absoluteBoosteroidUrl(it) }
    for (containerKey in listOf("images", "media", "assets")) {
        val value = obj[containerKey]
        value.asObject()?.let { nested ->
            preferredKeys.firstNotNullOfOrNull { key -> nested.string(key) }
                ?.let { return absoluteBoosteroidUrl(it) }
            nested.string("url", "src", "imageUrl")?.let { return absoluteBoosteroidUrl(it) }
        }
        value.asArray()?.forEach { item ->
            val image = item.asObject()?.string("url", "src", "imageUrl", "cover", "icon")
            if (!image.isNullOrBlank()) return absoluteBoosteroidUrl(image)
        }
    }
    return null
}

private fun labelsFrom(value: JsonElement?): List<String> =
    when (value) {
        is JsonArray -> value.mapNotNull { item ->
            item.asString()
                ?: item.asObject()?.string("name", "title", "label", "slug", "value")
        }
        is JsonPrimitive -> listOfNotNull(value.contentOrNull)
        else -> emptyList()
    }.map(String::trim).filter(String::isNotBlank).distinct()

private fun gameStore(obj: JsonObject): String {
    val direct = obj.string("store", "platform", "platformName")
    if (!direct.isNullOrBlank()) return direct
    return obj.obj("store", "platform")?.string("name", "title", "slug")
        ?: "Boosteroid"
}

private fun gameId(obj: JsonObject): Int? =
    obj.int("id", "appId", "applicationId", "gameId")

private fun boosteroidGame(element: JsonElement, installed: Boolean): GameInfo? {
    val source = nestedGameObject(element)
    val fallback = element.asObject() ?: source
    val id = gameId(source) ?: gameId(fallback) ?: return null
    val title = source.string("name", "title", "displayName")
        ?: fallback.string("name", "title", "displayName")
        ?: "Application " + id
    val store = gameStore(source)
    val genres = labelsFrom(source["genres"] ?: source["genre"])
    val features = (
        labelsFrom(source["features"]) +
            labelsFrom(source["controls"]) +
            labelsFrom(source["tags"])
        ).distinct()
    val installedFlag = installed ||
        source.boolean("installed", "isInstalled", "selected") == true ||
        fallback.boolean("installed", "isInstalled", "selected") == true
    val variant = GameVariant(
        id = id.toString(),
        store = store,
        supportedControls = labelsFrom(source["controls"]),
        librarySelected = installedFlag,
        libraryStatus = if (installedFlag) "IN_LIBRARY" else null,
        lastPlayedDate = source.string("lastPlayedAt", "lastPlayed", "updatedAt"),
    )
    return GameInfo(
        id = "$BOOSTEROID_GAME_PREFIX$id",
        uuid = "$BOOSTEROID_GAME_PREFIX$id",
        launchAppId = id.toString(),
        title = title,
        description = source.string("description", "shortDescription", "summary"),
        longDescription = source.string("longDescription", "fullDescription", "description"),
        featureLabels = features,
        genres = genres,
        imageUrl = imageFromGame(
            source,
            "cover", "coverUrl", "poster", "posterUrl", "image", "imageUrl", "icon", "logo",
        ),
        screenshotUrl = imageFromGame(
            source,
            "background", "backgroundImage", "hero", "heroImage", "cover", "coverUrl",
        ),
        tvBannerUrl = imageFromGame(
            source,
            "background", "backgroundImage", "hero", "cover", "coverUrl",
        ),
        playType = if (installedFlag) "PLAY" else "INSTALL_TO_PLAY",
        membershipTierLabel = source.string("subscription", "plan", "tariff"),
        publisherName = source.string("publisher", "publisherName", "developer"),
        contentRatings = labelsFrom(source["ratings"] ?: source["contentRatings"]),
        playabilityState = source.string("status", "state") ?: "AVAILABLE",
        availableStores = listOf(store),
        searchText = listOf(title, store, source.string("slug"), source.string("publisher"))
            .filterNotNull()
            .joinToString(" ")
            .lowercase(Locale.US),
        lastPlayed = source.string("lastPlayedAt", "lastPlayed", "updatedAt"),
        isInLibrary = installedFlag,
        selectedVariantIndex = 0,
        variants = listOf(variant),
    )
}

internal class BoosteroidCatalogRepository(private val http: BoosteroidHttp) {
    suspend fun games(token: String, search: String, page: Int, library: Boolean): List<GameInfo> {
        val path = when {
            library -> "/api/v1/boostore/applications/installed"
            search.isNotBlank() -> "/api/v1/boostore/applications/search"
            else -> "/api/v1/boostore/applications"
        }
        val filters = mapOf(
            "page" to page.toString(),
            "paginate" to BOOSTEROID_DEFAULT_PAGE_SIZE.toString(),
            "name" to search.trim().takeIf { it.isNotBlank() && !library },
        )
        val payload = http.call("GET", appendQuery(path, filters), token).requireSuccess()
        return unwrapArray(payload, listOf("applications", "apps", "games", "items", "installed"))
            .mapNotNull { boosteroidGame(it, library) }
            .distinctBy(GameInfo::id)
    }

    suspend fun setInstalled(token: String, appId: Int, installed: Boolean) {
        http.call(
            if (installed) "PATCH" else "DELETE",
            "/api/v1/boostore/applications/installed/$appId",
            token,
        ).requireSuccess()
    }

    suspend fun synchronizePlatform(token: String, platform: String) {
        http.call(
            "POST",
            "/api/v1/boostore/applications/installed/synchronize/" + encoded(platform),
            token,
        ).requireSuccess()
    }
}

internal class BoosteroidSubscriptionRepository(
    private val http: BoosteroidHttp,
) {
    suspend fun fetchSubscription(
        token: String,
    ): SubscriptionInfo {
        val response = http.call("GET",
            "/api/v1/payments/subscriptions/active",
            token,
        ).requireSuccess()
        val subscriptions = unwrapArray(response, listOf("subscriptions"))
        val active = subscriptions.firstOrNull()?.asObject()
            ?: return SubscriptionInfo(
                membershipTier = "FREE",
                state = "INACTIVE",
                isGamePlayAllowed = false,
            )
        val plan = findNestedString(
            active,
            setOf("name", "title", "plan", "tariff", "productName", "subscriptionName"),
        ) ?: "ACTIVE"
        val state = findNestedString(active, setOf("status", "state")) ?: "ACTIVE"
        val remainingMinutes = findNestedInt(
            active,
            setOf("remainingTimeInMinutes", "remainingMinutes"),
        )?.toDouble()
        val totalMinutes = findNestedInt(
            active,
            setOf("totalTimeInMinutes", "allottedTimeInMinutes", "totalMinutes"),
        )?.toDouble()
        return SubscriptionInfo(
            membershipTier = plan.uppercase(Locale.US),
            subscriptionType = active.string("type", "planType"),
            subscriptionSubType = active.string("subType", "billingPeriod"),
            allottedHours = (totalMinutes ?: 0.0) / 60.0,
            usedHours = if (totalMinutes != null && remainingMinutes != null) {
                max(0.0, totalMinutes - remainingMinutes) / 60.0
            } else {
                0.0
            },
            remainingHours = (remainingMinutes ?: 0.0) / 60.0,
            totalHours = (totalMinutes ?: 0.0) / 60.0,
            state = state,
            isGamePlayAllowed = state !in setOf("INACTIVE", "EXPIRED", "CANCELLED", "CANCELED"),
            isUnlimited = active.boolean("unlimited", "isUnlimited") == true ||
                plan.contains("unlimited", ignoreCase = true),
        )
    }
}

private data class BoosteroidSessionEntry(
    val sessionId: String,
    val appId: Int?,
    val statusText: String,
    val sessionToken: String?,
    val queries: List<String>,
    val gateways: List<String>,
    val queuePosition: Int?,
)

private data class BoosteroidSessionSignals(
    val tokens: List<String>,
    val queries: List<String>,
    val queuedSessionIds: List<String>,
    val entries: List<BoosteroidSessionEntry>,
    val gateways: List<String>,
    val queuePosition: Int?,
)

private data class PendingBoosteroidLaunch(
    val appId: Int,
    val createdAtMs: Long,
    val realtime: BoosteroidRealtimeQueueListener?,
)

private class BoosteroidRealtimeQueueListener(
    http: OkHttpClient,
    userId: String,
    accessToken: String,
    ownerJob: Job?,
) {
    private val messages = ArrayDeque<JsonElement>()
    private val messageLock = Any()
    private val ready = CompletableDeferred<Unit>()
    private val scope = CoroutineScope(SupervisorJob() + Dispatchers.IO)
    private val lifecycleLock = Any()
    @Volatile
    private var closed = false
    private var pingJob: Job? = null

    private val realtimeUrl = BOOSTEROID_BASE_URL.toHttpUrl().newBuilder()
        .encodedPath("/ws")
        .query(null)
        .addQueryParameter("uid", userId)
        .addQueryParameter("token", rawAccessToken(accessToken))
        .build()
        .toString()
        .replaceFirst("https://", "wss://")

    private val webSocket: WebSocket = http.newWebSocket(
        Request.Builder()
            .url(realtimeUrl)
            .build(),
        object : WebSocketListener() {
            override fun onOpen(webSocket: WebSocket, response: Response) {
                settleReady()
                pingJob = scope.launch {
                    while (isActive) {
                        delay(5_000L)
                        webSocket.send("{\"type\":\"ping\"}")
                    }
                }
            }

            override fun onMessage(webSocket: WebSocket, text: String) {
                record(text)
            }

            override fun onMessage(webSocket: WebSocket, bytes: ByteString) {
                record(bytes.utf8())
            }

            override fun onClosing(webSocket: WebSocket, code: Int, reason: String) {
                settleReady()
                stopBackgroundWork()
            }

            override fun onClosed(webSocket: WebSocket, code: Int, reason: String) {
                settleReady()
                stopBackgroundWork()
            }

            override fun onFailure(webSocket: WebSocket, t: Throwable, response: Response?) {
                settleReady()
                stopBackgroundWork()
            }
        },
    )
    @Suppress("unused")
    private val ownerCompletionHandle = ownerJob?.invokeOnCompletion { close() }

    suspend fun awaitReady() {
        withTimeoutOrNull(3_000L) { ready.await() }
    }

    fun payloads(): List<JsonElement> = synchronized(messageLock) { messages.toList() }

    fun close() {
        synchronized(lifecycleLock) {
            if (closed) return
            closed = true
        }
        if (!webSocket.close(1000, "launch-complete")) {
            webSocket.cancel()
        }
        stopBackgroundWork()
        settleReady()
    }

    private fun record(raw: String) {
        if (raw.length > 256_000) return
        val payload = boosteroidResult { OpenNowJson.parseToJsonElement(raw) }.getOrNull()
        if (payload !is JsonObject && payload !is JsonArray) return
        synchronized(messageLock) {
            if (messages.size == 64) messages.removeFirst()
            messages.addLast(payload)
        }
    }

    private fun settleReady() {
        if (!ready.isCompleted) ready.complete(Unit)
    }

    private fun stopBackgroundWork() {
        pingJob?.cancel()
        scope.cancel()
    }
}

internal class BoosteroidSessionRepository(
    private val authStore: BoosteroidAccountStore,
    private val http: BoosteroidHttp,
) {
    private val pendingLaunches = ConcurrentHashMap<String, PendingBoosteroidLaunch>()

    suspend fun createSession(
        token: String,
        appId: String,
    ): BoosteroidSession {
        val numericAppId = appId.toIntOrNull()?.takeIf { it > 0 }
            ?: error("Boosteroid requires a numeric appId.")
        val ownerJob = currentCoroutineContext()[Job]
        var realtime = createRealtimeQueueListener(token, ownerJob)
        var realtimeRetained = false
        try {
            realtime?.awaitReady()
            val enqueue = call(
                operation = "session.enqueue",
                method = "POST",
                path = "/api/v2/streaming/session/enqueue",
                token = token,
                body = buildJsonObject { put("appId", numericAppId) },
            ).requireSuccess()
            delay(BOOSTEROID_QUEUE_INITIAL_DELAY_MS)
            val refreshedAccessToken = http.currentTokens(token)?.accessToken
            if (
                !refreshedAccessToken.isNullOrBlank() &&
                !sameAccessToken(refreshedAccessToken, token)
            ) {
                realtime?.close()
                realtime = createRealtimeQueueListener(refreshedAccessToken, ownerJob)
                realtime?.awaitReady()
            }
            val signals = discoverSignals(
                token,
                numericAppId,
                listOf(enqueue) + realtime?.payloads().orEmpty(),
            )
            val started = startFromSignals(token, numericAppId, signals, realtime)
            if (started?.isReadyForStream() == true) return started

            val pendingId = started?.sessionId
                ?: signals.queuedSessionIds.firstOrNull()
                ?: signals.entries.firstOrNull()?.sessionId
                ?: "boosteroid-queue-" + numericAppId + "-" + UUID.randomUUID()
            pendingLaunches.put(
                pendingId,
                PendingBoosteroidLaunch(
                    appId = numericAppId,
                    createdAtMs = nowMs(),
                    realtime = realtime,
                ),
            )?.realtime?.close()
            realtimeRetained = true
            return started ?: pendingSession(pendingId, signals)
        } finally {
            if (!realtimeRetained) realtime?.close()
        }
    }

    suspend fun pollSession(
        token: String,
        sessionId: String,
    ): BoosteroidSession {
        val pending = pendingLaunches[sessionId]
        val appId = pending?.appId
            ?: sessionId.substringAfter("boosteroid-queue-", "").substringBefore("-").toIntOrNull()
            ?: activeEntry(token, sessionId)?.appId
            ?: error("Unable to recover the Boosteroid appId for session " + sessionId)
        val signals = discoverSignals(token, appId, pending?.realtime?.payloads().orEmpty())
        try {
            startFromSignals(token, appId, signals, pending?.realtime)?.let { candidate ->
                if (candidate.isReadyForStream()) {
                    finishPendingLaunch(sessionId)
                } else if (candidate.sessionId != sessionId) {
                    movePendingLaunch(sessionId, candidate.sessionId)
                }
                return candidate
            }
        } catch (error: Throwable) {
            finishPendingLaunch(sessionId)
            throw error
        }
        if (pending != null && nowMs() - pending.createdAtMs > BOOSTEROID_QUEUE_TIMEOUT_MS) {
            finishPendingLaunch(sessionId)
            error("Timed out waiting for Boosteroid to provide a startable virtual machine.")
        }
        return pendingSession(sessionId, signals)
    }

    fun closePendingLaunches() = pendingLaunches.keys.toList().forEach(::finishPendingLaunch)

    suspend fun stopSession(token: String) {
        try {
            call(
                operation = "session.dequeue",
                method = "POST",
                path = "/api/v2/streaming/session/dequeue",
                token = token,
                body = JsonObject(emptyMap()),
            ).requireSuccess()
        } finally {
            closePendingLaunches()
        }
    }

    suspend fun getActiveSessions(
        token: String,
    ): List<BoosteroidActiveSession> {
        val activePayload = call(
            operation = "session.active",
            method = "GET",
            path = "/api/v1/streaming/user/active-sessions",
            token = token,
        ).requireSuccess()
        val signals = collectSignals(listOf(activePayload))
        return signals.entries.map { entry ->
            val queries = entry.queries.ifEmpty {
                signals.queries.filter { query ->
                    sessionIdFromQuery(query)?.let { it == entry.sessionId } ?: true
                }.ifEmpty { signals.queries }
            }
            val gateways = entry.gateways.ifEmpty { signals.gateways }
            val status = statusCode(entry.statusText, queries, gateways)
            BoosteroidActiveSession(
                sessionId = entry.sessionId,
                appId = entry.appId ?: 0,
                status = status,
                queuePosition = entry.queuePosition,
                sessionToken = entry.sessionToken ?: signals.tokens.firstOrNull(),
                sessionQueries = queries,
                gateways = gateways,
            )
        }.filter { it.appId > 0 || it.sessionId.isNotBlank() }
    }

    suspend fun claimSession(
        token: String,
        active: BoosteroidActiveSession,
    ): BoosteroidSession {
        val signals = BoosteroidSessionSignals(
            tokens = listOfNotNull(active.sessionToken),
            queries = active.sessionQueries,
            queuedSessionIds = if (active.status == 1) listOf(active.sessionId) else emptyList(),
            entries = listOf(
                BoosteroidSessionEntry(
                    sessionId = active.sessionId,
                    appId = active.appId,
                    statusText = if (active.status in setOf(2, 3)) "READY" else "QUEUED",
                    sessionToken = active.sessionToken,
                    queries = active.sessionQueries,
                    gateways = active.gateways,
                    queuePosition = active.queuePosition,
                ),
            ),
            gateways = active.gateways,
            queuePosition = active.queuePosition,
        )
        startFromSignals(token, active.appId, signals)?.let { return it }
        val details = sessionDetails(token, active.sessionId)
        return sessionFromPayload(
            token = token,

            startPayload = details ?: JsonObject(emptyMap()),
            fallbackSessionId = active.sessionId,
            inheritedQueries = active.sessionQueries,
            inheritedGateways = active.gateways,

        )
    }

    private suspend fun startFromSignals(
        token: String,
        appId: Int,
        signals: BoosteroidSessionSignals,
        realtime: BoosteroidRealtimeQueueListener? = null,
    ): BoosteroidSession? {
        for (sessionToken in signals.tokens) {
            val startCall = call(
                operation = "session.start.v2",
                method = "POST",
                path = "/api/v2/streaming/session/start",
                token = token,
                body = buildJsonObject {
                    put("appId", appId)
                    put("sessionToken", sessionToken)
                },
                requireSuccess = false,
            )
            if (startCall.statusCode in 200..299) {
                return sessionFromPayload(
                    token = token,

                    startPayload = startCall.json(),
                    inheritedQueries = signals.queries,
                    inheritedGateways = signals.gateways,

                    realtime = realtime,
                )
            }
            val errorCode = extractErrorCode(startCall.json(), startCall.body)
            when {
                errorCode == 340005 -> {
                    call(
                        operation = "session.dequeue.retry",
                        method = "POST",
                        path = "/api/v2/streaming/session/dequeue",
                        token = token,
                        body = JsonObject(emptyMap()),
                        requireSuccess = false,
                    )
                    val enqueue = call(
                        operation = "session.enqueue.retry",
                        method = "POST",
                        path = "/api/v2/streaming/session/enqueue",
                        token = token,
                        body = buildJsonObject { put("appId", appId) },
                    ).requireSuccess()
                    val retried = discoverSignals(
                        token,
                        appId,
                        listOf(enqueue) + realtime?.payloads().orEmpty(),
                    )
                    for (nextToken in retried.tokens) {
                        val retryStart = call(
                            operation = "session.start.v2.retry",
                            method = "POST",
                            path = "/api/v2/streaming/session/start",
                            token = token,
                            body = buildJsonObject {
                                put("appId", appId)
                                put("sessionToken", nextToken)
                            },
                            requireSuccess = false,
                        )
                        if (retryStart.statusCode in 200..299) {
                            return sessionFromPayload(
                                token = token,

                                startPayload = retryStart.json(),
                                inheritedQueries = retried.queries,
                                inheritedGateways = retried.gateways,

                                realtime = realtime,
                            )
                        }
                        when (extractErrorCode(retryStart.json(), retryStart.body)) {
                            340006 -> return startV1(token, appId, retried, realtime)
                            340005, 340007 -> continue
                            else -> if (retryStart.statusCode != 400) retryStart.requireSuccess()
                        }
                    }
                    return null
                }
                errorCode == 340006 -> return startV1(token, appId, signals, realtime)
                errorCode == 340007 || startCall.statusCode == 400 -> continue
                else -> startCall.requireSuccess()
            }
        }

        val directSessionId = signals.queries
            .mapNotNull(::sessionIdFromQuery)
            .firstOrNull()
        if (directSessionId != null) {
            val details = sessionDetails(token, directSessionId)
            return sessionFromPayload(
                token = token,

                startPayload = details ?: buildJsonObject { put("sessionId", directSessionId) },
                fallbackSessionId = directSessionId,
                inheritedQueries = signals.queries,
                inheritedGateways = signals.gateways,
                realtime = realtime,
            )
        }
        return null
    }

    private suspend fun startV1(
        token: String,
        appId: Int,
        signals: BoosteroidSessionSignals,
        realtime: BoosteroidRealtimeQueueListener?,
    ): BoosteroidSession {
        val payload = call(
            operation = "session.start.v1",
            method = "POST",
            path = "/api/v1/streaming/session/start",
            token = token,
            body = buildJsonObject { put("appId", appId) },
        ).requireSuccess()
        return sessionFromPayload(
            token = token,

            startPayload = payload,
            inheritedQueries = signals.queries,
            inheritedGateways = signals.gateways,
            realtime = realtime,
        )
    }

    private suspend fun discoverSignals(
        token: String,
        appId: Int,
        seeds: List<JsonElement> = emptyList(),
    ): BoosteroidSessionSignals = coroutineScope {
        val live = async { optionalCall("session.live", "/api/v1/streaming/user/last-session/live", token) }
        val last = async { optionalCall("session.last", "/api/v1/streaming/user/last-session", token) }
        val active = async { optionalCall("session.active.poll", "/api/v1/streaming/user/active-sessions", token) }
        val payloads = seeds + listOfNotNull(live.await(), last.await(), active.await())
        collectSignals(payloads, appId)
    }

    private fun collectSignals(
        payloads: List<JsonElement>,
        appId: Int? = null,
    ): BoosteroidSessionSignals {
        val allEntries = payloads.flatMap(::extractSessionEntries)
            .filter { entry -> appId == null || entry.appId == null || entry.appId == appId }
        val queuedIds = allEntries
            .filter { isNotReadyStatus(it.statusText) }
            .map(BoosteroidSessionEntry::sessionId)
        val blocked = queuedIds.toSet()
        val queries = (
            payloads.flatMap(::extractSessionQueries) +
                allEntries.flatMap(BoosteroidSessionEntry::queries)
            ).distinct().filter { query ->
            val querySessionId = sessionIdFromQuery(query)
            querySessionId == null || querySessionId !in blocked
        }
        val tokens = (
            payloads.flatMap(::extractSessionTokens) +
                allEntries.mapNotNull(BoosteroidSessionEntry::sessionToken)
            ).distinct()
        val gateways = (
            payloads.flatMap(::extractGateways) +
                allEntries.flatMap(BoosteroidSessionEntry::gateways)
            ).distinct()
        val queuePosition = allEntries.mapNotNull(BoosteroidSessionEntry::queuePosition).minOrNull()
            ?: payloads.firstNotNullOfOrNull(::extractQueuePosition)
        return BoosteroidSessionSignals(
            tokens = tokens,
            queries = queries,
            queuedSessionIds = queuedIds.distinct(),
            entries = allEntries.distinctBy(BoosteroidSessionEntry::sessionId),
            gateways = gateways,
            queuePosition = queuePosition,
        )
    }

    private suspend fun sessionFromPayload(
        token: String,
        startPayload: JsonElement,
        fallbackSessionId: String? = null,
        inheritedQueries: List<String> = emptyList(),
        inheritedGateways: List<String> = emptyList(),
        realtime: BoosteroidRealtimeQueueListener? = null,
    ): BoosteroidSession {
        val startUrl = findNestedString(
            startPayload,
            setOf("url", "redirectUrl", "streamingUrl"),
        )
        val sessionId = findNestedString(
            startPayload,
            setOf("sessionId", "sessionID", "sid"),
        ) ?: startUrl?.let(::sessionIdFromUrl)
            ?: fallbackSessionId
            ?: error("Boosteroid did not return a streaming session id.")
        var gateways = extractGateways(startPayload)
        if (gateways.isEmpty()) {
            gateways = optionalCall(
                "session.gateways",
                "/api/v1/streaming/gateways",
                token,
            )?.let(::extractGatewayList).orEmpty()
        }
        val details = waitForSessionDetails(token, sessionId, realtime)
        val realtimePayloads = realtime?.payloads().orEmpty()
        val signalGateways = (
            inheritedGateways + realtimePayloads.flatMap(::extractGateways)
            ).distinct()
        val detailsGateways = extractGateways(details)
        gateways = when {
            gateways.isNotEmpty() -> gateways
            signalGateways.isNotEmpty() -> signalGateways
            detailsGateways.isNotEmpty() -> detailsGateways
            else -> emptyList()
        }
        val streamingUrl = createStreamingUrl(sessionId, startUrl)
        val queries = (
            extractSessionQueries(details) +
                extractSessionQueries(startPayload) +
                realtimePayloads.flatMap(::extractSessionQueries) +
                inheritedQueries +
                listOfNotNull(streamingUrl.substringAfter("?", "").takeIf(String::isNotBlank)?.let { "?" + it }) +
                listOf("?sessionId=" + sessionId)
            ).map(::normalizeQueryCandidate).filter(String::isNotBlank).distinct()
        val gatewayHost = gateways.firstNotNullOfOrNull { normalizeBoosteroidGatewayHost(it).takeIf(String::isNotBlank) }.orEmpty()
        val selectedQuery = preferredSessionQuery(queries, sessionId)
        val ready = gatewayHost.isNotBlank() &&
            selectedQuery.isNotBlank() &&
            selectedQuery != "?sessionId=" + sessionId
        return BoosteroidSession(
            sessionId = sessionId,
            status = if (ready) 2 else 1,
            queuePosition = if (ready) null else extractQueuePosition(details ?: startPayload),
            gatewayHost = gatewayHost,
            gatewayQuery = selectedQuery,
        )
    }

    private fun pendingSession(sessionId: String, signals: BoosteroidSessionSignals): BoosteroidSession =
        BoosteroidSession(sessionId = sessionId, status = 1, queuePosition = signals.queuePosition)

    private suspend fun waitForSessionDetails(
        token: String,
        sessionId: String,
        realtime: BoosteroidRealtimeQueueListener?,
    ): JsonElement? {
        val deadline = nowMs() + BOOSTEROID_DETAILS_TIMEOUT_MS
        var latest: JsonElement? = null
        while (nowMs() < deadline) {
            latest = sessionDetails(token, sessionId)
            val realtimeHasQuery = realtime?.payloads().orEmpty()
                .flatMap(::extractSessionQueries)
                .any(::isGatewayQuery)
            if (extractSessionQueries(latest).any(::isGatewayQuery) || realtimeHasQuery) {
                return latest
            }
            delay(BOOSTEROID_QUEUE_POLL_INTERVAL_MS)
        }
        return latest
    }

    private suspend fun sessionDetails(token: String, sessionId: String): JsonElement? =
        boosteroidResult {
            call(
                operation = "session.details",
                method = "POST",
                path = "/api/v1/streaming/session/details?sessionId=" + encoded(sessionId),
                token = token,
                body = JsonNull,
            ).requireSuccess()
        }.getOrNull()

    private suspend fun activeEntry(
        token: String,
        sessionId: String,
    ): BoosteroidSessionEntry? {
        val payload = optionalCall(
            "session.active.recover",
            "/api/v1/streaming/user/active-sessions",
            token,
        ) ?: return null
        return extractSessionEntries(payload).firstOrNull { it.sessionId == sessionId }
    }

    private fun createRealtimeQueueListener(
        token: String,
        ownerJob: Job?,
    ): BoosteroidRealtimeQueueListener? {
        val session = authStore.account ?: return null
        if (!sameAccessToken(session.tokens.accessToken, token)) return null
        val userId = session.user.userId.takeIf(String::isNotBlank) ?: return null
        val accessToken = rawAccessToken(session.tokens.accessToken).takeIf(String::isNotBlank)
            ?: return null
        return BoosteroidRealtimeQueueListener(http.client, userId, accessToken, ownerJob)
    }

    private fun finishPendingLaunch(sessionId: String) {
        pendingLaunches.remove(sessionId)?.realtime?.close()
    }

    private fun movePendingLaunch(fromSessionId: String, toSessionId: String) {
        val pending = pendingLaunches.remove(fromSessionId) ?: return
        pendingLaunches.put(toSessionId, pending)?.realtime?.close()
    }

    private suspend fun optionalCall(
        operation: String,
        path: String,
        token: String,
    ): JsonElement? =
        boosteroidResult {
            call(operation, "GET", path, token).requireSuccess()
        }.getOrNull()

    private suspend fun call(
        operation: String,
        method: String,
        path: String,
        token: String,
        body: JsonElement? = null,
        requireSuccess: Boolean = true,
    ): BoosteroidHttpCall {
        val call = http.call(method, path, token, body)
        if (call.statusCode !in 200..299 && BuildConfig.DEBUG) {
            android.util.Log.d("OpenNOW Boosteroid", "$operation HTTP ${call.statusCode}")
        }
        if (requireSuccess && call.statusCode !in 200..299) call.requireSuccess()
        return call
    }

    private fun statusCode(
        statusText: String,
        queries: List<String>,
        gateways: List<String>,
    ): Int {
        if (isNotReadyStatus(statusText)) return 1
        return if (queries.any(::isGatewayQuery) && gateways.isNotEmpty()) 2 else 1
    }
}

private fun extractSessionTokens(payload: JsonElement?): List<String> {
    val tokens = mutableListOf<String>()
    walkJson(payload) { key, value ->
        if (
            key.equals("token", ignoreCase = true) ||
            key.equals("sessionToken", ignoreCase = true) ||
            key.equals("session_token", ignoreCase = true)
        ) {
            value.asString()?.takeIf { it.length >= 8 }?.let(tokens::add)
        }
    }
    return tokens.distinct()
}

private fun normalizeQueryCandidate(value: String): String {
    val trimmed = value.trim()
    if (trimmed.isBlank()) return ""
    if (trimmed.startsWith("?")) return trimmed
    if (trimmed.contains("?")) return "?" + trimmed.substringAfter("?")
    if (trimmed.contains("=")) return "?" + trimmed
    if (Regex("^[A-Za-z0-9_-]+\\.[A-Za-z0-9_-]+\\.[A-Za-z0-9_-]+$").matches(trimmed)) return trimmed
    return ""
}

private fun sessionIdFromQuery(query: String): String? {
    val normalized = normalizeQueryCandidate(query).removePrefix("?")
    if (normalized.isBlank()) return null
    return normalized.split("&")
        .mapNotNull { part ->
            val key = part.substringBefore("=", "")
            val value = part.substringAfter("=", "")
            if (key.equals("sessionId", true) || key.equals("sessionid", true) || key.equals("session", true)) {
                value.takeIf(String::isNotBlank)
            } else {
                null
            }
        }
        .firstOrNull()
}

private fun sessionIdFromUrl(value: String): String? =
    boosteroidResult {
        val url = value.toHttpUrl()
        url.queryParameterNames
            .firstOrNull { name ->
                name.equals("sessionId", true) ||
                    name.equals("sessionid", true) ||
                    name.equals("session", true)
            }
            ?.let(url::queryParameter)
    }.getOrNull()

private fun extractSessionQueries(payload: JsonElement?): List<String> {
    val queries = mutableListOf<String>()
    walkJson(payload) { _, value ->
        val text = value.asString() ?: return@walkJson
        val query = normalizeQueryCandidate(text)
        if (query.isNotBlank()) {
            queries += query
        }
    }
    return queries.distinct()
}

private fun gatewayValuesFromContainer(container: JsonElement?): List<String> {
    val gateways = mutableListOf<String>()
    fun collect(value: JsonElement?) {
        when (value) {
            is JsonPrimitive -> value.asString()?.let(gateways::add)
            is JsonArray -> value.forEach(::collect)
            is JsonObject -> {
                val directValues = listOf("address", "gw", "gateway", "url", "host")
                    .mapNotNull { key -> value[key] }
                if (directValues.isNotEmpty()) {
                    directValues.forEach(::collect)
                } else {
                    value.values
                        .filter { it is JsonObject || it is JsonArray }
                        .forEach(::collect)
                }
            }
            null -> Unit
        }
    }
    collect(container)
    return gateways
        .map(String::trim)
        .filter(String::isNotBlank)
        .distinct()
}

private fun extractGateways(payload: JsonElement?): List<String> {
    val gateways = mutableListOf<String>()
    walkJson(payload) { key, value ->
        if (key.lowercase(Locale.US) in setOf("gateways", "gateway", "gw")) {
            gateways += gatewayValuesFromContainer(value)
        }
    }
    return gateways
        .map(String::trim)
        .filter(String::isNotBlank)
        .distinct()
}

private fun extractGatewayList(payload: JsonElement?): List<String> {
    val root = payload.asObject()?.get("data") ?: payload
    return (gatewayValuesFromContainer(root) + extractGateways(payload)).distinct()
}

private fun extractQueuePosition(payload: JsonElement?): Int? =
    findNestedInt(
        payload,
        setOf("queuePosition", "queue_position", "position", "place"),
    )?.takeIf { it > 0 }

private fun extractSessionEntries(payload: JsonElement?): List<BoosteroidSessionEntry> {
    val entries = mutableListOf<BoosteroidSessionEntry>()
    fun visit(element: JsonElement?) {
        when (element) {
            is JsonArray -> element.forEach(::visit)
            is JsonObject -> {
                val sessionId = element.string("sessionId", "sessionID", "session_id", "sid")
                    ?.takeIf { it.length >= 8 || UUID_PATTERN.containsMatchIn(it) }
                if (sessionId != null) {
                    entries += BoosteroidSessionEntry(
                        sessionId = sessionId,
                        appId = element.int("appId", "applicationId", "gameId"),
                        statusText = element.string(
                            "status", "sessionStatus", "state", "stage",
                        ).orEmpty().uppercase(Locale.US),
                        sessionToken = element.string("sessionToken", "session_token"),
                        queries = extractSessionQueries(element),
                        gateways = extractGateways(element),
                        queuePosition = extractQueuePosition(element),
                    )
                }
                element.values.forEach(::visit)
            }
            else -> Unit
        }
    }
    visit(payload)
    return entries.distinctBy(BoosteroidSessionEntry::sessionId)
}

private fun isNotReadyStatus(value: String): Boolean =
    value.trim().uppercase(Locale.US) in setOf(
        "EN",
        "QUEUE",
        "QUEUED",
        "WAIT",
        "WAITING",
        "PENDING",
        "INIT",
        "INITIALIZING",
        "STARTING",
        "CREATING",
        "NEW",
        "END",
        "ENDED",
        "FINISHED",
        "TERMINATED",
        "TIMEOUT",
        "EXPIRED",
        "FAILED",
        "ERROR",
        "CANCELLED",
        "CANCELED",
        "CLOSED",
    )

private fun isGatewayQuery(query: String): Boolean {
    val value = query.removePrefix("?")
    return value.contains(".") ||
        value.contains("&") ||
        Regex("token|signature|hash|nickName|language", RegexOption.IGNORE_CASE)
            .containsMatchIn(value)
}

private fun preferredSessionQuery(
    queries: List<String>,
    sessionId: String,
): String =
    queries.firstOrNull { query ->
        sessionIdFromQuery(query) == sessionId && isGatewayQuery(query)
    } ?: queries.firstOrNull(::isGatewayQuery)
        ?: queries.firstOrNull { sessionIdFromQuery(it) == sessionId }
        ?: queries.firstOrNull()
        ?: ""

internal fun normalizeBoosteroidGatewayHost(value: String): String {
    val trimmed = value.trim()
    val scheme = trimmed.substringBefore("://", "").lowercase(Locale.ROOT)
    if (scheme.isNotEmpty() && scheme !in setOf("wss", "https", "ws", "http")) return ""
    val authority = trimmed.substringAfter("://").substringBefore('/').substringBefore('?')
    if (
        authority.isBlank() || '@' in authority || '#' in authority ||
        authority.any { it.isWhitespace() || it == '\\' }
    ) return ""
    val parsed = "https://$authority".toHttpUrlOrNull() ?: return ""
    val host = if (':' in parsed.host) "[${parsed.host}]" else parsed.host
    return if (parsed.port == 443) host else "$host:${parsed.port}"
}

internal fun buildBoosteroidStreamRuntimeQuery(settings: StreamSettings): String {
    val (width, height) = streamResolutionPixels(settings)
    val runtime = linkedMapOf(
        "x" to width.toString(),
        "y" to height.toString(),
        "lang" to "en",
        "refreshRate" to settings.fps.toString(),
        "rtcEngine" to "webrtc",
        "clientType" to "web",
        "devType" to "desktop",
        "os" to "a",
        "rtcAudio" to "pcm",
    )
    if (width > 1920 || height > 1080 || settings.fps > 60) {
        runtime["tv"] = "1"
    }
    if (settings.codec == VideoCodec.AV1) runtime["codec"] = "av1"
    return runtime.entries.joinToString("&") { entry ->
        encoded(entry.key) + "=" + encoded(entry.value)
    }
}

internal fun mergeBoosteroidStreamRuntimeQuery(
    sessionQuery: String,
    settings: StreamSettings,
): String {
    val upstream = sessionQuery.removePrefix("?")
        .split('&')
        .filter(String::isNotBlank)
        .filterNot { parameter ->
            parameter.substringBefore('=').lowercase(Locale.ROOT) in BOOSTEROID_RUNTIME_QUERY_KEYS
        }
    val runtime = buildBoosteroidStreamRuntimeQuery(settings)
        .split('&')
        .filter(String::isNotBlank)
    return (upstream + runtime).joinToString("&")
}

private val BOOSTEROID_RUNTIME_QUERY_KEYS = setOf(
    "x",
    "y",
    "lang",
    "refreshrate",
    "rtcengine",
    "clienttype",
    "devtype",
    "os",
    "rtcaudio",
    "tv",
    "codec",
)

private fun createStreamingUrl(sessionId: String, upstreamUrl: String?): String {
    val candidate = upstreamUrl?.takeIf(String::isNotBlank)
    if (candidate != null && sessionIdFromUrl(candidate) != null) return candidate
    return BOOSTEROID_BASE_URL + "/static/streaming/streaming.html?sessionId=" + encoded(sessionId)
}

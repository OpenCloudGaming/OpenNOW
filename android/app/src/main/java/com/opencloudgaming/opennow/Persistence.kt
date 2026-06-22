package com.opencloudgaming.opennow

import android.content.Context
import kotlinx.coroutines.flow.MutableStateFlow
import kotlinx.coroutines.flow.StateFlow
import kotlinx.serialization.builtins.ListSerializer
import kotlinx.serialization.json.decodeFromJsonElement
import kotlinx.serialization.json.encodeToJsonElement
import kotlinx.serialization.json.jsonObject
import kotlinx.serialization.json.jsonPrimitive
import kotlinx.serialization.json.longOrNull
import kotlinx.serialization.json.put
import kotlinx.serialization.encodeToString
import java.security.MessageDigest
import java.util.UUID

private const val STORE_NAME = "opennow_native"
private const val KEY_SETTINGS = "settings"
private const val KEY_AUTH = "auth"
private const val KEY_DEVICE_ID = "gfn_device_id"
private const val KEY_CATALOG_CACHE_PREFIX = "catalog_cache_"
private const val CATALOG_CACHE_TTL_MS = 12L * 60L * 60L * 1000L

class SettingsStore(context: Context) {
    private val prefs = context.applicationContext.getSharedPreferences(STORE_NAME, Context.MODE_PRIVATE)
    private val _settings = MutableStateFlow(load())
    val settings: StateFlow<AppSettings> = _settings

    private fun load(): AppSettings {
        val raw = prefs.getString(KEY_SETTINGS, null) ?: return AppSettings()
        return runCatching { OpenNowJson.decodeFromString<AppSettings>(raw) }.getOrElse { AppSettings() }
    }

    fun update(transform: (AppSettings) -> AppSettings) {
        val next = transform(_settings.value).normalizedForAndroid()
        prefs.edit().putString(KEY_SETTINGS, OpenNowJson.encodeToString(next)).apply()
        _settings.value = next
    }

    fun replace(next: AppSettings) {
        val normalized = next.normalizedForAndroid()
        prefs.edit().putString(KEY_SETTINGS, OpenNowJson.encodeToString(normalized)).apply()
        _settings.value = normalized
    }

    fun reset() {
        replace(AppSettings())
    }

    private fun AppSettings.normalizedForAndroid(): AppSettings {
        val lowPowerSafe = stream.copy(
            codec = stream.codec,
            colorQuality = if (stream.hdrEnabled && !stream.colorQuality.name.startsWith("TenBit")) {
                ColorQuality.TenBit420
            } else {
                stream.colorQuality
            },
            sessionProxyUrl = stream.sessionProxyUrl.trim(),
            maxBitrateMbps = stream.maxBitrateMbps.coerceIn(1, 150),
            fps = stream.fps.coerceIn(30, 240),
            streamSharpeningAmount = stream.streamSharpeningAmount.coerceIn(0f, 1f),
        )
        return copy(
            stream = lowPowerSafe,
            posterSizeScale = posterSizeScale.coerceIn(0.82f, 1.08f),
            androidTouch = androidTouch.copy(
                opacity = androidTouch.opacity.coerceIn(0.15f, 1f),
                scale = androidTouch.scale.coerceIn(0.6f, 1.4f),
                buttonScale = androidTouch.buttonScale.coerceIn(0.65f, 1.5f),
                stickScale = androidTouch.stickScale.coerceIn(0.65f, 1.5f),
                edgePaddingDp = androidTouch.edgePaddingDp.coerceIn(0f, 72f),
                bottomPaddingDp = androidTouch.bottomPaddingDp.coerceIn(0f, 120f),
                leftOffsetXDp = androidTouch.leftOffsetXDp.coerceIn(-220f, 220f),
                leftOffsetYDp = androidTouch.leftOffsetYDp.coerceIn(-160f, 160f),
                rightOffsetXDp = androidTouch.rightOffsetXDp.coerceIn(-220f, 220f),
                rightOffsetYDp = androidTouch.rightOffsetYDp.coerceIn(-160f, 160f),
            ),
            autoFullScreen = true,
        )
    }
}

class AuthStore(context: Context) {
    private val prefs = context.applicationContext.getSharedPreferences(STORE_NAME, Context.MODE_PRIVATE)
    private val _state = MutableStateFlow(load())
    val state: StateFlow<PersistedAuthState> = _state

    private fun load(): PersistedAuthState {
        val raw = prefs.getString(KEY_AUTH, null) ?: return PersistedAuthState()
        return runCatching { OpenNowJson.decodeFromString<PersistedAuthState>(raw) }.getOrElse { PersistedAuthState() }
    }

    fun save(next: PersistedAuthState) {
        prefs.edit().putString(KEY_AUTH, OpenNowJson.encodeToString(next)).apply()
        _state.value = next
    }

    fun activeSession(): AuthSession? {
        val state = _state.value
        return state.sessions.firstOrNull { it.user.userId == state.activeUserId } ?: state.sessions.firstOrNull()
    }

    fun setActiveSession(userId: String) {
        val current = _state.value
        val session = current.sessions.firstOrNull { it.user.userId == userId } ?: return
        save(current.copy(activeUserId = session.user.userId, selectedProvider = session.provider))
    }

    fun upsertSession(session: AuthSession) {
        val current = _state.value
        val sessions = buildList {
            add(session)
            addAll(current.sessions.filterNot { it.user.userId == session.user.userId })
        }
        save(
            current.copy(
                sessions = sessions,
                activeUserId = session.user.userId,
                selectedProvider = session.provider,
            ),
        )
    }

    fun removeSession(userId: String) {
        val current = _state.value
        val sessions = current.sessions.filterNot { it.user.userId == userId }
        save(current.copy(sessions = sessions, activeUserId = sessions.firstOrNull()?.user?.userId))
    }

    fun clear() {
        save(PersistedAuthState())
    }

    fun stableDeviceId(): String {
        val existing = prefs.getString(KEY_DEVICE_ID, null)
        if (!existing.isNullOrBlank()) return existing
        val next = UUID.randomUUID().toString()
        prefs.edit().putString(KEY_DEVICE_ID, next).apply()
        return next
    }
}

class CatalogCacheStore(context: Context) {
    private val prefs = context.applicationContext.getSharedPreferences(STORE_NAME, Context.MODE_PRIVATE)

    fun loadMainGames(userId: String, providerStreamingBaseUrl: String): List<GameInfo>? =
        loadGameList(key("main", userId, providerStreamingBaseUrl))

    fun saveMainGames(userId: String, providerStreamingBaseUrl: String, games: List<GameInfo>) {
        saveGameList(key("main", userId, providerStreamingBaseUrl), games)
    }

    fun loadLibraryGames(userId: String, providerStreamingBaseUrl: String): List<GameInfo>? =
        loadGameList(key("library", userId, providerStreamingBaseUrl))

    fun saveLibraryGames(userId: String, providerStreamingBaseUrl: String, games: List<GameInfo>) {
        saveGameList(key("library", userId, providerStreamingBaseUrl), games)
    }

    fun loadCatalog(
        userId: String,
        providerStreamingBaseUrl: String,
        searchQuery: String,
        sortId: String,
        filterIds: List<String>,
    ): CatalogBrowseResult? =
        load(key("catalog", userId, providerStreamingBaseUrl, searchQuery, sortId, filterIds.sorted().joinToString(",")))

    fun saveCatalog(
        userId: String,
        providerStreamingBaseUrl: String,
        searchQuery: String,
        sortId: String,
        filterIds: List<String>,
        result: CatalogBrowseResult,
    ) {
        save(key("catalog", userId, providerStreamingBaseUrl, searchQuery, sortId, filterIds.sorted().joinToString(",")), result)
    }

    fun clear(): Int {
        val keys = prefs.all.keys.filter { it.startsWith(KEY_CATALOG_CACHE_PREFIX) }
        if (keys.isEmpty()) return 0
        prefs.edit().apply {
            keys.forEach(::remove)
        }.apply()
        return keys.size
    }

    private fun loadGameList(key: String): List<GameInfo>? =
        load(key, ListSerializer(GameInfo.serializer()))

    private fun saveGameList(key: String, games: List<GameInfo>) {
        save(key, games, ListSerializer(GameInfo.serializer()))
    }

    private inline fun <reified T> load(key: String): T? =
        runCatching {
            val raw = prefs.getString(storageKey(key), null) ?: return null
            val obj = OpenNowJson.parseToJsonElement(raw).jsonObject
            val expiresAt = obj["expiresAt"]?.jsonPrimitive?.longOrNull ?: return null
            if (System.currentTimeMillis() > expiresAt) return null
            val data = obj["data"] ?: return null
            OpenNowJson.decodeFromJsonElement<T>(data)
        }.getOrNull()

    private fun <T> load(key: String, serializer: kotlinx.serialization.KSerializer<T>): T? =
        runCatching {
            val raw = prefs.getString(storageKey(key), null) ?: return null
            val obj = OpenNowJson.parseToJsonElement(raw).jsonObject
            val expiresAt = obj["expiresAt"]?.jsonPrimitive?.longOrNull ?: return null
            if (System.currentTimeMillis() > expiresAt) return null
            val data = obj["data"] ?: return null
            OpenNowJson.decodeFromJsonElement(serializer, data)
        }.getOrNull()

    private inline fun <reified T> save(key: String, data: T) {
        val now = System.currentTimeMillis()
        val payload = kotlinx.serialization.json.buildJsonObject {
            put("expiresAt", kotlinx.serialization.json.JsonPrimitive(now + CATALOG_CACHE_TTL_MS))
            put("data", OpenNowJson.encodeToJsonElement(data))
        }
        prefs.edit().putString(storageKey(key), payload.toString()).apply()
    }

    private fun <T> save(key: String, data: T, serializer: kotlinx.serialization.KSerializer<T>) {
        val now = System.currentTimeMillis()
        val payload = kotlinx.serialization.json.buildJsonObject {
            put("expiresAt", kotlinx.serialization.json.JsonPrimitive(now + CATALOG_CACHE_TTL_MS))
            put("data", OpenNowJson.encodeToJsonElement(serializer, data))
        }
        prefs.edit().putString(storageKey(key), payload.toString()).apply()
    }

    private fun key(vararg parts: String): String =
        parts.joinToString("|") { it.trim() }

    private fun storageKey(key: String): String {
        val digest = MessageDigest.getInstance("SHA-256").digest(key.toByteArray())
        return KEY_CATALOG_CACHE_PREFIX + digest.joinToString("") { "%02x".format(it) }
    }
}

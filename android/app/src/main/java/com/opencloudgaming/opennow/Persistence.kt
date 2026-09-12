package com.opencloudgaming.opennow

import android.content.Context
import kotlinx.coroutines.flow.MutableStateFlow
import kotlinx.coroutines.flow.StateFlow
import kotlinx.serialization.builtins.ListSerializer
import kotlinx.serialization.encodeToString
import kotlinx.serialization.serializer
import java.security.MessageDigest
import java.util.UUID
import android.util.Xml
import org.xmlpull.v1.XmlPullParser
import java.io.File
import java.io.FileInputStream
import java.util.zip.GZIPInputStream
import java.util.zip.GZIPOutputStream
import kotlinx.coroutines.CoroutineScope
import kotlinx.coroutines.Dispatchers
import kotlinx.coroutines.flow.collectLatest
import kotlinx.coroutines.flow.drop
import kotlinx.coroutines.launch

private const val STORE_NAME = "opennow_native"
private const val CATALOG_CACHE_STORE_NAME = "opennow_catalog_cache"
private const val SECURE_STORE_NAME = "opennow_auth_secure"
private const val KEY_SETTINGS = "settings"
private const val KEY_AUTH = "auth"
private const val KEY_DEVICE_ID = "gfn_device_id"
private const val KEY_CATALOG_CACHE_PREFIX = "catalog_cache_"
private const val KEY_ANDROID_UPDATE_DISMISSED_NOTICE = "android_update_dismissed_notice"
private const val KEY_QUEUED_GAME_KEYS = "queued_game_keys"
private const val CATALOG_CACHE_TTL_MS = 12L * 60L * 60L * 1000L

/**
 * Largest compressed catalogue entry worth retaining on disk.
 *
 * This is both a storage bound and a decompression-memory guard. See `CatalogCacheStore.save`.
 */
private const val MAX_COMPRESSED_CATALOG_CACHE_BYTES = 768 * 1024
private const val CATALOG_CACHE_DIRECTORY_NAME = "catalog-cache-v2"

/**
 * Games kept in a cached list or browse result.
 *
 * A cache entry exists to put something on screen instantly at launch, not to mirror the whole
 * catalogue — the live fetch replaces it within seconds regardless. Bounding it up front means the
 * oversized case never reaches the encoder at all, rather than being detected by
 * [MAX_COMPRESSED_CATALOG_CACHE_BYTES] after compression. Android TV already
 * bounded what it restored (`TV_INITIAL_CATALOG_GAME_LIMIT`); this applies the same idea to what
 * gets written, on every profile.
 */
private const val MAX_CACHED_CATALOG_GAMES = 400
private const val QUEUED_GAME_LIMIT = 24
// Keys that must never be written to the external (potentially world-readable) file.
private val SENSITIVE_KEYS = setOf(KEY_AUTH, KEY_DEVICE_ID)
private val AUTH_STORE_LOCK = Any()

class ExternalPrefs private constructor(context: Context, val name: String) {
    private val primaryFile: File
    private val fallbackFile: File
    private val data = mutableMapOf<String, String>()
    private val lock = Any()
    // Single-thread dispatcher: apply() writes are serialized in submission order
    // (last-write-wins) without blocking the caller.
    private val writeScope = CoroutineScope(Dispatchers.IO.limitedParallelism(1))

    init {
        val extDir = runCatching { context.getExternalFilesDir(null) }.getOrNull()
        primaryFile = File(extDir ?: context.filesDir, "$name.xml")
        fallbackFile = File(context.filesDir, "$name.xml")
        synchronized(lock) {
            migrateFromInternal(context, name)
            load()
        }
    }

    companion object {
        private val instances = mutableMapOf<String, ExternalPrefs>()
        private val globalLock = Any()

        fun get(context: Context, name: String): ExternalPrefs {
            return synchronized(globalLock) {
                instances.getOrPut(name) {
                    ExternalPrefs(context.applicationContext, name)
                }
            }
        }
    }

    private fun migrateFromInternal(context: Context, name: String) {
        if (primaryFile.exists() || fallbackFile.exists()) return
        val internalPrefs = context.applicationContext.getSharedPreferences(name, Context.MODE_PRIVATE)
        val allInternal = internalPrefs.all
        if (allInternal.isNotEmpty()) {
            // Sensitive keys must not land in the external (world-readable) file.
            // Migrate them directly into the secure internal store instead,
            // skipping any key that is already present there.
            val securePrefs = context.applicationContext
                .getSharedPreferences(SECURE_STORE_NAME, Context.MODE_PRIVATE)
            val secureEdit = securePrefs.edit()
            var hasSensitive = false
            allInternal.forEach { (k, v) ->
                if (v is String && k in SENSITIVE_KEYS && !securePrefs.contains(k)) {
                    secureEdit.putString(k, v)
                    hasSensitive = true
                }
            }
            if (hasSensitive) secureEdit.commit()

            // Migrate non-sensitive keys to the external file
            allInternal.forEach { (k, v) ->
                if (v is String && k !in SENSITIVE_KEYS) data[k] = v
            }
            val primarySuccess = writeToFile(primaryFile, data.toMap())
            val fallbackSuccess = if (!primarySuccess) writeToFile(fallbackFile, data.toMap()) else true
            if (primarySuccess || fallbackSuccess) {
                internalPrefs.edit().clear().commit()
            }
        }
    }

    private fun load() {
        val hasPrimary = primaryFile.exists()
        val hasFallback = fallbackFile.exists()
        val targetFile = when {
            hasPrimary && hasFallback -> {
                if (primaryFile.lastModified() >= fallbackFile.lastModified()) {
                    primaryFile
                } else {
                    fallbackFile
                }
            }
            hasPrimary -> primaryFile
            hasFallback -> fallbackFile
            else -> return
        }
        // Snapshot existing data so we can restore it if parsing fails mid-way
        val existing = data.toMap()
        runCatching {
            val parser = Xml.newPullParser()
            FileInputStream(targetFile).use { fis ->
                parser.setInput(fis, "UTF-8")
                var event = parser.eventType
                while (event != XmlPullParser.END_DOCUMENT) {
                    if (event == XmlPullParser.START_TAG && parser.name == "string") {
                        val name = parser.getAttributeValue(null, "name")
                        val value = parser.nextText()
                        if (name != null) {
                            data[name] = value
                        }
                    }
                    event = parser.next()
                }
            }
        }.onFailure {
            it.printStackTrace()
            // Restore pre-parse snapshot to avoid leaving data in a partial state
            data.clear()
            data.putAll(existing)
            val alternativeFile = if (targetFile == primaryFile) fallbackFile else primaryFile
            if (alternativeFile.exists()) {
                val existingBeforeAlt = data.toMap()
                runCatching {
                    val parser = Xml.newPullParser()
                    FileInputStream(alternativeFile).use { fis ->
                        parser.setInput(fis, "UTF-8")
                        var event = parser.eventType
                        while (event != XmlPullParser.END_DOCUMENT) {
                            if (event == XmlPullParser.START_TAG && parser.name == "string") {
                                val name = parser.getAttributeValue(null, "name")
                                val value = parser.nextText()
                                if (name != null) {
                                    data[name] = value
                                }
                            }
                            event = parser.next()
                        }
                    }
                }.onFailure { e ->
                    e.printStackTrace()
                    // Restore again if the alternative file also failed mid-parse
                    data.clear()
                    data.putAll(existingBeforeAlt)
                }
            }
        }
    }

    private fun save(mapSnapshot: Map<String, String>) {
        synchronized(lock) {
            val success = writeToFile(primaryFile, mapSnapshot)
            if (!success) {
                writeToFile(fallbackFile, mapSnapshot)
            }
        }
    }

    private fun writeToFile(file: File, mapSnapshot: Map<String, String>): Boolean {
        return runCatching {
            val parent = file.parentFile ?: return false
            parent.mkdirs()
            val tmpFile = File(parent, "${file.name}.tmp")
            tmpFile.bufferedWriter().use { writer ->
                writer.write("<?xml version='1.0' encoding='utf-8' standalone='yes' ?>\n")
                writer.write("<map>\n")
                for ((k, v) in mapSnapshot) {
                    writer.write("    <string name=\"")
                    writer.write(escapeXmlAttribute(k))
                    writer.write("\">")
                    writer.write(escapeXmlText(v))
                    writer.write("</string>\n")
                }
                writer.write("</map>\n")
            }
            if (tmpFile.exists()) {
                if (tmpFile.renameTo(file)) {
                    true
                } else {
                    tmpFile.copyTo(file, overwrite = true)
                    tmpFile.delete()
                    true
                }
            } else {
                false
            }
        }.getOrElse {
            it.printStackTrace()
            false
        }
    }

    private fun escapeXmlAttribute(str: String): String =
        str.replace("&", "&amp;").replace("<", "&lt;").replace(">", "&gt;")
            .replace("\"", "&quot;").replace("'", "&apos;")

    private fun escapeXmlText(str: String): String =
        str.replace("&", "&amp;").replace("<", "&lt;").replace(">", "&gt;")

    fun getString(key: String, defValue: String?): String? = synchronized(lock) { data[key] ?: defValue }

    val all: Map<String, Any?> get() = synchronized(lock) { data.toMap() }

    fun edit(): Editor = Editor()

    inner class Editor {
        private val actions = mutableListOf<() -> Unit>()

        fun putString(key: String, value: String?): Editor {
            actions.add {
                if (value == null) data.remove(key) else data[key] = value
            }
            return this
        }

        fun remove(key: String): Editor {
            actions.add { data.remove(key) }
            return this
        }

        // Captures snapshot synchronously under the lock then dispatches the write
        // on a single-thread background scope, so:
        //   - The caller is never blocked by IO
        //   - Writes are still dispatched in submission order (last-write-wins guaranteed)
        fun apply() {
            val snapshot = synchronized(lock) {
                actions.forEach { it() }
                actions.clear()
                data.toMap()
            }
            writeScope.launch { save(snapshot) }
        }

        // Single synchronized block keeps action application, snapshot capture, and file
        // write atomic — eliminating the interleaving window where a concurrent commit()
        // could write a newer snapshot between our two previously-separate lock sections.
        fun commit(): Boolean = synchronized(lock) {
            actions.forEach { it() }
            actions.clear()
            val snapshot = data.toMap()
            val success = writeToFile(primaryFile, snapshot)
            if (!success) writeToFile(fallbackFile, snapshot) else true
        }
    }
}

private fun Float.finiteIn(minimum: Float, maximum: Float, fallback: Float): Float =
    if (isFinite()) coerceIn(minimum, maximum) else fallback

internal fun AppSettings.normalizedForAndroid(): AppSettings {
    val streamDefaults = StreamSettings()
    val touchDefaults = AndroidTouchSettings()
    val compatibleStream = stream.withAndroidSettingsAvailability()
    val lowPowerSafe = compatibleStream.copy(
        codec = compatibleStream.codec,
        sessionProxyUrl = stream.sessionProxyUrl.trim(),
        maxBitrateMbps = compatibleStream.maxBitrateMbps.coerceIn(1, 150),
        fps = compatibleStream.fps.coerceIn(30, 360),
        mouseSensitivity = compatibleStream.mouseSensitivity.finiteIn(0.25f, 3f, streamDefaults.mouseSensitivity),
        mouseAcceleration = compatibleStream.mouseAcceleration.coerceIn(1, 150),
        mouseScrollSensitivity = compatibleStream.mouseScrollSensitivity.coerceIn(10, 100),
        streamSharpeningAmount = compatibleStream.streamSharpeningAmount.finiteIn(
            0f,
            1f,
            streamDefaults.streamSharpeningAmount,
        ),
    )
    val normalizedCatalogSortId = catalogSortId.trim().ifBlank { DEFAULT_CATALOG_SORT_ID }
    val migratedCatalogSortId = if (
        catalogSortDefaultVersion < CATALOG_SORT_DEFAULT_VERSION &&
        normalizedCatalogSortId == "relevance"
    ) {
        DEFAULT_CATALOG_SORT_ID
    } else {
        normalizedCatalogSortId
    }
    return copy(
        uiAccent = if (uiAccent == UiAccent.LegacyOrange) UiAccent.Violet else uiAccent,
        stream = lowPowerSafe,
        posterSizeScale = posterSizeScale.finiteIn(MIN_GAME_CARD_SCALE, MAX_GAME_CARD_SCALE, 1f),
        uselessMascotDelaySeconds = normalizeMascotDelaySeconds(uselessMascotDelaySeconds),
        streamKeyboardButtonPosition = streamKeyboardButtonPosition.normalized(),
        androidTouch = androidTouch.copy(
            touchSkinTint = androidTouch.touchSkinTint.withoutRemovedWarmTint(),
            opacity = androidTouch.opacity.finiteIn(0f, 1f, touchDefaults.opacity),
            scale = androidTouch.scale.finiteIn(0.6f, 1.4f, touchDefaults.scale),
            buttonScale = androidTouch.buttonScale.finiteIn(0.65f, 1.5f, touchDefaults.buttonScale),
            stickScale = androidTouch.stickScale.finiteIn(0.65f, 1.5f, touchDefaults.stickScale),
            faceButtonScale = androidTouch.faceButtonScale.finiteIn(0.6f, 1.5f, touchDefaults.faceButtonScale),
            dpadScale = androidTouch.dpadScale.finiteIn(0.6f, 1.5f, touchDefaults.dpadScale),
            shoulderButtonScale = androidTouch.shoulderButtonScale.finiteIn(0.6f, 1.5f, touchDefaults.shoulderButtonScale),
            centerButtonScale = androidTouch.centerButtonScale.finiteIn(0.6f, 1.5f, touchDefaults.centerButtonScale),
            leftStickScale = androidTouch.leftStickScale.finiteIn(0.6f, 1.5f, touchDefaults.leftStickScale),
            rightStickScale = androidTouch.rightStickScale.finiteIn(0.6f, 1.5f, touchDefaults.rightStickScale),
            stickKnobScale = androidTouch.stickKnobScale.finiteIn(0.28f, 0.72f, touchDefaults.stickKnobScale),
            extraButtonActions = List(TOUCH_EXTRA_BUTTON_COUNT) { index ->
                androidTouch.extraButtonAction(index)
            },
            extraButtonScale = androidTouch.extraButtonScale.finiteIn(0.6f, 1.6f, touchDefaults.extraButtonScale),
            aimZoneScale = androidTouch.aimZoneScale.finiteIn(0.5f, 1.5f, touchDefaults.aimZoneScale),
            aimZoneSensitivity = androidTouch.aimZoneSensitivity.finiteIn(
                0.25f,
                3f,
                touchDefaults.aimZoneSensitivity,
            ),
            joystickDeadZone = androidTouch.joystickDeadZone.finiteIn(0f, 0.3f, touchDefaults.joystickDeadZone),
            gyroscopeSensitivity = androidTouch.gyroscopeSensitivity.finiteIn(0.25f, 3f, touchDefaults.gyroscopeSensitivity),
            gyroscopeDeadZone = androidTouch.gyroscopeDeadZone.finiteIn(0f, 0.2f, touchDefaults.gyroscopeDeadZone),
            gyroscopeSmoothing = androidTouch.gyroscopeSmoothing.finiteIn(0f, 0.9f, touchDefaults.gyroscopeSmoothing),
            edgePaddingDp = androidTouch.edgePaddingDp.finiteIn(0f, 72f, touchDefaults.edgePaddingDp),
            bottomPaddingDp = androidTouch.bottomPaddingDp.finiteIn(0f, 120f, touchDefaults.bottomPaddingDp),
            leftOffsetXDp = androidTouch.leftOffsetXDp.finiteIn(-220f, 220f, touchDefaults.leftOffsetXDp),
            leftOffsetYDp = androidTouch.leftOffsetYDp.finiteIn(-160f, 160f, touchDefaults.leftOffsetYDp),
            rightOffsetXDp = androidTouch.rightOffsetXDp.finiteIn(-220f, 220f, touchDefaults.rightOffsetXDp),
            rightOffsetYDp = androidTouch.rightOffsetYDp.finiteIn(-160f, 160f, touchDefaults.rightOffsetYDp),
            offsets = androidTouch.offsets.mapValues { (_, offset) ->
                TouchOffset(
                    x = offset.x.finiteIn(-320f, 320f, 0f),
                    y = offset.y.finiteIn(-320f, 320f, 0f),
                )
            },
        ),
        streamIntroMusic = streamIntroMusic,
        queueReadyMusic = queueReadyMusic,
        legacyCropStreamToFill = false,
        stretchStreamToFit = stretchStreamToFit,
        streamPresentationProfileVersion = streamPresentationProfileVersion.coerceAtLeast(STREAM_PRESENTATION_PROFILE_VERSION),
        showSessionReportAfterStream =
            if (sessionReportDefaultVersion < SESSION_REPORT_DEFAULT_VERSION) false
            else showSessionReportAfterStream,
        sessionReportDefaultVersion = SESSION_REPORT_DEFAULT_VERSION,
        nerdCatalogBackground = nerdCatalogBackground && !systemWallpaperBackground,
        ambientBackgroundEnabled = ambientBackgroundEnabled && !systemWallpaperBackground,
        nerdCatalogBackgroundUri = nerdCatalogBackgroundUri?.trim()?.takeIf { it.isNotBlank() },
        localAppPackageNames = normalizeLocalAppPackageNames(localAppPackageNames),
        absoluteCinemaEverywhere = absoluteCinemaEffects && absoluteCinemaEverywhere,
        catalogSortId = migratedCatalogSortId,
        catalogSortDefaultVersion = CATALOG_SORT_DEFAULT_VERSION,
        catalogFilterIds = catalogFilterIds.map(String::trim).filter(String::isNotBlank).distinct(),
        librarySortId = librarySortId.takeIf {
            it in setOf(LIBRARY_SORT_DEFAULT, LIBRARY_SORT_RECENT, LIBRARY_SORT_TITLE)
        } ?: LIBRARY_SORT_DEFAULT,
        libraryFilterIds = libraryFilterIds.map(String::trim).filter(String::isNotBlank).distinct(),
        tvSafeAreaPaddingDp = tvSafeAreaPaddingDp.finiteIn(0f, 120f, 16f),
        tvLayoutProfileVersion = tvLayoutProfileVersion.coerceAtLeast(0),
        controllerUiSounds = controllerUiSounds,
        autoFullScreen = true,
    )
}

class SettingsStore(context: Context) {
    private val prefs = ExternalPrefs.get(context, STORE_NAME)
    private val _settings = MutableStateFlow(
        load()
            .withCurrentStreamPresentationDefaults()
            .normalizedForAndroid(),
    )
    val settings: StateFlow<AppSettings> = _settings

    /**
     * Serializing [AppSettings] used to happen on whichever thread called [update] — in practice the
     * main thread, on every favourite tap, every toggle and every frame of a slider drag. The object
     * carries the full favourite list, the local-app list, the per-game variant map and the touch
     * layout offsets, so that encode is not cheap.
     *
     * The store is last-write-wins, so persistence conflates: [collectLatest] cancels an in-flight
     * encode the moment a newer value arrives, and a drag that produces fifty values writes once.
     * Reads stay synchronous off [_settings], so nothing observable is deferred — only the disk.
     */
    private val persistScope = CoroutineScope(Dispatchers.IO.limitedParallelism(1))

    init {
        persistScope.launch {
            // drop(1): the initial value came off disk; rewriting it verbatim on every launch would
            // burn a startup write for nothing.
            _settings.drop(1).collectLatest { snapshot ->
                runCatching {
                    prefs.edit().putString(KEY_SETTINGS, OpenNowJson.encodeToString(snapshot)).apply()
                }
            }
        }
    }

    private fun load(): AppSettings {
        val raw = prefs.getString(KEY_SETTINGS, null)
        // Persist the migration even if no setting is changed this launch. The regular writer
        // deliberately skips the initial StateFlow value, so it cannot save this reset for us.
        return loadSettingsWithNvstDefault(raw) { migrated ->
            prefs.edit().putString(KEY_SETTINGS, OpenNowJson.encodeToString(migrated)).apply()
        }
    }

    fun update(transform: (AppSettings) -> AppSettings) {
        _settings.value = transform(_settings.value)
            .withCurrentStreamPresentationDefaults()
            .normalizedForAndroid()
    }

    fun replace(next: AppSettings) {
        _settings.value = next
            .withCurrentStreamPresentationDefaults()
            .normalizedForAndroid()
    }

    fun reset() {
        replace(loadSettingsWithNvstDefault(null))
    }
}

class AuthStore(context: Context) {
    private val sharedPrefs = context.applicationContext.getSharedPreferences(SECURE_STORE_NAME, Context.MODE_PRIVATE)
    private val _state = MutableStateFlow(loadAndMigrate(context))
    val state: StateFlow<PersistedAuthState> = _state

    private fun loadAndMigrate(context: Context): PersistedAuthState {
        val legacyPrefs = ExternalPrefs.get(context, STORE_NAME)

        // Migrate auth credentials if not yet in secure storage
        val hasSecureAuth = sharedPrefs.contains(KEY_AUTH)
        var migratedState: PersistedAuthState? = null
        if (!hasSecureAuth) {
            val legacyRaw = legacyPrefs.getString(KEY_AUTH, null)
            if (!legacyRaw.isNullOrBlank()) {
                val parsed = runCatching { OpenNowJson.decodeFromString<PersistedAuthState>(legacyRaw) }.getOrNull()
                if (parsed != null) {
                    val secureCommitSuccess = sharedPrefs.edit().putString(KEY_AUTH, legacyRaw).commit()
                    if (secureCommitSuccess) {
                        migratedState = parsed
                        legacyPrefs.edit().remove(KEY_AUTH).commit()
                    }
                }
            }
        }

        // Migrate device ID independently — always run even if auth was already migrated,
        // since hasSecureAuth being true does not guarantee KEY_DEVICE_ID is in secure storage.
        if (!sharedPrefs.contains(KEY_DEVICE_ID)) {
            val legacyDeviceId = legacyPrefs.getString(KEY_DEVICE_ID, null)
            if (!legacyDeviceId.isNullOrBlank()) {
                val secureCommitSuccess = sharedPrefs.edit().putString(KEY_DEVICE_ID, legacyDeviceId).commit()
                if (secureCommitSuccess) {
                    legacyPrefs.edit().remove(KEY_DEVICE_ID).commit()
                }
            }
        }

        if (migratedState != null) {
            return migratedState
        }
        return load()
    }

    private fun load(): PersistedAuthState {
        val raw = sharedPrefs.getString(KEY_AUTH, null) ?: return PersistedAuthState()
        return runCatching { OpenNowJson.decodeFromString<PersistedAuthState>(raw) }.getOrElse { PersistedAuthState() }
    }

    fun reload(): PersistedAuthState = synchronized(AUTH_STORE_LOCK) {
        load().also { latest -> _state.value = latest }
    }

    fun save(next: PersistedAuthState) = synchronized(AUTH_STORE_LOCK) {
        sharedPrefs.edit().putString(KEY_AUTH, OpenNowJson.encodeToString(next)).commit()
        _state.value = next
    }

    fun activeSession(): AuthSession? = synchronized(AUTH_STORE_LOCK) {
        val state = _state.value
        state.sessions.firstOrNull { it.user.userId == state.activeUserId } ?: state.sessions.firstOrNull()
    }

    fun setActiveSession(userId: String) = synchronized(AUTH_STORE_LOCK) {
        val current = _state.value
        val session = current.sessions.firstOrNull { it.user.userId == userId } ?: return@synchronized
        save(current.copy(activeUserId = session.user.userId, selectedProvider = session.provider))
    }

    fun upsertSession(session: AuthSession) = synchronized(AUTH_STORE_LOCK) {
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

    fun updateSessionIfUnchanged(expected: AuthSession, updated: AuthSession): Boolean = synchronized(AUTH_STORE_LOCK) {
        val current = _state.value
        val existing = current.sessions.firstOrNull { it.user.userId == expected.user.userId }
        if (existing != expected) return@synchronized false
        val sessions = current.sessions.map { session ->
            if (session.user.userId == expected.user.userId) updated else session
        }
        save(current.copy(sessions = sessions))
        true
    }

    fun removeSession(userId: String) = synchronized(AUTH_STORE_LOCK) {
        val current = _state.value
        val sessions = current.sessions.filterNot { it.user.userId == userId }
        save(current.copy(sessions = sessions, activeUserId = sessions.firstOrNull()?.user?.userId))
    }

    fun clear() = synchronized(AUTH_STORE_LOCK) {
        save(PersistedAuthState())
    }

    fun stableDeviceId(): String {
        val existing = sharedPrefs.getString(KEY_DEVICE_ID, null)
        if (!existing.isNullOrBlank()) return existing
        val next = UUID.randomUUID().toString()
        sharedPrefs.edit().putString(KEY_DEVICE_ID, next).commit()
        return next
    }
}

private fun List<GameInfo>.boundedForCache(): List<GameInfo> =
    if (size <= MAX_CACHED_CATALOG_GAMES) this else take(MAX_CACHED_CATALOG_GAMES)

/**
 * The on-disk shape of a cache entry. Named rather than hand-built so reads and writes cannot
 * drift, and so decoding never has to go through an intermediate `JsonElement` tree.
 */
@kotlinx.serialization.Serializable
private data class CachedCatalogEntry<T>(val expiresAt: Long, val data: T)

private fun clearLegacyCatalogCache(context: Context) {
    // Older builds stored all catalogue entries in XML maps. Besides dropping oversized entries,
    // changing one result rewrote the entire cache file. Remove those disposable formats once.
    val legacyPrefs = ExternalPrefs.get(context, STORE_NAME)
    val legacyKeys = legacyPrefs.all.keys.filter { it.startsWith(KEY_CATALOG_CACHE_PREFIX) }
    if (legacyKeys.isNotEmpty()) {
        legacyPrefs.edit().apply {
            legacyKeys.forEach(::remove)
        }.apply()
    }
    listOfNotNull(
        context.getExternalFilesDir(null)?.let { File(it, "$CATALOG_CACHE_STORE_NAME.xml") },
        File(context.filesDir, "$CATALOG_CACHE_STORE_NAME.xml"),
    ).distinct().forEach(File::delete)
}

class CatalogCacheStore private constructor(
    private val cacheDirectory: File,
    clearLegacyCache: () -> Unit,
) {
    constructor(context: Context) : this(
        cacheDirectory = File(context.applicationContext.cacheDir, CATALOG_CACHE_DIRECTORY_NAME),
        clearLegacyCache = { clearLegacyCatalogCache(context.applicationContext) },
    )

    internal constructor(cacheDirectory: File) : this(cacheDirectory, {})

    init {
        clearLegacyCache()
    }

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
        save(
            key("catalog", userId, providerStreamingBaseUrl, searchQuery, sortId, filterIds.sorted().joinToString(",")),
            result.copy(games = result.games.boundedForCache()),
        )
    }

    @Synchronized
    fun clear(): Int {
        val files = cacheDirectory.listFiles()?.filter { it.isFile }.orEmpty()
        files.forEach(File::delete)
        return files.size
    }

    private fun loadGameList(key: String): List<GameInfo>? =
        load(key, ListSerializer(GameInfo.serializer()))

    private fun saveGameList(key: String, games: List<GameInfo>) {
        save(key, games.boundedForCache(), ListSerializer(GameInfo.serializer()))
    }

    private inline fun <reified T> load(key: String): T? =
        load(key, OpenNowJson.serializersModule.serializer())

    /**
     * Streams straight from the stored string into [T].
     *
     * The previous version parsed the whole entry into a `JsonElement` tree first and only then
     * decoded it, so reading a large catalogue held the string, the tree, and the result at once.
     */
    @Synchronized
    private fun <T> load(key: String, serializer: kotlinx.serialization.KSerializer<T>): T? {
        val file = cacheFile(key)
        recoverInterruptedReplacement(file)
        if (!file.isFile || file.length() <= 0L) return null
        return runCatching {
            val raw = GZIPInputStream(file.inputStream().buffered()).bufferedReader(Charsets.UTF_8).use { reader ->
                reader.readText()
            }
            val entry = OpenNowJson.decodeFromString(CachedCatalogEntry.serializer(serializer), raw)
            if (System.currentTimeMillis() > entry.expiresAt) {
                file.delete()
                null
            } else {
                entry.data
            }
        }.getOrElse {
            file.delete()
            null
        }
    }

    private inline fun <reified T> save(key: String, data: T) {
        save(key, data, OpenNowJson.serializersModule.serializer())
    }

    /**
     * Writes an entry to its own compressed file, or drops it when the compressed result is too big.
     *
     * Two things changed here, both about peak memory rather than disk. Encoding goes straight to a
     * string instead of building a `JsonElement` tree and then calling `toString()` on it, which
     * used to mean three copies of a multi-megabyte catalogue alive at the same time. And a result
     * over [MAX_COMPRESSED_CATALOG_CACHE_BYTES] is now simply not cached.
     *
     * The size guard is what makes the touch-controls filter safe. `catalogPageLimit` lifts mobile
     * from three pages to [MAX_CATALOG_REQUEST_PAGES] when that filter is on, because the
     * capability is evaluated locally and every page has to be inspected — so applying it produced
     * by far the largest payload the app ever writes, and writing it exhausted the heap on
     * low-memory devices. That is why the crash looked like it belonged to filtering. Skipping the
     * cache costs one refetch on the next launch; the alternative was an OutOfMemoryError.
     *
     * The stale entry is removed rather than left in place so a smaller, older result cannot go on
     * being served for a query that now returns much more.
     */
    @Synchronized
    private fun <T> save(key: String, data: T, serializer: kotlinx.serialization.KSerializer<T>) {
        cacheDirectory.mkdirs()
        val target = cacheFile(key)
        recoverInterruptedReplacement(target)
        val staged = File(cacheDirectory, "${target.name}.stage")
        staged.delete()
        val entry = CachedCatalogEntry(System.currentTimeMillis() + CATALOG_CACHE_TTL_MS, data)
        val payload = runCatching {
            OpenNowJson.encodeToString(CachedCatalogEntry.serializer(serializer), entry)
        }.getOrNull()
        if (payload == null) {
            target.delete()
            return
        }
        val wrote = runCatching {
            GZIPOutputStream(staged.outputStream().buffered()).bufferedWriter(Charsets.UTF_8).use { writer ->
                writer.write(payload)
            }
        }.isSuccess
        if (!wrote || staged.length() > MAX_COMPRESSED_CATALOG_CACHE_BYTES) {
            staged.delete()
            target.delete()
            return
        }
        replaceCacheFile(staged, target)
    }

    private fun key(vararg parts: String): String =
        parts.joinToString("|") { it.trim() }

    private fun cacheFile(key: String): File = File(cacheDirectory, "${storageKey(key)}.json.gz")

    private fun storageKey(key: String): String {
        val digest = MessageDigest.getInstance("SHA-256").digest(key.toByteArray())
        return KEY_CATALOG_CACHE_PREFIX + digest.joinToString("") { "%02x".format(it) }
    }

    private fun recoverInterruptedReplacement(target: File) {
        val backup = File(cacheDirectory, "${target.name}.backup")
        if (!target.exists() && backup.isFile) {
            backup.renameTo(target)
        } else if (target.exists()) {
            backup.delete()
        }
        File(cacheDirectory, "${target.name}.stage").takeIf { it.isFile }?.delete()
    }

    private fun replaceCacheFile(staged: File, target: File) {
        val backup = File(cacheDirectory, "${target.name}.backup")
        backup.delete()
        val hadTarget = target.isFile
        if (hadTarget && !target.renameTo(backup)) {
            staged.delete()
            return
        }
        if (!staged.renameTo(target)) {
            if (hadTarget) backup.renameTo(target)
            staged.delete()
            return
        }
        backup.delete()
    }
}

class QueuedGameStore(context: Context) {
    private val prefs = ExternalPrefs.get(context, STORE_NAME)

    fun load(): List<String> {
        val raw = prefs.getString(KEY_QUEUED_GAME_KEYS, null) ?: return emptyList()
        return runCatching { OpenNowJson.decodeFromString<List<String>>(raw) }
            .getOrElse { emptyList() }
            .map { it.trim() }
            .filter { it.isNotBlank() }
            .distinct()
            .take(QUEUED_GAME_LIMIT)
    }

    fun record(gameKey: String): List<String> {
        val normalized = gameKey.trim()
        if (normalized.isBlank()) return load()
        val next = (listOf(normalized) + load().filterNot { it == normalized })
            .take(QUEUED_GAME_LIMIT)
        prefs.edit().putString(KEY_QUEUED_GAME_KEYS, OpenNowJson.encodeToString(next)).apply()
        return next
    }
}

class AndroidUpdateNoticeStore(context: Context) {
    private val prefs = ExternalPrefs.get(context, STORE_NAME)

    fun dismissedKey(): String? =
        prefs.getString(KEY_ANDROID_UPDATE_DISMISSED_NOTICE, null)?.takeIf { it.isNotBlank() }

    fun dismiss(key: String) {
        prefs.edit().putString(KEY_ANDROID_UPDATE_DISMISSED_NOTICE, key).apply()
    }
}

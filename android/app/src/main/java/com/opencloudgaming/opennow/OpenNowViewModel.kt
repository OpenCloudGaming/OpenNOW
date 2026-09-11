package com.opencloudgaming.opennow

import android.app.Application
import android.content.Intent
import android.net.Uri
import android.os.Build
import android.os.SystemClock
import android.util.Log
import android.widget.Toast
import androidx.lifecycle.AndroidViewModel
import androidx.lifecycle.viewModelScope
import androidx.compose.runtime.Immutable
import kotlinx.coroutines.CancellationException
import kotlinx.coroutines.CompletableDeferred
import kotlinx.coroutines.Dispatchers
import kotlinx.coroutines.Deferred
import kotlinx.coroutines.Job
import kotlinx.coroutines.async
import kotlinx.coroutines.coroutineScope
import kotlinx.coroutines.delay
import kotlinx.coroutines.withTimeout
import kotlinx.coroutines.withContext
import kotlinx.coroutines.flow.distinctUntilChanged
import kotlinx.coroutines.flow.collectLatest
import kotlinx.coroutines.flow.map
import kotlinx.coroutines.flow.MutableStateFlow
import kotlinx.coroutines.flow.StateFlow
import kotlinx.coroutines.flow.asStateFlow
import kotlinx.coroutines.flow.update
import kotlinx.coroutines.launch
import kotlinx.coroutines.sync.Mutex
import kotlinx.coroutines.sync.withLock
import kotlinx.serialization.json.JsonObject
import kotlinx.serialization.json.contentOrNull
import kotlinx.serialization.json.intOrNull
import kotlinx.serialization.json.jsonObject
import kotlinx.serialization.json.jsonPrimitive
import okhttp3.HttpUrl.Companion.toHttpUrlOrNull
import okhttp3.OkHttpClient
import java.text.SimpleDateFormat
import java.text.DateFormat
import java.util.Date
import java.util.Locale

enum class AppPage {
    Home,
    Library,
    Settings,
    Stream,
}

enum class SettingsRouteTarget {
    Account,
    General,
    Stream,
    Interface,
}

internal fun canMinimizeStreamLaunch(streamStatus: String, sessionReady: Boolean): Boolean =
    streamStatus != "idle" && !sessionReady

internal fun manuallySelectedServerForReport(
    streamingBaseUrlOverride: String?,
    configuredRegion: String,
): Boolean = !streamingBaseUrlOverride.isNullOrBlank() || configuredRegion.isNotBlank()

/** Keeps the exact old-session GET visible after its full JSON payload rotates out. */
internal fun recoverySessionProbeDebugSummary(response: GfnSessionDiagnosticResponse): String? {
    if (!response.operation.startsWith("session.recovery.probe")) return null
    val payload = runCatching { OpenNowJson.parseToJsonElement(response.responseBody).jsonObject }.getOrNull()
    val requestStatus = payload?.get("requestStatus") as? JsonObject
    val session = payload?.get("session") as? JsonObject
    fun JsonObject?.intValue(key: String): Int? =
        this?.get(key)?.jsonPrimitive?.intOrNull
    fun JsonObject?.stringValue(key: String): String? =
        this?.get(key)?.jsonPrimitive?.contentOrNull
    val sessionId = response.url.toHttpUrlOrNull()
        ?.pathSegments
        ?.lastOrNull()
        ?.takeIf { it.isNotBlank() }
        ?.let(::shortDebugId)
        .orEmpty()
    return buildString {
        append("Old session GET")
        if (sessionId.isNotBlank()) append(" session=$sessionId")
        append(" source=${response.operation}")
        append(" http=${response.statusCode}")
        append(" requestStatus=${requestStatus.intValue("statusCode") ?: "unknown"}")
        append(" description=${requestStatus.stringValue("statusDescription").orEmpty().ifBlank { "unknown" }}")
        append(" unifiedError=${requestStatus.stringValue("unifiedErrorCode").orEmpty().ifBlank { "unknown" }}")
        append(" sessionStatus=${session.intValue("status") ?: "unknown"}")
        append(" sessionError=${session.intValue("errorCode") ?: "unknown"}")
    }
}

internal fun knownSessionRecoveryCandidate(
    session: SessionInfo,
    appId: Int,
    fallbackActive: ActiveSessionInfo?,
    settings: StreamSettings,
): ActiveSessionInfo? {
    if (session.serverIp.isBlank() || appId <= 0) return null
    val (width, height) = streamResolutionPixels(settings)
    return ActiveSessionInfo(
        sessionId = session.sessionId,
        appId = appId,
        gpuType = session.gpuType ?: fallbackActive?.gpuType,
        status = session.status.takeIf { it in setOf(2, 3) } ?: 2,
        queuePosition = session.queuePosition,
        seatSetupStep = session.seatSetupStep,
        streamingBaseUrl = session.streamingBaseUrl ?: fallbackActive?.streamingBaseUrl,
        serverIp = session.serverIp,
        signalingUrl = session.signalingUrl.takeIf { it.isNotBlank() } ?: fallbackActive?.signalingUrl,
        resolution = fallbackActive?.resolution ?: "${width}x$height",
        fps = fallbackActive?.fps ?: settings.fps,
        settingsSignature = fallbackActive?.settingsSignature ?: streamSettingsSessionSignature(settings),
    )
}

private const val ANDROID_UPDATE_LAUNCH_CHECK_DELAY_MS = 5_000L
internal const val ANDROID_UPDATE_PERIODIC_CHECK_INTERVAL_MS = 6L * 60L * 60L * 1000L
private const val ANDROID_UPDATE_STREAMING_RETRY_DELAY_MS = 30_000L
private const val DEBUG_EVENT_LIMIT = 140
private const val DEBUG_EVENT_MESSAGE_LIMIT = 640
private const val DEBUG_PAYLOAD_LIMIT = 12
private const val DEBUG_PAYLOAD_BODY_LIMIT = 8_000
private const val LOGIN_PHASE_GETTING_TOKENS = "Getting sign-in tokens"
private const val ACTIVE_DIAGNOSTIC_SNAPSHOT_INTERVAL_MS = 10_000L
private const val IDLE_DIAGNOSTIC_SNAPSHOT_INTERVAL_MS = 60_000L

internal class StreamSessionRecoveryTracker {
    private var sessionId: String? = null
    private var attempts: Int = 0

    fun nextAttempt(currentSessionId: String): Int {
        if (sessionId != currentSessionId) {
            sessionId = currentSessionId
            attempts = 0
        }
        attempts += 1
        return attempts
    }

    fun reset() {
        sessionId = null
        attempts = 0
    }
}

internal enum class StreamSessionRecoveryDisposition {
    ReclaimAllocatedSession,
    ReportEndedSession,
}

/**
 * Automatic recovery is deliberately pinned to the allocated session. The attempt count is kept
 * for diagnostics, but it must never become permission to stop that session and create a new one.
 */
internal fun streamSessionRecoveryDisposition(
    recoveryAttempt: Int,
    probedStatus: Int?,
): StreamSessionRecoveryDisposition {
    require(recoveryAttempt > 0)
    return if (probedStatus != null && isTerminalSessionStatus(probedStatus)) {
        StreamSessionRecoveryDisposition.ReportEndedSession
    } else {
        StreamSessionRecoveryDisposition.ReclaimAllocatedSession
    }
}

internal fun isLikelyDirectSessionServerUrl(value: String): Boolean {
    val host = value.toHttpUrlOrNull()?.host ?: return false
    fun isIpv4Parts(parts: List<String>): Boolean =
        parts.size == 4 && parts.all { part -> part.toIntOrNull() in 0..255 }

    return isIpv4Parts(host.split('.')) || isIpv4Parts(host.substringBefore('.').split('-'))
}

private data class DebugLogEvent(
    val timestampMs: Long,
    val category: String,
    val message: String,
)

private data class DebugPayloadEvent(
    val timestampMs: Long,
    val operation: String,
    val method: String,
    val url: String,
    val statusCode: Int,
    val requestBody: String,
    val body: String,
)

private data class TimedStreamRuntimeStats(
    val capturedAtMs: Long,
    val sessionId: String?,
    val stats: StreamRuntimeStats,
)

data class ActiveSessionDecision(
    val activeSession: ActiveSessionInfo,
    val requestedGameTitle: String,
)

@Immutable
data class DiagnosticShareState(
    val awaitingConsent: Boolean = false,
    val uploading: Boolean = false,
    val pasteUrl: String? = null,
    val clipboardSummary: String? = null,
    val error: String? = null,
)

@Immutable
data class BugReportSubmissionState(
    val uploading: Boolean = false,
    val submitted: Boolean = false,
    val reference: String? = null,
    val error: String? = null,
)

private data class PendingActiveSessionLaunch(
    val game: GameInfo,
    val launchAppId: String,
    val baseUrl: String,
    val settings: StreamSettings,
    val accountLinked: Boolean,
    val activeSession: ActiveSessionInfo,
    val returnPage: AppPage,
)

@Immutable
data class OpenNowUiState(
    val initializing: Boolean = false,
    val page: AppPage = AppPage.Home,
    val authSession: AuthSession? = null,
    val providers: List<LoginProvider> = listOf(defaultProvider()),
    val selectedProvider: LoginProvider = defaultProvider(),
    val savedAccounts: List<SavedAccount> = emptyList(),
    val subscriptionInfo: SubscriptionInfo? = null,
    val accountConnectors: List<AccountConnector> = emptyList(),
    val loadingAccountConnectors: Boolean = false,
    val connectorActionStore: String? = null,
    val regions: List<StreamRegion> = emptyList(),
    val games: List<GameInfo> = emptyList(),
    /** Dedicated provider-ordered feed for the portrait Store hero. */
    val newlyAddedGames: List<GameInfo> = emptyList(),
    val libraryGames: List<GameInfo> = emptyList(),
    val queuedGameKeys: List<String> = emptyList(),
    val catalogResult: CatalogBrowseResult = CatalogBrowseResult(emptyList()),
    val catalogSearch: String = "",
    val librarySearch: String = "",
    val catalogSortId: String = DEFAULT_CATALOG_SORT_ID,
    val catalogFilterIds: List<String> = emptyList(),
    val libraryFilterIds: List<String> = emptyList(),
    val librarySortId: String = LIBRARY_SORT_DEFAULT,
    val loadingGames: Boolean = false,
    /** True only while the selected Store search/sort/filter combination has no cached result. */
    val catalogQueryLoading: Boolean = false,
    val settingsRefreshing: Boolean = false,
    val settingsRouteTarget: SettingsRouteTarget? = null,
    val settings: AppSettings = AppSettings(),
    val androidTvProfile: Boolean = false,
    val codecReport: RuntimeCodecReport? = null,
    val recommendedStreamSettings: StreamSettings? = null,
    val selectedGame: GameInfo? = null,
    val activeSession: ActiveSessionInfo? = null,
    val activeSessionDecision: ActiveSessionDecision? = null,
    val streamSession: SessionInfo? = null,
    val manuallySelectedServerForReport: Boolean = false,
    val activeStreamSettings: StreamSettings? = null,
    val streamInputModeAtLaunch: StreamInputMode? = null,
    val streamGame: GameInfo? = null,
    val streamLaunchMinimized: Boolean = false,
    val streamReturnPage: AppPage? = null,
    val launchPhase: String = "",
    val queuePosition: Int? = null,
    val queueAdActiveId: String? = null,
    val streamStatus: String = "idle",
    val error: String? = null,
    val deviceLoginPrompt: DeviceLoginPrompt? = null,
    val pendingStoreChoiceGame: GameInfo? = null,
    /** Set when Play was pressed on a game whose membership tier this account cannot meet. */
    val pendingMembershipNotice: PendingMembershipNotice? = null,
    val pendingPrintedWasteGame: GameInfo? = null,
    val printedWasteQueue: Map<String, PrintedWasteZone> = emptyMap(),
    val printedWasteMapping: Map<String, PrintedWasteServerMappingEntry> = emptyMap(),
    val printedWastePings: Map<String, Long?> = emptyMap(),
    val printedWasteLoading: Boolean = false,
    val printedWasteError: String? = null,
    val androidUpdate: AndroidUpdateState = AndroidUpdateState(),
    val dismissedAndroidUpdateNoticeKey: String? = null,
    val androidPictureInPictureActive: Boolean = false,
    val diagnosticShare: DiagnosticShareState = DiagnosticShareState(),
    val bugReportSubmission: BugReportSubmissionState = BugReportSubmissionState(),
    val bugReportVersionCheck: AndroidBugReportVersionCheckState = AndroidBugReportVersionCheckState(),
    val loginToolsVisible: Boolean = false,
    val localTvConnector: LocalTvConnectorState = LocalTvConnectorState(),
    val remoteStreamMenuRequestToken: Int = 0,
    val remoteStatsToggleRequestToken: Int = 0,
    val sessionReport: SessionReport? = null,
)

internal fun OpenNowUiState.isAndroidUpdateCheckBlockedByStream(): Boolean =
    streamStatus != "idle" || streamSession != null || activeStreamSettings != null

/**
 * Whether the catalogue currently has anything to show.
 *
 * The Store, the Library and the cached "main" list all feed off the same fetch, so any one of
 * them holding games means that fetch has landed at least once.
 */
internal fun OpenNowUiState.hasLoadedCatalogGames(): Boolean =
    games.isNotEmpty() || catalogResult.games.isNotEmpty() || libraryGames.isNotEmpty()

/**
 * The catalogue was fetched exactly once per ViewModel with no retry, so a single failed attempt —
 * no network yet on a cold start, or sockets torn down while an aggressive OEM memory manager held
 * the process frozen — left the Store empty until the reader happened to pull-to-refresh. These
 * bound an automatic ladder instead.
 */
internal const val CATALOG_RETRY_MAX_ATTEMPTS = 4
internal const val CATALOG_RETRY_BASE_DELAY_MS = 2_000L
internal const val CATALOG_RETRY_MAX_DELAY_MS = 30_000L

internal fun catalogRetryDelayMs(attempt: Int): Long =
    (CATALOG_RETRY_BASE_DELAY_MS shl attempt.coerceIn(0, 16)).coerceAtMost(CATALOG_RETRY_MAX_DELAY_MS)

/**
 * [loadAttempted] keeps the foreground hook from racing the first-run load: on a cold start the
 * Activity resumes before the bootstrap has asked for anything, and firing here would run a second
 * identical fetch alongside it.
 */
/**
 * Whether the Store should still read as loading after the cache has been applied.
 *
 * The old rule asked whether a cache entry existed for this exact query, not whether anything
 * landed on screen. Priming from a library-only cache satisfied that test while leaving `games`
 * empty, which dropped the spinner and rendered "No games loaded" over a fetch that was still in
 * flight — the empty state, shown as if the request had already come back with nothing.
 */
internal const val CATALOG_SORT_DEFAULT = DEFAULT_CATALOG_SORT_ID

/**
 * A query narrower than "the whole catalogue in its default order".
 *
 * Cached results are keyed by the exact query that produced them, so a scoped query cannot borrow
 * the unscoped cache: showing default-ordered games under a user-chosen sort would be visibly
 * wrong rather than merely stale.
 */
internal fun isScopedCatalogQuery(
    searchQuery: String,
    sortId: String,
    filterIds: List<String>,
): Boolean = searchQuery.isNotBlank() || filterIds.isNotEmpty() || sortId != CATALOG_SORT_DEFAULT

/** Identifies the catalogue cache entry a query reads from. Filters sort to match the store's key. */
internal data class CatalogCacheKey(
    val userId: String,
    val baseUrl: String,
    val searchQuery: String,
    val sortId: String,
    val filterIds: List<String>,
) {
    companion object {
        fun of(
            userId: String,
            baseUrl: String,
            searchQuery: String,
            sortId: String,
            filterIds: List<String>,
        ): CatalogCacheKey = CatalogCacheKey(userId, baseUrl, searchQuery, sortId, filterIds.sorted())
    }
}

internal class CatalogCacheSnapshot(
    val key: CatalogCacheKey,
    val main: List<GameInfo>?,
    val library: List<GameInfo>?,
    val catalog: CatalogBrowseResult?,
    val newlyAdded: CatalogBrowseResult? = null,
)

internal fun isNewlyAddedCatalogQuery(
    searchQuery: String,
    sortId: String,
    filterIds: List<String>,
): Boolean = searchQuery.isBlank() &&
    filterIds.isEmpty() &&
    catalogSortKind(sortId) == CatalogSortKind.NewlyAdded

/** The games a primed snapshot can put on the Store grid, or empty when it has nothing usable. */
internal fun primedStoreGames(snapshot: CatalogCacheSnapshot): List<GameInfo> {
    snapshot.catalog?.let { return it.games }
    val scoped = isScopedCatalogQuery(snapshot.key.searchQuery, snapshot.key.sortId, snapshot.key.filterIds)
    return if (scoped) emptyList() else snapshot.main.orEmpty()
}

internal fun catalogStillLoadingAfterCache(
    hasGamesToShow: Boolean,
    keepRefreshVisible: Boolean,
): Boolean = !hasGamesToShow || keepRefreshVisible

internal fun shouldRetryCatalogLoad(
    signedIn: Boolean,
    loadAttempted: Boolean,
    hasGames: Boolean,
    loadInFlight: Boolean,
    streamActive: Boolean,
): Boolean = signedIn && loadAttempted && !hasGames && !loadInFlight && !streamActive

internal fun OpenNowUiState.isNativeStreamReady(): Boolean =
    streamStatus in setOf("connecting", "streaming") &&
        streamSession?.isReadyForStream() == true

/**
 * Native-touch capability is a local catalogue filter, so mobile must inspect every server page
 * before applying it. Other mobile browsing remains bounded. TV keeps the smallest startup request,
 * then uses the normal bounded budget for an explicit search or filter so scoped results are complete.
 */
internal fun catalogPageLimit(
    androidTvProfile: Boolean,
    filterIds: List<String>,
    searchQuery: String = "",
): Int = when {
    androidTvProfile && searchQuery.isBlank() && filterIds.isEmpty() -> 1
    androidTvProfile -> TV_SCOPED_CATALOG_REQUEST_PAGES
    CATALOG_FILTER_TOUCHSCREEN in filterIds -> MAX_CATALOG_REQUEST_PAGES
    else -> 3
}

private const val TV_SCOPED_CATALOG_REQUEST_PAGES = 3

private fun List<GameInfo>.withHydratedGameDetails(details: GameInfo): List<GameInfo> {
    val detailsKey = gameTrackingKey(details)
    return map { game ->
        if (gameTrackingKey(game) == detailsKey) mergeGameInfo(game, details) else game
    }
}

internal fun shouldHydrateGameDetails(game: GameInfo): Boolean =
    game.genres.isEmpty() && !game.uuid.isNullOrBlank()

class OpenNowViewModel(application: Application) : AndroidViewModel(application) {
    private val openNowApplication = application as OpenNowApplication
    private val http: OkHttpClient = openNowApplication.httpClient
    private val settingsStore = SettingsStore(application)
    private val sessionTimerAnchorStore = SessionTimerAnchorStore(application)
    private val authStore = openNowApplication.authStore
    private val authRepository = openNowApplication.authRepository
    private val catalogRepository = GfnCatalogRepository(http) {
        gfnLocaleForAndroidLanguageTag(currentAndroidAppLocale(getApplication()).effectiveLanguageTag)
    }
    private val catalogCacheStore = CatalogCacheStore(application)
    private val queuedGameStore = QueuedGameStore(application)
    private val subscriptionRepository = GfnSubscriptionRepository(http)
    private val accountConnectorRepository = GfnAccountConnectorRepository(http)
    private val printedWasteRepository = PrintedWasteRepository(http)
    private val sessionRepository = GfnSessionRepository(
        authStore = authStore,
        http = http,
        physicalDisplayResolutionProvider = { application.physicalStreamDisplayResolution() },
        diagnosticsSink = { response -> recordSessionDiagnosticResponse(response) },
        isAndroidTv = isAndroidTvProfile(application),
    )
    private val appUpdater = AndroidAppUpdater(application, http)
    private val androidUpdateNoticeStore = AndroidUpdateNoticeStore(application)
    private val localTvConnector = openNowApplication.localTvConnector
    private val diagnosticHistoryStore = openNowApplication.diagnosticHistoryStore
    private val queueAdReportMutex = Mutex()
    private val accountConnectorRefreshMutex = Mutex()
    private val runtimeResolutionNoticeKeys = mutableSetOf<String>()
    private val debugEventsLock = Any()
    private val debugEvents = ArrayDeque<DebugLogEvent>()
    private val debugPayloadsLock = Any()
    private val debugPayloads = ArrayDeque<DebugPayloadEvent>()
    private val authRestoreMutex = Mutex()
    @Volatile
    private var latestStreamRuntimeStats: TimedStreamRuntimeStats? = null
    private var streamReportLaunchProfile: StreamReportLaunchProfile? = null
    private var streamSessionReportAccumulator: StreamSessionReportAccumulator? = null
    private val streamRuntimeDiagnosticsSampler = StreamRuntimeDiagnosticsSampler(
        scope = viewModelScope,
        readSnapshot = { includeDevice ->
            if (includeDevice) AndroidRuntimeDiagnostics.snapshot(application)
            else AndroidRuntimeDiagnostics.networkSnapshot(application)
        },
    )
    private var sessionReportFinalizedForStop: Boolean = false
    private var deviceRecommendation: AndroidDeviceRecommendation? = null
    /**
     * Completes when the codec probe has landed.
     *
     * The probe is no longer on the path to first paint, so anything that depends on device
     * capability — only stream launch does — waits on this instead of on startup order.
     */
    private val deviceCapabilityProbe = CompletableDeferred<Unit>()
    private val settingsDiagnosticTapTracker = RapidTapTracker()
    private val loginIconTapTracker = RapidTapTracker()
    private val streamSessionRecoveryTracker = StreamSessionRecoveryTracker()

    private val initialAuthSession = authStore.activeSession()
    private val androidTvProfile = isAndroidTvProfile(application)
    private val initialSettings = settingsStore.settings.value.let { current ->
        if (androidTvProfile && current.tvLayoutProfileVersion < TV_LAYOUT_PROFILE_VERSION) {
            settingsStore.update { saved ->
                saved.copy(
                    // 36dp on every edge consumed 144 physical pixels per axis at the
                    // common TV density. Migrate only the legacy default; preserve custom values.
                    tvSafeAreaPaddingDp = if (saved.tvSafeAreaPaddingDp == 36f) 16f else saved.tvSafeAreaPaddingDp,
                    tvLayoutProfileVersion = TV_LAYOUT_PROFILE_VERSION,
                )
            }
            settingsStore.settings.value
        } else {
            current
        }
    }
    private val _state = MutableStateFlow(
        OpenNowUiState(
            page = defaultLaunchAppPage(initialSettings),
            authSession = initialAuthSession,
            providers = initialProviders(initialAuthSession),
            selectedProvider = authStore.state.value.selectedProvider ?: initialAuthSession?.provider ?: defaultProvider(),
            savedAccounts = authStore.state.value.sessions.map { session -> session.toSavedAccount() },
            loadingGames = initialAuthSession != null,
            settings = initialSettings,
            catalogSortId = initialSettings.catalogSortId,
            catalogFilterIds = initialSettings.catalogFilterIds,
            librarySortId = initialSettings.librarySortId,
            libraryFilterIds = initialSettings.libraryFilterIds,
            androidTvProfile = androidTvProfile,
            androidUpdate = appUpdater.state.value,
            dismissedAndroidUpdateNoticeKey = androidUpdateNoticeStore.dismissedKey(),
            queuedGameKeys = queuedGameStore.load(),
        ),
    )
    val state: StateFlow<OpenNowUiState> = _state.asStateFlow()

    private var gamesJob: Job? = null
    private var gameDetailsJob: Job? = null
    private var launchJob: Job? = null
    private var activeSubscriptionJob: Job? = null
    private var pendingActiveSessionLaunch: PendingActiveSessionLaunch? = null
    private var loginJob: Job? = null
    private var androidUpdateJob: Job? = null
    private var androidUpdateAutoJob: Job? = null
    private var bugReportUpdateVerificationJob: Job? = null
    private var bugReportUpdateCheckActive: Boolean = false
    private var settingsRefreshJob: Job? = null
    private var authRefreshJob: Job? = null
    private var catalogRetryJob: Job? = null
    /** Handed to [refreshAfterAuth] so startup parses the cache once, not twice. */
    @Volatile
    private var primedCatalogCache: CatalogCacheSnapshot? = null
    private var catalogRetryAttempt = 0
    /** False until the first fetch has been asked for; see [shouldRetryCatalogLoad]. */
    private var catalogLoadAttempted = false

    init {
        viewModelScope.launch {
            settingsStore.settings.collect { next ->
                _state.update { it.copy(settings = next) }
            }
        }
        if (androidTvProfile) {
            viewModelScope.launch {
                settingsStore.settings
                    .map { it.localTvRemoteEnabled }
                    .distinctUntilChanged()
                    .collect { enabled ->
                        if (enabled) localTvConnector.startHosting() else localTvConnector.stopHosting()
                    }
            }
        }
        viewModelScope.launch {
            localTvConnector.state.collect { next ->
                _state.update { it.copy(localTvConnector = next) }
            }
        }
        viewModelScope.launch {
            localTvConnector.launchRequests.collect { request ->
                if (!state.value.androidTvProfile) return@collect
                val allGames = state.value.games + state.value.libraryGames
                val game = allGames.firstOrNull { game ->
                    game.id == request.gameId ||
                        game.uuid == request.gameId ||
                        game.launchAppId == request.gameId ||
                        game.variants.any { it.id == request.gameId }
                } ?: GameInfo(
                    id = request.gameId,
                    uuid = request.gameId,
                    launchAppId = request.gameId.takeIf { it.all(Char::isDigit) },
                    title = request.title ?: "Game ${request.gameId}",
                    variants = listOf(GameVariant(id = request.gameId, store = "Unknown")),
                )
                recordDebugEvent("tv-connector", "Accepted encrypted local launch game=${game.title}")
                play(game)
            }
        }
        viewModelScope.launch {
            localTvConnector.signInRequests.collect { transferredSession ->
                if (!state.value.androidTvProfile) return@collect
                acceptLocalTvSignIn(transferredSession)
            }
        }
        viewModelScope.launch {
            localTvConnector.remoteRequests.collect { request ->
                if (!state.value.androidTvProfile) return@collect
                handleLocalTvRemoteRequest(request)
            }
        }
        viewModelScope.launch {
            appUpdater.state.collect { next ->
                _state.update { it.copy(androidUpdate = next) }
            }
        }
        viewModelScope.launch {
            state
                .map { it.isAndroidUpdateCheckBlockedByStream() to it.androidUpdate.status }
                .distinctUntilChanged()
                .collect { (blocked, updateStatus) ->
                    if (blocked && updateStatus == AndroidUpdateStatus.Checking && !bugReportUpdateCheckActive) {
                        cancelAndroidUpdateCheckForStreaming()
                    }
                }
        }
        if (appUpdater.state.value.updateChecksSupported) {
            startAndroidUpdateAutoChecks()
        }
        startDiagnosticSnapshotPersistence()
        primeCatalogFromCache()
        startDeviceCapabilityProbe()
        initialize()
    }

    private fun startDiagnosticSnapshotPersistence() {
        viewModelScope.launch {
            state
                .map { snapshot ->
                    snapshot.streamStatus != "idle" ||
                        snapshot.streamSession != null ||
                        snapshot.activeStreamSettings != null
                }
                .distinctUntilChanged()
                .collectLatest { streamActive ->
                    persistCurrentDiagnosticSnapshot()
                    val intervalMs = if (streamActive) {
                        ACTIVE_DIAGNOSTIC_SNAPSHOT_INTERVAL_MS
                    } else {
                        IDLE_DIAGNOSTIC_SNAPSHOT_INTERVAL_MS
                    }
                    while (true) {
                        delay(intervalMs)
                        persistCurrentDiagnosticSnapshot()
                    }
                }
        }
    }

    private suspend fun persistCurrentDiagnosticSnapshot() {
        withContext(Dispatchers.IO) {
            runCatching {
                val current = sanitizeDiagnosticExport(currentDebugLogText())
                diagnosticHistoryStore.saveCurrent(current)
            }
                .onFailure { error ->
                    Log.w(OPENNOW_DEBUG_LOG_TAG, "Could not persist diagnostic history", error)
                }
        }
    }

    private fun recordDebugEvent(category: String, message: String) {
        val oneLineMessage = message
            .lineSequence()
            .joinToString(" ") { it.trim() }
            .take(DEBUG_EVENT_MESSAGE_LIMIT)
        val event = DebugLogEvent(
            timestampMs = System.currentTimeMillis(),
            category = category,
            message = oneLineMessage,
        )
        synchronized(debugEventsLock) {
            debugEvents.addLast(event)
            while (debugEvents.size > DEBUG_EVENT_LIMIT) {
                debugEvents.removeFirst()
            }
        }
        Log.d(OPENNOW_DEBUG_LOG_TAG, "${event.category}: ${event.message}")
    }

    private fun debugEventSnapshot(): List<DebugLogEvent> =
        synchronized(debugEventsLock) { debugEvents.toList() }

    private fun recordSessionDiagnosticResponse(response: GfnSessionDiagnosticResponse) {
        val sanitizedBody = sanitizeDiagnosticLogPayload(response.responseBody, DEBUG_PAYLOAD_BODY_LIMIT)
        val event = DebugPayloadEvent(
            timestampMs = System.currentTimeMillis(),
            operation = response.operation,
            method = response.method,
            url = response.url,
            statusCode = response.statusCode,
            requestBody = response.requestBody
                .takeIf { it.isNotBlank() }
                ?.let { sanitizeDiagnosticLogPayload(it, DEBUG_PAYLOAD_BODY_LIMIT) }
                .orEmpty(),
            body = sanitizedBody,
        )
        synchronized(debugPayloadsLock) {
            debugPayloads.addLast(event)
            while (debugPayloads.size > DEBUG_PAYLOAD_LIMIT) {
                debugPayloads.removeFirst()
            }
        }
        recoverySessionProbeDebugSummary(response)?.let { summary ->
            recordDebugEvent("recovery", summary)
        }
        Log.d(
            OPENNOW_DEBUG_LOG_TAG,
            "gfn-json: ${response.operation} ${response.method} http=${response.statusCode} requestBytes=${response.requestBody.length} responseBytes=${response.responseBody.length} captured=${sanitizedBody.length} host=${hostForDebug(response.url)}",
        )
    }

    private fun debugPayloadSnapshot(): List<DebugPayloadEvent> =
        synchronized(debugPayloadsLock) { debugPayloads.toList() }

    private fun defaultLaunchAppPage(settings: AppSettings = settingsStore.settings.value): AppPage =
        when (settings.launchPage) {
            AppLaunchPage.Store -> AppPage.Home
            AppLaunchPage.Library -> AppPage.Library
        }

    /**
     * Paints the last known catalogue before any network work begins.
     *
     * The cache was only opened inside [refreshAfterAuth], which sits behind a codec probe, a token
     * restore and a provider fetch — two network round trips. That left a complete, warm catalogue
     * sitting on disk while the reader watched a skeleton for ten seconds, for no reason: the cache
     * key needs nothing but the session already on disk, so this runs concurrently with startup
     * rather than after it.
     */
    private fun primeCatalogFromCache() {
        val session = initialAuthSession ?: return
        viewModelScope.launch {
            val key = CatalogCacheKey.of(
                userId = session.user.userId,
                baseUrl = effectiveStreamingBaseUrl(session),
                searchQuery = state.value.catalogSearch,
                sortId = state.value.catalogSortId,
                filterIds = state.value.catalogFilterIds,
            )
            val snapshot = withContext(Dispatchers.IO) {
                runCatching {
                    CatalogCacheSnapshot(
                        key = key,
                        main = catalogCacheStore.loadMainGames(key.userId, key.baseUrl),
                        library = catalogCacheStore.loadLibraryGames(key.userId, key.baseUrl),
                        catalog = catalogCacheStore.loadCatalog(
                            userId = key.userId,
                            providerStreamingBaseUrl = key.baseUrl,
                            searchQuery = key.searchQuery,
                            sortId = key.sortId,
                            filterIds = key.filterIds,
                        ),
                        newlyAdded = catalogCacheStore.loadCatalog(
                            userId = key.userId,
                            providerStreamingBaseUrl = key.baseUrl,
                            searchQuery = "",
                            sortId = NEWLY_ADDED_CATALOG_SORT_ID,
                            filterIds = emptyList(),
                        ),
                    )
                }.getOrNull()
            } ?: return@launch
            primedCatalogCache = snapshot
            applyPrimedCatalogCache(snapshot)
        }
    }

    private fun applyPrimedCatalogCache(snapshot: CatalogCacheSnapshot) {
        val cachedGfnThursdayGames = gfnThursdayCatalogGames(snapshot.main.orEmpty())
        val gfnThursdayQuery = isNewlyAddedCatalogQuery(
            searchQuery = snapshot.key.searchQuery,
            sortId = snapshot.key.sortId,
            filterIds = snapshot.key.filterIds,
        )
        val officialCachedCatalog = if (gfnThursdayQuery && cachedGfnThursdayGames.isNotEmpty()) {
            catalogResultWithGfnThursdayGames(
                fallback = snapshot.catalog ?: CatalogBrowseResult(
                    games = emptyList(),
                    selectedSortId = NEWLY_ADDED_CATALOG_SORT_ID,
                ),
                games = cachedGfnThursdayGames,
            )
        } else {
            snapshot.catalog
        }
        val storeGames = officialCachedCatalog?.games ?: primedStoreGames(snapshot)
        val libraryGames = snapshot.library.orEmpty()
        val newlyAddedGames = cachedGfnThursdayGames
            .ifEmpty { snapshot.newlyAdded?.games.orEmpty() }
        if (storeGames.isEmpty() && libraryGames.isEmpty() && newlyAddedGames.isEmpty()) return
        var applied = false
        _state.update { current ->
            // The live fetch always wins. This only fills a screen that is still blank, so a slow
            // disk read can never overwrite results that arrived while it was parsing.
            if (current.hasLoadedCatalogGames()) return@update current
            applied = true
            current.copy(
                games = storeGames,
                newlyAddedGames = newlyAddedGames.ifEmpty { current.newlyAddedGames },
                catalogResult = officialCachedCatalog ?: current.catalogResult,
                libraryGames = libraryGames.ifEmpty { current.libraryGames },
                // A refresh is still on its way; the pull-to-refresh indicator should say so.
                loadingGames = true,
                error = null,
            )
        }
        if (applied) {
            recordDebugEvent(
                "catalog",
                "Primed catalog from cache store=${storeGames.size} library=${libraryGames.size}",
            )
        }
    }

    /**
     * Probes decoder capability, off the path to first paint.
     *
     * [CodecProbe.report] calls `WebRtcRuntime.ensureInitialized`, which loads the multi-megabyte
     * WebRTC native library and stands up a PeerConnectionFactory. Running that before clearing
     * `initializing` meant every cold start paid for the streaming engine before it could draw a
     * catalogue — work that only matters once a stream is actually launched.
     */
    private fun startDeviceCapabilityProbe() {
        viewModelScope.launch {
            val codecReport = runCatching {
                withContext(Dispatchers.Default) { CodecProbe.report(getApplication()) }
            }.getOrElse { error ->
                recordDebugEvent("codec", "Codec probe failed error=${error.debugMessage()}")
                // Leave the report null: every consumer already treats that as "not probed".
                deviceCapabilityProbe.complete(Unit)
                return@launch
            }
            val recommendation = recommendedAndroidStreamProfile(getApplication(), codecReport)
            deviceRecommendation = recommendation
            val currentSettings = settingsStore.settings.value
            val recommendedStream = recommendation.stream.withUserStreamOptionsFrom(currentSettings.stream)
            if (
                currentSettings.streamPreset == StreamPreset.Recommended &&
                currentSettings.stream != recommendedStream
            ) {
                settingsStore.update { settings ->
                    if (settings.streamPreset == StreamPreset.Recommended) {
                        settings.copy(stream = recommendedStream)
                    } else {
                        settings
                    }
                }
            }
            _state.update {
                it.copy(
                    codecReport = codecReport,
                    recommendedStreamSettings = recommendation.stream,
                    settings = settingsStore.settings.value,
                )
            }
            deviceCapabilityProbe.complete(Unit)
        }
    }

    /** Stream launch is the only caller: it must not pick a profile before the device is known. */
    private suspend fun awaitDeviceCapabilityProbe() {
        if (deviceCapabilityProbe.isCompleted) return
        recordDebugEvent("codec", "Waiting on codec probe before launch")
        deviceCapabilityProbe.await()
    }

    fun initialize() {
        viewModelScope.launch {
            // Nothing here touches the streaming engine, so the catalogue can paint immediately.
            _state.update { it.copy(initializing = false) }
            val restoreResult = restoreAuthSession()
            val providers = runCatching { authRepository.loginProviders() }.getOrDefault(listOf(defaultProvider()))
            val restored = restoreResult.getOrNull()
            val activeSession = restored ?: authStore.activeSession()
            val selected = activeSession?.provider ?: authStore.state.value.selectedProvider ?: providers.firstOrNull() ?: defaultProvider()
            val restoreError = restoreResult.exceptionOrNull()?.message?.takeIf { activeSession == null }
            _state.update {
                it.copy(
                    providers = providers,
                    selectedProvider = selected,
                    authSession = activeSession,
                    savedAccounts = authStore.state.value.sessions.map { session -> session.toSavedAccount() },
                    initializing = false,
                    launchPhase = "",
                    // refreshAfterAuth runs on the next line, so the Store is about to load. Carrying
                    // a stale false in here renders the empty state over a fetch that is starting.
                    loadingGames = activeSession != null,
                    games = if (activeSession == null) emptyList() else it.games,
                    libraryGames = if (activeSession == null) emptyList() else it.libraryGames,
                    catalogResult = if (activeSession == null) CatalogBrowseResult(emptyList()) else it.catalogResult,
                    error = restoreError ?: it.error,
                )
            }
            if (activeSession != null) {
                refreshAfterAuth(activeSession)
            }
        }
    }

    private fun initialProviders(activeSession: AuthSession?): List<LoginProvider> =
        listOfNotNull(authStore.state.value.selectedProvider, activeSession?.provider, defaultProvider())
            .distinctBy { provider -> provider.code.uppercase(Locale.US) }

    private suspend fun restoreAuthSession(throwOnRefreshFailure: Boolean = false): Result<AuthSession?> =
        authRestoreMutex.withLock {
            runCatching {
                authRepository.restore(
                    throwOnRefreshFailure = throwOnRefreshFailure,
                    removeExpiredSessionOnFailure = !throwOnRefreshFailure,
                )
            }
        }

    /** Returns the in-flight refresh so a caller can wait for a fresh token before retrying. */
    fun refreshAuthSessionIfNeeded(): Job? {
        authRefreshJob?.takeIf { it.isActive }?.let { return it }
        val expectedUserId = state.value.authSession?.user?.userId ?: return null
        val job = viewModelScope.launch {
            try {
                val result = restoreAuthSession(throwOnRefreshFailure = true)
                val refreshed = result.getOrNull()
                if (refreshed != null) {
                    val tokenChanged = refreshed.tokens != state.value.authSession?.tokens
                    _state.update { current ->
                        if (current.authSession?.user?.userId != expectedUserId) {
                            current
                        } else {
                            current.copy(
                                authSession = refreshed,
                                selectedProvider = refreshed.provider,
                                savedAccounts = authStore.state.value.sessions.map { session -> session.toSavedAccount() },
                            )
                        }
                    }
                    if (tokenChanged) {
                        recordDebugEvent("auth", "Refreshed saved sign-in tokens in the background")
                    }
                }
                result.exceptionOrNull()?.let { error ->
                    recordDebugEvent("auth", "Background sign-in refresh failed error=${error.debugMessage()}")
                }
            } finally {
                authRefreshJob = null
            }
        }
        authRefreshJob = job
        return job
    }

    /**
     * Called when the app comes back to the foreground.
     *
     * Two things can leave a signed-in reader looking at an empty Store: the one startup fetch
     * failed and its retry ladder ran out, or the process was frozen long enough for its tokens to
     * go stale — and the only in-process token refresh is a 15-minute WorkManager job. Returning to
     * the app is the natural moment to repair both.
     */
    fun onAppForegrounded() {
        val snapshot = state.value
        if (snapshot.authSession == null) return
        // A fresh visit re-arms the ladder that the last run of failures exhausted.
        catalogRetryAttempt = 0
        if (
            !shouldRetryCatalogLoad(
                signedIn = true,
                loadAttempted = catalogLoadAttempted,
                hasGames = snapshot.hasLoadedCatalogGames(),
                loadInFlight = gamesJob?.isActive == true,
                streamActive = snapshot.isAndroidUpdateCheckBlockedByStream(),
            )
        ) {
            return
        }
        recordDebugEvent("catalog", "Reloading empty catalog after returning to the foreground")
        startCatalogRecovery(delayMs = 0L)
    }

    private fun scheduleCatalogRetry() {
        if (catalogRetryAttempt >= CATALOG_RETRY_MAX_ATTEMPTS) {
            recordDebugEvent("catalog", "Catalog retries exhausted after $catalogRetryAttempt attempts")
            return
        }
        val delayMs = catalogRetryDelayMs(catalogRetryAttempt)
        catalogRetryAttempt += 1
        recordDebugEvent("catalog", "Scheduling catalog retry attempt=$catalogRetryAttempt inMs=$delayMs")
        startCatalogRecovery(delayMs)
    }

    private fun startCatalogRecovery(delayMs: Long) {
        catalogRetryJob?.cancel()
        catalogRetryJob = viewModelScope.launch {
            if (delayMs > 0L) delay(delayMs)
            // An expired token is the likeliest reason the previous attempt failed, and retrying
            // the catalogue with the same dead token would only burn an attempt.
            refreshAuthSessionIfNeeded()?.join()
            val snapshot = state.value
            val session = snapshot.authSession ?: return@launch
            if (
                !shouldRetryCatalogLoad(
                    signedIn = true,
                    loadAttempted = catalogLoadAttempted,
                    hasGames = snapshot.hasLoadedCatalogGames(),
                    loadInFlight = gamesJob?.isActive == true,
                    streamActive = snapshot.isAndroidUpdateCheckBlockedByStream(),
                )
            ) {
                return@launch
            }
            refreshAfterAuth(session)
        }
    }

    fun setPage(page: AppPage) {
        _state.update { it.copy(page = page, selectedGame = null) }
    }

    fun recordSettingsIconTap() {
        if (settingsDiagnosticTapTracker.recordTap(SystemClock.elapsedRealtime())) requestDiagnosticShare()
    }

    fun recordLoginIconTap() {
        if (!loginIconTapTracker.recordTap(SystemClock.elapsedRealtime())) return
        _state.update { it.copy(loginToolsVisible = true) }
    }

    fun dismissLoginTools() {
        _state.update { it.copy(loginToolsVisible = false) }
    }

    fun dismissDiagnosticShare() {
        _state.update { it.copy(diagnosticShare = DiagnosticShareState()) }
    }

    fun dismissSessionReport() {
        _state.update { it.copy(sessionReport = null) }
    }

    fun requestDiagnosticShare() {
        _state.update {
            it.copy(diagnosticShare = DiagnosticShareState(awaitingConsent = true))
        }
    }

    fun resetBugReportSubmission() {
        if (state.value.bugReportSubmission.uploading) return
        _state.update { it.copy(bugReportSubmission = BugReportSubmissionState()) }
    }

    fun submitBugReport(title: String, description: String) =
        submitBugReport(title, description, knownIssueOverrideKey = null)

    fun submitBugReport(title: String, description: String, knownIssueOverrideKey: String?) {
        if (state.value.bugReportSubmission.uploading) return
        val snapshot = state.value
        val versionBlock = androidBugReportBlockMessage(
            update = snapshot.androidUpdate,
            versionCheck = snapshot.bugReportVersionCheck,
        )
        val appLocale = currentAndroidAppLocale(getApplication())
        val contentError = androidBugReportTitleError(title)
            ?: androidBugReportDescriptionError(description)
        val validationError = when {
            versionBlock != null -> versionBlock
            !appLocale.bugReportsAllowed ->
                "Set the OpenNOW or device language to English before sending a bug report"
            contentError != null -> contentError
            else -> null
        }
        if (validationError != null) {
            _state.update {
                it.copy(
                    bugReportSubmission = BugReportSubmissionState(error = validationError),
                )
            }
            return
        }
        _state.update {
            it.copy(bugReportSubmission = BugReportSubmissionState(uploading = true))
        }
        viewModelScope.launch {
            try {
                val languageCheck = identifyAndroidBugReportLanguage(title, description)
                val logFileName = debugLogFileName()
                val metadata = buildAndroidBugReportMetadata(
                    logFileName = logFileName,
                    knownIssueOverrideKey = knownIssueOverrideKey,
                    device = AndroidDeviceDiagnostics.snapshot(getApplication()),
                )
                val logBytes = withContext(Dispatchers.Default) {
                    sanitizedDebugLogText().toByteArray(Charsets.UTF_8)
                }
                val receipt = uploadAndroidBugReport(
                    http = http,
                    report = AndroidBugReport(
                        title = title,
                        description = description,
                        versionName = BuildConfig.VERSION_NAME,
                        versionCode = BuildConfig.VERSION_CODE.toString(),
                        reporterId = androidBugReportReporterId(authStore.stableDeviceId()),
                        appLanguageSelectionTag = appLocale.bugReportLanguageTag.orEmpty(),
                        languageCheck = languageCheck,
                        metadata = metadata,
                        files = listOf(
                            AndroidBugReportAttachment(
                                fileName = logFileName,
                                contentType = "text/plain; charset=utf-8",
                                bytes = logBytes,
                            ),
                        ),
                    ),
                )
                recordDebugEvent(
                    "bug-report",
                    "PrintedWaste bug report submitted knownIssueOverride=${knownIssueOverrideKey ?: "none"}",
                )
                _state.update {
                    it.copy(
                        bugReportSubmission = BugReportSubmissionState(
                            submitted = true,
                            reference = receipt.reference,
                        ),
                    )
                }
            } catch (error: CancellationException) {
                throw error
            } catch (error: Throwable) {
                recordDebugEvent(
                    "bug-report",
                    "PrintedWaste bug report failed error=${error.debugMessage()}",
                )
                _state.update {
                    it.copy(
                        bugReportSubmission = BugReportSubmissionState(
                            error = error.message ?: "Could not send the bug report",
                        ),
                    )
                }
            }
        }
    }

    fun verifyBugReportVersion() {
        val snapshot = state.value
        if (!snapshot.androidUpdate.installSource.isGooglePlay) return
        if (bugReportUpdateVerificationJob?.isActive == true) return

        _state.update {
            it.copy(
                bugReportVersionCheck = AndroidBugReportVersionCheckState(
                    status = AndroidBugReportVersionCheckStatus.Checking,
                    message = "Checking Google Play...",
                ),
            )
        }
        bugReportUpdateCheckActive = true
        bugReportUpdateVerificationJob = viewModelScope.launch {
            try {
                val existingUpdateCheck = androidUpdateJob?.takeIf { it.isActive }
                if (existingUpdateCheck != null) {
                    existingUpdateCheck.join()
                } else {
                    appUpdater.checkForUpdate()
                }
                val update = appUpdater.state.value
                val versionCheck = when (update.status) {
                    AndroidUpdateStatus.Available,
                    AndroidUpdateStatus.Downloading,
                    AndroidUpdateStatus.Downloaded,
                    -> AndroidBugReportVersionCheckState(
                        status = AndroidBugReportVersionCheckStatus.UpdateRequired,
                        message = update.message,
                    )
                    AndroidUpdateStatus.NotAvailable -> AndroidBugReportVersionCheckState(
                        status = AndroidBugReportVersionCheckStatus.Current,
                        message = update.message,
                    )
                    else -> AndroidBugReportVersionCheckState(
                        status = AndroidBugReportVersionCheckStatus.CheckFailed,
                        message = update.message.takeIf { it.isNotBlank() },
                    )
                }
                recordDebugEvent(
                    "bug-report",
                    "Google Play version preflight result=${versionCheck.status} currentBuild=${update.currentVersionCode} availableBuild=${update.availableVersionCode ?: -1}",
                )
                _state.update { it.copy(bugReportVersionCheck = versionCheck) }
            } catch (error: CancellationException) {
                throw error
            } catch (error: Throwable) {
                recordDebugEvent(
                    "bug-report",
                    "Google Play version preflight failed error=${error.debugMessage()}",
                )
                _state.update {
                    it.copy(
                        bugReportVersionCheck = AndroidBugReportVersionCheckState(
                            status = AndroidBugReportVersionCheckStatus.CheckFailed,
                            message = error.message ?: "Google Play update check failed.",
                        ),
                    )
                }
            } finally {
                bugReportUpdateCheckActive = false
                bugReportUpdateVerificationJob = null
            }
        }
    }

    fun startLocalTvConnector() {
        if (!state.value.androidTvProfile) return
        settingsStore.update { it.copy(localTvRemoteEnabled = true) }
        localTvConnector.startHosting()
    }

    fun stopLocalTvConnector() {
        if (state.value.androidTvProfile) {
            settingsStore.update { it.copy(localTvRemoteEnabled = false) }
        }
        localTvConnector.stopHosting()
    }

    fun refreshLocalTvPairingCode() {
        if (!state.value.androidTvProfile || !state.value.settings.localTvRemoteEnabled) return
        localTvConnector.refreshPairingCode()
    }

    fun setLocalTvDeviceTrusted(trusted: Boolean) {
        if (!state.value.androidTvProfile) return
        localTvConnector.setPairedDeviceTrusted(trusted)
    }

    fun setLocalTvTrustRequested(requested: Boolean) {
        if (state.value.androidTvProfile) return
        localTvConnector.setPhoneTrustRequest(requested)
    }

    fun forgetLocalTvConnector() {
        localTvConnector.forgetPhoneTarget()
    }

    fun discoverLocalTvs() {
        if (state.value.androidTvProfile) return
        localTvConnector.discoverTvs()
    }

    fun pairDiscoveredLocalTv(tv: DiscoveredLocalTv, code: String) {
        if (state.value.androidTvProfile) return
        localTvConnector.pairDiscoveredTv(tv, code)
    }

    fun pairLocalTvQrValue(value: String?) {
        if (state.value.androidTvProfile) return
        val uri = value?.trim()?.takeIf(String::isNotBlank)?.let(Uri::parse)
        if (!localTvConnector.isPairUri(uri) || uri == null) {
            localTvConnector.reportPairingError("That QR code is not an OpenNOW TV pairing code")
            return
        }
        localTvConnector.pairPhone(uri)
    }

    fun playOnLocalTv(game: GameInfo) {
        if (state.value.androidTvProfile) return
        localTvConnector.sendLaunch(gameTrackingKey(game), game.title)
    }

    fun signInLocalTv() {
        if (state.value.androidTvProfile) return
        val session = state.value.authSession ?: run {
            _state.update { it.copy(error = "Sign in on the phone first") }
            return
        }
        localTvConnector.sendSignIn(session)
    }

    fun switchLocalTvAccount(userId: String) {
        if (state.value.androidTvProfile) return
        val session = authStore.state.value.sessions.firstOrNull { it.user.userId == userId } ?: run {
            _state.update { it.copy(error = "That account is no longer available on this phone") }
            return
        }
        localTvConnector.sendSignIn(session)
    }

    fun sendLocalTvRemoteAction(action: String, value: String? = null) {
        if (state.value.androidTvProfile) return
        localTvConnector.sendRemoteAction(action, value)
    }

    private fun handleLocalTvRemoteRequest(request: LocalTvRemoteRequest) {
        recordDebugEvent("tv-remote", "Accepted encrypted action=${request.action}")
        when (request.action) {
            "open_stream_menu" -> _state.update {
                it.copy(remoteStreamMenuRequestToken = it.remoteStreamMenuRequestToken + 1)
            }
            "toggle_stream_stats" -> _state.update {
                it.copy(remoteStatsToggleRequestToken = it.remoteStatsToggleRequestToken + 1)
            }
            "stop_stream" -> stopStream()
            "apply_recommended" -> applyStreamPreset(StreamPreset.Recommended)
            "set_codec" -> request.value
                ?.let { value -> runCatching { VideoCodec.valueOf(value) }.getOrNull() }
                ?.let { codec -> updateStreamSettings { it.copy(codec = codec) } }
            "set_resolution" -> request.value
                ?.takeIf { streamAspectRatioForResolution(it) != null }
                ?.let { resolution ->
                    updateStreamSettings {
                        it.copy(
                            resolution = resolution,
                            aspectRatio = streamAspectRatioForResolution(resolution) ?: it.aspectRatio,
                        )
                    }
                }
            "set_fps" -> request.value?.toIntOrNull()
                ?.takeIf { it in setOf(30, 60, 120) }
                ?.let { fps -> updateStreamSettings { it.copy(fps = fps) } }
            "set_background" -> request.value?.toBooleanStrictOrNull()?.let { enabled ->
                settingsStore.update {
                    it.copy(
                        nerdCatalogBackground = enabled,
                        systemWallpaperBackground = false,
                    )
                }
            }
            "set_ui_sounds" -> request.value?.toBooleanStrictOrNull()?.let { enabled ->
                settingsStore.update { it.copy(controllerUiSounds = enabled) }
            }
            "set_safe_area" -> request.value?.toFloatOrNull()?.coerceIn(0f, 120f)?.let { padding ->
                settingsStore.update { it.copy(tvSafeAreaPaddingDp = padding) }
            }
            "set_hide_server_selector" -> request.value?.toBooleanStrictOrNull()?.let { hidden ->
                settingsStore.update { it.copy(hideServerSelector = hidden) }
            }
        }
    }

    private fun acceptLocalTvSignIn(transferredSession: AuthSession) {
        viewModelScope.launch {
            authStore.upsertSession(transferredSession)
            val restored = restoreAuthSession(throwOnRefreshFailure = true).getOrElse { error ->
                authStore.removeSession(transferredSession.user.userId)
                _state.update { it.copy(error = "Phone sign-in could not be verified: ${error.message.orEmpty()}") }
                return@launch
            } ?: run {
                authStore.removeSession(transferredSession.user.userId)
                _state.update { it.copy(error = "Phone sign-in could not be verified") }
                return@launch
            }
            _state.update {
                it.copy(
                    authSession = restored,
                    selectedProvider = restored.provider,
                    savedAccounts = authStore.state.value.sessions.map { saved -> saved.toSavedAccount() },
                    error = null,
                    loadingGames = true,
                )
            }
            Toast.makeText(
                getApplication(),
                getApplication<Application>().getString(R.string.toast_signed_in_from_phone),
                Toast.LENGTH_SHORT,
            ).show()
            recordDebugEvent("tv-connector", "Accepted encrypted local sign-in provider=${restored.provider.code}")
            refreshAfterAuth(restored)
        }
    }

    fun uploadDiagnosticShare() {
        if (state.value.diagnosticShare.uploading) return
        _state.update {
            it.copy(diagnosticShare = DiagnosticShareState(uploading = true))
        }
        viewModelScope.launch {
            val snapshot = state.value
            val summaryHeader = diagnosticSummaryHeader(snapshot)
            val sanitizedLog = sanitizedDebugLogText()
            val payload = sanitizeDiagnosticExport(
                buildString {
                    appendLine(summaryHeader)
                    appendLine()
                    append(sanitizedLog)
                },
            )
            runCatching { uploadAndroidDiagnosticPaste(http, payload) }
                .onSuccess { pasteUrl ->
                    _state.update {
                        it.copy(
                            diagnosticShare = DiagnosticShareState(
                                pasteUrl = pasteUrl,
                                clipboardSummary = "$summaryHeader\nPaste: $pasteUrl",
                            ),
                        )
                    }
                }
                .onFailure { error ->
                    if (error is CancellationException) return@onFailure
                    _state.update {
                        it.copy(
                            diagnosticShare = DiagnosticShareState(
                                awaitingConsent = true,
                                error = error.message ?: "Could not upload diagnostics",
                            ),
                        )
                    }
                }
        }
    }

    private fun diagnosticSummaryHeader(snapshot: OpenNowUiState): String {
        val recommendation = deviceRecommendation
        val model = listOf(Build.MANUFACTURER, Build.MODEL)
            .map(String::trim)
            .filter(String::isNotBlank)
            .distinct()
            .joinToString(" ")
        val accountType = snapshot.subscriptionInfo?.membershipTier
            ?: snapshot.authSession?.user?.membershipTier
            ?: "Unknown"
        val provider = snapshot.authSession?.provider?.displayName?.takeIf { it.isNotBlank() } ?: "Unknown"
        return buildString {
            appendLine("OpenNOW Android ${BuildConfig.VERSION_NAME} (${BuildConfig.VERSION_CODE})")
            appendLine("Client: ${if (snapshot.androidTvProfile) "Android TV" else "Android mobile"}")
            appendLine("Hardware: $model · Android ${Build.VERSION.RELEASE}")
            appendLine("Screen: ${recommendation?.displayWidth ?: "?"}x${recommendation?.displayHeight ?: "?"} · processors ${recommendation?.processorCount ?: "?"} · memory ${recommendation?.totalMemoryMiB?.let { "$it MiB" } ?: "unknown"}")
            appendLine("Membership: $provider · $accountType")
            appendLine("Profile: ${snapshot.settings.streamPreset} · ${snapshot.settings.stream.resolution}@${snapshot.settings.stream.fps} · ${snapshot.settings.stream.codec} · ${snapshot.settings.stream.maxBitrateMbps} Mbps")
            append("Status: ${snapshot.streamStatus} · ${snapshot.error?.take(160)?.let(::sanitizeDiagnosticExport) ?: "no current error"}")
        }
    }

    fun openAndroidUpdateSettings() {
        _state.update {
            it.copy(
                page = AppPage.Settings,
                selectedGame = null,
                settingsRouteTarget = SettingsRouteTarget.General,
            )
        }
    }

    fun openAccountSettings() {
        _state.update {
            it.copy(
                page = AppPage.Settings,
                selectedGame = null,
                settingsRouteTarget = SettingsRouteTarget.Account,
            )
        }
    }

    fun openStreamSettings() {
        _state.update {
            it.copy(
                page = AppPage.Settings,
                selectedGame = null,
                settingsRouteTarget = SettingsRouteTarget.Stream,
            )
        }
    }

    fun openInterfaceSettings() {
        _state.update {
            it.copy(
                page = AppPage.Settings,
                selectedGame = null,
                settingsRouteTarget = SettingsRouteTarget.Interface,
            )
        }
    }

    fun consumeSettingsRouteTarget(target: SettingsRouteTarget) {
        _state.update { current ->
            if (current.settingsRouteTarget == target) {
                current.copy(settingsRouteTarget = null)
            } else {
                current
            }
        }
    }

    fun selectProvider(provider: LoginProvider) {
        if (!provider.supportsDeviceCodeLogin && state.value.deviceLoginPrompt != null) {
            loginJob?.cancel()
            loginJob = null
        }
        _state.update {
            it.copy(
                selectedProvider = provider,
                deviceLoginPrompt = if (provider.supportsDeviceCodeLogin) it.deviceLoginPrompt else null,
                launchPhase = if (provider.supportsDeviceCodeLogin) it.launchPhase else "",
            )
        }
    }

    fun login(provider: LoginProvider = state.value.selectedProvider) {
        loginJob?.cancel()
        loginJob = viewModelScope.launch {
            val useDeviceCode = state.value.androidTvProfile && provider.supportsDeviceCodeLogin
            _state.update {
                it.copy(
                    error = null,
                    launchPhase = if (useDeviceCode) "Requesting TV sign-in code" else "Opening ${provider.displayName} login",
                    deviceLoginPrompt = null,
                )
            }
            runCatching {
                loginWithBestAvailableMethod(provider, useDeviceCode)
            }
                .onSuccess { session ->
                    completeLogin(session)
                }
                .onFailure { error ->
                    if (error is CancellationException) return@onFailure
                    _state.update { it.copy(error = error.message ?: "Login failed", launchPhase = "", deviceLoginPrompt = null) }
                }
        }
    }

    fun loginWithCode(provider: LoginProvider = state.value.selectedProvider) {
        if (!provider.supportsDeviceCodeLogin) {
            login(provider)
            return
        }
        loginJob?.cancel()
        loginJob = viewModelScope.launch {
            _state.update {
                it.copy(
                    error = null,
                    launchPhase = "Requesting sign-in code",
                    deviceLoginPrompt = null,
                )
            }
            runCatching {
                loginWithDeviceCode(provider)
            }
                .onSuccess { session ->
                    completeLogin(session)
                }
                .onFailure { error ->
                    if (error is CancellationException) return@onFailure
                    _state.update { it.copy(error = error.message ?: "Code sign-in failed", launchPhase = "", deviceLoginPrompt = null) }
                }
        }
    }

    fun loginWithToken(tokenInput: String, provider: LoginProvider = state.value.selectedProvider) {
        loginJob?.cancel()
        loginJob = viewModelScope.launch {
            _state.update {
                it.copy(
                    error = null,
                    launchPhase = "Checking sign-in token",
                    deviceLoginPrompt = null,
                    loginToolsVisible = false,
                )
            }
            runCatching { authRepository.loginWithToken(provider, tokenInput) }
                .onSuccess { session -> completeLogin(session) }
                .onFailure { error ->
                    if (error is CancellationException) return@onFailure
                    _state.update {
                        it.copy(
                            error = error.message ?: "Token sign-in failed",
                            launchPhase = "",
                            deviceLoginPrompt = null,
                        )
                    }
                }
        }
    }

    private suspend fun completeLogin(session: AuthSession) {
        _state.update {
            it.copy(
                authSession = session,
                selectedProvider = session.provider,
                savedAccounts = authStore.state.value.sessions.map { saved -> saved.toSavedAccount() },
                launchPhase = "",
                deviceLoginPrompt = null,
                error = null,
                page = defaultLaunchAppPage(),
                loginToolsVisible = false,
            )
        }
        refreshAfterAuth(session)
    }

    private suspend fun loginWithBestAvailableMethod(provider: LoginProvider, useDeviceCode: Boolean): AuthSession {
        if (useDeviceCode) {
            return loginWithDeviceCode(provider)
        }

        return try {
            authRepository.login(provider) {
                _state.update { it.copy(launchPhase = LOGIN_PHASE_GETTING_TOKENS, error = null) }
            }
        } catch (error: Throwable) {
            if (error is CancellationException || !isLoopbackLoginFailure(error)) {
                throw error
            }
            if (!provider.supportsDeviceCodeLogin) {
                throw error
            }
            _state.update {
                it.copy(
                    launchPhase = "Requesting sign-in code",
                    error = "Browser sign-in could not reach the local callback. Use this code to finish sign-in.",
                )
            }
            loginWithDeviceCode(provider, clearErrorOnPrompt = false)
        }
    }

    private suspend fun loginWithDeviceCode(provider: LoginProvider, clearErrorOnPrompt: Boolean = true): AuthSession =
        authRepository.loginWithDeviceCode(provider) { prompt ->
            _state.update {
                it.copy(
                    deviceLoginPrompt = prompt,
                    launchPhase = "Waiting for sign-in",
                    error = if (clearErrorOnPrompt) null else it.error,
                )
            }
        }

    private fun isLoopbackLoginFailure(error: Throwable): Boolean {
        val message = generateSequence(error) { it.cause }
            .mapNotNull { it.message }
            .joinToString(" ")
            .lowercase()
        return "oauth callback" in message ||
            "callback ports" in message ||
            "local callback" in message ||
            "localhost" in message ||
            "127.0.0.1" in message
    }

    fun cancelLogin() {
        loginJob?.cancel()
        loginJob = null
        _state.update { it.copy(launchPhase = "", deviceLoginPrompt = null) }
    }

    fun logout() {
        viewModelScope.launch {
            pendingActiveSessionLaunch = null
            authRepository.logout()
            val nextSession = authStore.activeSession()
            _state.update {
                it.copy(
                    authSession = nextSession,
                    selectedProvider = nextSession?.provider ?: it.selectedProvider,
                    savedAccounts = authStore.state.value.sessions.map { saved -> saved.toSavedAccount() },
                    subscriptionInfo = null,
                    accountConnectors = emptyList(),
                    loadingAccountConnectors = false,
                    connectorActionStore = null,
                    games = emptyList(),
                    newlyAddedGames = emptyList(),
                    libraryGames = emptyList(),
                    libraryFilterIds = emptyList(),
                    streamSession = null,
                    activeStreamSettings = null,
                    activeSession = null,
                    activeSessionDecision = null,
                    deviceLoginPrompt = null,
                    pendingStoreChoiceGame = null,
                    page = AppPage.Home,
                )
            }
            if (nextSession != null) {
                refreshAfterAuth(nextSession)
            }
        }
    }

    fun switchAccount(userId: String) {
        viewModelScope.launch {
            pendingActiveSessionLaunch = null
            _state.update { it.copy(settingsRefreshing = true, error = null) }
            try {
                authStore.setActiveSession(userId)
                val sessionResult = restoreAuthSession()
                val session = sessionResult.getOrElse { error ->
                    _state.update { current ->
                        current.copy(
                            authSession = authStore.activeSession(),
                            savedAccounts = authStore.state.value.sessions.map { saved -> saved.toSavedAccount() },
                            error = error.message ?: "Could not refresh the selected account. Please sign in again.",
                            settingsRefreshing = false,
                        )
                    }
                    recordDebugEvent("auth", "Account switch refresh failed error=${error.debugMessage()}")
                    return@launch
                }
                if (session == null) {
                    // target session was expired/invalid and got removed.
                    // Fall back to whatever active session is left.
                    val fallbackSession = restoreAuthSession().getOrNull()
                    _state.update { current ->
                        current.copy(
                            authSession = fallbackSession,
                            selectedProvider = fallbackSession?.provider ?: current.selectedProvider,
                            savedAccounts = authStore.state.value.sessions.map { saved -> saved.toSavedAccount() },
                            subscriptionInfo = null,
                            accountConnectors = emptyList(),
                            loadingAccountConnectors = false,
                            connectorActionStore = null,
                            games = emptyList(),
                            newlyAddedGames = emptyList(),
                            libraryGames = emptyList(),
                            catalogResult = CatalogBrowseResult(emptyList()),
                            libraryFilterIds = emptyList(),
                            selectedGame = null,
                            activeSession = null,
                            activeSessionDecision = null,
                            error = "Failed to switch account: session expired. Please log in again.",
                            page = AppPage.Home,
                            settingsRefreshing = false,
                        )
                    }
                    return@launch
                }
                gamesJob?.cancel()
                _state.update { current ->
                    current.copy(
                        authSession = session,
                        selectedProvider = session.provider,
                        savedAccounts = authStore.state.value.sessions.map { saved -> saved.toSavedAccount() },
                        subscriptionInfo = null,
                        accountConnectors = emptyList(),
                        loadingAccountConnectors = false,
                        connectorActionStore = null,
                        games = emptyList(),
                        newlyAddedGames = emptyList(),
                        libraryGames = emptyList(),
                        catalogResult = CatalogBrowseResult(emptyList()),
                        libraryFilterIds = emptyList(),
                        catalogQueryLoading = false,
                        selectedGame = null,
                        activeSession = null,
                        activeSessionDecision = null,
                        error = null,
                        page = AppPage.Home,
                        settingsRefreshing = false,
                    )
                }
                refreshAfterAuth(session)
            } catch (e: Exception) {
                if (e is CancellationException) throw e
                _state.update { current ->
                    current.copy(
                        error = e.message ?: "Failed to switch account",
                        settingsRefreshing = false,
                    )
                }
            }
        }
    }

    fun logoutAll() {
        pendingActiveSessionLaunch = null
        authRepository.logoutAll()
        _state.update {
            it.copy(
                authSession = null,
                savedAccounts = emptyList(),
                subscriptionInfo = null,
                accountConnectors = emptyList(),
                loadingAccountConnectors = false,
                connectorActionStore = null,
                games = emptyList(),
                newlyAddedGames = emptyList(),
                libraryGames = emptyList(),
                libraryFilterIds = emptyList(),
                catalogQueryLoading = false,
                streamSession = null,
                activeStreamSettings = null,
                activeSession = null,
                activeSessionDecision = null,
                deviceLoginPrompt = null,
                pendingStoreChoiceGame = null,
                page = AppPage.Home,
            )
        }
    }

    fun refreshGames() {
        val session = state.value.authSession ?: return
        catalogRetryJob?.cancel()
        catalogRetryAttempt = 0
        viewModelScope.launch {
            refreshAfterAuth(session, keepRefreshVisibleWithCache = true)
        }
    }

    fun setCatalogSearch(query: String) {
        _state.update { it.copy(catalogSearch = query, catalogQueryLoading = true) }
        refreshCatalogDebounced()
    }

    fun setLibrarySearch(query: String) {
        _state.update { it.copy(librarySearch = query) }
    }

    fun toggleLibraryFilter(filterId: String) {
        val nextFilters = state.value.libraryFilterIds.let { current ->
            if (filterId in current) current - filterId else current + filterId
        }
        _state.update {
            it.copy(libraryFilterIds = nextFilters)
        }
        settingsStore.update { it.copy(libraryFilterIds = nextFilters) }
    }

    fun clearLibraryFilters() {
        _state.update { it.copy(libraryFilterIds = emptyList()) }
        settingsStore.update { it.copy(libraryFilterIds = emptyList()) }
    }

    fun setLibrarySort(sortId: String) {
        if (sortId !in setOf(LIBRARY_SORT_DEFAULT, LIBRARY_SORT_RECENT, LIBRARY_SORT_TITLE)) return
        _state.update { it.copy(librarySortId = sortId) }
        settingsStore.update { it.copy(librarySortId = sortId) }
    }

    fun setCatalogSort(sortId: String) {
        _state.update { it.copy(catalogSortId = sortId, catalogQueryLoading = true) }
        settingsStore.update { it.copy(catalogSortId = sortId) }
        refreshCatalogDebounced()
    }

    fun toggleCatalogFilter(filterId: String) {
        val nextFilters = state.value.catalogFilterIds.let { current ->
            if (filterId in current) current - filterId else current + filterId
        }
        _state.update { it.copy(catalogFilterIds = nextFilters, catalogQueryLoading = true) }
        settingsStore.update { it.copy(catalogFilterIds = nextFilters) }
        refreshCatalogDebounced()
    }

    fun clearCatalogFilters() {
        _state.update { it.copy(catalogFilterIds = emptyList(), catalogQueryLoading = true) }
        settingsStore.update { it.copy(catalogFilterIds = emptyList()) }
        refreshCatalogDebounced()
    }

    fun selectGame(game: GameInfo) {
        gameDetailsJob?.cancel()
        _state.update { it.copy(selectedGame = game) }
        if (!shouldHydrateGameDetails(game)) return
        val auth = state.value.authSession ?: return
        val selectedKey = gameTrackingKey(game)
        gameDetailsJob = viewModelScope.launch {
            val details = try {
                withContext(Dispatchers.IO) {
                    catalogRepository.hydrateGameDetails(
                        token = auth.tokens.idToken ?: auth.tokens.accessToken,
                        providerStreamingBaseUrl = effectiveStreamingBaseUrl(auth),
                        game = game,
                    )
                }
            } catch (error: CancellationException) {
                throw error
            } catch (error: Exception) {
                recordDebugEvent("catalog", "Game detail metadata failed title=${game.title} error=${error.debugMessage()}")
                return@launch
            }
            _state.update { current ->
                val selected = current.selectedGame
                if (selected == null || gameTrackingKey(selected) != selectedKey) {
                    current
                } else {
                    current.copy(
                        selectedGame = mergeGameInfo(selected, details),
                        games = current.games.withHydratedGameDetails(details),
                        newlyAddedGames = current.newlyAddedGames.withHydratedGameDetails(details),
                        libraryGames = current.libraryGames.withHydratedGameDetails(details),
                        catalogResult = current.catalogResult.copy(
                            games = current.catalogResult.games.withHydratedGameDetails(details),
                        ),
                    )
                }
            }
        }
    }

    fun clearSelectedGame() {
        gameDetailsJob?.cancel()
        gameDetailsJob = null
        _state.update { it.copy(selectedGame = null) }
    }

    fun updateSettings(next: AppSettings) {
        settingsStore.replace(next)
    }

    fun addLocalApp(packageName: String) {
        if (packageName.isBlank() || packageName == getApplication<Application>().packageName) return
        settingsStore.update { current ->
            current.copy(
                localAppPackageNames = (current.localAppPackageNames + packageName).distinct(),
            )
        }
    }

    fun removeLocalApp(packageName: String) {
        settingsStore.update { current ->
            current.copy(localAppPackageNames = current.localAppPackageNames - packageName)
        }
    }

    fun setLocalAppsCollapsed(collapsed: Boolean) {
        settingsStore.update { current -> current.copy(localAppsCollapsed = collapsed) }
    }

    fun checkAndroidUpdate() {
        startAndroidUpdateCheck(automatic = false)
    }

    fun dismissAndroidUpdateNotice() {
        val key = androidUpdateNoticeKey(state.value.androidUpdate) ?: return
        androidUpdateNoticeStore.dismiss(key)
        _state.update { it.copy(dismissedAndroidUpdateNoticeKey = key) }
    }

    fun downloadAndroidUpdate() {
        if (androidUpdateJob?.isActive == true || !state.value.androidUpdate.canDownload) return
        androidUpdateJob = viewModelScope.launch {
            appUpdater.downloadUpdate()
        }
    }

    fun performAndroidUpdatePrimaryAction() {
        val update = state.value.androidUpdate
        if (update.canOpenPlayStore) {
            appUpdater.openPlayStoreListing()
        } else {
            downloadAndroidUpdate()
        }
    }

    fun installAndroidUpdate() {
        if (!state.value.androidUpdate.canInstall) return
        appUpdater.installDownloadedUpdate()
    }

    private fun startAndroidUpdateAutoChecks() {
        if (androidUpdateAutoJob?.isActive == true) return
        if (!state.value.androidUpdate.updateChecksSupported) return
        androidUpdateAutoJob = viewModelScope.launch {
            delay(ANDROID_UPDATE_LAUNCH_CHECK_DELAY_MS)
            while (true) {
                runAutomaticAndroidUpdateCheck()
                delay(ANDROID_UPDATE_PERIODIC_CHECK_INTERVAL_MS)
            }
        }
    }

    private suspend fun runAutomaticAndroidUpdateCheck() {
        if (!state.value.androidUpdate.updateChecksSupported) return
        if (!state.value.settings.autoCheckForUpdates) return
        waitForAndroidUpdateCheckWindow()
        val snapshot = state.value
        if (!snapshot.settings.autoCheckForUpdates || !snapshot.androidUpdate.shouldRunAutomaticCheck()) return
        startAndroidUpdateCheck(automatic = true)?.join()
    }

    private suspend fun waitForAndroidUpdateCheckWindow() {
        while (state.value.isAndroidUpdateCheckBlockedByStream()) {
            delay(ANDROID_UPDATE_STREAMING_RETRY_DELAY_MS)
        }
    }

    private fun startAndroidUpdateCheck(automatic: Boolean): Job? {
        if (androidUpdateJob?.isActive == true) return null
        if (!state.value.androidUpdate.updateChecksSupported) return null
        if (state.value.isAndroidUpdateCheckBlockedByStream()) {
            if (!automatic) {
                appUpdater.markCheckDeferredForStreaming()
            }
            return null
        }
        return viewModelScope.launch {
            if (state.value.isAndroidUpdateCheckBlockedByStream()) {
                if (!automatic) {
                    appUpdater.markCheckDeferredForStreaming()
                }
                return@launch
            }
            appUpdater.checkForUpdate()
        }.also { job ->
            androidUpdateJob = job
        }
    }

    private fun cancelAndroidUpdateCheckForStreaming() {
        if (state.value.androidUpdate.status != AndroidUpdateStatus.Checking) return
        androidUpdateJob?.cancel()
        androidUpdateJob = null
        appUpdater.markCheckDeferredForStreaming()
    }

    fun refreshSettings() {
        if (settingsRefreshJob?.isActive == true) return
        settingsRefreshJob = viewModelScope.launch {
            _state.update { it.copy(settingsRefreshing = true, error = null) }
            try {
                val updateJob = startAndroidUpdateCheck(automatic = false)
                val session = state.value.authSession
                val accountJob = session?.let { activeSession ->
                    launch { refreshSettingsAccountData(activeSession) }
                }
                val accountConnectorsJob = session?.let { activeSession ->
                    launch { refreshAccountConnectors(activeSession) }
                }
                updateJob?.join()
                accountJob?.join()
                accountConnectorsJob?.join()
            } finally {
                _state.update { it.copy(settingsRefreshing = false) }
            }
        }
    }

    fun resetSettings() {
        Toast.makeText(
            getApplication(),
            getApplication<Application>().getString(R.string.toast_clearing_app_data),
            Toast.LENGTH_SHORT,
        ).show()
        wipeAppDataAndRelaunch(getApplication())
    }

    fun resetStreamTutorial() {
        settingsStore.update { it.copy(androidStreamGuideDismissed = false) }
        Toast.makeText(
            getApplication(),
            getApplication<Application>().getString(R.string.toast_tutorial_reset),
            Toast.LENGTH_SHORT,
        ).show()
    }

    fun clearCatalogCache() {
        viewModelScope.launch {
            val removed = withContext(Dispatchers.IO) { catalogCacheStore.clear() }
            Toast.makeText(
                getApplication(),
                getApplication<Application>().getString(
                    if (removed == 0) R.string.toast_cache_already_clear else R.string.toast_cache_cleared,
                ),
                Toast.LENGTH_SHORT,
            ).show()
            state.value.authSession?.let { refreshAfterAuth(it) }
        }
    }

    fun refreshAccountConnectors() {
        val session = state.value.authSession ?: return
        viewModelScope.launch {
            refreshAccountConnectors(session)
        }
    }

    private suspend fun refreshAccountConnectors(session: AuthSession) {
        accountConnectorRefreshMutex.withLock {
            val token = accountConnectorAuthToken(session)
            _state.update { it.copy(loadingAccountConnectors = true) }
            runCatching { withTimeout(15_000L) { accountConnectorRepository.fetchConnectors(token) } }
                .onSuccess { connectors ->
                    _state.update { it.copy(accountConnectors = connectors, loadingAccountConnectors = false) }
                }
                .onFailure { error ->
                    if (error is CancellationException) return@onFailure
                    _state.update { it.copy(loadingAccountConnectors = false, error = error.message ?: "Failed to load account connections") }
                }
        }
    }

    private suspend fun refreshSettingsAccountData(session: AuthSession) {
        val token = accountConnectorAuthToken(session)
        coroutineScope {
            val subscription = async {
                runCatching {
                    val vpcId = catalogRepository.getVpcId(token, session.provider.streamingServiceUrl)
                    subscriptionRepository.fetchSubscription(token, session.user.userId, vpcId)
                }.getOrNull()
            }
            val regions = async {
                runCatching { fetchDynamicRegions(http, token, session.provider.streamingServiceUrl).first }
                    .getOrDefault(emptyList())
            }
            val fetchedSubscription = subscription.await()
            val enrichedSession = persistSubscriptionTier(session, fetchedSubscription)
            val fetchedRegions = regions.await()
            _state.update { current ->
                current.copy(
                    authSession = current.authSession
                        ?.takeIf { it.user.userId == enrichedSession.user.userId }
                        ?.let { enrichedSession }
                        ?: current.authSession,
                    savedAccounts = savedAccountsSnapshot(),
                    subscriptionInfo = fetchedSubscription ?: current.subscriptionInfo,
                    regions = fetchedRegions.ifEmpty { current.regions },
                )
            }
        }
    }

    fun connectAccountConnector(store: String, openUrl: (String) -> Unit) {
        val session = state.value.authSession ?: return
        viewModelScope.launch {
            val token = accountConnectorAuthToken(session)
            _state.update { it.copy(connectorActionStore = store, error = null) }
            runCatching { withTimeout(20_000L) { accountConnectorRepository.loginUrl(store, token) } }
                .onSuccess { url ->
                    _state.update { it.copy(connectorActionStore = null) }
                    openUrl(url)
                    refreshAccountConnectorsAfterLinking()
                }
                .onFailure { error ->
                    if (error is CancellationException) return@onFailure
                    val message = error.message
                        ?: getApplication<Application>().getString(R.string.error_store_connect_failed)
                    Toast.makeText(getApplication(), message, Toast.LENGTH_SHORT).show()
                    _state.update { it.copy(connectorActionStore = null, error = message) }
                }
        }
    }

    private suspend fun accountConnectorAuthToken(session: AuthSession): String {
        val latestSession = refreshedSessionOrFallback(
            fallback = session,
            refresh = { authRepository.restore(forceRefresh = false) },
            onFailure = { error ->
                recordDebugEvent("auth", "Account token refresh failed error=${error.debugMessage()}")
            },
        )
        return latestSession.tokens.idToken ?: latestSession.tokens.accessToken
    }

    private fun refreshAccountConnectorsAfterLinking() {
        viewModelScope.launch {
            repeat(6) { attempt ->
                delay(if (attempt == 0) 5_000L else 10_000L)
                val session = state.value.authSession ?: return@launch
                refreshAccountConnectors(session)
            }
        }
    }

    fun disconnectAccountConnector(store: String) {
        val session = state.value.authSession ?: return
        viewModelScope.launch {
            val token = accountConnectorAuthToken(session)
            _state.update { it.copy(connectorActionStore = store, error = null) }
            runCatching { accountConnectorRepository.disconnect(store, token) }
                .onSuccess {
                    Toast.makeText(
                        getApplication(),
                        getApplication<Application>().getString(R.string.toast_store_disconnected),
                        Toast.LENGTH_SHORT,
                    ).show()
                    _state.update { it.copy(connectorActionStore = null) }
                    refreshAccountConnectors()
                }
                .onFailure { error ->
                    if (error is CancellationException) return@onFailure
                    val message = error.message
                        ?: getApplication<Application>().getString(R.string.error_store_disconnect_failed)
                    Toast.makeText(getApplication(), message, Toast.LENGTH_SHORT).show()
                    _state.update { it.copy(connectorActionStore = null, error = message) }
                }
        }
    }

    fun updateStreamSettings(transform: (StreamSettings) -> StreamSettings) {
        val snapshot = state.value
        settingsStore.update {
            it.copy(
                stream = transform(it.stream)
                    .withAndroidSettingsAvailability()
                    .withFpsAllowed(snapshot.subscriptionInfo, snapshot.authSession?.user?.membershipTier)
                    .withAndroidHdrCompatibility(androidTvProfile),
                streamPreset = StreamPreset.Custom,
            )
        }
    }

    fun applyStreamPreset(preset: StreamPreset) {
        val snapshot = state.value
        settingsStore.update { settings ->
            val presetStream = (if (preset == StreamPreset.Recommended) {
                (deviceRecommendation ?: recommendedAndroidStreamProfile(getApplication(), snapshot.codecReport)).stream
                    .withoutExperimentalTransportRequests()
            } else {
                settings.stream.applyingStreamPreset(preset)
            }).withUserStreamOptionsFrom(settings.stream)
            settings.copy(
                streamPreset = preset,
                stream = presetStream
                    .withAndroidSettingsAvailability()
                    .withResolutionAllowed(snapshot.subscriptionInfo, snapshot.authSession?.user?.membershipTier)
                    .withFpsAllowed(snapshot.subscriptionInfo, snapshot.authSession?.user?.membershipTier)
                    .withAndroidHdrCompatibility(androidTvProfile),
            )
        }
    }

    fun updateFavorites(gameId: String) {
        settingsStore.update {
            val next = if (gameId in it.favoriteGameIds) it.favoriteGameIds - gameId else it.favoriteGameIds + gameId
            it.copy(favoriteGameIds = next)
        }
    }

    fun setDefaultGameVariant(gameId: String, variantId: String?) {
        settingsStore.update {
            val next = it.defaultGameVariantIds.toMutableMap()
            if (variantId.isNullOrBlank()) {
                next.remove(gameId)
            } else {
                next[gameId] = variantId
            }
            it.copy(defaultGameVariantIds = next)
        }
    }

    fun dismissMembershipNotice() {
        _state.update { it.copy(pendingMembershipNotice = null) }
    }

    /** Launches anyway. The warning informs; it does not decide for the player. */
    fun continuePastMembershipNotice() {
        val pending = state.value.pendingMembershipNotice ?: return
        _state.update { it.copy(pendingMembershipNotice = null) }
        play(
            game = pending.game,
            streamingBaseUrlOverride = pending.streamingBaseUrlOverride,
            skipPrintedWaste = pending.skipPrintedWaste,
            skipStoreChoice = pending.skipStoreChoice,
            skipMembershipNotice = true,
        )
    }

    fun dismissStoreChoice() {
        _state.update { it.copy(pendingStoreChoiceGame = null) }
    }

    fun chooseStore(game: GameInfo) {
        val launchVariants = launchableGameVariants(game.variants)
        if (launchVariants.size > 1) {
            _state.update { it.copy(pendingStoreChoiceGame = game, selectedGame = null, error = null) }
        } else {
            play(game, skipStoreChoice = true)
        }
    }

    fun playVariant(game: GameInfo, variant: GameVariant) {
        _state.update { it.copy(pendingStoreChoiceGame = null) }
        play(game.withSelectedVariant(variant.id), skipStoreChoice = true)
    }

    fun play(
        game: GameInfo,
        streamingBaseUrlOverride: String? = null,
        skipPrintedWaste: Boolean = false,
        skipStoreChoice: Boolean = false,
        skipMembershipNotice: Boolean = false,
    ) {
        if (launchJob?.isActive == true) {
            recordDebugEvent("launch", "Ignored play request while another launch is active game=${game.title}")
            return
        }
        // Warn before launching rather than after: GFN accepts the session and then fails, or
        // silently downgrades, and from the player's side that is indistinguishable from a bug.
        if (!skipMembershipNotice) {
            val requirement = gameMembershipRequirement(
                game = game,
                subscriptionInfo = state.value.subscriptionInfo,
                fallbackMembershipTier = state.value.authSession?.user?.membershipTier,
            )
            if (requirement != null) {
                recordDebugEvent(
                    "launch",
                    "Membership gate game=${game.title} requires=${requirement.requiredPlanLabel} " +
                        "current=${requirement.currentPlanLabel}",
                )
                _state.update {
                    it.copy(
                        pendingMembershipNotice = PendingMembershipNotice(
                            game = game,
                            requirement = requirement,
                            streamingBaseUrlOverride = streamingBaseUrlOverride,
                            skipPrintedWaste = skipPrintedWaste,
                            skipStoreChoice = skipStoreChoice,
                        ),
                        selectedGame = null,
                        error = null,
                    )
                }
                return
            }
        }
        if (!skipStoreChoice) {
            val launchVariants = launchableGameVariants(game.variants)
            val defaultVariantId = state.value.settings.defaultGameVariantIds[game.id]
            val defaultVariant = launchVariants.firstOrNull { it.id == defaultVariantId }
            if (defaultVariant != null) {
                recordDebugEvent("launch", "Using default launcher ${gameStoreDisplayName(defaultVariant.store)} for ${game.title}")
                Toast.makeText(
                    getApplication(),
                    getApplication<Application>().getString(
                        R.string.store_selector_default_launch_notice,
                        gameStoreDisplayName(defaultVariant.store),
                    ),
                    Toast.LENGTH_SHORT,
                ).show()
                play(
                    game.withSelectedVariant(defaultVariant.id),
                    streamingBaseUrlOverride,
                    skipPrintedWaste,
                    skipStoreChoice = true,
                    skipMembershipNotice = true,
                )
                return
            }
            if (launchVariants.size > 1) {
                recordDebugEvent("launch", "Waiting for launcher choice game=${game.title} variants=${launchVariants.size}")
                _state.update { it.copy(pendingStoreChoiceGame = game, selectedGame = null, error = null) }
                return
            }
        }
        launchJob = viewModelScope.launch {
            val auth = state.value.authSession ?: run {
                recordDebugEvent("launch", "Play request ignored without an auth session game=${game.title}")
                return@launch
            }
            // Wait for active subscription fetch to finish so we have accurate membership info to allow resolutions
            activeSubscriptionJob?.join()
            val returnPage = state.value.page.takeUnless { it == AppPage.Stream } ?: state.value.streamReturnPage ?: AppPage.Home
            if (!skipPrintedWaste && streamingBaseUrlOverride == null && shouldUsePrintedWasteQueue(auth)) {
                recordDebugEvent("queue", "Opening PrintedWaste selector game=${game.title}")
                showPrintedWasteSelector(game)
                return@launch
            }
            awaitDeviceCapabilityProbe()
            val requestedSettings = streamSettingsBeforeDeviceAdjustment()
            val settings = requestedSettings.adjustedForDevice(state.value.codecReport)
            prepareSessionReport(
                gameTitle = game.title,
                selectedSettings = state.value.settings.stream,
                eligibleSettings = requestedSettings,
                initialSettings = settings,
            )
            if (settings != requestedSettings) {
                recordDebugEvent(
                    "launch",
                    "Adjusted stream settings requested=${requestedSettings.debugSummary()} effective=${settings.debugSummary()}",
                )
            }
            val token = auth.tokens.idToken ?: auth.tokens.accessToken
            val baseUrl = streamingBaseUrlOverride ?: effectiveStreamingBaseUrl()
            val manuallySelectedServer = manuallySelectedServerForReport(
                streamingBaseUrlOverride = streamingBaseUrlOverride,
                configuredRegion = requestedSettings.region,
            )
            pendingActiveSessionLaunch = null
            recordDebugEvent(
                "launch",
                "Starting launch game=${game.title} base=${hostForDebug(baseUrl)} settings=${settings.debugSummary()} override=${streamingBaseUrlOverride != null}",
            )
            recordQueuedGame(game)
            _state.update {
                it.copy(
                    streamStatus = "queue",
                    launchPhase = "Resolving game",
                    streamGame = game,
                    manuallySelectedServerForReport = manuallySelectedServer,
                    activeStreamSettings = settings,
                    streamInputModeAtLaunch = null,
                    selectedGame = null,
                    page = AppPage.Stream,
                    streamReturnPage = returnPage,
                    streamLaunchMinimized = false,
                    error = null,
                    queuePosition = null,
                    queueAdActiveId = null,
                    pendingStoreChoiceGame = null,
                    pendingPrintedWasteGame = null,
                    activeSessionDecision = null,
                    printedWasteError = null,
                    printedWastePings = emptyMap(),
                    sessionReport = null,
                )
            }
            runCatching {
                val requestedVariantId = game.variants.getOrNull(game.selectedVariantIndex)?.id
                    ?: game.variants.firstOrNull()?.id
                var launchGame = game
                var selectedVariant = launchGame.variants.firstOrNull { it.id == requestedVariantId }
                    ?: launchGame.variants.getOrNull(launchGame.selectedVariantIndex)
                    ?: launchGame.variants.firstOrNull()
                _state.update { it.copy(launchPhase = "Refreshing game access") }
                runCatching {
                    catalogRepository.hydrateGameForLaunch(token, baseUrl, launchGame, selectedVariant)
                }.onSuccess { hydrated ->
                    launchGame = hydrated
                    selectedVariant = launchGame.variants.firstOrNull { it.id == requestedVariantId }
                        ?: launchGame.variants.getOrNull(launchGame.selectedVariantIndex)
                        ?: launchGame.variants.firstOrNull()
                }.onFailure { error ->
                    recordDebugEvent("launch", "Game access refresh failed; using cached metadata error=${error.debugMessage()}")
                }
                if (shouldMarkVariantOwnedBeforeLaunch(selectedVariant)) {
                    val unownedVariant = checkNotNull(selectedVariant)
                    _state.update { it.copy(launchPhase = "Marking game as owned") }
                    catalogRepository.addOwnedVariant(token, unownedVariant.id)
                    launchGame = launchGame.withManuallyOwnedVariant(unownedVariant.id)
                    selectedVariant = launchGame.variants.firstOrNull { it.id == unownedVariant.id }
                    recordDebugEvent("launch", "Marked variant as owned in GFN library variant=${unownedVariant.id}")
                }
                val accountLinked = shouldSendAccountLinked(launchGame, selectedVariant)
                _state.update { it.copy(launchPhase = "Resolving game", streamGame = launchGame) }
                val candidateId = selectedVariant?.id ?: launchGame.launchAppId ?: launchGame.uuid ?: launchGame.id
                val launchAppId = candidateId.takeIf { it.all(Char::isDigit) }
                    ?: launchGame.launchAppId?.takeIf { it.all(Char::isDigit) }
                    ?: catalogRepository.resolveLaunchAppId(token, candidateId, baseUrl)
                    ?: error("Could not resolve numeric appId for ${launchGame.title}")
                recordDebugEvent("launch", "Resolved appId=$launchAppId candidate=$candidateId game=${launchGame.title}")

                _state.update { it.copy(launchPhase = "Checking active sessions") }
                val active = sessionRepository.getActiveSessions(token, baseUrl, settings)
                recordDebugEvent("queue", "Active sessions checked count=${active.size} ${active.joinToString(limit = 4) { it.debugSummary() }}")
                val numericLaunchAppId = launchAppId.toIntOrNull()
                val activeConflict = activeSessionLaunchConflict(active, numericLaunchAppId, settings)
                if (activeConflict != null) {
                    pendingActiveSessionLaunch = PendingActiveSessionLaunch(
                        game = launchGame,
                        launchAppId = launchAppId,
                        baseUrl = baseUrl,
                        settings = settings,
                        accountLinked = accountLinked,
                        activeSession = activeConflict,
                        returnPage = returnPage,
                    )
                    recordDebugEvent("queue", "Active session decision required ${activeConflict.debugSummary()} requestedApp=$launchAppId")
                    _state.update {
                        it.copy(
                            activeSession = activeConflict,
                            activeSessionDecision = ActiveSessionDecision(
                                activeSession = activeConflict,
                                requestedGameTitle = launchGame.title,
                            ),
                            streamSession = null,
                            launchPhase = "Active session found",
                            queuePosition = activeConflict.queuePosition,
                            queueAdActiveId = null,
                        )
                    }
                    return@runCatching null
                }
                _state.update { it.copy(launchPhase = "Creating session") }
                val created = sessionRepository.createSession(
                    token = token,
                    streamingBaseUrl = baseUrl,
                    appId = launchAppId,
                    internalTitle = launchGame.title,
                    zone = "prod",
                    settings = settings,
                    accountLinked = accountLinked,
                    appLaunchMode = appLaunchModeFor(launchGame, settings),
                )
                recordDebugEvent("queue", "Created session ${created.debugSummary()}")
                pollUntilReady(token, created, settings)
            }.onSuccess { readySession ->
                if (readySession == null) return@onSuccess
                markSessionReadyForNativeStream(readySession, settings)
            }.onFailure { error ->
                if (error is CancellationException) return@onFailure
                recordDebugEvent("launch", "Launch failed game=${game.title} error=${error.debugMessage()}")
                val returnPage = state.value.streamReturnPage ?: AppPage.Home
                _state.update {
                    it.copy(
                        error = normalizeLaunchError(error, game.title),
                        streamStatus = "idle",
                        activeStreamSettings = null,
                        streamReturnPage = null,
                        launchPhase = "",
                        streamLaunchMinimized = false,
                        queuePosition = null,
                        queueAdActiveId = null,
                        pendingStoreChoiceGame = null,
                        activeSessionDecision = null,
                        page = returnPage,
                    )
                }
            }
        }
    }

    private fun recordQueuedGame(game: GameInfo) {
        val next = queuedGameStore.record(gameTrackingKey(game))
        _state.update { it.copy(queuedGameKeys = next) }
    }

    private fun markSessionReadyForNativeStream(readySession: SessionInfo, settings: StreamSettings) {
        val anchoredSession = readySession.withSessionTimerAnchor()
        if (streamReportLaunchProfile == null) {
            prepareSessionReport(
                gameTitle = state.value.streamGame?.title.orEmpty(),
                selectedSettings = state.value.settings.stream,
                eligibleSettings = settings,
                initialSettings = settings,
            )
        }
        recordDebugEvent("stream", "Session ready for native stream ${anchoredSession.debugSummary()}")
        _state.update {
            it.copy(
                streamSession = anchoredSession,
                activeStreamSettings = settings,
                streamStatus = "connecting",
                launchPhase = "Connecting stream",
                streamLaunchMinimized = false,
                queuePosition = null,
                queueAdActiveId = null,
                activeSessionDecision = null,
                page = AppPage.Stream,
            )
        }
    }

    private fun prepareSessionReport(
        gameTitle: String,
        selectedSettings: StreamSettings,
        eligibleSettings: StreamSettings,
        initialSettings: StreamSettings,
    ) {
        streamReportLaunchProfile = StreamReportLaunchProfile(
            gameTitle = gameTitle,
            selectedSettings = selectedSettings,
            eligibleSettings = eligibleSettings,
            initialSettings = initialSettings,
        )
        streamSessionReportAccumulator = null
        streamRuntimeDiagnosticsSampler.reset()
        sessionReportFinalizedForStop = false
    }

    private fun ensureSessionReportAccumulator(nowMs: Long = System.currentTimeMillis()) {
        if (sessionReportFinalizedForStop || streamSessionReportAccumulator != null) return
        val snapshot = state.value
        if (snapshot.streamSession == null || snapshot.streamStatus !in setOf("connecting", "streaming")) return
        val initialSettings = snapshot.activeStreamSettings ?: return
        val profile = streamReportLaunchProfile ?: StreamReportLaunchProfile(
            gameTitle = snapshot.streamGame?.title.orEmpty(),
            selectedSettings = snapshot.settings.stream,
            eligibleSettings = initialSettings,
            initialSettings = initialSettings,
        ).also { streamReportLaunchProfile = it }
        streamSessionReportAccumulator = StreamSessionReportAccumulator(profile, startedAtMs = nowMs)
        streamRuntimeDiagnosticsSampler.reset()
    }

    private fun finishSessionReport(nowMs: Long = System.currentTimeMillis()): SessionReport? {
        if (sessionReportFinalizedForStop) return null
        sessionReportFinalizedForStop = true
        val report = streamSessionReportAccumulator?.finish(nowMs)
        if (report != null) {
            recordDebugEvent(
                "stream",
                "Session report score=${report.score} samples=${report.sampleCount} " +
                    "ping=${report.averagePingMs ?: -1} loss=${report.packetLossPct ?: -1.0} " +
                    "bitrate=${report.averageBitrateKbps ?: -1}",
            )
        }
        streamSessionReportAccumulator = null
        streamReportLaunchProfile = null
        streamRuntimeDiagnosticsSampler.reset()
        return report
    }

    fun stopStream() {
        val beforeStop = state.value
        streamSessionRecoveryTracker.reset()
        val completedSessionReport = finishSessionReport()
        val shouldShowCompletedSessionReport = beforeStop.settings.showSessionReportAfterStream
        recordDebugEvent(
            "stream",
            "Stop requested status=${beforeStop.streamStatus} session=${beforeStop.streamSession?.shortDebugId().orEmpty()} game=${beforeStop.streamGame?.title.orEmpty()}",
        )
        launchJob?.cancel()
        launchJob = null
        pendingActiveSessionLaunch = null
        viewModelScope.launch {
            val auth = state.value.authSession
            val snapshot = state.value
            val returnPage = snapshot.streamReturnPage ?: AppPage.Home
            val session = snapshot.streamSession
            val streamSettings = snapshot.activeStreamSettings ?: effectiveStreamSettings()
            if (auth != null && session != null) {
                runCatching { sessionRepository.stopSession(auth.tokens.idToken ?: auth.tokens.accessToken, session, streamSettings) }
                    .onSuccess {
                        sessionTimerAnchorStore.clear(session.sessionId)
                        recordDebugEvent("stream", "Stopped cloud session ${session.shortDebugId()}")
                    }
                    .onFailure { error -> recordDebugEvent("stream", "Failed to stop cloud session ${session.shortDebugId()} error=${error.debugMessage()}") }
            } else if (auth != null) {
                val token = auth.tokens.idToken ?: auth.tokens.accessToken
                val active = snapshot.activeSession
                    ?: runCatching {
                        sessionRepository.getActiveSessions(token, effectiveStreamingBaseUrl(auth), streamSettings)
                            .firstOrNull { it.status in setOf(1, 2, 3) }
                    }.getOrNull()
                if (active != null) {
                    runCatching { sessionRepository.stopActiveSession(token, active, streamSettings) }
                        .onSuccess {
                            sessionTimerAnchorStore.clear(active.sessionId)
                            recordDebugEvent("stream", "Stopped active session ${active.shortDebugId()}")
                        }
                        .onFailure { error -> recordDebugEvent("stream", "Failed to stop active session ${active.shortDebugId()} error=${error.debugMessage()}") }
                } else {
                    recordDebugEvent("stream", "No cloud session found to stop")
                }
            }
            _state.update {
                it.copy(
                    streamSession = null,
                    activeStreamSettings = null,
                    streamInputModeAtLaunch = null,
                    streamGame = null,
                    streamStatus = "idle",
                    streamLaunchMinimized = false,
                    streamReturnPage = null,
                    launchPhase = "",
                    queuePosition = null,
                    queueAdActiveId = null,
                    pendingStoreChoiceGame = null,
                    activeSessionDecision = null,
                    page = returnPage,
                    sessionReport = if (shouldShowCompletedSessionReport) {
                        completedSessionReport ?: it.sessionReport
                    } else {
                        null
                    },
                )
            }
            refreshActiveSession()
            recordDebugEvent("stream", "Stream state reset returnPage=$returnPage")
        }
    }

    fun refreshPrintedWasteQueues() {
        val game = state.value.pendingPrintedWasteGame ?: return
        recordDebugEvent("queue", "Refreshing PrintedWaste queues game=${game.title}")
        viewModelScope.launch {
            loadPrintedWasteQueue(game)
        }
    }

    fun minimizeStreamLaunch() {
        recordDebugEvent("queue", "Minimize launch requested status=${state.value.streamStatus} phase=${state.value.launchPhase}")
        _state.update { current ->
            if (!canMinimizeStreamLaunch(current.streamStatus, current.streamSession?.isReadyForStream() == true)) {
                current
            } else {
                current.copy(streamLaunchMinimized = true, page = current.streamReturnPage ?: AppPage.Home)
            }
        }
    }

    fun restoreStreamLaunch() {
        recordDebugEvent("queue", "Restore launch requested status=${state.value.streamStatus} phase=${state.value.launchPhase}")
        _state.update { current ->
            if (current.streamStatus == "idle") current else current.copy(streamLaunchMinimized = false, page = AppPage.Stream)
        }
    }

    fun dismissActiveSessionDecision() {
        val pending = pendingActiveSessionLaunch
        recordDebugEvent("queue", "Active session decision dismissed session=${pending?.activeSession?.shortDebugId().orEmpty()}")
        pendingActiveSessionLaunch = null
        val returnPage = pending?.returnPage ?: state.value.streamReturnPage ?: AppPage.Home
        _state.update {
            it.copy(
                streamStatus = "idle",
                activeStreamSettings = null,
                streamInputModeAtLaunch = null,
                streamGame = null,
                streamSession = null,
                activeSessionDecision = null,
                streamReturnPage = null,
                launchPhase = "",
                streamLaunchMinimized = false,
                queuePosition = null,
                queueAdActiveId = null,
                page = returnPage,
            )
        }
    }

    fun terminateActiveSessionAndStartNew() {
        if (launchJob?.isActive == true) {
            recordDebugEvent("queue", "Ignored replace active session request while another launch is active")
            return
        }
        val pending = pendingActiveSessionLaunch ?: run {
            recordDebugEvent("queue", "Replace active session ignored without pending launch")
            return
        }
        pendingActiveSessionLaunch = null
        recordDebugEvent("queue", "Replace active session requested active=${pending.activeSession.debugSummary()} game=${pending.game.title}")
        launchJob = viewModelScope.launch {
            val auth = state.value.authSession ?: run {
                recordDebugEvent("queue", "Replace active session ignored without an auth session")
                return@launch
            }
            val token = auth.tokens.idToken ?: auth.tokens.accessToken
            _state.update {
                it.copy(
                    streamStatus = "queue",
                    launchPhase = "Ending active session",
                    activeSession = pending.activeSession,
                    activeSessionDecision = null,
                    streamSession = null,
                    streamGame = pending.game,
                    activeStreamSettings = pending.settings,
                    page = AppPage.Stream,
                    streamReturnPage = pending.returnPage,
                    streamLaunchMinimized = false,
                    error = null,
                    queuePosition = null,
                    queueAdActiveId = null,
                    sessionReport = null,
                )
            }
            runCatching {
                runCatching { sessionRepository.stopActiveSession(token, pending.activeSession, pending.settings) }
                    .onSuccess {
                        sessionTimerAnchorStore.clear(pending.activeSession.sessionId)
                        recordDebugEvent("queue", "Stopped active session before new launch ${pending.activeSession.shortDebugId()}")
                    }
                    .onFailure { error -> recordDebugEvent("queue", "Failed to stop active session before new launch ${pending.activeSession.shortDebugId()} error=${error.debugMessage()}") }
                _state.update { it.copy(activeSession = null, launchPhase = "Creating session") }
                val created = sessionRepository.createSession(
                    token = token,
                    streamingBaseUrl = pending.baseUrl,
                    appId = pending.launchAppId,
                    internalTitle = pending.game.title,
                    zone = "prod",
                    settings = pending.settings,
                    accountLinked = pending.accountLinked,
                    appLaunchMode = appLaunchModeFor(pending.game, pending.settings),
                )
                recordDebugEvent("queue", "Created replacement session ${created.debugSummary()}")
                pollUntilReady(token, created, pending.settings)
            }.onSuccess { readySession ->
                markSessionReadyForNativeStream(readySession, pending.settings)
            }.onFailure { error ->
                if (error is CancellationException) return@onFailure
                recordDebugEvent("launch", "Replace active session launch failed game=${pending.game.title} error=${error.debugMessage()}")
                val returnPage = state.value.streamReturnPage ?: pending.returnPage
                _state.update {
                    it.copy(
                        error = normalizeLaunchError(error, pending.game.title),
                        streamStatus = "idle",
                        activeStreamSettings = null,
                        streamReturnPage = null,
                        activeSessionDecision = null,
                        launchPhase = "",
                        streamLaunchMinimized = false,
                        queuePosition = null,
                        queueAdActiveId = null,
                        pendingStoreChoiceGame = null,
                        page = returnPage,
                    )
                }
            }
        }
    }

    fun resumeActiveSession() {
        if (launchJob?.isActive == true) {
            recordDebugEvent("queue", "Ignored resume request while another launch is active")
            return
        }
        pendingActiveSessionLaunch?.let { pending ->
            resumePendingActiveSession(pending)
            return
        }
        recordDebugEvent("queue", "Resume active session requested cached=${state.value.activeSession?.debugSummary().orEmpty()}")
        launchJob = viewModelScope.launch {
            val auth = state.value.authSession ?: run {
                recordDebugEvent("queue", "Resume ignored without an auth session")
                return@launch
            }
            val settings = effectiveStreamSettings()
            val token = auth.tokens.idToken ?: auth.tokens.accessToken
            val baseUrl = effectiveStreamingBaseUrl(auth)
            val cachedActive = state.value.activeSession
            val returnPage = state.value.page.takeUnless { it == AppPage.Stream } ?: state.value.streamReturnPage ?: AppPage.Home
            _state.update {
                it.copy(
                    streamStatus = "queue",
                    launchPhase = "Checking active sessions",
                    activeStreamSettings = settings,
                    page = AppPage.Stream,
                    streamReturnPage = returnPage,
                    streamLaunchMinimized = false,
                    selectedGame = null,
                    pendingStoreChoiceGame = null,
                    pendingPrintedWasteGame = null,
                    activeSessionDecision = null,
                    error = null,
                    queuePosition = null,
                    queueAdActiveId = null,
                    sessionReport = null,
                )
            }
            runCatching {
                val active = cachedActive ?: sessionRepository.getActiveSessions(token, baseUrl, settings)
                    .let { activeSessionLaunchConflict(it, launchAppId = null, settings = settings) }
                    ?: error("No active cloud session was found. Start a game to create a new one.")
                val resumeSettings = resumeSettingsForActiveSession(active, settings)
                prepareSessionReport(
                    gameTitle = gameForActiveSession(active)?.title.orEmpty(),
                    selectedSettings = state.value.settings.stream,
                    eligibleSettings = streamSettingsBeforeDeviceAdjustment(),
                    initialSettings = resumeSettings,
                )
                recordDebugEvent("queue", "Resume found active ${active.debugSummary()} base=${hostForDebug(baseUrl)} settings=${resumeSettings.debugSummary()}")
                val matchingGame = gameForActiveSession(active)
                _state.update {
                    it.copy(
                        activeSession = active,
                        streamGame = matchingGame,
                        streamSession = active.toPendingSession(zone = "prod"),
                        activeStreamSettings = resumeSettings,
                        launchPhase = if (active.isReadyForClaim()) "Resuming session" else loadingPhaseFor(active.toPendingSession(zone = "prod")),
                    )
                }
                resumeKnownActiveSession(token, active, resumeSettings, baseUrl)
            }.onSuccess { readySession ->
                recordDebugEvent("stream", "Resume ready for native stream ${readySession.debugSummary()}")
                markSessionReadyForNativeStream(readySession, state.value.activeStreamSettings ?: settings)
            }.onFailure { error ->
                if (error is CancellationException) return@onFailure
                recordDebugEvent("queue", "Resume failed error=${error.debugMessage()}")
                val returnPage = state.value.streamReturnPage ?: AppPage.Home
                _state.update {
                    it.copy(
                        error = normalizeLaunchError(error, state.value.streamGame?.title),
                        streamStatus = "idle",
                        activeStreamSettings = null,
                        streamReturnPage = null,
                        launchPhase = "",
                        streamLaunchMinimized = false,
                        queuePosition = null,
                        queueAdActiveId = null,
                        pendingStoreChoiceGame = null,
                        pendingPrintedWasteGame = null,
                        activeSessionDecision = null,
                        page = returnPage,
                    )
                }
            }
        }
    }

    private fun resumePendingActiveSession(pending: PendingActiveSessionLaunch) {
        pendingActiveSessionLaunch = null
        recordDebugEvent("queue", "Resume pending active session requested active=${pending.activeSession.debugSummary()} requestedGame=${pending.game.title}")
        launchJob = viewModelScope.launch {
            val auth = state.value.authSession ?: run {
                recordDebugEvent("queue", "Pending resume ignored without an auth session")
                return@launch
            }
            val token = auth.tokens.idToken ?: auth.tokens.accessToken
            val resumeSettings = resumeSettingsForActiveSession(pending.activeSession, pending.settings)
            val pendingSession = pending.activeSession.toPendingSession(zone = "prod")
            prepareSessionReport(
                gameTitle = gameForActiveSession(pending.activeSession)?.title ?: pending.game.title,
                selectedSettings = state.value.settings.stream,
                eligibleSettings = streamSettingsBeforeDeviceAdjustment(),
                initialSettings = resumeSettings,
            )
            _state.update {
                it.copy(
                    streamStatus = "queue",
                    launchPhase = if (pending.activeSession.isReadyForClaim()) "Resuming session" else loadingPhaseFor(pendingSession),
                    activeSession = pending.activeSession,
                    activeSessionDecision = null,
                    streamSession = pendingSession,
                    streamGame = gameForActiveSession(pending.activeSession) ?: pending.game.takeIf { pending.activeSession.appId == pending.launchAppId.toIntOrNull() },
                    activeStreamSettings = resumeSettings,
                    page = AppPage.Stream,
                    streamReturnPage = pending.returnPage,
                    streamLaunchMinimized = false,
                    error = null,
                    queuePosition = queueDisplayPosition(pendingSession),
                    queueAdActiveId = null,
                    sessionReport = null,
                )
            }
            runCatching {
                resumeKnownActiveSession(token, pending.activeSession, resumeSettings, pending.baseUrl)
            }.onSuccess { readySession ->
                recordDebugEvent("stream", "Pending resume ready for native stream ${readySession.debugSummary()}")
                markSessionReadyForNativeStream(readySession, resumeSettings)
            }.onFailure { error ->
                if (error is CancellationException) return@onFailure
                recordDebugEvent("queue", "Pending resume failed error=${error.debugMessage()}")
                _state.update {
                    it.copy(
                        error = normalizeLaunchError(error, pending.game.title),
                        streamStatus = "idle",
                        activeStreamSettings = null,
                        streamReturnPage = null,
                        activeSessionDecision = null,
                        launchPhase = "",
                        streamLaunchMinimized = false,
                        queuePosition = null,
                        queueAdActiveId = null,
                        pendingStoreChoiceGame = null,
                        pendingPrintedWasteGame = null,
                        page = pending.returnPage,
                    )
                }
            }
        }
    }

    fun launchWithPrintedWaste(zoneUrl: String?) {
        val game = state.value.pendingPrintedWasteGame ?: return
        recordDebugEvent("queue", "PrintedWaste selection game=${game.title} zone=${hostForDebug(zoneUrl)} auto=${zoneUrl == null}")
        launchJob?.cancel()
        launchJob = null
        _state.update {
            it.copy(
                pendingPrintedWasteGame = null,
                printedWasteError = null,
                printedWasteLoading = false,
            )
        }
        play(game, streamingBaseUrlOverride = zoneUrl, skipPrintedWaste = true, skipStoreChoice = true)
    }

    private fun effectiveStreamSettings(): StreamSettings {
        return streamSettingsBeforeDeviceAdjustment().adjustedForDevice(state.value.codecReport)
            .withHdrDeviceSupport(getApplication())
    }

    private fun resumeSettingsForActiveSession(active: ActiveSessionInfo, requested: StreamSettings): StreamSettings {
        val resolution = active.resolution?.takeIf { parseResolutionPixelsOrNull(it) != null }
        return requested
            .let { settings ->
                if (resolution == null) {
                    settings
                } else {
                    settings.copy(
                        resolution = resolution,
                        aspectRatio = streamAspectRatioForResolution(resolution) ?: settings.aspectRatio,
                    )
                }
            }
            .let { settings -> active.fps?.takeIf { it > 0 }?.let { settings.copy(fps = it) } ?: settings }
            .withCodecColorCompatibility()
    }

    private fun gameForActiveSession(active: ActiveSessionInfo): GameInfo? =
        (state.value.games + state.value.libraryGames)
            .firstOrNull { game ->
                game.launchAppId == active.appId.toString() ||
                    game.variants.any { variant -> variant.id == active.appId.toString() }
            }

    private fun streamSettingsBeforeDeviceAdjustment(): StreamSettings {
        val snapshot = state.value
        return snapshot.settings.stream.eligibleForAndroidLaunch(
            subscriptionInfo = snapshot.subscriptionInfo,
            fallbackMembershipTier = snapshot.authSession?.user?.membershipTier,
            androidTvProfile = androidTvProfile,
        )
    }

    private suspend fun resolveFallbackLaunchAppId(
        token: String,
        game: GameInfo?,
        active: ActiveSessionInfo?,
        baseUrl: String,
    ): String {
        if (game == null) {
            return active?.appId?.takeIf { it > 0 }?.toString()
                ?: error("Could not resolve appId for safe H264 retry.")
        }
        val selectedVariant = game.variants.getOrNull(game.selectedVariantIndex) ?: game.variants.firstOrNull()
        val candidateId = selectedVariant?.id ?: game.launchAppId ?: game.uuid ?: game.id
        return candidateId.takeIf { it.all(Char::isDigit) }
            ?: game.launchAppId?.takeIf { it.all(Char::isDigit) }
            ?: active?.appId?.takeIf { it > 0 }?.toString()
            ?: catalogRepository.resolveLaunchAppId(token, candidateId, baseUrl)
            ?: error("Could not resolve numeric appId for ${game.title}")
    }

    private fun String.isLikelyDirectServerUrl(): Boolean {
        return isLikelyDirectSessionServerUrl(this)
    }

    fun dismissPrintedWasteSelector() {
        _state.update {
            it.copy(
                pendingPrintedWasteGame = null,
                printedWasteLoading = false,
                printedWasteError = null,
            )
        }
    }

    fun reportQueueAd(
        adId: String,
        action: String,
        watchedTimeInMs: Long? = null,
        pausedTimeInMs: Long? = null,
        cancelReason: String? = null,
        errorInfo: String? = null,
    ) {
        viewModelScope.launch {
            val auth = state.value.authSession ?: run {
                recordDebugEvent("ad", "Ignoring ad report without auth ad=${shortDebugId(adId)} action=$action")
                return@launch
            }
            val session = state.value.streamSession ?: run {
                recordDebugEvent("ad", "Ignoring ad report without session ad=${shortDebugId(adId)} action=$action")
                return@launch
            }
            val normalizedAction = action.lowercase()
            val isTerminalAction = normalizedAction == "finish" || normalizedAction == "cancel"
            recordDebugEvent(
                "ad",
                "Report action=$normalizedAction ad=${shortDebugId(adId)} session=${session.shortDebugId()} watched=${watchedTimeInMs ?: 0} reason=${cancelReason.orEmpty()}",
            )
            if (!isTerminalAction) {
                _state.update {
                    it.copy(queueAdActiveId = adId)
                }
            }
            runCatching {
                queueAdReportMutex.withLock {
                    val reportSession = state.value.streamSession
                        ?.takeIf { it.sessionId == session.sessionId }
                        ?: session
                    if (isTerminalAction) {
                        val nextAdId = nextSessionAdId(reportSession.adState, adId)
                        _state.update { current ->
                            val currentSession = current.streamSession
                            if (currentSession?.sessionId == reportSession.sessionId) {
                                current.copy(
                                    streamSession = removeSessionAdItem(currentSession, adId),
                                    queueAdActiveId = nextAdId,
                                )
                            } else {
                                current
                            }
                        }
                    }
                    sessionRepository.reportSessionAd(
                        token = auth.tokens.idToken ?: auth.tokens.accessToken,
                        session = reportSession,
                        adId = adId,
                        action = normalizedAction,
                        settings = state.value.settings.stream,
                        watchedTimeInMs = watchedTimeInMs,
                        pausedTimeInMs = pausedTimeInMs ?: 0L,
                        cancelReason = cancelReason,
                        errorInfo = errorInfo,
                    )
                }
            }.onSuccess { updated ->
                recordDebugEvent("ad", "Report accepted action=$normalizedAction updated=${updated.debugSummary()}")
                _state.update { current ->
                    val previous = current.streamSession?.takeIf { it.sessionId == updated.sessionId } ?: session
                    val merged = mergeQueueAdReportResult(
                        previous = previous,
                        updated = updated,
                        adId = adId,
                        terminalAction = isTerminalAction,
                    )
                    current.copy(
                        streamSession = merged,
                        queuePosition = queueDisplayPosition(merged),
                        queueAdActiveId = chooseQueueAdActiveId(current.queueAdActiveId, merged),
                    )
                }
            }.onFailure { error ->
                recordDebugEvent("ad", "Report failed action=$normalizedAction ad=${shortDebugId(adId)} error=${error.debugMessage()}")
                _state.update { current ->
                    val currentSession = current.streamSession
                    if (normalizedAction == "finish" && currentSession?.adState != null) {
                        current.copy(
                            streamSession = currentSession.copy(
                                adState = currentSession.adState.copy(
                                    sessionAds = emptyList(),
                                    ads = emptyList(),
                                    serverSentEmptyAds = false,
                                ),
                            ),
                            queueAdActiveId = null,
                        )
                    } else {
                        current.copy(error = error.message ?: "Queue ad update failed")
                    }
                }
            }
        }
    }

    fun markStreamConnected() {
        ensureSessionReportAccumulator()
        if (state.value.streamStatus == "streaming") return
        recordDebugEvent("stream", "Native stream connected session=${state.value.streamSession?.shortDebugId().orEmpty()} game=${state.value.streamGame?.title.orEmpty()}")
        _state.update { it.copy(streamStatus = "streaming", launchPhase = "") }
    }

    fun setAndroidPictureInPictureActive(active: Boolean) {
        _state.update { current ->
            if (current.androidPictureInPictureActive == active) current else current.copy(androidPictureInPictureActive = active)
        }
    }

    fun updateStreamRuntimeStats(stats: StreamRuntimeStats) {
        if (!stats.hasDebugValues()) return
        val now = System.currentTimeMillis()
        ensureSessionReportAccumulator(now)
        val accumulator = streamSessionReportAccumulator
        // Record every media sample immediately. Binder diagnostics must not delay input/UI work
        // or reorder the consecutive samples used to detect decoder overload.
        accumulator?.record(stats)
        val sessionId = state.value.streamSession?.sessionId
        latestStreamRuntimeStats = TimedStreamRuntimeStats(
            capturedAtMs = now,
            sessionId = sessionId,
            stats = stats,
        )
        if (accumulator == null || sessionId == null) return
        streamRuntimeDiagnosticsSampler.sampleIfDue { diagnostics, includeDevice ->
            // A completed sample belongs only to the session/report that requested it.
            if (state.value.streamSession?.sessionId != sessionId || streamSessionReportAccumulator !== accumulator) {
                return@sampleIfDue
            }
            accumulator.recordNetwork(diagnostics)
            if (includeDevice) {
                val requestedSettings = streamSettingsBeforeDeviceAdjustment()
                val transportSettings = state.value.activeStreamSettings ?: requestedSettings
                recordDebugEvent(
                    "runtime",
                    "stats requestedMaxBitrateMbps=${requestedSettings.maxBitrateMbps} transportMaxBitrateMbps=${transportSettings.maxBitrateMbps} " +
                        "${stats.debugSummary()} device=${diagnostics.debugSummary()}",
                )
            }
        }
    }

    fun markStreamError(message: String) {
        recordDebugEvent("stream", "Native stream error message=${message.take(DEBUG_EVENT_MESSAGE_LIMIT)} session=${state.value.streamSession?.shortDebugId().orEmpty()}")
        _state.update { it.copy(error = message, streamStatus = "idle", activeStreamSettings = null, launchPhase = "") }
    }

    fun recordNativeStreamState(message: String) {
        recordDebugEvent("native", "state=$message session=${state.value.streamSession?.shortDebugId().orEmpty()}")
    }

    fun recordLocalVideoTransportFallback(reason: String, fallbackSettings: StreamSettings) {
        ensureSessionReportAccumulator()
        val currentSettings = state.value.activeStreamSettings ?: effectiveStreamSettings()
        _state.update { current ->
            if (current.streamSession == null || current.streamStatus == "idle") {
                current
            } else {
                current.copy(activeStreamSettings = fallbackSettings)
            }
        }
        streamSessionReportAccumulator?.recordRecovery(reason, fallbackSettings)
        recordDebugEvent(
            "recovery",
            "Restarted local transport with codec fallback while keeping cloud session reason=${reason.take(DEBUG_EVENT_MESSAGE_LIMIT)} current=${currentSettings.debugSummary()} fallback=${fallbackSettings.debugSummary()}",
        )
    }

    internal fun recordActiveStreamMode(status: ActiveStreamModeStatus) {
        ensureSessionReportAccumulator()
        streamSessionReportAccumulator?.recordActiveMode(status)
        val current = state.value
        val currentSettings = current.activeStreamSettings ?: effectiveStreamSettings()
        val noticeKey = listOf(
            current.streamGame?.id ?: current.activeSession?.appId?.toString() ?: current.streamSession?.sessionId.orEmpty(),
            streamSettingsSessionSignature(currentSettings),
            status.displayedResolution,
            status.requestedResolution,
            status.serverNegotiatedResolution.orEmpty(),
            status.serverFinalSelectedResolution.orEmpty(),
            status.resolutionSource?.name.orEmpty(),
            status.safeVideoRecoveryActive.toString(),
            status.transportCodec.name,
        ).joinToString("|")
        if (!runtimeResolutionNoticeKeys.add(noticeKey)) return
        val resolutionSource = when (status.resolutionSource) {
            StreamResolutionChangeSource.ServerNegotiatedFallback -> "Server negotiated fallback"
            StreamResolutionChangeSource.ProviderOrGameModeChange -> "Provider/game runtime mode changed"
            null -> "Client transport profile changed"
        }
        val recovery = if (status.safeVideoRecoveryActive) {
            " clientRecovery=safe-${status.transportCodec.name}"
        } else {
            ""
        }
        recordDebugEvent(
            "stream",
            "$resolutionSource displayed=${status.displayedResolution} requested=${status.requestedResolution} " +
                "server=${status.serverNegotiatedResolution.orEmpty()} final=${status.serverFinalSelectedResolution.orEmpty()}" +
                "$recovery; keeping connected transport=${currentSettings.debugSummary()}",
        )
        if (status.resolutionSource != null) {
            refreshRuntimeSessionSnapshot(status)
        }
    }

    private fun refreshRuntimeSessionSnapshot(observedMode: ActiveStreamModeStatus) {
        val initial = state.value
        val auth = initial.authSession ?: return
        val session = initial.streamSession ?: return
        val settings = initial.activeStreamSettings ?: effectiveStreamSettings()
        viewModelScope.launch {
            val latest = runCatching {
                sessionRepository.pollSession(
                    token = auth.tokens.idToken ?: auth.tokens.accessToken,
                    streamingBaseUrl = session.streamingBaseUrl ?: effectiveStreamingBaseUrl(auth),
                    serverIp = session.serverIp,
                    zone = session.zone,
                    sessionId = session.sessionId,
                    clientId = session.clientId,
                    deviceId = session.deviceId,
                    settings = settings,
                )
            }.getOrElse { error ->
                if (error is CancellationException) throw error
                recordDebugEvent(
                    "stream",
                    "Runtime mode server snapshot failed session=${session.shortDebugId()} " +
                        "displayed=${observedMode.displayedResolution} error=${error.debugMessage()}",
                )
                return@launch
            }
            if (state.value.streamSession?.sessionId != session.sessionId) return@launch
            recordDebugEvent(
                "stream",
                "Runtime mode server snapshot session=${session.shortDebugId()} status=${latest.status} " +
                    "source=${observedMode.resolutionSource?.name.orEmpty()} displayed=${observedMode.displayedResolution} " +
                    "${latest.monitorSnapshot?.debugSummary().orEmpty()}",
            )
            _state.update { current ->
                val currentSession = current.streamSession
                if (currentSession?.sessionId != session.sessionId) {
                    current
                } else {
                    current.copy(
                        streamSession = currentSession.copy(
                            status = latest.status,
                            negotiatedStreamProfile = latest.negotiatedStreamProfile,
                            monitorSnapshot = latest.monitorSnapshot,
                            requestedStreamingFeatures = latest.requestedStreamingFeatures,
                            finalizedStreamingFeatures = latest.finalizedStreamingFeatures,
                        ),
                    )
                }
            }
        }
    }

    fun recoverStreamSession(reason: String) {
        if (launchJob?.isActive == true) {
            recordDebugEvent("recovery", "Ignored stream recovery while launch job is active reason=${reason.take(DEBUG_EVENT_MESSAGE_LIMIT)}")
            return
        }
        val initial = state.value
        val auth = initial.authSession ?: run {
            recordDebugEvent("recovery", "Recovery missing auth reason=${reason.take(DEBUG_EVENT_MESSAGE_LIMIT)}")
            markStreamError(reason)
            return
        }
        val initialSession = initial.streamSession ?: run {
            recordDebugEvent("recovery", "Recovery missing stream session reason=${reason.take(DEBUG_EVENT_MESSAGE_LIMIT)}")
            markStreamError(reason)
            return
        }
        val currentSettings = initial.activeStreamSettings ?: effectiveStreamSettings()
        val recoveryAttempt = streamSessionRecoveryTracker.nextAttempt(initialSession.sessionId)
        recordDebugEvent(
            "recovery",
            "Recovery requested attempt=$recoveryAttempt reason=${reason.take(DEBUG_EVENT_MESSAGE_LIMIT)} " +
                "session=${initialSession.debugSummary()} settings=${currentSettings.debugSummary()}",
        )
        launchJob = viewModelScope.launch {
            val token = auth.tokens.idToken ?: auth.tokens.accessToken
            val snapshot = state.value
            val previousSession = snapshot.streamSession ?: initialSession
            val active = snapshot.activeSession
            val game = snapshot.streamGame
            val baseUrl = listOfNotNull(
                previousSession.streamingBaseUrl,
                active?.streamingBaseUrl,
                effectiveStreamingBaseUrl(auth),
            ).firstOrNull { !it.isLikelyDirectServerUrl() } ?: effectiveStreamingBaseUrl(auth)
            val returnPage = snapshot.streamReturnPage ?: snapshot.page.takeUnless { it == AppPage.Stream } ?: AppPage.Home

            _state.update {
                it.copy(
                    streamSession = null,
                    activeStreamSettings = currentSettings,
                    streamStatus = "connecting",
                    launchPhase = "Recovering stream",
                    page = AppPage.Stream,
                    streamReturnPage = returnPage,
                    streamLaunchMinimized = false,
                    error = null,
                    queuePosition = null,
                    queueAdActiveId = null,
                )
            }

            runCatching {
                val probedPreviousSession = runCatching {
                    sessionRepository.pollSession(
                        token = token,
                        streamingBaseUrl = previousSession.streamingBaseUrl ?: baseUrl,
                        serverIp = previousSession.serverIp,
                        zone = previousSession.zone,
                        sessionId = previousSession.sessionId,
                        clientId = previousSession.clientId,
                        deviceId = previousSession.deviceId,
                        settings = currentSettings,
                        diagnosticOperation = "session.recovery.probe",
                    )
                }.onSuccess { probed ->
                    recordDebugEvent(
                        "recovery",
                        "Old session GET completed session=${probed.shortDebugId()} status=${probed.status}",
                    )
                }.onFailure { error ->
                    if (error is CancellationException) throw error
                    recordDebugEvent(
                        "recovery",
                        "Old session GET failed session=${previousSession.shortDebugId()} error=${error.debugMessage()}",
                    )
                }.getOrNull()
                when (
                    streamSessionRecoveryDisposition(
                        recoveryAttempt = recoveryAttempt,
                        probedStatus = probedPreviousSession?.status,
                    )
                ) {
                    StreamSessionRecoveryDisposition.ReportEndedSession -> {
                        val ended = checkNotNull(probedPreviousSession)
                        recordDebugEvent(
                            "recovery",
                            "Provider ended allocated session status=${ended.status}; automatic replacement disabled",
                        )
                        throw TerminalSessionStatusException(ended.status, ended)
                    }
                    StreamSessionRecoveryDisposition.ReclaimAllocatedSession -> {
                        if (recoveryAttempt > 1) {
                            recordDebugEvent(
                                "recovery",
                                "Repeated recovery remains pinned to allocated session=${previousSession.shortDebugId()} attempt=$recoveryAttempt",
                            )
                        }
                    }
                }
                val resolvedAppId = runCatching {
                    resolveFallbackLaunchAppId(
                        token = token,
                        game = game,
                        active = active,
                        baseUrl = baseUrl,
                    )
                }.getOrNull()
                val activeSessions = sessionRepository.getActiveSessions(token, baseUrl, currentSettings)
                recordDebugEvent("recovery", "Recovery active sessions count=${activeSessions.size} base=${hostForDebug(baseUrl)}")
                val readyCandidate = activeSessionRecoveryCandidate(
                    sessions = activeSessions,
                    previousSessionId = previousSession.sessionId,
                    launchAppId = resolvedAppId?.toIntOrNull(),
                    settings = currentSettings,
                )
                if (readyCandidate?.sessionId == previousSession.sessionId && !readyCandidate.matchesStreamSettings(currentSettings)) {
                    recordDebugEvent(
                        "recovery",
                        "Reclaiming current session after local profile fallback active=${readyCandidate.debugSummary()} settings=${currentSettings.debugSummary()}",
                    )
                }
                val cachedCurrentSession = active?.takeIf {
                    it.sessionId == previousSession.sessionId && it.matchesStreamGeometry(currentSettings)
                }
                val probedCandidate = probedPreviousSession?.let { probed ->
                    knownSessionRecoveryCandidate(
                        session = probed,
                        appId = resolvedAppId?.toIntOrNull() ?: active?.appId ?: 0,
                        fallbackActive = cachedCurrentSession,
                        settings = currentSettings,
                    )
                }
                val fallbackCandidate = readyCandidate
                    ?: probedCandidate
                    ?: knownSessionRecoveryCandidate(
                        session = previousSession,
                        appId = resolvedAppId?.toIntOrNull() ?: active?.appId ?: 0,
                        fallbackActive = cachedCurrentSession,
                        settings = currentSettings,
                    )?.takeIf { it.matchesStreamGeometry(currentSettings) }
                    ?: error("The running session could not be found anymore, so recovery was not possible.")
                recordDebugEvent("recovery", "Claiming recovery candidate ${fallbackCandidate.debugSummary()}")
                claimActiveSessionOrContinuePolling(
                    token = token,
                    active = fallbackCandidate,
                    settings = currentSettings,
                    recoveryMode = true,
                )
            }.onSuccess { readySession ->
                val anchoredSession = readySession.withSessionTimerAnchor()
                recordDebugEvent("recovery", "Recovery claim ready ${anchoredSession.debugSummary()}")
                _state.update {
                    it.copy(
                        streamSession = anchoredSession,
                        activeSession = anchoredSession.toActiveRecoverySession(active, currentSettings),
                        activeStreamSettings = currentSettings,
                        streamStatus = "connecting",
                        launchPhase = "Reconnecting stream",
                        streamLaunchMinimized = false,
                        queuePosition = null,
                        queueAdActiveId = null,
                        page = AppPage.Stream,
                    )
                }
            }.onFailure { error ->
                if (error is CancellationException) return@onFailure
                recordDebugEvent("recovery", "Recovery failed error=${error.debugMessage()}")
                _state.update {
                    it.copy(
                        error = normalizeLaunchError(error, game?.title),
                        streamStatus = "idle",
                        activeStreamSettings = null,
                        streamReturnPage = null,
                        launchPhase = "",
                        streamLaunchMinimized = false,
                        queuePosition = null,
                        queueAdActiveId = null,
                        page = returnPage,
                    )
                }
            }
        }
    }

    private fun SessionInfo.withSessionTimerAnchor(): SessionInfo =
        copy(
            timerStartedAtMs = sessionTimerAnchorStore.startedAtMsFor(
                sessionId = sessionId,
                preferredStartedAtMs = timerStartedAtMs,
            ),
        )

    fun handleExternalLaunchIntent(intent: Intent?) {
        if (intent == null) return
        val uri = intent.data
        if (localTvConnector.isPairUri(uri)) {
            if (state.value.androidTvProfile) {
                _state.update { it.copy(error = "Pairing links must be opened on the Android phone") }
            } else if (uri != null) {
                localTvConnector.pairPhone(uri)
            }
            return
        }
        if (authRepository.handleOAuthRedirect(uri)) {
            _state.update { it.copy(launchPhase = LOGIN_PHASE_GETTING_TOKENS, error = null) }
            return
        }
        val id = extractExternalLaunchId(intent)
        if (id.isNullOrBlank()) return
        val allGames = state.value.games + state.value.libraryGames
        val game = allGames.firstOrNull { game ->
            game.id == id || game.uuid == id || game.launchAppId == id || game.variants.any { it.id == id }
        } ?: GameInfo(
            id = id,
            uuid = id,
            launchAppId = id.takeIf { it.all(Char::isDigit) },
            title = intent.getStringExtra("title") ?: "Game $id",
            selectedVariantIndex = 0,
            variants = listOf(GameVariant(id = id, store = "Unknown")),
        )
        play(game)
    }

    private fun extractExternalLaunchId(intent: Intent): String? {
        val uri = intent.data
        return externalLaunchIdFromParts(
            extras = listOf(
                intent.getStringExtra("id"),
                intent.getStringExtra("appId"),
                intent.getStringExtra("launchAppId"),
            ),
            scheme = uri?.scheme,
            host = uri?.host,
            pathSegments = uri?.pathSegments.orEmpty(),
            schemeSpecificPart = uri?.schemeSpecificPart,
            queryParameters = mapOf(
                "id" to uri?.let { runCatching { it.getQueryParameter("id") }.getOrNull() },
                "appId" to uri?.let { runCatching { it.getQueryParameter("appId") }.getOrNull() },
                "launchAppId" to uri?.let { runCatching { it.getQueryParameter("launchAppId") }.getOrNull() },
            ),
        )
    }

    private fun GameInfo.withSelectedVariant(variantId: String): GameInfo {
        val selectedIndex = variants.indexOfFirst { it.id == variantId }
        return if (selectedIndex >= 0) copy(selectedVariantIndex = selectedIndex) else this
    }

    private fun currentDebugLogText(): String {
        val snapshot = state.value
        val session = snapshot.streamSession
        val codecReport = snapshot.codecReport
        return buildString {
            appendLine("OpenNOW Android diagnostics")
            appendLine(snapshot.androidUpdate.debugHeaderLine())
            appendLine(AndroidDeviceDiagnostics.snapshot(getApplication()).debugSummary())
            appendLine("page=${snapshot.page} initializing=${snapshot.initializing} loadingGames=${snapshot.loadingGames}")
            appendLine("user=${snapshot.authSession?.user?.displayName.orEmpty()} tier=${snapshot.subscriptionInfo?.membershipTier ?: snapshot.authSession?.user?.membershipTier.orEmpty()} provider=${snapshot.authSession?.provider?.code.orEmpty()}")
            appendLine("streamStatus=${snapshot.streamStatus} launchPhase=${snapshot.launchPhase} queuePosition=${snapshot.queuePosition}")
            appendLine("streamGame=${snapshot.streamGame?.title.orEmpty()} selectedGame=${snapshot.selectedGame?.title.orEmpty()}")
            appendLine("sessionId=${session?.sessionId.orEmpty()} sessionStatus=${session?.status} seatSetupStep=${session?.seatSetupStep} serverIp=${session?.serverIp.orEmpty()} base=${session?.streamingBaseUrl.orEmpty()}")
            appendLine("adsRequired=${isSessionAdsRequired(session?.adState)} ads=${sessionAdItems(session?.adState).size} activeAd=${snapshot.queueAdActiveId.orEmpty()} queuePaused=${session?.adState?.isQueuePaused}")
            appendLine("adMessage=${session?.adState?.message.orEmpty()} grace=${session?.adState?.gracePeriodSeconds} serverSentEmptyAds=${session?.adState?.serverSentEmptyAds}")
            appendLine("negotiated=${session?.negotiatedStreamProfile?.debugSummary().orEmpty()} monitors=${session?.monitorSnapshot?.debugSummary().orEmpty()} requestedFeatures=${session?.requestedStreamingFeatures?.debugSummary().orEmpty()} finalizedFeatures=${session?.finalizedStreamingFeatures?.debugSummary().orEmpty()}")
            appendLine("printedWaste.loading=${snapshot.printedWasteLoading} queueZones=${snapshot.printedWasteQueue.size} mappingZones=${snapshot.printedWasteMapping.size} pings=${snapshot.printedWastePings.size} error=${snapshot.printedWasteError.orEmpty()}")
            appendLine("settings.resolution=${snapshot.settings.stream.resolution} fps=${snapshot.settings.stream.fps} codec=${snapshot.settings.stream.codec} bitrate=${snapshot.settings.stream.maxBitrateMbps}")
            appendLine("settings.preset=${snapshot.settings.streamPreset} recommendation=${deviceRecommendation?.debugSummary() ?: "pending"}")
            snapshot.activeStreamSettings?.let { active ->
                appendLine("active.resolution=${active.resolution} fps=${active.fps} codec=${active.codec} bitrate=${active.maxBitrateMbps}")
            }
            appendLine(
                "input.keyboardLayout=${snapshot.settings.stream.keyboardLayout} " +
                    "mouseLock=${snapshot.settings.externalMousePointerLock} touch=${snapshot.settings.androidTouch}",
            )
            appendLine("codec.native=${codecReport?.nativeRuntimeSummary.orEmpty()} lowPower=${codecReport?.lowPowerGpuProfile} constrained=${codecReport?.constrainedRuntimeProfile} tv=${codecReport?.androidTvProfile}")
            appendLine("device.runtime=${AndroidRuntimeDiagnostics.snapshot(getApplication()).debugSummary()}")
            appendLine("stream.runtime.latest=${latestStreamRuntimeStats?.debugSummary(System.currentTimeMillis()) ?: "empty"}")
            appendLine(ProcessCpuDiagnostics.snapshot())
            codecReport?.capabilities?.forEach { cap ->
                appendLine("codec.${cap.codec}: decoder=${cap.decoderName ?: "none"} hardware=${cap.hardwareDecoder} nativeAvailable=${cap.nativeDecoderAvailable ?: "unknown"} webRtc=${cap.webRtcDecoderName ?: "none"} webRtcAvailable=${cap.webRtcDecoderAvailable ?: "unknown"} webRtcHardware=${cap.webRtcHardwareDecoderAvailable ?: "unknown"} encoder=${cap.encoderName ?: "none"}")
            }
            appendLine(DisplayRefreshDiagnostics.snapshot())
            appendLine(NativeInputDiagnostics.snapshot())
            appendLine(OpenNowHttpDiagnostics.snapshot())
            snapshot.error?.let { appendLine("error=$it") }
            val events = debugEventSnapshot()
            appendLine("events.count=${events.size} max=$DEBUG_EVENT_LIMIT")
            if (events.isEmpty()) {
                appendLine("events=(empty)")
            } else {
                val formatter = DateFormat.getTimeInstance(DateFormat.MEDIUM, Locale.US)
                events.forEachIndexed { index, event ->
                    appendLine("event.${index + 1} ${formatter.format(Date(event.timestampMs))} [${event.category}] ${event.message}")
                }
            }
            val payloads = debugPayloadSnapshot()
            appendLine("advancedJson.count=${payloads.size} max=$DEBUG_PAYLOAD_LIMIT")
            if (payloads.isEmpty()) {
                appendLine("advancedJson=(empty)")
            } else {
                val formatter = DateFormat.getTimeInstance(DateFormat.MEDIUM, Locale.US)
                payloads.forEachIndexed { index, payload ->
                    appendLine("advancedJson.${index + 1} ${formatter.format(Date(payload.timestampMs))} [${payload.operation}] ${payload.method} http=${payload.statusCode} url=${payload.url}")
                    if (payload.requestBody.isNotBlank()) {
                        appendLine("request:")
                        appendLine(payload.requestBody)
                    }
                    appendLine("response:")
                    appendLine(payload.body)
                }
            }
        }
    }

    fun debugLogText(): String = appendPreviousDiagnosticSnapshot(
        current = currentDebugLogText(),
        previous = diagnosticHistoryStore.previousSnapshot(),
    )

    suspend fun sanitizedDebugLogText(): String {
        val raw = withContext(Dispatchers.IO) { debugLogText() }
        return withContext(Dispatchers.Default) { sanitizeDiagnosticExport(raw) }
    }

    fun debugLogFileName(): String {
        val timestamp = SimpleDateFormat("yyyyMMdd-HHmmss", Locale.US).format(Date())
        return "opennow-android-logs-$timestamp.txt"
    }

    private companion object {
        const val TV_INITIAL_CATALOG_GAME_LIMIT = 120
        const val TV_LAYOUT_PROFILE_VERSION = 1
    }

    private suspend fun refreshAfterAuth(session: AuthSession, keepRefreshVisibleWithCache: Boolean = false) {
        catalogLoadAttempted = true
        _state.update { it.copy(loadingGames = true, error = null) }
        val baseUrl = effectiveStreamingBaseUrl(session)
        val token = session.tokens.idToken ?: session.tokens.accessToken
        val initialCatalogSearch = state.value.catalogSearch
        val initialCatalogSortId = state.value.catalogSortId
        val initialCatalogFilterIds = state.value.catalogFilterIds
        val cacheKey = CatalogCacheKey.of(
            userId = session.user.userId,
            baseUrl = baseUrl,
            searchQuery = initialCatalogSearch,
            sortId = initialCatalogSortId,
            filterIds = initialCatalogFilterIds,
        )
        // One-shot: startup already parsed these, and after this the network result is authoritative.
        val primed = primedCatalogCache?.takeIf { it.key == cacheKey }
        primedCatalogCache = null
        val (cachedMain, cachedLibrary, unboundedCachedCatalog) = primed?.let {
            Triple(it.main, it.library, it.catalog)
        } ?: withContext(Dispatchers.IO) {
            Triple(
                catalogCacheStore.loadMainGames(session.user.userId, baseUrl),
                catalogCacheStore.loadLibraryGames(session.user.userId, baseUrl),
                catalogCacheStore.loadCatalog(
                    userId = session.user.userId,
                    providerStreamingBaseUrl = baseUrl,
                    searchQuery = initialCatalogSearch,
                    sortId = initialCatalogSortId,
                    filterIds = initialCatalogFilterIds,
                ),
            )
        }
        val cachedGfnThursdayGames = gfnThursdayCatalogGames(cachedMain.orEmpty())
        val cachedNewlyAddedQuery = isNewlyAddedCatalogQuery(
            searchQuery = initialCatalogSearch,
            sortId = initialCatalogSortId,
            filterIds = initialCatalogFilterIds,
        )
        val officialCachedCatalog = if (cachedNewlyAddedQuery && cachedGfnThursdayGames.isNotEmpty()) {
            catalogResultWithGfnThursdayGames(
                fallback = unboundedCachedCatalog ?: CatalogBrowseResult(
                    games = emptyList(),
                    selectedSortId = NEWLY_ADDED_CATALOG_SORT_ID,
                ),
                games = cachedGfnThursdayGames,
            )
        } else {
            unboundedCachedCatalog
        }
        val cachedCatalog = if (androidTvProfile) {
            officialCachedCatalog?.copy(games = officialCachedCatalog.games.take(TV_INITIAL_CATALOG_GAME_LIMIT))
        } else {
            officialCachedCatalog
        }
        val cachedNewlyAdded = primed?.newlyAdded ?: withContext(Dispatchers.IO) {
            catalogCacheStore.loadCatalog(
                userId = session.user.userId,
                providerStreamingBaseUrl = baseUrl,
                searchQuery = "",
                sortId = NEWLY_ADDED_CATALOG_SORT_ID,
                filterIds = emptyList(),
            )
        }
        val hasScopedCatalogQuery =
            isScopedCatalogQuery(initialCatalogSearch, initialCatalogSortId, initialCatalogFilterIds)
        if (cachedMain != null || cachedLibrary != null || cachedCatalog != null || cachedNewlyAdded != null) {
            val cachedMergedLibrary = withContext(Dispatchers.Default) {
                mergeKnownLibraryGames(
                    cachedLibrary.orEmpty(),
                    cachedMain.orEmpty(),
                    cachedCatalog?.games.orEmpty(),
                )
            }
            _state.update {
                val nextGames = cachedCatalog?.games ?: if (hasScopedCatalogQuery) {
                    emptyList()
                } else {
                    it.games.ifEmpty { cachedMain.orEmpty() }
                }
                val nextCatalogResult = cachedCatalog ?: if (hasScopedCatalogQuery) {
                    it.catalogResult.copy(games = emptyList())
                } else {
                    it.catalogResult
                }
                // The Store grid renders from these two and nothing else, so a warm library cache
                // is not a reason to tell the reader the load has finished.
                val hasGamesToShow = nextGames.isNotEmpty() || nextCatalogResult.games.isNotEmpty()
                it.copy(
                    games = nextGames,
                    newlyAddedGames = cachedGfnThursdayGames.ifEmpty {
                        cachedNewlyAdded?.games?.ifEmpty { it.newlyAddedGames } ?: it.newlyAddedGames
                    },
                    libraryGames = cachedMergedLibrary.ifEmpty { cachedLibrary ?: it.libraryGames },
                    catalogResult = nextCatalogResult,
                    loadingGames = catalogStillLoadingAfterCache(hasGamesToShow, keepRefreshVisibleWithCache),
                    catalogQueryLoading = !hasGamesToShow,
                    error = null,
                )
            }
        }
        val subscriptionJob = viewModelScope.launch {
            val sub = withContext(Dispatchers.IO) {
                runCatching {
                    val vpcId = catalogRepository.getVpcId(token, session.provider.streamingServiceUrl)
                    subscriptionRepository.fetchSubscription(token, session.user.userId, vpcId)
                }.getOrNull()
            }
            val enrichedSession = persistSubscriptionTier(session, sub)
            _state.update { current ->
                current.copy(
                    authSession = current.authSession
                        ?.takeIf { it.user.userId == enrichedSession.user.userId }
                        ?.let { enrichedSession }
                        ?: current.authSession,
                    savedAccounts = savedAccountsSnapshot(),
                    subscriptionInfo = sub,
                )
            }
        }
        activeSubscriptionJob = subscriptionJob
        val accountConnectorsJob = viewModelScope.launch {
            _state.update { it.copy(loadingAccountConnectors = true) }
            val connectors = withContext(Dispatchers.IO) {
                runCatching { accountConnectorRepository.fetchConnectors(token) }.getOrDefault(emptyList())
            }
            _state.update { it.copy(accountConnectors = connectors, loadingAccountConnectors = false) }
        }
        val regionsJob = viewModelScope.launch {
            val regions = withContext(Dispatchers.IO) {
                runCatching { fetchDynamicRegions(http, token, session.provider.streamingServiceUrl).first }.getOrDefault(emptyList())
            }
            _state.update { it.copy(regions = regions) }
        }
        gamesJob?.cancel()
        gamesJob = viewModelScope.launch {
            runCatching {
                coroutineScope {
                    val includeSupplementalPublicVariants = !androidTvProfile
                    val initialNewlyAddedQuery = isNewlyAddedCatalogQuery(
                        searchQuery = initialCatalogSearch,
                        sortId = initialCatalogSortId,
                        filterIds = initialCatalogFilterIds,
                    )
                    val catalogDeferred = async(Dispatchers.IO) {
                        catalogRepository.browseCatalog(
                            token = token,
                            providerStreamingBaseUrl = baseUrl,
                            searchQuery = initialCatalogSearch,
                            sortId = initialCatalogSortId,
                            filterIds = initialCatalogFilterIds,
                            maxPages = catalogPageLimit(
                                androidTvProfile = androidTvProfile,
                                filterIds = initialCatalogFilterIds,
                                searchQuery = initialCatalogSearch,
                            ),
                            includeSupplementalPublicVariants = includeSupplementalPublicVariants,
                        ).also { catalog ->
                            _state.update { current ->
                                if (
                                    !initialNewlyAddedQuery &&
                                    current.authSession?.user?.userId == session.user.userId &&
                                    current.catalogSearch == initialCatalogSearch &&
                                    current.catalogSortId == initialCatalogSortId &&
                                    current.catalogFilterIds == initialCatalogFilterIds
                                ) {
                                    current.copy(
                                        catalogResult = catalog,
                                        games = if (androidTvProfile) catalog.games else current.games,
                                    )
                                } else {
                                    current
                                }
                            }
                        }
                    }
                    // MainV2 is a ~600KB personalized panel response. Keep the normal TV path
                    // bounded, but fetch it for an explicit Latest Added query because its
                    // GFN Thursday section is NVIDIA's authoritative weekly content.
                    val mainDeferred: Deferred<Result<List<GameInfo>>>? = if (androidTvProfile && !initialNewlyAddedQuery) {
                        null
                    } else {
                        async(Dispatchers.IO) {
                            try {
                                Result.success(
                                    catalogRepository.fetchMainGames(token, baseUrl, includeSupplementalPublicVariants),
                                )
                            } catch (error: CancellationException) {
                                throw error
                            } catch (error: Exception) {
                                Result.failure(error)
                            }
                        }
                    }
                    val libraryDeferred = async(Dispatchers.IO) {
                        catalogRepository.fetchLibraryGames(token, baseUrl, includeSupplementalPublicVariants)
                            .also { library ->
                                _state.update { current ->
                                    if (current.authSession?.user?.userId == session.user.userId) {
                                        current.copy(libraryGames = library)
                                    } else {
                                        current
                                    }
                                }
                            }
                    }
                    val providerCatalog = catalogDeferred.await()
                    val main = mainDeferred?.await()?.getOrElse { error ->
                        recordDebugEvent("catalog", "Main panel refresh failed; using catalog fallback error=${error.debugMessage()}")
                        providerCatalog.games
                    } ?: providerCatalog.games
                    val gfnThursdayGames = gfnThursdayCatalogGames(main)
                    val newlyAddedCatalog = when {
                        gfnThursdayGames.isNotEmpty() ->
                            catalogResultWithGfnThursdayGames(providerCatalog, gfnThursdayGames)
                        initialNewlyAddedQuery -> providerCatalog
                        else -> try {
                            withContext(Dispatchers.IO) {
                                catalogRepository.browseCatalog(
                                    token = token,
                                    providerStreamingBaseUrl = baseUrl,
                                    searchQuery = "",
                                    sortId = NEWLY_ADDED_CATALOG_SORT_ID,
                                    filterIds = emptyList(),
                                    maxPages = 1,
                                    includeSupplementalPublicVariants = false,
                                )
                            }
                        } catch (error: CancellationException) {
                            throw error
                        } catch (error: Exception) {
                            recordDebugEvent("catalog", "Newly added hero refresh failed error=${error.debugMessage()}")
                            cachedNewlyAdded
                        }
                    }
                    val catalog = if (initialNewlyAddedQuery) {
                        newlyAddedCatalog ?: providerCatalog
                    } else {
                        providerCatalog
                    }
                    newlyAddedCatalog?.let { newest ->
                        _state.update { current ->
                            if (current.authSession?.user?.userId == session.user.userId) {
                                current.copy(newlyAddedGames = newest.games)
                            } else {
                                current
                            }
                        }
                    }
                    val library = libraryDeferred.await()
                    val mergedLibrary = withContext(Dispatchers.Default) {
                        mergeKnownLibraryGames(library, main, catalog.games)
                    }
                    recordDebugEvent(
                        "catalog",
                        "Library counts raw=${library.size} main=${main.size} mainOwned=${main.count(::isGameInLibrary)} " +
                            "catalog=${catalog.games.size} catalogOwned=${catalog.games.count(::isGameInLibrary)} " +
                            "gfnThursday=${gfnThursdayGames.size} merged=${mergedLibrary.size} " +
                            "activeFilters=${state.value.libraryFilterIds.sorted().joinToString(",").ifBlank { "none" }}",
                    )
                    withContext(Dispatchers.IO) {
                        catalogCacheStore.saveMainGames(session.user.userId, baseUrl, main)
                        catalogCacheStore.saveLibraryGames(session.user.userId, baseUrl, mergedLibrary)
                        catalogCacheStore.saveCatalog(
                            userId = session.user.userId,
                            providerStreamingBaseUrl = baseUrl,
                            searchQuery = initialCatalogSearch,
                            sortId = initialCatalogSortId,
                            filterIds = initialCatalogFilterIds,
                            result = catalog,
                        )
                        newlyAddedCatalog?.let { newest ->
                            catalogCacheStore.saveCatalog(
                                userId = session.user.userId,
                                providerStreamingBaseUrl = baseUrl,
                                searchQuery = "",
                                sortId = NEWLY_ADDED_CATALOG_SORT_ID,
                                filterIds = emptyList(),
                                result = newest,
                            )
                        }
                    }
                    Triple(mergedLibrary, catalog, newlyAddedCatalog)
                }
            }.onSuccess { (library, catalog, newlyAddedCatalog) ->
                _state.update { current ->
                    if (
                        current.authSession?.user?.userId == session.user.userId &&
                        current.catalogSearch == initialCatalogSearch &&
                        current.catalogSortId == initialCatalogSortId &&
                        current.catalogFilterIds == initialCatalogFilterIds
                    ) {
                        current.copy(
                            // The browse result owns catalogue order and filtering. MainV2 is
                            // supplemental metadata and must never replace a user-sorted page.
                            games = catalog.games,
                            newlyAddedGames = newlyAddedCatalog?.games ?: current.newlyAddedGames,
                            libraryGames = library,
                            catalogResult = catalog,
                            loadingGames = false,
                            catalogQueryLoading = false,
                            error = null,
                        )
                    } else {
                        current
                    }
                }
                catalogRetryJob?.cancel()
                catalogRetryAttempt = 0
                refreshActiveSession()
            }.onFailure { error ->
                if (error is CancellationException) return@onFailure
                _state.update { current ->
                    val hasUsableGames =
                        cachedMain != null ||
                        cachedLibrary != null ||
                        cachedCatalog != null ||
                        current.games.isNotEmpty() ||
                        current.libraryGames.isNotEmpty() ||
                        current.catalogResult.games.isNotEmpty()
                    current.copy(
                        loadingGames = false,
                        catalogQueryLoading = false,
                        error = if (hasUsableGames) null else error.message ?: "Failed to load games",
                    )
                }
                recordDebugEvent("catalog", "Catalog load failed error=${error.debugMessage()}")
                // An empty Store with an error on it used to be terminal until someone pulled to
                // refresh. Nothing else in the app ever asks again.
                if (!state.value.hasLoadedCatalogGames()) {
                    scheduleCatalogRetry()
                }
            }
        }
        subscriptionJob.join()
        accountConnectorsJob.join()
        regionsJob.join()
    }

    private fun refreshCatalogDebounced() {
        gamesJob?.cancel()
        gamesJob = viewModelScope.launch {
            val auth = state.value.authSession ?: return@launch
            val baseUrl = effectiveStreamingBaseUrl(auth)
            val searchQuery = state.value.catalogSearch
            val sortId = state.value.catalogSortId
            val filterIds = state.value.catalogFilterIds
            val newlyAddedQuery = isNewlyAddedCatalogQuery(searchQuery, sortId, filterIds)
            val unboundedCachedCatalog = withContext(Dispatchers.IO) {
                catalogCacheStore.loadCatalog(
                    userId = auth.user.userId,
                    providerStreamingBaseUrl = baseUrl,
                    searchQuery = searchQuery,
                    sortId = sortId,
                    filterIds = filterIds,
                )
            }
            val cachedCatalog = if (androidTvProfile) {
                unboundedCachedCatalog?.copy(games = unboundedCachedCatalog.games.take(TV_INITIAL_CATALOG_GAME_LIMIT))
            } else {
                unboundedCachedCatalog
            }
            _state.update { current ->
                if (
                    current.catalogSearch == searchQuery &&
                    current.catalogSortId == sortId &&
                    current.catalogFilterIds == filterIds
                ) {
                    // A cached result that exists but holds no games still leaves the grid blank.
                    val hasGamesToShow = cachedCatalog?.games?.isNotEmpty() == true
                    current.copy(
                        loadingGames = catalogStillLoadingAfterCache(hasGamesToShow, keepRefreshVisible = false),
                        catalogQueryLoading = !hasGamesToShow,
                        catalogResult = cachedCatalog ?: current.catalogResult.copy(games = emptyList()),
                        games = cachedCatalog?.games ?: emptyList(),
                        error = null,
                    )
                } else {
                    current
                }
            }
            runCatching {
                withContext(Dispatchers.IO) {
                    coroutineScope {
                        val token = auth.tokens.idToken ?: auth.tokens.accessToken
                        val providerCatalogDeferred = async {
                            catalogRepository.browseCatalog(
                                token = token,
                                providerStreamingBaseUrl = baseUrl,
                                searchQuery = searchQuery,
                                sortId = sortId,
                                filterIds = filterIds,
                                maxPages = catalogPageLimit(
                                    androidTvProfile = androidTvProfile,
                                    filterIds = filterIds,
                                    searchQuery = searchQuery,
                                ),
                                includeSupplementalPublicVariants = !androidTvProfile,
                            )
                        }
                        val gfnThursdayDeferred: Deferred<Result<List<GameInfo>>>? = if (newlyAddedQuery) {
                            async {
                                try {
                                    Result.success(
                                        catalogRepository.fetchGfnThursdayGames(
                                            token = token,
                                            providerStreamingBaseUrl = baseUrl,
                                            includeSupplementalPublicVariants = !androidTvProfile,
                                        ),
                                    )
                                } catch (error: CancellationException) {
                                    throw error
                                } catch (error: Exception) {
                                    Result.failure(error)
                                }
                            }
                        } else {
                            null
                        }
                        val providerCatalog = providerCatalogDeferred.await()
                        val gfnThursdayGames = gfnThursdayDeferred?.await()?.getOrElse { error ->
                            recordDebugEvent("catalog", "GFN Thursday refresh failed; using provider sort error=${error.debugMessage()}")
                            emptyList()
                        }.orEmpty()
                        catalogResultWithGfnThursdayGames(providerCatalog, gfnThursdayGames)
                    }
                }
            }.onSuccess { result ->
                val mergedLibrary = withContext(Dispatchers.Default) {
                    mergeKnownLibraryGames(state.value.libraryGames, result.games)
                }
                withContext(Dispatchers.IO) {
                    catalogCacheStore.saveCatalog(
                        userId = auth.user.userId,
                        providerStreamingBaseUrl = baseUrl,
                        searchQuery = searchQuery,
                        sortId = sortId,
                        filterIds = filterIds,
                        result = result,
                    )
                    if (newlyAddedQuery && sortId != NEWLY_ADDED_CATALOG_SORT_ID) {
                        catalogCacheStore.saveCatalog(
                            userId = auth.user.userId,
                            providerStreamingBaseUrl = baseUrl,
                            searchQuery = "",
                            sortId = NEWLY_ADDED_CATALOG_SORT_ID,
                            filterIds = emptyList(),
                            result = result,
                        )
                    }
                }
                _state.update {
                    if (
                        it.catalogSearch == searchQuery &&
                        it.catalogSortId == sortId &&
                        it.catalogFilterIds == filterIds
                    ) {
                        it.copy(
                            catalogResult = result,
                            newlyAddedGames = if (newlyAddedQuery) result.games else it.newlyAddedGames,
                            loadingGames = false,
                            catalogQueryLoading = false,
                            games = result.games,
                            libraryGames = mergedLibrary.ifEmpty { it.libraryGames },
                        )
                    } else {
                        it
                    }
                }
            }.onFailure { error ->
                if (error is CancellationException) return@onFailure
                _state.update { current ->
                    if (
                        current.catalogSearch == searchQuery &&
                        current.catalogSortId == sortId &&
                        current.catalogFilterIds == filterIds
                    ) {
                        current.copy(
                            error = if (cachedCatalog != null) null else error.message ?: "Catalog refresh failed",
                            loadingGames = false,
                            catalogQueryLoading = false,
                        )
                    } else {
                        current
                    }
                }
            }
        }
    }

    private suspend fun showPrintedWasteSelector(game: GameInfo) {
        recordDebugEvent("queue", "Loading PrintedWaste selector game=${game.title}")
        _state.update {
            it.copy(
                pendingStoreChoiceGame = null,
                pendingPrintedWasteGame = game,
                printedWasteLoading = true,
                printedWasteError = null,
                printedWasteQueue = emptyMap(),
                printedWasteMapping = emptyMap(),
                printedWastePings = emptyMap(),
            )
        }
        loadPrintedWasteQueue(game)
    }

    private suspend fun loadPrintedWasteQueue(game: GameInfo) {
        recordDebugEvent("queue", "Fetching PrintedWaste queue data game=${game.title}")
        _state.update {
            it.copy(
                pendingStoreChoiceGame = null,
                pendingPrintedWasteGame = game,
                printedWasteLoading = true,
                printedWasteError = null,
            )
        }
        runCatching {
            coroutineScope {
                val queue = async { printedWasteRepository.fetchQueue() }
                val mapping = async { printedWasteRepository.fetchServerMapping() }
                val queueData = queue.await()
                val mappingData = mapping.await()
                val regions = queueData
                    .filter { (zoneId, _) -> isStandardPrintedWasteZone(zoneId) && mappingData[zoneId]?.nuked != true }
                    .map { (zoneId, _) ->
                        StreamRegion(name = zoneId, url = printedWasteZoneUrl(zoneId), pingMs = null)
                    }
                val pings = printedWasteRepository.pingRegions(regions).associate { it.url to it.pingMs }
                Triple(queueData, mappingData, pings)
            }
        }.onSuccess { (queue, mapping, pings) ->
            val usableZones = queue
                .filter { (zoneId, _) -> isStandardPrintedWasteZone(zoneId) && mapping[zoneId]?.nuked != true }
                .keys
            val bestZone = usableZones
                .mapNotNull { zoneId ->
                    val zone = queue[zoneId] ?: return@mapNotNull null
                    val url = printedWasteZoneUrl(zoneId)
                    Triple(zoneId, zone.QueuePosition, pings[url])
                }
                .minWithOrNull(
                    compareBy<Triple<String, Int, Long?>>(
                        { it.third ?: Long.MAX_VALUE },
                        { it.second },
                    ),
                )
            recordDebugEvent(
                "queue",
                "PrintedWaste queue loaded zones=${queue.size} usable=${usableZones.size} best=${bestZone?.first.orEmpty()} bestQueue=${bestZone?.second ?: 0} bestPing=${bestZone?.third ?: -1}",
            )
            _state.update {
                it.copy(
                    printedWasteQueue = queue,
                    printedWasteMapping = mapping,
                    printedWastePings = pings,
                    printedWasteLoading = false,
                    printedWasteError = null,
                )
            }
        }.onFailure { error ->
            if (error is CancellationException) return@onFailure
            recordDebugEvent(
                "queue",
                "PrintedWaste queue load failed error=${error.debugMessage()} using=default",
            )
            launchWithPrintedWaste(null)
        }
    }

    private suspend fun refreshActiveSession() {
        val auth = state.value.authSession ?: return
        val settings = effectiveStreamSettings()
        val token = auth.tokens.idToken ?: auth.tokens.accessToken
        val active = runCatching { sessionRepository.getActiveSessions(token, effectiveStreamingBaseUrl(auth), settings) }
            .getOrDefault(emptyList())
            .firstOrNull { it.status in setOf(1, 2, 3) && it.matchesStreamSettings(settings) }
        _state.update { it.copy(activeSession = active) }
    }

    private suspend fun resumeKnownActiveSession(
        token: String,
        active: ActiveSessionInfo,
        settings: StreamSettings,
        baseUrl: String,
    ): SessionInfo {
        if (!active.matchesStreamSettings(settings)) {
            recordDebugEvent(
                "queue",
                "Explicit resume is using active session settings active=${active.debugSummary()} requested=${settings.debugSummary()}",
            )
        }
        if (active.isReadyForClaim()) {
            recordDebugEvent("queue", "Active session already ready for claim ${active.debugSummary()}")
            _state.update { it.copy(launchPhase = "Resuming session") }
            return claimActiveSessionOrContinuePolling(token, active, settings)
        }

        val pending = active.toPendingSession(zone = "prod")
        recordDebugEvent("queue", "Hydrating active session before resume ${pending.debugSummary()}")
        val hydrated = runCatching {
            sessionRepository.pollSession(
                token = token,
                streamingBaseUrl = active.streamingBaseUrl ?: baseUrl,
                serverIp = active.serverIp,
                zone = "prod",
                sessionId = active.sessionId,
                clientId = null,
                deviceId = null,
                settings = settings,
            )
        }.getOrElse { error ->
            recordDebugEvent("queue", "Resume hydrate failed session=${pending.shortDebugId()} error=${error.debugMessage()}")
            pending
        }
        val latest = mergeQueueSessionState(pending, hydrated)
        recordDebugEvent("queue", "Resume hydrate result ${latest.debugSummary()}")
        _state.update {
            it.copy(
                streamSession = latest,
                launchPhase = loadingPhaseFor(latest),
                queuePosition = queueDisplayPosition(latest),
                queueAdActiveId = chooseQueueAdActiveId(it.queueAdActiveId, latest),
            )
        }
        if (latest.isReadyForStream()) {
            val hydratedActive = active.copy(
                status = latest.status,
                queuePosition = latest.queuePosition,
                seatSetupStep = latest.seatSetupStep,
                streamingBaseUrl = latest.streamingBaseUrl ?: active.streamingBaseUrl,
                serverIp = latest.serverIp,
                signalingUrl = latest.signalingUrl,
            )
            recordDebugEvent("queue", "Resume session became ready ${latest.debugSummary()}")
            _state.update { it.copy(launchPhase = "Resuming session") }
            return claimActiveSessionOrContinuePolling(token, hydratedActive, settings)
        }
        return pollUntilReady(token, latest, settings)
    }

    /**
     * The host builds its virtual input devices from this when the session is created, and never
     * revisits it. It must therefore agree with what [shouldUseNativeTouch] decides at stream time:
     * a session created as GAMEPAD_FRIENDLY has no touchscreen, and will silently drop perfectly
     * well-formed touch packets.
     */
    private fun appLaunchModeFor(game: GameInfo?, settings: StreamSettings): Int {
        val nativeTouch = resolveStreamInputModeAtLaunch(game, settings) == StreamInputMode.NativeTouch
        return if (nativeTouch) GfnAppLaunchMode.TOUCH_FRIENDLY else GfnAppLaunchMode.GAMEPAD_FRIENDLY
    }

    private fun resolveStreamInputModeAtLaunch(game: GameInfo?, settings: StreamSettings): StreamInputMode {
        _state.value.streamInputModeAtLaunch?.let { return it }
        val nativeTouchAvailable = !androidTvProfile && shouldUseNativeTouch(
            _state.value.settings.androidTouch.effectiveNativeTouchMode(),
            game,
            settings,
        )
        val mode = streamInputModeAtStart(
            nativeTouchAvailable = nativeTouchAvailable,
            keyboardMouseConnected = hasConnectedPhysicalKeyboardOrMouse(),
        )
        _state.update { current -> current.copy(streamInputModeAtLaunch = mode) }
        return mode
    }

    private suspend fun claimActiveSessionOrContinuePolling(
        token: String,
        active: ActiveSessionInfo,
        settings: StreamSettings,
        recoveryMode: Boolean = false,
    ): SessionInfo {
        return try {
            // Claiming re-sends the session request body, so repeating the mode the session was
            // created with keeps it from being downgraded mid-flight.
            sessionRepository.claimSession(
                token = token,
                active = active,
                settings = settings,
                appLaunchMode = appLaunchModeFor(_state.value.streamGame, settings),
                recoveryMode = recoveryMode,
            )
        } catch (error: SessionClaimNotReadyException) {
            val fallback = active.toPendingSession(zone = "prod")
            val latest = error.latestSession?.let { mergeQueueSessionState(fallback, it) } ?: fallback
            if (isTerminalSessionStatus(latest.status)) {
                throw TerminalSessionStatusException(latest.status, latest)
            }
            recordDebugEvent("queue", "Claim stayed pending; continuing queue polling ${latest.debugSummary()}")
            _state.update {
                it.copy(
                    streamSession = latest,
                    activeStreamSettings = settings,
                    launchPhase = loadingPhaseFor(latest),
                    queuePosition = queueDisplayPosition(latest),
                    queueAdActiveId = chooseQueueAdActiveId(it.queueAdActiveId, latest),
                )
            }
            pollUntilReady(token, latest, settings)
        }
    }

    private suspend fun pollUntilReady(token: String, created: SessionInfo, settings: StreamSettings): SessionInfo {
        var latest = created
        var pollCount = 0
        if (isTerminalSessionStatus(latest.status)) {
            throw TerminalSessionStatusException(latest.status, latest)
        }
        recordDebugEvent("queue", "Begin polling ${latest.debugSummary()}")
        _state.update {
            it.copy(
                streamSession = latest,
                launchPhase = loadingPhaseFor(latest),
                queuePosition = queueDisplayPosition(latest),
                queueAdActiveId = chooseQueueAdActiveId(it.queueAdActiveId, latest),
            )
        }
        while (!latest.isReadyForStream()) {
            val waitMs = if (shouldWaitForQueueAdPlayback(latest.adState)) 30_000L else 2_000L
            if (waitMs > 2_000L) {
                recordDebugEvent(
                    "queue",
                    "Waiting for queue ad playback session=${latest.shortDebugId()} ads=${sessionAdItems(latest.adState).size} paused=${latest.adState?.isQueuePaused} message=${latest.adState?.message.orEmpty()}",
                )
            }
            if (waitMs > 2_000L) {
                var elapsedMs = 0L
                while (elapsedMs < waitMs) {
                    kotlinx.coroutines.delay(500L)
                    elapsedMs += 500L
                    state.value.streamSession
                        ?.takeIf { it.sessionId == latest.sessionId }
                        ?.let { latest = mergeQueueSessionState(latest, it) }
                    if (!shouldWaitForQueueAdPlayback(latest.adState) || latest.isReadyForStream()) {
                        break
                    }
                }
            } else {
                kotlinx.coroutines.delay(waitMs)
            }
            if (latest.isReadyForStream()) {
                break
            }
            pollCount += 1
            val polled = try {
                sessionRepository.pollSession(
                    token = token,
                    streamingBaseUrl = latest.streamingBaseUrl ?: effectiveStreamingBaseUrl(),
                    serverIp = latest.serverIp,
                    zone = latest.zone,
                    sessionId = latest.sessionId,
                    clientId = latest.clientId,
                    deviceId = latest.deviceId,
                    settings = settings,
                )
            } catch (e: Exception) {
                if (e is CancellationException) throw e
                recordDebugEvent("queue", "Poll #$pollCount failed due to network error: ${e.message}. Retrying in 2 seconds...")
                kotlinx.coroutines.delay(2_000L)
                continue
            }
            latest = mergeQueueSessionState(latest, polled)
            recordDebugEvent("queue", "Poll #$pollCount result ${latest.debugSummary()}")
            if (isTerminalSessionStatus(latest.status)) {
                recordDebugEvent("queue", "Polling stopped at terminal session status=${latest.status} ${latest.shortDebugId()}")
                throw TerminalSessionStatusException(latest.status, latest)
            }
            _state.update {
                it.copy(
                    streamSession = latest,
                    launchPhase = loadingPhaseFor(latest),
                    queuePosition = queueDisplayPosition(latest),
                    queueAdActiveId = chooseQueueAdActiveId(it.queueAdActiveId, latest),
                )
            }
        }
        recordDebugEvent("queue", "Polling complete after $pollCount polls ${latest.debugSummary()}")
        return latest
    }

    private fun effectiveStreamingBaseUrl(sessionOverride: AuthSession? = null): String {
        val settings = state.value.settings
        val auth = sessionOverride ?: state.value.authSession
        return settings.stream.region.trim().ifBlank { auth?.provider?.streamingServiceUrl ?: state.value.selectedProvider.streamingServiceUrl }
    }

    private fun shouldUsePrintedWasteQueue(auth: AuthSession): Boolean {
        if (state.value.settings.hideServerSelector) return false
        if (!auth.provider.code.equals("NVIDIA", ignoreCase = true)) return false
        if (!isFreeTier()) return false
        return !isAllianceStreamingBaseUrl(effectiveStreamingBaseUrl(auth))
    }

    private fun isFreeTier(): Boolean {
        val tier = state.value.subscriptionInfo?.membershipTier ?: state.value.authSession?.user?.membershipTier
        return tier.isNullOrBlank() || tier.equals("FREE", ignoreCase = true)
    }

    private fun isAllianceStreamingBaseUrl(streamingBaseUrl: String): Boolean {
        val host = runCatching { Uri.parse(streamingBaseUrl).host.orEmpty() }.getOrDefault("")
        return host.isNotBlank() && !host.endsWith(".nvidiagrid.net", ignoreCase = true)
    }

    private fun chooseQueueAdActiveId(currentId: String?, session: SessionInfo?): String? {
        val ads = sessionAdItems(session?.adState)
        if (!isSessionAdsRequired(session?.adState) || ads.isEmpty()) return null
        return ads.firstOrNull { it.adId == currentId }?.adId ?: ads.first().adId
    }

    private fun loadingPhaseFor(session: SessionInfo): String =
        when {
            queueDisplayPosition(session) != null || session.seatSetupStep == 1 -> "Queue"
            session.status == 0 || session.status == 1 -> "Checking queue"
            else -> "Setting up rig"
        }

    private fun ActiveSessionInfo.toPendingSession(zone: String): SessionInfo {
        val host = serverIp.orEmpty()
        val signalingServer = when {
            host.isBlank() -> ""
            host.contains(":") -> host
            else -> "$host:443"
        }
        return SessionInfo(
            sessionId = sessionId,
            status = status,
            queuePosition = queuePosition,
            seatSetupStep = seatSetupStep,
            zone = zone,
            streamingBaseUrl = streamingBaseUrl,
            serverIp = host,
            signalingServer = signalingServer,
            signalingUrl = signalingUrl ?: host.takeIf { it.isNotBlank() }?.let { "wss://$it:443/nvst/" }.orEmpty(),
            gpuType = gpuType,
            deviceId = authStore.stableDeviceId(),
        )
    }

    private fun SessionInfo.toActiveRecoverySession(
        fallbackActive: ActiveSessionInfo?,
        settings: StreamSettings,
    ): ActiveSessionInfo? {
        val appId = fallbackActive?.takeIf { it.sessionId == sessionId }?.appId ?: fallbackActive?.appId ?: return null
        return knownSessionRecoveryCandidate(
            session = this,
            appId = appId,
            fallbackActive = fallbackActive,
            settings = settings,
        )
    }

    private fun shouldSendAccountLinked(game: GameInfo, variant: GameVariant?): Boolean {
        return shouldLaunchWithAccountLinked(game, variant)
    }

    private fun normalizeLaunchError(error: Throwable, gameTitle: String? = null): String =
        normalizeLaunchErrorMessage(error, gameTitle)

    private fun AuthSession.toSavedAccount(): SavedAccount =
        SavedAccount(
            userId = user.userId,
            displayName = user.displayName,
            email = user.email,
            avatarUrl = user.avatarUrl,
            membershipTier = user.membershipTier,
            providerCode = provider.code,
        )

    private fun AuthSession.withSubscriptionTier(subscription: SubscriptionInfo?): AuthSession {
        val tier = subscription?.membershipTier?.takeIf { it.isNotBlank() } ?: return this
        return if (user.membershipTier == tier) this else copy(user = user.copy(membershipTier = tier))
    }

    private fun persistSubscriptionTier(session: AuthSession, subscription: SubscriptionInfo?): AuthSession {
        val enriched = session.withSubscriptionTier(subscription)
        if (enriched != session) authStore.upsertSession(enriched)
        return enriched
    }

    private fun savedAccountsSnapshot(): List<SavedAccount> =
        authStore.state.value.sessions.map { session -> session.toSavedAccount() }
}

internal fun externalLaunchIdFromParts(
    extras: List<String?>,
    scheme: String?,
    host: String?,
    pathSegments: List<String>,
    schemeSpecificPart: String?,
    queryParameters: Map<String, String?>,
): String? {
    val normalizedScheme = scheme.orEmpty().lowercase(Locale.US)
    val uriCandidates = if (normalizedScheme == "opennow") {
        buildList {
            add(queryParameters["id"])
            add(queryParameters["appId"])
            add(queryParameters["launchAppId"])
            val routeHost = host.orEmpty()
            if (routeHost.equals("launch", ignoreCase = true)) {
                add(pathSegments.firstOrNull())
            } else if (routeHost.isNotBlank()) {
                add(routeHost)
            }
            if (pathSegments.firstOrNull()?.equals("launch", ignoreCase = true) == true) {
                add(pathSegments.getOrNull(1))
            }
            add(pathSegments.lastOrNull())
            add(schemeSpecificPart)
        }
    } else {
        emptyList()
    }
    return (extras + uriCandidates).firstNotNullOfOrNull { it.normalizedExternalLaunchId() }
}

private fun String?.normalizedExternalLaunchId(): String? {
    val trimmed = this?.trim()?.trim('/', '?', '#') ?: return null
    if (trimmed.isBlank() || trimmed.equals("launch", ignoreCase = true)) return null
    val cleaned = trimmed
        .removePrefix("//")
        .substringBefore('#')
        .substringBefore('?')
        .trim('/')
    if (cleaned.isBlank() || cleaned.equals("launch", ignoreCase = true)) return null
    return cleaned.split('/').lastOrNull { it.isNotBlank() && !it.equals("launch", ignoreCase = true) }
}

private fun shortDebugId(value: String?): String {
    val text = value.orEmpty()
    if (text.length <= 12) return text
    return "${text.take(6)}...${text.takeLast(4)}"
}

private fun hostForDebug(url: String?): String =
    runCatching { Uri.parse(url.orEmpty()).host.orEmpty() }
        .getOrDefault("")
        .ifBlank { url.orEmpty().take(80) }

private fun Throwable.debugMessage(): String {
    val type = javaClass.simpleName.ifBlank { "Throwable" }
    val text = message.orEmpty()
        .lineSequence()
        .joinToString(" ") { it.trim() }
        .take(DEBUG_EVENT_MESSAGE_LIMIT)
    return if (text.isBlank()) type else "$type: $text"
}

private fun StreamSettings.debugSummary(): String =
    "res=$resolution aspect=$aspectRatio fps=$fps bitrate=$maxBitrateMbps codec=$codec color=${colorQuality.name} hdr=$hdrEnabled l4s=$enableL4S nvst=$experimentalNvst sharp=$streamSharpeningEnabled"

private fun StreamRuntimeStats.hasDebugValues(): Boolean =
    bitrateKbps != null ||
        availableIncomingBitrateKbps != null ||
        pingMs != null ||
        fps != null ||
        receivedFps != null ||
        decodedFps != null ||
        processCpuPercent != null ||
        !resolution.isNullOrBlank() ||
        !codec.isNullOrBlank()

private fun StreamRuntimeStats.debugSummary(): String =
    "bitrateKbps=${bitrateKbps ?: 0} availableIncomingBitrateKbps=${availableIncomingBitrateKbps ?: -1} " +
        "pingMs=${pingMs ?: -1} fps=${fps ?: 0} receivedFps=${receivedFps ?: 0} decodedFps=${decodedFps ?: 0} " +
        "decodeMs=${decodeMs ?: -1.0} jitterMs=${jitterMs ?: -1.0} packetLossPct=${packetLossPct ?: -1.0} " +
        "packetsLostDelta=${packetsLostDelta ?: -1} packetsReceivedDelta=${packetsReceivedDelta ?: -1} " +
        "processCpuPct=${processCpuPercent ?: -1.0} deviceCpuCapacityPct=${deviceCpuCapacityPercent ?: -1.0} cpuCores=${cpuLogicalCoreCount ?: 0} " +
        "resolution=${resolution.orEmpty()} codec=${codec.orEmpty()}"

private fun TimedStreamRuntimeStats.debugSummary(nowMs: Long): String {
    val formatter = DateFormat.getTimeInstance(DateFormat.MEDIUM, Locale.US)
    val ageMs = (nowMs - capturedAtMs).coerceAtLeast(0L)
    return "capturedAt=${formatter.format(Date(capturedAtMs))} ageMs=$ageMs session=${shortDebugId(sessionId)} ${stats.debugSummary()}"
}

private fun SessionInfo.shortDebugId(): String = shortDebugId(sessionId)

private fun SessionInfo.debugSummary(): String =
    "id=${shortDebugId()} status=$status ready=${isReadyForStream()} queue=${queuePosition ?: "-"} seat=${seatSetupStep ?: "-"} adsRequired=${isSessionAdsRequired(adState)} ads=${sessionAdItems(adState).size} paused=${adState?.isQueuePaused} base=${hostForDebug(streamingBaseUrl)} server=${serverIp.take(80)}"

private fun ActiveSessionInfo.shortDebugId(): String = shortDebugId(sessionId)

private fun ActiveSessionInfo.debugSummary(): String =
    "id=${shortDebugId()} app=$appId status=$status queue=${queuePosition ?: "-"} seat=${seatSetupStep ?: "-"} base=${hostForDebug(streamingBaseUrl)} server=${serverIp.orEmpty().take(80)} res=${resolution.orEmpty()} fps=${fps ?: 0}"

private fun NegotiatedStreamProfile.debugSummary(): String =
    "res=${resolution.orEmpty()} fps=${fps ?: 0} codec=${codec?.name.orEmpty()} color=${colorQuality?.name.orEmpty()} l4s=$enableL4S reflex=$enableReflex"

private fun SessionMonitorSnapshot.debugSummary(): String =
    "requested=${requestedResolution.orEmpty()}@${requestedFps ?: 0} " +
        "returned=${returnedResolution.orEmpty()}@${returnedFps ?: 0} final=${finalSelectedResolution.orEmpty()}"

private fun StreamingFeatures.debugSummary(): String =
    "reflex=$reflex bitDepth=$bitDepth chroma=$chromaFormat l4s=$enabledL4S hdr=$trueHdr"

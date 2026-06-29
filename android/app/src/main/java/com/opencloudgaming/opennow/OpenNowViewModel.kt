package com.opencloudgaming.opennow

import android.app.Application
import android.content.Intent
import android.net.Uri
import android.util.Log
import android.widget.Toast
import androidx.lifecycle.AndroidViewModel
import androidx.lifecycle.viewModelScope
import kotlinx.coroutines.CancellationException
import kotlinx.coroutines.Job
import kotlinx.coroutines.async
import kotlinx.coroutines.coroutineScope
import kotlinx.coroutines.delay
import kotlinx.coroutines.flow.distinctUntilChanged
import kotlinx.coroutines.flow.map
import kotlinx.coroutines.flow.MutableStateFlow
import kotlinx.coroutines.flow.StateFlow
import kotlinx.coroutines.flow.asStateFlow
import kotlinx.coroutines.flow.update
import kotlinx.coroutines.launch
import kotlinx.coroutines.sync.Mutex
import kotlinx.coroutines.sync.withLock
import okhttp3.OkHttpClient
import java.text.DateFormat
import java.util.Date
import java.util.Locale

enum class AppPage {
    Home,
    Library,
    Settings,
    Stream,
}

private const val ANDROID_UPDATE_LAUNCH_CHECK_DELAY_MS = 5_000L
internal const val ANDROID_UPDATE_PERIODIC_CHECK_INTERVAL_MS = 6L * 60L * 60L * 1000L
private const val ANDROID_UPDATE_STREAMING_RETRY_DELAY_MS = 30_000L
private const val DEBUG_EVENT_LIMIT = 140
private const val DEBUG_EVENT_MESSAGE_LIMIT = 640
private const val DEBUG_LOG_TAG = "OpenNOWDebug"

private data class DebugLogEvent(
    val timestampMs: Long,
    val category: String,
    val message: String,
)

data class OpenNowUiState(
    val initializing: Boolean = true,
    val page: AppPage = AppPage.Home,
    val authSession: AuthSession? = null,
    val providers: List<LoginProvider> = listOf(defaultProvider()),
    val selectedProvider: LoginProvider = defaultProvider(),
    val savedAccounts: List<SavedAccount> = emptyList(),
    val subscriptionInfo: SubscriptionInfo? = null,
    val accountConnectors: List<AccountConnector> = emptyList(),
    val loadingAccountConnectors: Boolean = false,
    val regions: List<StreamRegion> = emptyList(),
    val games: List<GameInfo> = emptyList(),
    val libraryGames: List<GameInfo> = emptyList(),
    val catalogResult: CatalogBrowseResult = CatalogBrowseResult(emptyList()),
    val catalogSearch: String = "",
    val librarySearch: String = "",
    val catalogSortId: String = "relevance",
    val catalogFilterIds: List<String> = emptyList(),
    val libraryFilterIds: List<String> = emptyList(),
    val loadingGames: Boolean = false,
    val settings: AppSettings = AppSettings(),
    val codecReport: RuntimeCodecReport? = null,
    val selectedGame: GameInfo? = null,
    val activeSession: ActiveSessionInfo? = null,
    val streamSession: SessionInfo? = null,
    val activeStreamSettings: StreamSettings? = null,
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
    val pendingPrintedWasteGame: GameInfo? = null,
    val printedWasteQueue: Map<String, PrintedWasteZone> = emptyMap(),
    val printedWasteMapping: Map<String, PrintedWasteServerMappingEntry> = emptyMap(),
    val printedWastePings: Map<String, Long?> = emptyMap(),
    val printedWasteLoading: Boolean = false,
    val printedWasteError: String? = null,
    val androidUpdate: AndroidUpdateState = AndroidUpdateState(),
)

internal fun OpenNowUiState.isAndroidUpdateCheckBlockedByStream(): Boolean =
    streamStatus != "idle" || streamSession != null || activeStreamSettings != null

class OpenNowViewModel(application: Application) : AndroidViewModel(application) {
    private val http: OkHttpClient = defaultHttpClient()
    private val settingsStore = SettingsStore(application)
    private val authStore = AuthStore(application)
    private val authRepository = GfnAuthRepository(application, authStore, http)
    private val catalogRepository = GfnCatalogRepository(http)
    private val catalogCacheStore = CatalogCacheStore(application)
    private val subscriptionRepository = GfnSubscriptionRepository(http)
    private val accountConnectorRepository = GfnAccountConnectorRepository(http)
    private val printedWasteRepository = PrintedWasteRepository(http)
    private val sessionRepository = GfnSessionRepository(authStore, http)
    private val appUpdater = AndroidAppUpdater(application, http)
    private val queueAdReportMutex = Mutex()
    private val debugEventsLock = Any()
    private val debugEvents = ArrayDeque<DebugLogEvent>()

    private val _state = MutableStateFlow(OpenNowUiState(settings = settingsStore.settings.value))
    val state: StateFlow<OpenNowUiState> = _state.asStateFlow()

    private var gamesJob: Job? = null
    private var launchJob: Job? = null
    private var loginJob: Job? = null
    private var androidUpdateJob: Job? = null
    private var androidUpdateAutoJob: Job? = null

    init {
        viewModelScope.launch {
            settingsStore.settings.collect { next ->
                OpenNowAnalytics.applyOptOut(next.analyticsOptOut)
                _state.update { it.copy(settings = next) }
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
                    if (blocked && updateStatus == AndroidUpdateStatus.Checking) {
                        cancelAndroidUpdateCheckForStreaming()
                    }
                }
        }
        startAndroidUpdateAutoChecks()
        initialize()
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
        Log.d(DEBUG_LOG_TAG, "${event.category}: ${event.message}")
    }

    private fun debugEventSnapshot(): List<DebugLogEvent> =
        synchronized(debugEventsLock) { debugEvents.toList() }

    fun initialize() {
        viewModelScope.launch {
            val codecReport = CodecProbe.report(getApplication())
            _state.update { it.copy(codecReport = codecReport, initializing = true, launchPhase = "Restoring session") }
            val providers = runCatching { authRepository.loginProviders() }.getOrDefault(listOf(defaultProvider()))
            val restored = runCatching { authRepository.restore(forceRefresh = false) }.getOrNull()
            val selected = restored?.provider ?: providers.firstOrNull() ?: defaultProvider()
            _state.update {
                it.copy(
                    providers = providers,
                    selectedProvider = selected,
                    authSession = restored,
                    savedAccounts = authStore.state.value.sessions.map { session -> session.toSavedAccount() },
                    initializing = false,
                    launchPhase = "",
                )
            }
            if (restored != null) {
                refreshAfterAuth(restored)
            }
        }
    }

    fun setPage(page: AppPage) {
        _state.update { it.copy(page = page, selectedGame = null) }
    }

    fun handleControllerBackNavigation() {
        _state.update { current ->
            when {
                current.pendingStoreChoiceGame != null -> current.copy(pendingStoreChoiceGame = null)
                current.pendingPrintedWasteGame != null -> current.copy(
                    pendingPrintedWasteGame = null,
                    printedWasteLoading = false,
                    printedWasteError = null,
                )
                current.selectedGame != null -> current.copy(selectedGame = null)
                current.page != AppPage.Home -> current.copy(page = AppPage.Home, selectedGame = null)
                else -> current
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
            val useDeviceCode = state.value.codecReport?.androidTvProfile == true && provider.supportsDeviceCodeLogin
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
                    _state.update {
                        it.copy(
                            authSession = session,
                            selectedProvider = session.provider,
                            savedAccounts = authStore.state.value.sessions.map { saved -> saved.toSavedAccount() },
                            launchPhase = "",
                            deviceLoginPrompt = null,
                            error = null,
                        )
                    }
                    OpenNowAnalytics.identify(session)
                    OpenNowAnalytics.capture(
                        event = "user_logged_in",
                        properties = mapOf(
                            "provider" to session.provider.code,
                            "membership_tier" to (session.user.membershipTier ?: ""),
                        ),
                    )
                    refreshAfterAuth(session)
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
                    _state.update {
                        it.copy(
                            authSession = session,
                            selectedProvider = session.provider,
                            savedAccounts = authStore.state.value.sessions.map { saved -> saved.toSavedAccount() },
                            launchPhase = "",
                            deviceLoginPrompt = null,
                            error = null,
                        )
                    }
                    OpenNowAnalytics.identify(session)
                    OpenNowAnalytics.capture(
                        event = "user_logged_in",
                        properties = mapOf(
                            "provider" to session.provider.code,
                            "membership_tier" to (session.user.membershipTier ?: ""),
                            "login_method" to "device_code",
                        ),
                    )
                    refreshAfterAuth(session)
                }
                .onFailure { error ->
                    if (error is CancellationException) return@onFailure
                    _state.update { it.copy(error = error.message ?: "Code sign-in failed", launchPhase = "", deviceLoginPrompt = null) }
                }
        }
    }

    private suspend fun loginWithBestAvailableMethod(provider: LoginProvider, useDeviceCode: Boolean): AuthSession {
        if (useDeviceCode) {
            return loginWithDeviceCode(provider)
        }

        return try {
            authRepository.login(provider)
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
            OpenNowAnalytics.capture(event = "user_logged_out")
            OpenNowAnalytics.reset()
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
                    games = emptyList(),
                    libraryGames = emptyList(),
                    libraryFilterIds = emptyList(),
                    streamSession = null,
                    activeStreamSettings = null,
                    activeSession = null,
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
            authStore.setActiveSession(userId)
            val session = authRepository.restore(forceRefresh = false) ?: return@launch
            gamesJob?.cancel()
            _state.update {
                it.copy(
                    authSession = session,
                    selectedProvider = session.provider,
                    savedAccounts = authStore.state.value.sessions.map { saved -> saved.toSavedAccount() },
                    subscriptionInfo = null,
                    accountConnectors = emptyList(),
                    loadingAccountConnectors = false,
                    games = emptyList(),
                    libraryGames = emptyList(),
                    catalogResult = CatalogBrowseResult(emptyList()),
                    libraryFilterIds = emptyList(),
                    selectedGame = null,
                    activeSession = null,
                    error = null,
                    page = AppPage.Home,
                )
            }
            OpenNowAnalytics.identify(session)
            OpenNowAnalytics.capture(
                event = "account_switched",
                properties = mapOf(
                    "provider" to session.provider.code,
                    "membership_tier" to (session.user.membershipTier ?: ""),
                ),
            )
            refreshAfterAuth(session)
        }
    }

    fun logoutAll() {
        authRepository.logoutAll()
        _state.update {
            it.copy(
                authSession = null,
                savedAccounts = emptyList(),
                subscriptionInfo = null,
                accountConnectors = emptyList(),
                loadingAccountConnectors = false,
                games = emptyList(),
                libraryGames = emptyList(),
                libraryFilterIds = emptyList(),
                streamSession = null,
                activeStreamSettings = null,
                activeSession = null,
                deviceLoginPrompt = null,
                pendingStoreChoiceGame = null,
                page = AppPage.Home,
            )
        }
    }

    fun refreshGames() {
        val session = state.value.authSession ?: return
        viewModelScope.launch {
            refreshAfterAuth(session)
        }
    }

    fun setCatalogSearch(query: String) {
        _state.update { it.copy(catalogSearch = query) }
        if (query.isNotBlank()) {
            OpenNowAnalytics.capture(
                event = "catalog_searched",
                properties = mapOf("query" to query),
            )
        }
        refreshCatalogDebounced()
    }

    fun setLibrarySearch(query: String) {
        _state.update { it.copy(librarySearch = query) }
    }

    fun toggleLibraryFilter(filterId: String) {
        _state.update {
            val filters = if (filterId in it.libraryFilterIds) it.libraryFilterIds - filterId else it.libraryFilterIds + filterId
            it.copy(libraryFilterIds = filters)
        }
    }

    fun clearLibraryFilters() {
        _state.update { it.copy(libraryFilterIds = emptyList()) }
    }

    fun setCatalogSort(sortId: String) {
        _state.update { it.copy(catalogSortId = sortId) }
        refreshCatalogDebounced()
    }

    fun toggleCatalogFilter(filterId: String) {
        val adding = filterId !in state.value.catalogFilterIds
        _state.update {
            val filters = if (filterId in it.catalogFilterIds) it.catalogFilterIds - filterId else it.catalogFilterIds + filterId
            it.copy(catalogFilterIds = filters)
        }
        OpenNowAnalytics.capture(
            event = "catalog_filter_applied",
            properties = mapOf(
                "filter_id" to filterId,
                "action" to if (adding) "add" else "remove",
            ),
        )
        refreshCatalogDebounced()
    }

    fun clearCatalogFilters() {
        _state.update { it.copy(catalogFilterIds = emptyList()) }
        refreshCatalogDebounced()
    }

    fun selectGame(game: GameInfo) {
        _state.update { it.copy(selectedGame = game) }
        OpenNowAnalytics.capture(
            event = "game_selected",
            properties = mapOf(
                "game_id" to game.id,
                "game_title" to game.title,
            ),
        )
    }

    fun clearSelectedGame() {
        _state.update { it.copy(selectedGame = null) }
    }

    fun updateSettings(next: AppSettings) {
        settingsStore.replace(next)
    }

    fun checkAndroidUpdate() {
        startAndroidUpdateCheck(automatic = false)
    }

    fun downloadAndroidUpdate() {
        if (androidUpdateJob?.isActive == true) return
        OpenNowAnalytics.capture(event = "app_update_downloaded")
        androidUpdateJob = viewModelScope.launch {
            appUpdater.downloadUpdate()
        }
    }

    fun installAndroidUpdate() {
        appUpdater.installDownloadedUpdate()
    }

    private fun startAndroidUpdateAutoChecks() {
        if (androidUpdateAutoJob?.isActive == true) return
        androidUpdateAutoJob = viewModelScope.launch {
            delay(ANDROID_UPDATE_LAUNCH_CHECK_DELAY_MS)
            while (true) {
                runAutomaticAndroidUpdateCheck()
                delay(ANDROID_UPDATE_PERIODIC_CHECK_INTERVAL_MS)
            }
        }
    }

    private suspend fun runAutomaticAndroidUpdateCheck() {
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

    fun resetSettings() {
        settingsStore.reset()
        Toast.makeText(getApplication(), "Settings reset to recommended defaults", Toast.LENGTH_SHORT).show()
    }

    fun clearCatalogCache() {
        val removed = catalogCacheStore.clear()
        Toast.makeText(
            getApplication(),
            if (removed == 0) "Game cache was already clear" else "Cleared game cache",
            Toast.LENGTH_SHORT,
        ).show()
        val session = state.value.authSession ?: return
        viewModelScope.launch {
            refreshAfterAuth(session)
        }
    }

    fun refreshAccountConnectors() {
        val session = state.value.authSession ?: return
        viewModelScope.launch {
            val token = authRepository.restore(forceRefresh = false)?.tokens?.let { it.idToken ?: it.accessToken }
                ?: (session.tokens.idToken ?: session.tokens.accessToken)
            _state.update { it.copy(loadingAccountConnectors = true) }
            runCatching { accountConnectorRepository.fetchConnectors(token) }
                .onSuccess { connectors ->
                    _state.update { it.copy(accountConnectors = connectors, loadingAccountConnectors = false) }
                }
                .onFailure { error ->
                    if (error is CancellationException) return@onFailure
                    _state.update { it.copy(loadingAccountConnectors = false, error = error.message ?: "Failed to load account connections") }
                }
        }
    }

    fun updateStreamSettings(transform: (StreamSettings) -> StreamSettings) {
        settingsStore.update { it.copy(stream = transform(it.stream)) }
    }

    fun updateFavorites(gameId: String) {
        val adding = gameId !in settingsStore.settings.value.favoriteGameIds
        settingsStore.update {
            val next = if (gameId in it.favoriteGameIds) it.favoriteGameIds - gameId else it.favoriteGameIds + gameId
            it.copy(favoriteGameIds = next)
        }
        OpenNowAnalytics.capture(
            event = "favorite_toggled",
            properties = mapOf(
                "game_id" to gameId,
                "action" to if (adding) "add" else "remove",
            ),
        )
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

    fun play(game: GameInfo, streamingBaseUrlOverride: String? = null, skipPrintedWaste: Boolean = false, skipStoreChoice: Boolean = false) {
        if (launchJob?.isActive == true) {
            recordDebugEvent("launch", "Ignored play request while another launch is active game=${game.title}")
            return
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
                play(game.withSelectedVariant(defaultVariant.id), streamingBaseUrlOverride, skipPrintedWaste, skipStoreChoice = true)
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
            val returnPage = state.value.page.takeUnless { it == AppPage.Stream } ?: state.value.streamReturnPage ?: AppPage.Home
            if (!skipPrintedWaste && streamingBaseUrlOverride == null && shouldUsePrintedWasteQueue(auth)) {
                recordDebugEvent("queue", "Opening PrintedWaste selector game=${game.title}")
                showPrintedWasteSelector(game)
                return@launch
            }
            val settings = effectiveStreamSettings()
            val token = auth.tokens.idToken ?: auth.tokens.accessToken
            val baseUrl = streamingBaseUrlOverride ?: effectiveStreamingBaseUrl()
            recordDebugEvent(
                "launch",
                "Starting launch game=${game.title} base=${hostForDebug(baseUrl)} settings=${settings.debugSummary()} override=${streamingBaseUrlOverride != null}",
            )
            OpenNowAnalytics.capture(
                event = "stream_started",
                properties = mapOf(
                    "game_id" to game.id,
                    "game_title" to game.title,
                    "resolution" to settings.resolution,
                    "fps" to settings.fps,
                    "codec" to settings.codec.name,
                ),
            )
            _state.update {
                it.copy(
                    streamStatus = "queue",
                    launchPhase = "Resolving game",
                    streamGame = game,
                    activeStreamSettings = settings,
                    selectedGame = null,
                    page = AppPage.Stream,
                    streamReturnPage = returnPage,
                    streamLaunchMinimized = false,
                    error = null,
                    queuePosition = null,
                    queueAdActiveId = null,
                    pendingStoreChoiceGame = null,
                    pendingPrintedWasteGame = null,
                    printedWasteError = null,
                    printedWastePings = emptyMap(),
                )
            }
            runCatching {
                val selectedVariant = game.variants.getOrNull(game.selectedVariantIndex) ?: game.variants.firstOrNull()
                val candidateId = selectedVariant?.id ?: game.launchAppId ?: game.uuid ?: game.id
                val launchAppId = candidateId.takeIf { it.all(Char::isDigit) }
                    ?: game.launchAppId?.takeIf { it.all(Char::isDigit) }
                    ?: catalogRepository.resolveLaunchAppId(token, candidateId, baseUrl)
                    ?: error("Could not resolve numeric appId for ${game.title}")
                recordDebugEvent("launch", "Resolved appId=$launchAppId candidate=$candidateId game=${game.title}")

                _state.update { it.copy(launchPhase = "Checking active sessions") }
                val active = sessionRepository.getActiveSessions(token, baseUrl, settings)
                recordDebugEvent("queue", "Active sessions checked count=${active.size} ${active.joinToString(limit = 4) { it.debugSummary() }}")
                val numericLaunchAppId = launchAppId.toIntOrNull()
                val compatibleActive = active.filter { it.matchesStreamSettings(settings) }
                val staleLaunchSession = active.firstOrNull {
                    it.appId == numericLaunchAppId &&
                        it.status in setOf(1, 2, 3) &&
                        !it.matchesStreamSettings(settings)
                }
                val readyCandidate = compatibleActive.firstOrNull {
                    it.appId == numericLaunchAppId && it.isReadyForClaim()
                }
                val launchingCandidate = compatibleActive.firstOrNull {
                    it.appId == numericLaunchAppId && it.status == 1
                } ?: compatibleActive.firstOrNull { it.status == 1 }
                recordDebugEvent(
                    "queue",
                    "Launch candidates compatible=${compatibleActive.size} ready=${readyCandidate?.shortDebugId().orEmpty()} launching=${launchingCandidate?.shortDebugId().orEmpty()} stale=${staleLaunchSession?.shortDebugId().orEmpty()}",
                )
                val readySession = when {
                    readyCandidate != null -> {
                        recordDebugEvent("queue", "Claiming ready active session ${readyCandidate.debugSummary()}")
                        _state.update { it.copy(launchPhase = "Resuming session", activeSession = readyCandidate) }
                        sessionRepository.claimSession(token, readyCandidate, settings)
                    }
                    launchingCandidate != null -> {
                        val pending = launchingCandidate.toPendingSession(zone = "prod")
                        recordDebugEvent("queue", "Polling existing launching session ${pending.debugSummary()}")
                        _state.update {
                            it.copy(
                                launchPhase = loadingPhaseFor(pending),
                                activeSession = launchingCandidate,
                                streamSession = pending,
                                activeStreamSettings = settings,
                                queuePosition = pending.queuePosition?.takeIf { position -> position > 0 },
                                queueAdActiveId = null,
                            )
                        }
                        val hydrated = runCatching {
                            sessionRepository.pollSession(
                                token = token,
                                streamingBaseUrl = launchingCandidate.streamingBaseUrl ?: baseUrl,
                                serverIp = launchingCandidate.serverIp,
                                zone = "prod",
                                sessionId = launchingCandidate.sessionId,
                                clientId = null,
                                deviceId = null,
                                settings = settings,
                            )
                        }.getOrElse { error ->
                            recordDebugEvent("queue", "Initial hydrate failed session=${pending.shortDebugId()} error=${error.debugMessage()}")
                            pending
                        }
                        pollUntilReady(token, mergeQueueSessionState(pending, hydrated), settings)
                    }
                    else -> {
                        if (staleLaunchSession != null) {
                            recordDebugEvent("queue", "Stopping stale active session ${staleLaunchSession.debugSummary()}")
                            _state.update { it.copy(launchPhase = "Restarting session at requested resolution") }
                            runCatching { sessionRepository.stopActiveSession(token, staleLaunchSession, settings) }
                                .onFailure { error -> recordDebugEvent("queue", "Failed to stop stale active session ${staleLaunchSession.shortDebugId()} error=${error.debugMessage()}") }
                        }
                        _state.update { it.copy(launchPhase = "Creating session") }
                        val created = sessionRepository.createSession(
                            token = token,
                            streamingBaseUrl = baseUrl,
                            appId = launchAppId,
                            internalTitle = game.title,
                            zone = "prod",
                            settings = settings,
                            accountLinked = shouldSendAccountLinked(game, selectedVariant),
                        )
                        recordDebugEvent("queue", "Created session ${created.debugSummary()}")
                        pollUntilReady(token, created, settings)
                    }
                }
                recordDebugEvent("stream", "Session ready for native stream ${readySession.debugSummary()}")
                _state.update {
                    it.copy(
                        streamSession = readySession,
                        activeStreamSettings = settings,
                        streamStatus = "connecting",
                        launchPhase = "Connecting stream",
                        streamLaunchMinimized = false,
                        queuePosition = null,
                        queueAdActiveId = null,
                        page = AppPage.Stream,
                    )
                }
            }.onFailure { error ->
                if (error is CancellationException) return@onFailure
                recordDebugEvent("launch", "Launch failed game=${game.title} error=${error.debugMessage()}")
                val returnPage = state.value.streamReturnPage ?: AppPage.Home
                _state.update {
                    it.copy(
                        error = normalizeLaunchError(error),
                        streamStatus = "idle",
                        activeStreamSettings = null,
                        streamReturnPage = null,
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

    fun stopStream() {
        val beforeStop = state.value
        recordDebugEvent(
            "stream",
            "Stop requested status=${beforeStop.streamStatus} session=${beforeStop.streamSession?.shortDebugId().orEmpty()} game=${beforeStop.streamGame?.title.orEmpty()}",
        )
        launchJob?.cancel()
        launchJob = null
        viewModelScope.launch {
            val auth = state.value.authSession
            val snapshot = state.value
            val returnPage = snapshot.streamReturnPage ?: AppPage.Home
            val session = snapshot.streamSession
            val streamSettings = snapshot.activeStreamSettings ?: effectiveStreamSettings()
            if (auth != null && session != null) {
                runCatching { sessionRepository.stopSession(auth.tokens.idToken ?: auth.tokens.accessToken, session, streamSettings) }
                    .onSuccess { recordDebugEvent("stream", "Stopped cloud session ${session.shortDebugId()}") }
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
                        .onSuccess { recordDebugEvent("stream", "Stopped active session ${active.shortDebugId()}") }
                        .onFailure { error -> recordDebugEvent("stream", "Failed to stop active session ${active.shortDebugId()} error=${error.debugMessage()}") }
                } else {
                    recordDebugEvent("stream", "No cloud session found to stop")
                }
            }
            OpenNowAnalytics.capture(
                event = "stream_stopped",
                properties = mapOf(
                    "game_title" to (state.value.streamGame?.title ?: ""),
                    "game_id" to (state.value.streamGame?.id ?: ""),
                ),
            )
            _state.update {
                it.copy(
                    streamSession = null,
                    activeStreamSettings = null,
                    streamGame = null,
                    streamStatus = "idle",
                    streamLaunchMinimized = false,
                    streamReturnPage = null,
                    launchPhase = "",
                    queuePosition = null,
                    queueAdActiveId = null,
                    pendingStoreChoiceGame = null,
                    page = returnPage,
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
            if (current.streamStatus == "idle" || current.streamSession?.isReadyForStream() == true) {
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

    fun resumeActiveSession() {
        if (launchJob?.isActive == true) {
            recordDebugEvent("queue", "Ignored resume request while another launch is active")
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
                    error = null,
                    queuePosition = null,
                    queueAdActiveId = null,
                )
            }
            runCatching {
                val active = cachedActive ?: sessionRepository.getActiveSessions(token, baseUrl, settings)
                    .firstOrNull { it.status in setOf(1, 2, 3) }
                    ?: error("No active session to resume.")
                recordDebugEvent("queue", "Resume found active ${active.debugSummary()} base=${hostForDebug(baseUrl)}")
                val matchingGame = (state.value.games + state.value.libraryGames)
                    .firstOrNull { it.launchAppId == active.appId.toString() || it.variants.any { variant -> variant.id == active.appId.toString() } }
                _state.update {
                    it.copy(
                        activeSession = active,
                        streamGame = matchingGame,
                        streamSession = active.toPendingSession(zone = "prod"),
                        activeStreamSettings = settings,
                        launchPhase = if (active.isReadyForClaim()) "Resuming session" else loadingPhaseFor(active.toPendingSession(zone = "prod")),
                    )
                }
                resumeKnownActiveSession(token, active, settings, baseUrl)
            }.onSuccess { readySession ->
                recordDebugEvent("stream", "Resume ready for native stream ${readySession.debugSummary()}")
                _state.update {
                    it.copy(
                        streamSession = readySession,
                        activeStreamSettings = settings,
                        streamStatus = "connecting",
                        launchPhase = "Connecting stream",
                        streamLaunchMinimized = false,
                        queuePosition = null,
                        queueAdActiveId = null,
                        page = AppPage.Stream,
                    )
                }
            }.onFailure { error ->
                if (error is CancellationException) return@onFailure
                recordDebugEvent("queue", "Resume failed error=${error.debugMessage()}")
                val returnPage = state.value.streamReturnPage ?: AppPage.Home
                _state.update {
                    it.copy(
                        error = normalizeLaunchError(error),
                        streamStatus = "idle",
                        activeStreamSettings = null,
                        streamReturnPage = null,
                        launchPhase = "",
                        streamLaunchMinimized = false,
                        queuePosition = null,
                        queueAdActiveId = null,
                        pendingStoreChoiceGame = null,
                        pendingPrintedWasteGame = null,
                        page = returnPage,
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
        val snapshot = state.value
        return snapshot.settings.stream
            .withResolutionAllowed(snapshot.subscriptionInfo, snapshot.authSession?.user?.membershipTier)
            .withHdrAllowed(snapshot.subscriptionInfo, snapshot.authSession?.user?.membershipTier)
            .adjustedForDevice(snapshot.codecReport)
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
        val host = runCatching { Uri.parse(this).host.orEmpty() }.getOrDefault("")
        return Regex("^\\d{1,3}(\\.\\d{1,3}){3}$").matches(host)
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
                    val nextAdId = nextQueueAdId(reportSession, adId)
                    if (isTerminalAction) {
                        _state.update { current ->
                            val currentSession = current.streamSession
                            if (currentSession?.sessionId == reportSession.sessionId) {
                                current.copy(
                                    streamSession = removeSessionAdItem(currentSession, adId),
                                    queueAdActiveId = nextAdId,
                                )
                            } else {
                                current.copy(queueAdActiveId = nextAdId)
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
                    val merged = mergeQueueSessionState(previous, updated)
                    current.copy(
                        streamSession = merged,
                        queuePosition = merged.queuePosition?.takeIf { position -> position > 0 },
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
        recordDebugEvent("stream", "Native stream connected session=${state.value.streamSession?.shortDebugId().orEmpty()} game=${state.value.streamGame?.title.orEmpty()}")
        OpenNowAnalytics.capture(
            event = "stream_connected",
            properties = mapOf(
                "game_title" to (state.value.streamGame?.title ?: ""),
                "game_id" to (state.value.streamGame?.id ?: ""),
                "resolution" to (state.value.activeStreamSettings?.resolution ?: ""),
                "fps" to (state.value.activeStreamSettings?.fps ?: 0),
                "codec" to (state.value.activeStreamSettings?.codec?.name ?: ""),
            ),
        )
        _state.update { it.copy(streamStatus = "streaming", launchPhase = "") }
    }

    fun markStreamError(message: String) {
        recordDebugEvent("stream", "Native stream error message=${message.take(DEBUG_EVENT_MESSAGE_LIMIT)} session=${state.value.streamSession?.shortDebugId().orEmpty()}")
        OpenNowAnalytics.capture(
            event = "stream_error",
            properties = mapOf(
                "error_message" to message,
                "game_title" to (state.value.streamGame?.title ?: ""),
                "game_id" to (state.value.streamGame?.id ?: ""),
            ),
        )
        _state.update { it.copy(error = message, streamStatus = "idle", activeStreamSettings = null, launchPhase = "") }
    }

    fun recordNativeStreamState(message: String) {
        recordDebugEvent("native", "state=$message session=${state.value.streamSession?.shortDebugId().orEmpty()}")
    }

    fun restartStreamWithSafeVideoProfile(reason: String) {
        if (launchJob?.isActive == true) {
            recordDebugEvent("recovery", "Ignored safe video restart while launch job is active reason=${reason.take(DEBUG_EVENT_MESSAGE_LIMIT)}")
            return
        }
        val initial = state.value
        val auth = initial.authSession ?: run {
            recordDebugEvent("recovery", "Safe video restart ignored without auth reason=${reason.take(DEBUG_EVENT_MESSAGE_LIMIT)}")
            return
        }
        val currentSettings = initial.activeStreamSettings ?: effectiveStreamSettings()
        val safeSettings = currentSettings.androidSafeVideoFallback()
        recordDebugEvent("recovery", "Safe video restart requested reason=${reason.take(DEBUG_EVENT_MESSAGE_LIMIT)} current=${currentSettings.debugSummary()} safe=${safeSettings.debugSummary()}")
        if (currentSettings == safeSettings) {
            if (initial.streamSession != null) {
                recoverStreamSession("$reason. Reconnecting the existing session.")
                return
            }
            _state.update {
                it.copy(
                    error = "$reason. Safe H264 profile also stalled.",
                    streamStatus = "idle",
                    activeStreamSettings = null,
                    launchPhase = "",
                )
            }
            return
        }
        launchJob = viewModelScope.launch {
            val token = auth.tokens.idToken ?: auth.tokens.accessToken
            val snapshot = state.value
            val previousSession = snapshot.streamSession
            val previousSettings = snapshot.activeStreamSettings ?: currentSettings
            val game = snapshot.streamGame
            val active = snapshot.activeSession
            val baseUrl = listOfNotNull(
                previousSession?.streamingBaseUrl,
                active?.streamingBaseUrl,
                effectiveStreamingBaseUrl(auth),
            ).firstOrNull { !it.isLikelyDirectServerUrl() } ?: effectiveStreamingBaseUrl(auth)
            val returnPage = snapshot.streamReturnPage ?: snapshot.page.takeUnless { it == AppPage.Stream } ?: AppPage.Home

            _state.update {
                it.copy(
                    streamSession = null,
                    activeSession = null,
                    activeStreamSettings = safeSettings,
                    streamStatus = "queue",
                    launchPhase = "Restarting with safe H264 profile",
                    page = AppPage.Stream,
                    streamReturnPage = returnPage,
                    streamLaunchMinimized = false,
                    error = null,
                    queuePosition = null,
                    queueAdActiveId = null,
                )
            }

            runCatching {
                previousSession?.let { session ->
                    runCatching { sessionRepository.stopSession(token, session, previousSettings) }
                        .onFailure { error -> recordDebugEvent("recovery", "Failed to stop previous session ${session.shortDebugId()} error=${error.debugMessage()}") }
                }
                active?.takeIf { it.sessionId != previousSession?.sessionId }?.let { activeSession ->
                    runCatching { sessionRepository.stopActiveSession(token, activeSession, previousSettings) }
                        .onFailure { error -> recordDebugEvent("recovery", "Failed to stop previous active session ${activeSession.shortDebugId()} error=${error.debugMessage()}") }
                }

                val selectedVariant = game?.variants?.getOrNull(game.selectedVariantIndex) ?: game?.variants?.firstOrNull()
                val launchAppId = resolveFallbackLaunchAppId(
                    token = token,
                    game = game,
                    active = active,
                    baseUrl = baseUrl,
                )
                _state.update { it.copy(launchPhase = "Creating safe H264 session") }
                val created = sessionRepository.createSession(
                    token = token,
                    streamingBaseUrl = baseUrl,
                    appId = launchAppId,
                    internalTitle = game?.title ?: "Cloud session",
                    zone = previousSession?.zone?.takeIf { it.isNotBlank() } ?: "prod",
                    settings = safeSettings,
                    accountLinked = game?.let { shouldSendAccountLinked(it, selectedVariant) } ?: true,
                )
                recordDebugEvent("recovery", "Created safe video session ${created.debugSummary()}")
                pollUntilReady(token, created, safeSettings)
            }.onSuccess { readySession ->
                recordDebugEvent("recovery", "Safe video session ready ${readySession.debugSummary()}")
                _state.update {
                    it.copy(
                        streamSession = readySession,
                        activeStreamSettings = safeSettings,
                        streamStatus = "connecting",
                        launchPhase = "Connecting safe H264 stream",
                        streamLaunchMinimized = false,
                        queuePosition = null,
                        queueAdActiveId = null,
                        page = AppPage.Stream,
                    )
                }
            }.onFailure { error ->
                if (error is CancellationException) return@onFailure
                recordDebugEvent("recovery", "Safe video restart failed error=${error.debugMessage()}")
                _state.update {
                    it.copy(
                        error = normalizeLaunchError(error),
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
        recordDebugEvent("recovery", "Recovery requested reason=${reason.take(DEBUG_EVENT_MESSAGE_LIMIT)} session=${initialSession.debugSummary()} settings=${currentSettings.debugSummary()}")
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
                val activeSessions = sessionRepository.getActiveSessions(token, baseUrl, currentSettings)
                recordDebugEvent("recovery", "Recovery active sessions count=${activeSessions.size} base=${hostForDebug(baseUrl)}")
                val resolvedAppId = runCatching {
                    resolveFallbackLaunchAppId(
                        token = token,
                        game = game,
                        active = active,
                        baseUrl = baseUrl,
                    ).toIntOrNull()
                }.getOrNull()
                val readyCandidate = activeSessions.firstOrNull {
                    it.sessionId == previousSession.sessionId &&
                        it.isReadyForClaim()
                } ?: resolvedAppId?.let { appId ->
                    activeSessions.firstOrNull {
                        it.appId == appId &&
                            it.isReadyForClaim()
                    }
                }
                val fallbackCandidate = readyCandidate
                    ?: previousSession.toRecoveryActiveSession(
                        appId = resolvedAppId ?: active?.appId ?: 0,
                        fallbackActive = active,
                    )
                    ?: error("The running session could not be found anymore, so recovery was not possible.")
                recordDebugEvent("recovery", "Claiming recovery candidate ${fallbackCandidate.debugSummary()}")
                sessionRepository.claimSession(token, fallbackCandidate, currentSettings)
            }.onSuccess { readySession ->
                recordDebugEvent("recovery", "Recovery claim ready ${readySession.debugSummary()}")
                _state.update {
                    it.copy(
                        streamSession = readySession,
                        activeSession = readySession.toActiveRecoverySession(active),
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
                        error = normalizeLaunchError(error),
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

    fun handleExternalLaunchIntent(intent: Intent?) {
        if (intent == null) return
        val uri = intent.data
        if (authRepository.handleOAuthRedirect(uri)) {
            _state.update { it.copy(launchPhase = "Finishing login", error = null) }
            return
        }
        val id = intent.getStringExtra("id")
            ?: intent.getStringExtra("appId")
            ?: intent.getStringExtra("launchAppId")
            ?: uri?.getQueryParameter("id")
            ?: uri?.getQueryParameter("appId")
            ?: uri?.lastPathSegment
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

    private fun GameInfo.withSelectedVariant(variantId: String): GameInfo {
        val selectedIndex = variants.indexOfFirst { it.id == variantId }
        return if (selectedIndex >= 0) copy(selectedVariantIndex = selectedIndex) else this
    }

    fun debugLogText(): String {
        val snapshot = state.value
        val session = snapshot.streamSession
        val codecReport = snapshot.codecReport
        return buildString {
            appendLine("OpenNOW Android diagnostics")
            appendLine("page=${snapshot.page} initializing=${snapshot.initializing} loadingGames=${snapshot.loadingGames}")
            appendLine("user=${snapshot.authSession?.user?.displayName.orEmpty()} tier=${snapshot.subscriptionInfo?.membershipTier ?: snapshot.authSession?.user?.membershipTier.orEmpty()} provider=${snapshot.authSession?.provider?.code.orEmpty()}")
            appendLine("streamStatus=${snapshot.streamStatus} launchPhase=${snapshot.launchPhase} queuePosition=${snapshot.queuePosition}")
            appendLine("streamGame=${snapshot.streamGame?.title.orEmpty()} selectedGame=${snapshot.selectedGame?.title.orEmpty()}")
            appendLine("sessionId=${session?.sessionId.orEmpty()} sessionStatus=${session?.status} seatSetupStep=${session?.seatSetupStep} serverIp=${session?.serverIp.orEmpty()} base=${session?.streamingBaseUrl.orEmpty()}")
            appendLine("adsRequired=${isSessionAdsRequired(session?.adState)} ads=${sessionAdItems(session?.adState).size} activeAd=${snapshot.queueAdActiveId.orEmpty()} queuePaused=${session?.adState?.isQueuePaused}")
            appendLine("adMessage=${session?.adState?.message.orEmpty()} grace=${session?.adState?.gracePeriodSeconds} serverSentEmptyAds=${session?.adState?.serverSentEmptyAds}")
            appendLine("negotiated=${session?.negotiatedStreamProfile?.debugSummary().orEmpty()} requestedFeatures=${session?.requestedStreamingFeatures?.debugSummary().orEmpty()} finalizedFeatures=${session?.finalizedStreamingFeatures?.debugSummary().orEmpty()}")
            appendLine("printedWaste.loading=${snapshot.printedWasteLoading} queueZones=${snapshot.printedWasteQueue.size} mappingZones=${snapshot.printedWasteMapping.size} pings=${snapshot.printedWastePings.size} error=${snapshot.printedWasteError.orEmpty()}")
            appendLine("settings.resolution=${snapshot.settings.stream.resolution} fps=${snapshot.settings.stream.fps} codec=${snapshot.settings.stream.codec} bitrate=${snapshot.settings.stream.maxBitrateMbps}")
            snapshot.activeStreamSettings?.let { active ->
                appendLine("active.resolution=${active.resolution} fps=${active.fps} codec=${active.codec} bitrate=${active.maxBitrateMbps}")
            }
            appendLine("input.keyboardLayout=${snapshot.settings.stream.keyboardLayout} touch=${snapshot.settings.androidTouch}")
            appendLine("codec.native=${codecReport?.nativeRuntimeSummary.orEmpty()} lowPower=${codecReport?.lowPowerGpuProfile} tv=${codecReport?.androidTvProfile}")
            codecReport?.capabilities?.forEach { cap ->
                appendLine("codec.${cap.codec}: decoder=${cap.decoderName ?: "none"} hardware=${cap.hardwareDecoder} webRtc=${cap.webRtcDecoderName ?: "none"} webRtcAvailable=${cap.webRtcDecoderAvailable ?: "unknown"} webRtcHardware=${cap.webRtcHardwareDecoderAvailable ?: "unknown"} encoder=${cap.encoderName ?: "none"}")
            }
            appendLine(NativeInputDiagnostics.snapshot())
            snapshot.error?.let { appendLine("error=$it") }
            val events = debugEventSnapshot()
            appendLine("events.count=${events.size}")
            if (events.isEmpty()) {
                appendLine("events=(empty)")
            } else {
                val formatter = DateFormat.getTimeInstance(DateFormat.MEDIUM, Locale.US)
                events.forEachIndexed { index, event ->
                    appendLine("event.${index + 1} ${formatter.format(Date(event.timestampMs))} [${event.category}] ${event.message}")
                }
            }
        }
    }

    private suspend fun refreshAfterAuth(session: AuthSession) {
        _state.update { it.copy(loadingGames = true, error = null) }
        val baseUrl = effectiveStreamingBaseUrl(session)
        val token = session.tokens.idToken ?: session.tokens.accessToken
        val cachedMain = catalogCacheStore.loadMainGames(session.user.userId, baseUrl)
        val cachedLibrary = catalogCacheStore.loadLibraryGames(session.user.userId, baseUrl)
        val cachedCatalog = catalogCacheStore.loadCatalog(
            userId = session.user.userId,
            providerStreamingBaseUrl = baseUrl,
            searchQuery = state.value.catalogSearch,
            sortId = state.value.catalogSortId,
            filterIds = state.value.catalogFilterIds,
        )
        if (cachedMain != null || cachedLibrary != null || cachedCatalog != null) {
            val cachedMergedLibrary = mergeKnownLibraryGames(
                cachedLibrary.orEmpty(),
                cachedMain.orEmpty(),
                cachedCatalog?.games.orEmpty(),
            )
            _state.update {
                it.copy(
                    games = cachedMain ?: it.games,
                    libraryGames = cachedMergedLibrary.ifEmpty { cachedLibrary ?: it.libraryGames },
                    catalogResult = cachedCatalog ?: it.catalogResult,
                    loadingGames = false,
                    error = null,
                )
            }
        }
        val subscriptionJob = viewModelScope.launch {
            val sub = runCatching {
                val vpcId = catalogRepository.getVpcId(token, session.provider.streamingServiceUrl)
                subscriptionRepository.fetchSubscription(token, session.user.userId, vpcId)
            }.getOrNull()
            _state.update { it.copy(subscriptionInfo = sub) }
        }
        val accountConnectorsJob = viewModelScope.launch {
            _state.update { it.copy(loadingAccountConnectors = true) }
            val connectors = runCatching { accountConnectorRepository.fetchConnectors(token) }.getOrDefault(emptyList())
            _state.update { it.copy(accountConnectors = connectors, loadingAccountConnectors = false) }
        }
        val regionsJob = viewModelScope.launch {
            val regions = runCatching { fetchDynamicRegions(http, token, session.provider.streamingServiceUrl).first }.getOrDefault(emptyList())
            _state.update { it.copy(regions = regions) }
        }
        gamesJob?.cancel()
        gamesJob = viewModelScope.launch {
            runCatching {
                val main = catalogRepository.fetchMainGames(token, baseUrl)
                val library = catalogRepository.fetchLibraryGames(token, baseUrl)
                val catalog = catalogRepository.browseCatalog(token, baseUrl, state.value.catalogSearch, state.value.catalogSortId, state.value.catalogFilterIds)
                val mergedLibrary = mergeKnownLibraryGames(library, main, catalog.games)
                catalogCacheStore.saveMainGames(session.user.userId, baseUrl, main)
                catalogCacheStore.saveLibraryGames(session.user.userId, baseUrl, mergedLibrary)
                catalogCacheStore.saveCatalog(
                    userId = session.user.userId,
                    providerStreamingBaseUrl = baseUrl,
                    searchQuery = state.value.catalogSearch,
                    sortId = state.value.catalogSortId,
                    filterIds = state.value.catalogFilterIds,
                    result = catalog,
                )
                Triple(main, mergedLibrary, catalog)
            }.onSuccess { (main, library, catalog) ->
                _state.update {
                    it.copy(
                        games = main,
                        libraryGames = library,
                        catalogResult = catalog,
                        loadingGames = false,
                        error = null,
                    )
                }
                refreshActiveSession()
            }.onFailure { error ->
                if (error is CancellationException) return@onFailure
                val hasUsableCache = cachedMain != null || cachedLibrary != null || cachedCatalog != null
                _state.update { it.copy(loadingGames = false, error = if (hasUsableCache) null else error.message ?: "Failed to load games") }
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
            val cachedCatalog = catalogCacheStore.loadCatalog(
                userId = auth.user.userId,
                providerStreamingBaseUrl = baseUrl,
                searchQuery = state.value.catalogSearch,
                sortId = state.value.catalogSortId,
                filterIds = state.value.catalogFilterIds,
            )
            _state.update {
                it.copy(
                    loadingGames = cachedCatalog == null,
                    catalogResult = cachedCatalog ?: it.catalogResult,
                    games = cachedCatalog?.games ?: it.games,
                )
            }
            runCatching {
                catalogRepository.browseCatalog(
                    token = auth.tokens.idToken ?: auth.tokens.accessToken,
                    providerStreamingBaseUrl = baseUrl,
                    searchQuery = state.value.catalogSearch,
                    sortId = state.value.catalogSortId,
                    filterIds = state.value.catalogFilterIds,
                )
            }.onSuccess { result ->
                val mergedLibrary = mergeKnownLibraryGames(state.value.libraryGames, result.games)
                catalogCacheStore.saveCatalog(
                    userId = auth.user.userId,
                    providerStreamingBaseUrl = baseUrl,
                    searchQuery = state.value.catalogSearch,
                    sortId = state.value.catalogSortId,
                    filterIds = state.value.catalogFilterIds,
                    result = result,
                )
                _state.update {
                    it.copy(
                        catalogResult = result,
                        loadingGames = false,
                        games = result.games,
                        libraryGames = mergedLibrary.ifEmpty { it.libraryGames },
                    )
                }
            }.onFailure { error ->
                if (error is CancellationException) return@onFailure
                _state.update { it.copy(error = if (cachedCatalog != null) null else error.message ?: "Catalog refresh failed", loadingGames = false) }
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
                    .filter { (zoneId, _) -> isStandardPrintedWasteZoneId(zoneId) && mappingData[zoneId]?.nuked != true }
                    .map { (zoneId, _) ->
                        StreamRegion(name = zoneId, url = printedWasteZoneUrlForId(zoneId), pingMs = null)
                    }
                val pings = printedWasteRepository.pingRegions(regions).associate { it.url to it.pingMs }
                Triple(queueData, mappingData, pings)
            }
        }.onSuccess { (queue, mapping, pings) ->
            val usableZones = queue
                .filter { (zoneId, _) -> isStandardPrintedWasteZoneId(zoneId) && mapping[zoneId]?.nuked != true }
                .keys
            val bestZone = usableZones
                .mapNotNull { zoneId ->
                    val zone = queue[zoneId] ?: return@mapNotNull null
                    val url = printedWasteZoneUrlForId(zoneId)
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
            recordDebugEvent("queue", "PrintedWaste queue load failed error=${error.debugMessage()}")
            _state.update {
                it.copy(
                    printedWasteLoading = false,
                    printedWasteError = error.message ?: "PrintedWaste queue data unavailable",
                )
            }
        }
    }

    private suspend fun refreshActiveSession() {
        val auth = state.value.authSession ?: return
        val token = auth.tokens.idToken ?: auth.tokens.accessToken
        val active = runCatching { sessionRepository.getActiveSessions(token, effectiveStreamingBaseUrl(auth), state.value.settings.stream) }
            .getOrDefault(emptyList())
            .firstOrNull { it.status in setOf(1, 2, 3) }
        _state.update { it.copy(activeSession = active) }
    }

    private suspend fun resumeKnownActiveSession(
        token: String,
        active: ActiveSessionInfo,
        settings: StreamSettings,
        baseUrl: String,
    ): SessionInfo {
        if (active.isReadyForClaim()) {
            recordDebugEvent("queue", "Active session already ready for claim ${active.debugSummary()}")
            _state.update { it.copy(launchPhase = "Resuming session") }
            return sessionRepository.claimSession(token, active, settings)
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
                queuePosition = latest.queuePosition?.takeIf { position -> position > 0 },
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
            return sessionRepository.claimSession(token, hydratedActive, settings)
        }
        return pollUntilReady(token, latest, settings)
    }

    private suspend fun pollUntilReady(token: String, created: SessionInfo, settings: StreamSettings): SessionInfo {
        var latest = created
        var pollCount = 0
        recordDebugEvent("queue", "Begin polling ${latest.debugSummary()}")
        _state.update {
            it.copy(
                streamSession = latest,
                launchPhase = loadingPhaseFor(latest),
                queuePosition = latest.queuePosition?.takeIf { position -> position > 0 },
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
            val polled = sessionRepository.pollSession(
                token = token,
                streamingBaseUrl = latest.streamingBaseUrl ?: effectiveStreamingBaseUrl(),
                serverIp = latest.serverIp,
                zone = latest.zone,
                sessionId = latest.sessionId,
                clientId = latest.clientId,
                deviceId = latest.deviceId,
                settings = settings,
            )
            latest = mergeQueueSessionState(latest, polled)
            recordDebugEvent("queue", "Poll #$pollCount result ${latest.debugSummary()}")
            _state.update {
                it.copy(
                    streamSession = latest,
                    launchPhase = loadingPhaseFor(latest),
                    queuePosition = latest.queuePosition?.takeIf { position -> position > 0 },
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

    private fun isStandardPrintedWasteZoneId(zoneId: String): Boolean =
        zoneId.startsWith("NP-") && !zoneId.startsWith("NPA-")

    private fun printedWasteZoneUrlForId(zoneId: String): String =
        "https://${zoneId.lowercase()}.cloudmatchbeta.nvidiagrid.net/"

    private fun nextQueueAdId(session: SessionInfo, currentAdId: String): String? {
        val ads = sessionAdItems(session.adState)
        val currentIndex = ads.indexOfFirst { it.adId == currentAdId }
        return if (currentIndex >= 0) ads.getOrNull(currentIndex + 1)?.adId else ads.firstOrNull()?.adId
    }

    private fun chooseQueueAdActiveId(currentId: String?, session: SessionInfo?): String? {
        val ads = sessionAdItems(session?.adState)
        if (!isSessionAdsRequired(session?.adState) || ads.isEmpty()) return null
        return ads.firstOrNull { it.adId == currentId }?.adId ?: ads.first().adId
    }

    private fun loadingPhaseFor(session: SessionInfo): String =
        when {
            (session.queuePosition ?: 0) > 0 || session.seatSetupStep == 1 -> "Queue"
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

    private fun SessionInfo.toRecoveryActiveSession(appId: Int, fallbackActive: ActiveSessionInfo?): ActiveSessionInfo? {
        if (serverIp.isBlank() || appId <= 0) return null
        return ActiveSessionInfo(
            sessionId = sessionId,
            appId = appId,
            gpuType = gpuType ?: fallbackActive?.gpuType,
            status = status.takeIf { it in setOf(2, 3) } ?: 2,
            queuePosition = queuePosition,
            seatSetupStep = seatSetupStep,
            streamingBaseUrl = streamingBaseUrl ?: fallbackActive?.streamingBaseUrl,
            serverIp = serverIp,
            signalingUrl = signalingUrl.takeIf { it.isNotBlank() } ?: fallbackActive?.signalingUrl,
            resolution = fallbackActive?.resolution,
            fps = fallbackActive?.fps,
        )
    }

    private fun SessionInfo.toActiveRecoverySession(fallbackActive: ActiveSessionInfo?): ActiveSessionInfo? {
        val appId = fallbackActive?.takeIf { it.sessionId == sessionId }?.appId ?: fallbackActive?.appId ?: return null
        return toRecoveryActiveSession(appId, fallbackActive)
    }

    private fun shouldSendAccountLinked(game: GameInfo, variant: GameVariant?): Boolean {
        return shouldLaunchWithAccountLinked(game, variant)
    }

    private fun normalizeLaunchError(error: Throwable): String {
        val text = error.message ?: return "Launch failed"
        return if (text.contains("patch", true) || text.contains("maintenance", true)) {
            "Game is patching or under maintenance. Try again when NVIDIA finishes updating it."
        } else {
            text
        }
    }

    private fun AuthSession.toSavedAccount(): SavedAccount =
        SavedAccount(
            userId = user.userId,
            displayName = user.displayName,
            email = user.email,
            avatarUrl = user.avatarUrl,
            membershipTier = user.membershipTier,
            providerCode = provider.code,
        )
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
    "res=$resolution aspect=$aspectRatio fps=$fps bitrate=$maxBitrateMbps codec=$codec color=${colorQuality.name} hdr=$hdrEnabled l4s=$enableL4S gsync=$enableCloudGsync sharp=$streamSharpeningEnabled"

private fun SessionInfo.shortDebugId(): String = shortDebugId(sessionId)

private fun SessionInfo.debugSummary(): String =
    "id=${shortDebugId()} status=$status ready=${isReadyForStream()} queue=${queuePosition ?: "-"} seat=${seatSetupStep ?: "-"} adsRequired=${isSessionAdsRequired(adState)} ads=${sessionAdItems(adState).size} paused=${adState?.isQueuePaused} base=${hostForDebug(streamingBaseUrl)} server=${serverIp.take(80)}"

private fun ActiveSessionInfo.shortDebugId(): String = shortDebugId(sessionId)

private fun ActiveSessionInfo.debugSummary(): String =
    "id=${shortDebugId()} app=$appId status=$status queue=${queuePosition ?: "-"} seat=${seatSetupStep ?: "-"} base=${hostForDebug(streamingBaseUrl)} server=${serverIp.orEmpty().take(80)} res=${resolution.orEmpty()} fps=${fps ?: 0}"

private fun NegotiatedStreamProfile.debugSummary(): String =
    "res=${resolution.orEmpty()} fps=${fps ?: 0} codec=${codec?.name.orEmpty()} color=${colorQuality?.name.orEmpty()} l4s=$enableL4S gsync=$enableCloudGsync reflex=$enableReflex"

private fun StreamingFeatures.debugSummary(): String =
    "reflex=$reflex bitDepth=$bitDepth gsync=$cloudGsync chroma=$chromaFormat l4s=$enabledL4S hdr=$trueHdr"

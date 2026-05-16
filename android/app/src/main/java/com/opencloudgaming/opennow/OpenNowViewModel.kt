package com.opencloudgaming.opennow

import android.app.Application
import android.content.Intent
import android.net.Uri
import androidx.lifecycle.AndroidViewModel
import androidx.lifecycle.viewModelScope
import kotlinx.coroutines.Job
import kotlinx.coroutines.CancellationException
import kotlinx.coroutines.async
import kotlinx.coroutines.coroutineScope
import kotlinx.coroutines.flow.MutableStateFlow
import kotlinx.coroutines.flow.StateFlow
import kotlinx.coroutines.flow.asStateFlow
import kotlinx.coroutines.flow.update
import kotlinx.coroutines.launch
import okhttp3.OkHttpClient
import kotlin.math.roundToInt

enum class AppPage {
    Home,
    Library,
    Settings,
    Stream,
}

data class OpenNowUiState(
    val initializing: Boolean = true,
    val page: AppPage = AppPage.Home,
    val authSession: AuthSession? = null,
    val providers: List<LoginProvider> = listOf(defaultProvider()),
    val selectedProvider: LoginProvider = defaultProvider(),
    val savedAccounts: List<SavedAccount> = emptyList(),
    val subscriptionInfo: SubscriptionInfo? = null,
    val regions: List<StreamRegion> = emptyList(),
    val games: List<GameInfo> = emptyList(),
    val libraryGames: List<GameInfo> = emptyList(),
    val catalogResult: CatalogBrowseResult = CatalogBrowseResult(emptyList()),
    val catalogSearch: String = "",
    val librarySearch: String = "",
    val catalogSortId: String = "relevance",
    val catalogFilterIds: List<String> = emptyList(),
    val loadingGames: Boolean = false,
    val settings: AppSettings = AppSettings(),
    val codecReport: RuntimeCodecReport? = null,
    val selectedGame: GameInfo? = null,
    val activeSession: ActiveSessionInfo? = null,
    val streamSession: SessionInfo? = null,
    val activeStreamSettings: StreamSettings? = null,
    val streamGame: GameInfo? = null,
    val streamLaunchMinimized: Boolean = false,
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
)

class OpenNowViewModel(application: Application) : AndroidViewModel(application) {
    private val http: OkHttpClient = defaultHttpClient()
    private val settingsStore = SettingsStore(application)
    private val authStore = AuthStore(application)
    private val authRepository = GfnAuthRepository(application, authStore, http)
    private val catalogRepository = GfnCatalogRepository(http)
    private val catalogCacheStore = CatalogCacheStore(application)
    private val subscriptionRepository = GfnSubscriptionRepository(http)
    private val printedWasteRepository = PrintedWasteRepository(http)
    private val sessionRepository = GfnSessionRepository(authStore, http)

    private val _state = MutableStateFlow(OpenNowUiState(settings = settingsStore.settings.value))
    val state: StateFlow<OpenNowUiState> = _state.asStateFlow()

    private var gamesJob: Job? = null
    private var launchJob: Job? = null
    private var loginJob: Job? = null

    init {
        viewModelScope.launch {
            settingsStore.settings.collect { next ->
                _state.update { it.copy(settings = next) }
            }
        }
        initialize()
    }

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

    fun selectProvider(provider: LoginProvider) {
        _state.update { it.copy(selectedProvider = provider) }
    }

    fun login(provider: LoginProvider = state.value.selectedProvider) {
        loginJob?.cancel()
        loginJob = viewModelScope.launch {
            val useDeviceCode = state.value.codecReport?.androidTvProfile == true
            _state.update {
                it.copy(
                    error = null,
                    launchPhase = if (useDeviceCode) "Requesting TV sign-in code" else "Opening NVIDIA login",
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
                    refreshAfterAuth(session)
                }
                .onFailure { error ->
                    if (error is CancellationException) return@onFailure
                    _state.update { it.copy(error = error.message ?: "Login failed", launchPhase = "", deviceLoginPrompt = null) }
                }
        }
    }

    fun loginWithCode(provider: LoginProvider = state.value.selectedProvider) {
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
            _state.update {
                it.copy(
                    launchPhase = "Requesting sign-in code",
                    error = "Browser sign-in could not reach the local callback. Use this code to finish sign-in.",
                )
            }
            loginWithDeviceCode(provider)
        }
    }

    private suspend fun loginWithDeviceCode(provider: LoginProvider): AuthSession =
        authRepository.loginWithDeviceCode(provider) { prompt ->
            _state.update {
                it.copy(
                    deviceLoginPrompt = prompt,
                    launchPhase = "Waiting for sign-in",
                    error = null,
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
            authRepository.logout()
            _state.update {
                it.copy(
                    authSession = authStore.activeSession(),
                    savedAccounts = authStore.state.value.sessions.map { saved -> saved.toSavedAccount() },
                    games = emptyList(),
                    libraryGames = emptyList(),
                    streamSession = null,
                    activeStreamSettings = null,
                    activeSession = null,
                    deviceLoginPrompt = null,
                    pendingStoreChoiceGame = null,
                    page = AppPage.Home,
                )
            }
        }
    }

    fun logoutAll() {
        authRepository.logoutAll()
        _state.update {
            it.copy(
                authSession = null,
                savedAccounts = emptyList(),
                games = emptyList(),
                libraryGames = emptyList(),
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
        refreshCatalogDebounced()
    }

    fun setLibrarySearch(query: String) {
        _state.update { it.copy(librarySearch = query) }
    }

    fun setCatalogSort(sortId: String) {
        _state.update { it.copy(catalogSortId = sortId) }
        refreshCatalogDebounced()
    }

    fun toggleCatalogFilter(filterId: String) {
        _state.update {
            val filters = if (filterId in it.catalogFilterIds) it.catalogFilterIds - filterId else it.catalogFilterIds + filterId
            it.copy(catalogFilterIds = filters)
        }
        refreshCatalogDebounced()
    }

    fun selectGame(game: GameInfo) {
        _state.update { it.copy(selectedGame = game) }
    }

    fun clearSelectedGame() {
        _state.update { it.copy(selectedGame = null) }
    }

    fun updateSettings(next: AppSettings) {
        settingsStore.replace(next)
    }

    fun updateStreamSettings(transform: (StreamSettings) -> StreamSettings) {
        settingsStore.update { it.copy(stream = transform(it.stream)) }
    }

    fun updateFavorites(gameId: String) {
        settingsStore.update {
            val next = if (gameId in it.favoriteGameIds) it.favoriteGameIds - gameId else it.favoriteGameIds + gameId
            it.copy(favoriteGameIds = next)
        }
    }

    fun dismissStoreChoice() {
        _state.update { it.copy(pendingStoreChoiceGame = null) }
    }

    fun playVariant(game: GameInfo, variant: GameVariant) {
        _state.update { it.copy(pendingStoreChoiceGame = null) }
        play(game.withSelectedVariant(variant.id), skipStoreChoice = true)
    }

    fun play(game: GameInfo, streamingBaseUrlOverride: String? = null, skipPrintedWaste: Boolean = false, skipStoreChoice: Boolean = false) {
        if (launchJob?.isActive == true) return
        if (!skipStoreChoice) {
            val launchVariants = launchableGameVariants(game.variants)
            if (launchVariants.size > 1) {
                _state.update { it.copy(pendingStoreChoiceGame = game, selectedGame = null, error = null) }
                return
            }
        }
        launchJob = viewModelScope.launch {
            val auth = state.value.authSession ?: return@launch
            if (!skipPrintedWaste && streamingBaseUrlOverride == null && shouldUsePrintedWasteQueue(auth)) {
                showPrintedWasteSelector(game)
                return@launch
            }
            val settings = state.value.settings.stream.adjustedForDevice(state.value.codecReport)
            val token = auth.tokens.idToken ?: auth.tokens.accessToken
            val baseUrl = streamingBaseUrlOverride ?: effectiveStreamingBaseUrl()
            _state.update {
                it.copy(
                    streamStatus = "queue",
                    launchPhase = "Resolving game",
                    streamGame = game,
                    activeStreamSettings = settings,
                    selectedGame = null,
                    page = AppPage.Stream,
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

                _state.update { it.copy(launchPhase = "Checking active sessions") }
                val active = sessionRepository.getActiveSessions(token, baseUrl, settings)
                val numericLaunchAppId = launchAppId.toIntOrNull()
                val readyCandidate = active.firstOrNull {
                    it.appId == numericLaunchAppId && it.serverIp != null && it.status in setOf(2, 3)
                }
                val launchingCandidate = active.firstOrNull {
                    it.appId == numericLaunchAppId && it.status == 1
                } ?: active.firstOrNull { it.status == 1 }
                val readySession = when {
                    readyCandidate != null -> {
                        _state.update { it.copy(launchPhase = "Resuming session", activeSession = readyCandidate) }
                        sessionRepository.claimSession(token, readyCandidate, settings)
                    }
                    launchingCandidate != null -> {
                        val pending = launchingCandidate.toPendingSession(zone = "prod")
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
                        }.getOrElse { pending }
                        pollUntilReady(token, mergeQueueSessionState(pending, hydrated), settings)
                    }
                    else -> {
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
                        pollUntilReady(token, created, settings)
                    }
                }
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
                _state.update {
                    it.copy(
                        error = normalizeLaunchError(error),
                        streamStatus = "idle",
                        activeStreamSettings = null,
                        launchPhase = "",
                        streamLaunchMinimized = false,
                        queuePosition = null,
                        queueAdActiveId = null,
                        pendingStoreChoiceGame = null,
                    )
                }
            }
        }
    }

    fun stopStream() {
        launchJob?.cancel()
        launchJob = null
        viewModelScope.launch {
            val auth = state.value.authSession
            val snapshot = state.value
            val session = snapshot.streamSession
            val streamSettings = snapshot.activeStreamSettings ?: state.value.settings.stream.adjustedForDevice(state.value.codecReport)
            if (auth != null && session != null) {
                runCatching { sessionRepository.stopSession(auth.tokens.idToken ?: auth.tokens.accessToken, session, streamSettings) }
            } else if (auth != null) {
                val token = auth.tokens.idToken ?: auth.tokens.accessToken
                val active = snapshot.activeSession
                    ?: runCatching {
                        sessionRepository.getActiveSessions(token, effectiveStreamingBaseUrl(auth), streamSettings)
                            .firstOrNull { it.status in setOf(1, 2, 3) }
                    }.getOrNull()
                if (active != null) {
                    runCatching { sessionRepository.stopActiveSession(token, active, streamSettings) }
                }
            }
            _state.update {
                it.copy(
                    streamSession = null,
                    activeStreamSettings = null,
                    streamGame = null,
                    streamStatus = "idle",
                    streamLaunchMinimized = false,
                    launchPhase = "",
                    queuePosition = null,
                    queueAdActiveId = null,
                    pendingStoreChoiceGame = null,
                    page = AppPage.Home,
                )
            }
            refreshActiveSession()
        }
    }

    fun refreshPrintedWasteQueues() {
        val game = state.value.pendingPrintedWasteGame ?: return
        viewModelScope.launch {
            loadPrintedWasteQueue(game)
        }
    }

    fun minimizeStreamLaunch() {
        _state.update { current ->
            if (current.streamStatus == "idle" || current.streamSession?.status in setOf(2, 3)) {
                current
            } else {
                current.copy(streamLaunchMinimized = true, page = AppPage.Home)
            }
        }
    }

    fun restoreStreamLaunch() {
        _state.update { current ->
            if (current.streamStatus == "idle") current else current.copy(streamLaunchMinimized = false, page = AppPage.Stream)
        }
    }

    fun resumeActiveSession() {
        if (launchJob?.isActive == true) return
        launchJob = viewModelScope.launch {
            val auth = state.value.authSession ?: return@launch
            val settings = state.value.settings.stream.adjustedForDevice(state.value.codecReport)
            val token = auth.tokens.idToken ?: auth.tokens.accessToken
            val baseUrl = effectiveStreamingBaseUrl(auth)
            val cachedActive = state.value.activeSession
            _state.update {
                it.copy(
                    streamStatus = "queue",
                    launchPhase = "Checking active sessions",
                    activeStreamSettings = settings,
                    page = AppPage.Stream,
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
                val matchingGame = (state.value.games + state.value.libraryGames)
                    .firstOrNull { it.launchAppId == active.appId.toString() || it.variants.any { variant -> variant.id == active.appId.toString() } }
                _state.update {
                    it.copy(
                        activeSession = active,
                        streamGame = matchingGame,
                        streamSession = active.toPendingSession(zone = "prod"),
                        activeStreamSettings = settings,
                        launchPhase = if (active.status in setOf(2, 3) && active.serverIp != null) "Resuming session" else loadingPhaseFor(active.toPendingSession(zone = "prod")),
                    )
                }
                resumeKnownActiveSession(token, active, settings, baseUrl)
            }.onSuccess { readySession ->
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
                _state.update {
                    it.copy(
                        error = normalizeLaunchError(error),
                        streamStatus = "idle",
                        activeStreamSettings = null,
                        launchPhase = "",
                        streamLaunchMinimized = false,
                        queuePosition = null,
                        queueAdActiveId = null,
                        pendingStoreChoiceGame = null,
                        pendingPrintedWasteGame = null,
                    )
                }
            }
        }
    }

    fun launchWithPrintedWaste(zoneUrl: String?) {
        val game = state.value.pendingPrintedWasteGame ?: return
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
            val auth = state.value.authSession ?: return@launch
            val session = state.value.streamSession ?: return@launch
            val normalizedAction = action.lowercase()
            if (normalizedAction == "finish" || normalizedAction == "cancel") {
                _state.update {
                    it.copy(queueAdActiveId = nextQueueAdId(session, adId))
                }
            } else {
                _state.update {
                    it.copy(queueAdActiveId = adId)
                }
            }
            runCatching {
                sessionRepository.reportSessionAd(
                    token = auth.tokens.idToken ?: auth.tokens.accessToken,
                    session = session,
                    adId = adId,
                    action = normalizedAction,
                    settings = state.value.settings.stream,
                    watchedTimeInMs = watchedTimeInMs,
                    pausedTimeInMs = pausedTimeInMs ?: 0L,
                    cancelReason = cancelReason,
                    errorInfo = errorInfo,
                )
            }.onSuccess { updated ->
                _state.update { current ->
                    val previous = current.streamSession?.takeIf { it.sessionId == updated.sessionId } ?: session
                    val merged = mergeQueueSessionState(
                        previous,
                        updated,
                        preserveMissingAdState = normalizedAction != "finish" && normalizedAction != "cancel",
                    )
                    current.copy(
                        streamSession = merged,
                        queuePosition = merged.queuePosition?.takeIf { position -> position > 0 },
                        queueAdActiveId = chooseQueueAdActiveId(current.queueAdActiveId, merged),
                    )
                }
            }.onFailure { error ->
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
        _state.update { it.copy(streamStatus = "streaming", launchPhase = "") }
    }

    fun markStreamError(message: String) {
        _state.update { it.copy(error = message, streamStatus = "idle", activeStreamSettings = null, launchPhase = "") }
    }

    fun handleExternalLaunchIntent(intent: Intent?) {
        if (intent == null) return
        val uri = intent.data
        if (authRepository.handleOAuthRedirect(uri)) {
            _state.update { it.copy(launchPhase = "Finishing NVIDIA login", error = null) }
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
            appendLine("settings.resolution=${snapshot.settings.stream.resolution} fps=${snapshot.settings.stream.fps} codec=${snapshot.settings.stream.codec} bitrate=${snapshot.settings.stream.maxBitrateMbps}")
            appendLine("input.keyboardLayout=${snapshot.settings.stream.keyboardLayout} mouseCapture=${snapshot.settings.mouseCapture} touch=${snapshot.settings.androidTouch}")
            appendLine("codec.native=${codecReport?.nativeRuntimeSummary.orEmpty()} lowPower=${codecReport?.lowPowerGpuProfile} tv=${codecReport?.androidTvProfile}")
            codecReport?.capabilities?.forEach { cap ->
                appendLine("codec.${cap.codec}: decoder=${cap.decoderName ?: "none"} hardware=${cap.hardwareDecoder} encoder=${cap.encoderName ?: "none"}")
            }
            appendLine(NativeInputDiagnostics.snapshot())
            snapshot.error?.let { appendLine("error=$it") }
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
            _state.update {
                it.copy(
                    games = cachedMain ?: it.games,
                    libraryGames = cachedLibrary ?: it.libraryGames,
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
                catalogCacheStore.saveMainGames(session.user.userId, baseUrl, main)
                catalogCacheStore.saveLibraryGames(session.user.userId, baseUrl, library)
                catalogCacheStore.saveCatalog(
                    userId = session.user.userId,
                    providerStreamingBaseUrl = baseUrl,
                    searchQuery = state.value.catalogSearch,
                    sortId = state.value.catalogSortId,
                    filterIds = state.value.catalogFilterIds,
                    result = catalog,
                )
                Triple(main, library, catalog)
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
                catalogCacheStore.saveCatalog(
                    userId = auth.user.userId,
                    providerStreamingBaseUrl = baseUrl,
                    searchQuery = state.value.catalogSearch,
                    sortId = state.value.catalogSortId,
                    filterIds = state.value.catalogFilterIds,
                    result = result,
                )
                _state.update { it.copy(catalogResult = result, loadingGames = false, games = result.games) }
            }.onFailure { error ->
                if (error is CancellationException) return@onFailure
                _state.update { it.copy(error = if (cachedCatalog != null) null else error.message ?: "Catalog refresh failed", loadingGames = false) }
            }
        }
    }

    private suspend fun showPrintedWasteSelector(game: GameInfo) {
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
        if (active.status in setOf(2, 3) && active.serverIp != null) {
            _state.update { it.copy(launchPhase = "Resuming session") }
            return sessionRepository.claimSession(token, active, settings)
        }

        val pending = active.toPendingSession(zone = "prod")
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
        }.getOrElse { pending }
        val latest = mergeQueueSessionState(pending, hydrated)
        _state.update {
            it.copy(
                streamSession = latest,
                launchPhase = loadingPhaseFor(latest),
                queuePosition = latest.queuePosition?.takeIf { position -> position > 0 },
                queueAdActiveId = chooseQueueAdActiveId(it.queueAdActiveId, latest),
            )
        }
        if (latest.status in setOf(2, 3) && latest.serverIp.isNotBlank()) {
            val hydratedActive = active.copy(
                status = latest.status,
                queuePosition = latest.queuePosition,
                seatSetupStep = latest.seatSetupStep,
                streamingBaseUrl = latest.streamingBaseUrl ?: active.streamingBaseUrl,
                serverIp = latest.serverIp,
                signalingUrl = latest.signalingUrl,
            )
            _state.update { it.copy(launchPhase = "Resuming session") }
            return sessionRepository.claimSession(token, hydratedActive, settings)
        }
        return pollUntilReady(token, latest, settings)
    }

    private suspend fun pollUntilReady(token: String, created: SessionInfo, settings: StreamSettings): SessionInfo {
        var latest = created
        _state.update {
            it.copy(
                streamSession = latest,
                launchPhase = loadingPhaseFor(latest),
                queuePosition = latest.queuePosition?.takeIf { position -> position > 0 },
                queueAdActiveId = chooseQueueAdActiveId(it.queueAdActiveId, latest),
            )
        }
        while (latest.status !in setOf(2, 3)) {
            val waitMs = if (shouldWaitForQueueAdPlayback(latest.adState)) 30_000L else 2_000L
            if (waitMs > 2_000L) {
                var elapsedMs = 0L
                while (elapsedMs < waitMs) {
                    kotlinx.coroutines.delay(500L)
                    elapsedMs += 500L
                    state.value.streamSession
                        ?.takeIf { it.sessionId == latest.sessionId }
                        ?.let { latest = mergeQueueSessionState(latest, it) }
                    if (!shouldWaitForQueueAdPlayback(latest.adState) || latest.status in setOf(2, 3)) {
                        break
                    }
                }
            } else {
                kotlinx.coroutines.delay(waitMs)
            }
            if (latest.status in setOf(2, 3)) {
                break
            }
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
            _state.update {
                it.copy(
                    streamSession = latest,
                    launchPhase = loadingPhaseFor(latest),
                    queuePosition = latest.queuePosition?.takeIf { position -> position > 0 },
                    queueAdActiveId = chooseQueueAdActiveId(it.queueAdActiveId, latest),
                )
            }
        }
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

    private fun StreamSettings.adjustedForDevice(report: RuntimeCodecReport?): StreamSettings {
        val aspectMatched = fitToDisplayAspect()
        if (report?.lowPowerGpuProfile == true) {
            return aspectMatched.copy(codec = VideoCodec.H264, colorQuality = ColorQuality.EightBit420, maxBitrateMbps = minOf(maxBitrateMbps, 25), fps = minOf(fps, 60))
        }
        if (report?.androidTvProfile == true) {
            return aspectMatched.copy(codec = VideoCodec.H264, colorQuality = ColorQuality.EightBit420, resolution = capResolution(aspectMatched.resolution, 1920, 1080), maxBitrateMbps = minOf(maxBitrateMbps, 35), fps = minOf(fps, 60))
        }
        val capability = report?.capabilities?.firstOrNull { it.codec == codec }
        val codecSafe = capability?.let { it.decoderAvailable && it.realtimeSafe } ?: (codec == VideoCodec.H264)
        return if (codec == VideoCodec.H264 && codecSafe) {
            aspectMatched.copy(colorQuality = ColorQuality.EightBit420, maxBitrateMbps = minOf(maxBitrateMbps, 45), fps = minOf(fps, 60))
        } else {
            aspectMatched.copy(codec = VideoCodec.H264, colorQuality = ColorQuality.EightBit420, maxBitrateMbps = minOf(maxBitrateMbps, 35), fps = minOf(fps, 60))
        }
    }

    private fun StreamSettings.fitToDisplayAspect(): StreamSettings {
        val displayMetrics = getApplication<Application>().resources.displayMetrics
        val longSide = maxOf(displayMetrics.widthPixels, displayMetrics.heightPixels).coerceAtLeast(1)
        val shortSide = minOf(displayMetrics.widthPixels, displayMetrics.heightPixels).coerceAtLeast(1)
        val displayAspect = longSide.toFloat() / shortSide.toFloat()
        if (displayAspect < 1.25f || displayAspect > 2.5f) return this

        val baseWidth = resolution.substringBefore("x").toIntOrNull() ?: return this
        val baseHeight = resolution.substringAfter("x").toIntOrNull() ?: return this
        val fittedHeight = makeEven((baseWidth / displayAspect).roundToInt()).coerceAtLeast(360)
        val (width, height) = if (fittedHeight <= baseHeight) {
            baseWidth to fittedHeight
        } else {
            makeEven((baseHeight * displayAspect).roundToInt()).coerceAtLeast(640) to baseHeight
        }
        return copy(resolution = "${width}x$height", aspectRatio = "$width:$height")
    }

    private fun capResolution(value: String, maxWidth: Int, maxHeight: Int): String {
        val width = value.substringBefore("x").toIntOrNull() ?: return "${maxWidth}x$maxHeight"
        val height = value.substringAfter("x").toIntOrNull() ?: return "${maxWidth}x$maxHeight"
        return if (width <= maxWidth && height <= maxHeight) value else "${maxWidth}x$maxHeight"
    }

    private fun makeEven(value: Int): Int = if (value % 2 == 0) value else value + 1

    private fun shouldSendAccountLinked(game: GameInfo, variant: GameVariant?): Boolean {
        val store = variant?.store?.lowercase().orEmpty()
        return !store.contains("epic") || game.isInLibrary || variant?.libraryStatus in setOf("MANUAL", "PLATFORM_SYNC", "IN_LIBRARY")
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

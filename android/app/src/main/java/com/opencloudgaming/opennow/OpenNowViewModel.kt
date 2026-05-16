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
    val streamGame: GameInfo? = null,
    val streamLaunchMinimized: Boolean = false,
    val launchPhase: String = "",
    val queuePosition: Int? = null,
    val streamStatus: String = "idle",
    val error: String? = null,
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
        viewModelScope.launch {
            _state.update { it.copy(error = null, launchPhase = "Opening NVIDIA login") }
            runCatching { authRepository.login(provider) }
                .onSuccess { session ->
                    _state.update {
                        it.copy(
                            authSession = session,
                            selectedProvider = session.provider,
                            savedAccounts = authStore.state.value.sessions.map { saved -> saved.toSavedAccount() },
                            launchPhase = "",
                            error = null,
                        )
                    }
                    refreshAfterAuth(session)
                }
                .onFailure { error ->
                    _state.update { it.copy(error = error.message ?: "Login failed", launchPhase = "") }
                }
        }
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
                    activeSession = null,
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
                activeSession = null,
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
                    selectedGame = null,
                    page = AppPage.Stream,
                    streamLaunchMinimized = false,
                    error = null,
                    queuePosition = null,
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
                val active = sessionRepository.getActiveSessions(token, baseUrl)
                val matching = active.firstOrNull { it.appId == launchAppId.toIntOrNull() && it.status in setOf(2, 3) }
                val readySession = if (matching != null) {
                    _state.update { it.copy(launchPhase = "Resuming session", activeSession = matching) }
                    sessionRepository.claimSession(token, matching, settings)
                } else {
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
                _state.update {
                    it.copy(
                        streamSession = readySession,
                        streamStatus = "connecting",
                        launchPhase = "Connecting stream",
                        streamLaunchMinimized = false,
                        queuePosition = null,
                        page = AppPage.Stream,
                    )
                }
            }.onFailure { error ->
                if (error is CancellationException) return@onFailure
                _state.update {
                    it.copy(
                        error = normalizeLaunchError(error),
                        streamStatus = "idle",
                        launchPhase = "",
                        streamLaunchMinimized = false,
                        queuePosition = null,
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
            val session = state.value.streamSession
            if (auth != null && session != null) {
                runCatching { sessionRepository.stopSession(auth.tokens.idToken ?: auth.tokens.accessToken, session) }
            }
            _state.update {
                it.copy(
                    streamSession = null,
                    streamGame = null,
                    streamStatus = "idle",
                    streamLaunchMinimized = false,
                    launchPhase = "",
                    queuePosition = null,
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

    fun reportQueueAd(adId: String, action: String) {
        viewModelScope.launch {
            val auth = state.value.authSession ?: return@launch
            val session = state.value.streamSession ?: return@launch
            runCatching {
                sessionRepository.reportSessionAd(auth.tokens.idToken ?: auth.tokens.accessToken, session, adId, action)
            }.onSuccess { updated ->
                _state.update { it.copy(streamSession = updated, queuePosition = updated.queuePosition?.takeIf { position -> position > 0 }) }
            }.onFailure { error ->
                _state.update { it.copy(error = error.message ?: "Queue ad update failed") }
            }
        }
    }

    fun markStreamConnected() {
        _state.update { it.copy(streamStatus = "streaming", launchPhase = "") }
    }

    fun markStreamError(message: String) {
        _state.update { it.copy(error = message, streamStatus = "idle", launchPhase = "") }
    }

    fun handleExternalLaunchIntent(intent: Intent?) {
        if (intent == null) return
        val uri = intent.data
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
            appendLine("adsRequired=${session?.adState?.isAdsRequired} ads=${session?.adState?.sessionAds?.size ?: 0} queuePaused=${session?.adState?.isQueuePaused}")
            appendLine("settings.resolution=${snapshot.settings.stream.resolution} fps=${snapshot.settings.stream.fps} codec=${snapshot.settings.stream.codec} bitrate=${snapshot.settings.stream.maxBitrateMbps}")
            appendLine("input.keyboardLayout=${snapshot.settings.stream.keyboardLayout} mouseCapture=${snapshot.settings.mouseCapture} touch=${snapshot.settings.androidTouch}")
            appendLine("codec.native=${codecReport?.nativeRuntimeSummary.orEmpty()} lowPower=${codecReport?.lowPowerGpuProfile} tv=${codecReport?.androidTvProfile}")
            codecReport?.capabilities?.forEach { cap ->
                appendLine("codec.${cap.codec}: decoder=${cap.decoderName ?: "none"} hardware=${cap.hardwareDecoder} encoder=${cap.encoderName ?: "none"}")
            }
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
        val active = runCatching { sessionRepository.getActiveSessions(token, effectiveStreamingBaseUrl(auth)) }
            .getOrDefault(emptyList())
            .firstOrNull { it.status in setOf(2, 3) }
        _state.update { it.copy(activeSession = active) }
    }

    private suspend fun pollUntilReady(token: String, created: SessionInfo, settings: StreamSettings): SessionInfo {
        var latest = created
        _state.update {
            it.copy(
                streamSession = latest,
                launchPhase = loadingPhaseFor(latest),
                queuePosition = latest.queuePosition?.takeIf { position -> position > 0 },
            )
        }
        while (latest.status !in setOf(2, 3)) {
            val waitMs = if (latest.adState?.isAdsRequired == true) 30_000L else 2_000L
            kotlinx.coroutines.delay(waitMs)
            latest = sessionRepository.pollSession(
                token = token,
                streamingBaseUrl = latest.streamingBaseUrl ?: effectiveStreamingBaseUrl(),
                serverIp = latest.serverIp,
                zone = latest.zone,
                sessionId = latest.sessionId,
                clientId = latest.clientId,
                deviceId = latest.deviceId,
            )
            _state.update {
                it.copy(
                    streamSession = latest,
                    launchPhase = loadingPhaseFor(latest),
                    queuePosition = latest.queuePosition?.takeIf { position -> position > 0 },
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

    private fun loadingPhaseFor(session: SessionInfo): String =
        when {
            (session.queuePosition ?: 0) > 0 || session.seatSetupStep == 1 -> "Queue"
            session.status == 0 || session.status == 1 -> "Checking queue"
            else -> "Setting up rig"
        }

    private fun StreamSettings.adjustedForDevice(report: RuntimeCodecReport?): StreamSettings {
        if (report?.lowPowerGpuProfile == true) {
            return copy(codec = VideoCodec.H264, colorQuality = ColorQuality.EightBit420, maxBitrateMbps = minOf(maxBitrateMbps, 25), fps = minOf(fps, 60))
        }
        if (report?.androidTvProfile == true) {
            return copy(codec = VideoCodec.H264, colorQuality = ColorQuality.EightBit420, resolution = capResolution(resolution, 1920, 1080), maxBitrateMbps = minOf(maxBitrateMbps, 35), fps = minOf(fps, 60))
        }
        val codecSupported = report?.capabilities?.firstOrNull { it.codec == codec }?.decoderAvailable ?: true
        return if (codecSupported) this else copy(codec = VideoCodec.H264, colorQuality = ColorQuality.EightBit420)
    }

    private fun capResolution(value: String, maxWidth: Int, maxHeight: Int): String {
        val width = value.substringBefore("x").toIntOrNull() ?: return "${maxWidth}x$maxHeight"
        val height = value.substringAfter("x").toIntOrNull() ?: return "${maxWidth}x$maxHeight"
        return if (width <= maxWidth && height <= maxHeight) value else "${maxWidth}x$maxHeight"
    }

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

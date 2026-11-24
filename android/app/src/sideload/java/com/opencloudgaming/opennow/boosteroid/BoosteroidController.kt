package com.opencloudgaming.opennow.boosteroid

import android.app.Application
import android.view.KeyEvent
import android.view.MotionEvent
import androidx.compose.runtime.Composable
import androidx.compose.runtime.getValue
import androidx.lifecycle.compose.collectAsStateWithLifecycle
import com.opencloudgaming.opennow.ActiveSessionInfo
import com.opencloudgaming.opennow.BOOSTEROID_PROVIDER_CODE
import com.opencloudgaming.opennow.BoosteroidIntegration
import com.opencloudgaming.opennow.BoosteroidPresentation
import com.opencloudgaming.opennow.OpenNowUiState
import com.opencloudgaming.opennow.SavedAccount
import com.opencloudgaming.opennow.streamResolutionPixels
import kotlinx.coroutines.CoroutineScope
import kotlinx.coroutines.flow.map
import com.opencloudgaming.opennow.AuthUser
import com.opencloudgaming.opennow.ColorQuality
import com.opencloudgaming.opennow.DeviceLoginPrompt
import com.opencloudgaming.opennow.GameInfo
import com.opencloudgaming.opennow.R
import com.opencloudgaming.opennow.StreamSettings
import com.opencloudgaming.opennow.VideoCodec
import com.opencloudgaming.opennow.androidSafeVideoFallback
import kotlinx.coroutines.Dispatchers
import kotlinx.coroutines.Job
import kotlinx.coroutines.cancelAndJoin
import kotlinx.coroutines.coroutineScope
import kotlinx.coroutines.delay
import kotlinx.coroutines.flow.MutableStateFlow
import kotlinx.coroutines.flow.asStateFlow
import kotlinx.coroutines.flow.update
import kotlinx.coroutines.launch
import kotlinx.coroutines.withContext
import kotlinx.coroutines.withTimeoutOrNull

private data class BoosteroidUiState(
    val initialized: Boolean = false,
    val user: AuthUser? = null,
    val loginPrompt: DeviceLoginPrompt? = null,
    val signingIn: Boolean = false,
    val active: Boolean = false,
    val store: List<GameInfo> = emptyList(),
    val library: List<GameInfo> = emptyList(),
    val query: String = "",
    val loading: Boolean = false,
    val loadingLibrary: Boolean = false,
    val selectedGame: GameInfo? = null,
    val connected: Boolean = false,
    val configuredStreamSettings: StreamSettings = StreamSettings(),
    val membership: String? = null,
    val activeSession: BoosteroidActiveSession? = null,
    val launch: BoosteroidLaunch? = null,
    val stopping: Boolean = false,
    val error: Int? = null,
)

internal data class BoosteroidLaunch(
    val title: String?,
    val settings: StreamSettings,
    val session: BoosteroidSession? = null,
)

internal class BoosteroidController(
    application: Application,
    private val scope: CoroutineScope,
) : BoosteroidIntegration {
    private lateinit var accountStore: BoosteroidAccountStore
    private lateinit var http: BoosteroidHttp
    private lateinit var catalog: BoosteroidCatalogRepository
    private lateinit var sessions: BoosteroidSessionRepository
    private lateinit var subscription: BoosteroidSubscriptionRepository
    private val _state = MutableStateFlow(BoosteroidUiState())
    private val state = _state.asStateFlow()
    override val active: Boolean get() = state.value.active
    override val busy: Boolean get() = state.value.launch != null || state.value.stopping || state.value.signingIn
    override val presentation = state.map { screen ->
        BoosteroidPresentation(
            active = screen.active,
            account = screen.user?.let { user ->
                SavedAccount(
                    userId = user.userId,
                    displayName = user.displayName,
                    email = user.email,
                    avatarUrl = user.avatarUrl,
                    membershipTier = screen.membership ?: user.membershipTier,
                    providerCode = BOOSTEROID_PROVIDER_CODE,
                )
            },
            loginPrompt = screen.loginPrompt,
            signingIn = screen.signingIn,
            error = screen.error,
            store = screen.store,
            library = screen.library,
            search = screen.query,
            loading = screen.loading || screen.loadingLibrary,
            selectedGame = screen.selectedGame,
            resumable = screen.activeSession?.let {
                ActiveSessionInfo(sessionId = it.sessionId, appId = it.appId, status = it.status, queuePosition = it.queuePosition)
            },
            streamStatus = when {
                screen.stopping -> "stopping"
                screen.connected -> "streaming"
                screen.launch?.session?.isReadyForStream() == true -> "connecting"
                screen.launch != null -> "queue"
                else -> "idle"
            },
            streamSettings = screen.launch?.settings,
            configuredStreamSettings = screen.configuredStreamSettings,
        )
    }

    private var streamClient: BoosteroidStreamClient? = null
    private var streamMenuOpen = false
    private var loginJob: Job? = null
    private var catalogJob: Job? = null
    private var accountJob: Job? = null
    private var libraryJob: Job? = null
    private var launchJob: Job? = null
    private var recoveryAttempted = false
    private var installedCatalogJob: Job? = null
    private var pendingActivation = false

    private data class LaunchResult(val session: BoosteroidSession?)

    init {
        scope.launch {
            accountStore = withContext(Dispatchers.IO) { BoosteroidAccountStore(application) }
            http = BoosteroidHttp(accountStore)
            catalog = BoosteroidCatalogRepository(http)
            subscription = BoosteroidSubscriptionRepository(http)
            sessions = BoosteroidSessionRepository(accountStore, http)
            _state.value = BoosteroidUiState(
                initialized = true,
                user = accountStore.account?.user,
                active = accountStore.isActive && accountStore.account != null,
                configuredStreamSettings = withContext(Dispatchers.IO) {
                    boosteroidStreamSettings(accountStore.loadStreamSettings())
                },
            )
            if (pendingActivation) signInOrSwitch()
            else if (active) refresh()
        }
    }

    override fun signInOrSwitch() {
        if (!state.value.initialized) {
            pendingActivation = true
            return
        }
        pendingActivation = false
        if (state.value.signingIn || state.value.stopping) return
        if (state.value.user != null) {
            activate()
            return
        }
        _state.update { it.copy(signingIn = true, error = null) }
        loginJob = scope.launch {
            boosteroidResult { http.login { prompt -> _state.update { it.copy(loginPrompt = prompt) } } }
                .onSuccess { account ->
                    _state.update { it.copy(user = account.user, signingIn = false, loginPrompt = null) }
                    activate()
                }
                .onFailure {
                    _state.update {
                        it.copy(signingIn = false, loginPrompt = null, error = R.string.boosteroid_sign_in_failed)
                    }
                }
        }
    }

    private fun activate() {
        accountStore.isActive = true
        _state.update { it.copy(active = true, error = null, selectedGame = null) }
        refresh()
    }

    override fun cancelSignIn() {
        pendingActivation = false
        loginJob?.cancel()
        _state.update { it.copy(signingIn = false, loginPrompt = null) }
    }

    override fun switchAway() {
        if (state.value.launch != null || state.value.stopping) return
        cancelSignIn()
        catalogJob?.cancel()
        installedCatalogJob?.cancel()
        accountJob?.cancel()
        if (::accountStore.isInitialized) accountStore.isActive = false
        _state.update { it.copy(active = false, selectedGame = null, error = null, loading = false, loadingLibrary = false) }
    }

    override fun signOut() {
        if (state.value.launch != null || state.value.stopping || !::accountStore.isInitialized) return
        switchAway()
        libraryJob?.cancel()
        _state.update { BoosteroidUiState(configuredStreamSettings = it.configuredStreamSettings) }
        scope.launch {
            http.logout()
            _state.update { it.copy(initialized = true) }
        }
    }

    override fun refresh() {
        val token = token() ?: return
        refreshCatalog()
        refreshLibrary()
        accountJob?.cancel()
        accountJob = scope.launch {
            coroutineScope {
                launch {
                    val plan = boosteroidResult { subscription.fetchSubscription(token) }.getOrNull()
                    _state.update { it.copy(membership = plan?.membershipTier) }
                }
                launch { refreshActiveSession(token) }
            }
        }
    }

    override fun updateStreamSettings(settings: StreamSettings) {
        if (!::accountStore.isInitialized) return
        val next = boosteroidStreamSettings(settings)
        if (next == state.value.configuredStreamSettings) return
        accountStore.saveStreamSettings(next)
        _state.update { it.copy(configuredStreamSettings = next) }
    }

    override fun search(query: String) {
        _state.update { it.copy(query = query, store = emptyList()) }
        refreshCatalog(debounce = true)
    }

    override fun selectGame(game: GameInfo?) {
        _state.update { it.copy(selectedGame = game) }
    }

    private fun refreshCatalog(debounce: Boolean = false) {
        val token = token() ?: return
        catalogJob?.cancel()
        val query = state.value.query
        _state.update { it.copy(loading = true) }
        catalogJob = scope.launch {
            if (debounce) delay(300)
            loadGames(token, query, library = false)
        }
    }

    private fun refreshLibrary() {
        val token = token() ?: return
        installedCatalogJob?.cancel()
        _state.update { it.copy(loadingLibrary = true) }
        installedCatalogJob = scope.launch { loadGames(token, "", library = true) }
    }

    private suspend fun loadGames(token: String, query: String, library: Boolean) {
        var games = emptyList<GameInfo>()
        for (page in 1..MAX_CATALOG_PAGES) {
            val results = boosteroidResult {
                withContext(Dispatchers.IO) { catalog.games(token, query, page, library) }
            }.getOrElse { error ->
                if (error is BoosteroidApiException && error.statusCode == 401) expireAccount()
                else _state.update {
                    it.copy(
                        loading = if (library) it.loading else false,
                        loadingLibrary = if (library) false else it.loadingLibrary,
                        error = R.string.boosteroid_catalog_failed,
                    )
                }
                return
            }
            val next = (games + results).distinctBy(GameInfo::id)
            val more = next.size > games.size && results.size >= BOOSTEROID_DEFAULT_PAGE_SIZE && page < MAX_CATALOG_PAGES
            games = next
            _state.update {
                if (library) it.copy(library = games, loadingLibrary = false)
                else it.copy(store = games, loading = false)
            }
            if (!more) return
        }
    }

    override fun setInstalled(game: GameInfo, installed: Boolean) {
        val id = game.launchAppId?.toIntOrNull() ?: return
        updateLibrary { token -> catalog.setInstalled(token, id, installed) }
    }

    override fun syncLibrary(platform: String) = updateLibrary { token -> catalog.synchronizePlatform(token, platform) }

    private fun updateLibrary(action: suspend (String) -> Unit) {
        val token = token() ?: return
        if (libraryJob?.isActive == true) return
        libraryJob = scope.launch {
            boosteroidResult { action(token) }
                .onSuccess {
                    selectGame(null)
                    refreshLibrary()
                    refreshCatalog()
                }
                .onFailure { _state.update { it.copy(error = R.string.boosteroid_library_failed) } }
        }
    }

    override fun play(game: GameInfo, settings: StreamSettings) {
        val id = game.launchAppId?.toIntOrNull()?.takeIf { it > 0 } ?: return
        val token = token() ?: return
        if (state.value.launch != null || state.value.stopping) return
        recoveryAttempted = false
        launch(game.title, boosteroidStreamSettings(settings)) {
            val active = sessions.getActiveSessions(token).firstOrNull()
            if (active != null) {
                _state.update { it.copy(activeSession = active) }
                if (active.appId != id) return@launch null
                sessions.claimSession(token, active)
            } else {
                sessions.createSession(token, id.toString())
            }
        }
    }

    override fun resume(settings: StreamSettings) {
        val active = state.value.activeSession ?: return
        val token = token() ?: return
        if (state.value.launch != null || state.value.stopping) return
        recoveryAttempted = false
        val title = (state.value.store + state.value.library).firstOrNull { it.launchAppId == active.appId.toString() }?.title
        launch(title, boosteroidStreamSettings(settings)) { sessions.claimSession(token, active) }
    }

    private fun launch(
        title: String?,
        settings: StreamSettings,
        start: suspend () -> BoosteroidSession?,
    ) {
        val token = token() ?: return
        _state.update { it.copy(launch = BoosteroidLaunch(title, settings), connected = false, error = null, selectedGame = null) }
        launchJob = scope.launch {
            boosteroidResult {
                val result = withTimeoutOrNull(240_000L) {
                    var session = start() ?: return@withTimeoutOrNull LaunchResult(null)
                    while (!session.isReadyForStream()) {
                        val pending = session
                        _state.update { it.copy(launch = BoosteroidLaunch(title, settings, pending)) }
                        delay(2_000L)
                        session = sessions.pollSession(token, pending.sessionId)
                    }
                    LaunchResult(session)
                } ?: error("Boosteroid launch timed out.")
                result.session
            }.onSuccess { session ->
                _state.update { it.copy(launch = session?.let { BoosteroidLaunch(title, settings, it) }) }
            }.onFailure { error ->
                sessions.closePendingLaunches()
                if (error is BoosteroidApiException && error.statusCode == 401) {
                    expireAccount()
                } else {
                    _state.update { it.copy(launch = null, error = R.string.boosteroid_launch_failed) }
                    refreshActiveSession(token)
                }
            }
        }
    }

    fun recover(session: BoosteroidSession, fallback: Boolean) {
        val launch = state.value.launch ?: return
        if (launch.session !== session || state.value.stopping) return
        val token = token() ?: return
        if (recoveryAttempted) {
            streamError(session)
            return
        }
        recoveryAttempted = true
        val settings = if (fallback) launch.settings.androidSafeVideoFallback() else launch.settings
        launch(launch.title, settings) {
            val active = sessions.getActiveSessions(token).firstOrNull { it.sessionId == session.sessionId }
                ?: error("The Boosteroid session ended.")
            sessions.claimSession(token, active)
        }
    }

    fun streamError(session: BoosteroidSession) {
        if (state.value.launch?.session !== session || state.value.stopping) return
        _state.update { it.copy(launch = null, connected = false, error = R.string.boosteroid_session_failed) }
        token()?.let { token -> scope.launch { refreshActiveSession(token) } }
    }

    override fun stop() {
        val token = token() ?: return
        if (state.value.stopping) return
        _state.update { it.copy(stopping = true, launch = null, connected = false, error = null) }
        scope.launch {
            launchJob?.cancelAndJoin()
            boosteroidResult { sessions.stopSession(token) }
                .onSuccess { _state.update { it.copy(activeSession = null) } }
                .onFailure { _state.update { it.copy(error = R.string.boosteroid_stop_failed) } }
            refreshActiveSession(token)
            _state.update { it.copy(stopping = false) }
        }
    }

    override fun dismissError() {
        _state.update { it.copy(error = null) }
    }

    override fun dispatchKey(event: KeyEvent): Boolean =
        !streamMenuOpen && event.keyCode !in androidSystemKeys && streamClient?.dispatchKey(event) == true

    override fun dispatchMotion(event: MotionEvent): Boolean =
        !streamMenuOpen && streamClient?.dispatchMotion(event) == true

    override fun releaseInput() {
        streamClient?.releaseInput()
    }

    @Composable
    override fun Stream(state: OpenNowUiState) {
        val screen by this.state.collectAsStateWithLifecycle()
        val launch = screen.launch
        val session = launch?.session
        if (launch != null && session?.isReadyForStream() == true) {
            BoosteroidStreamScreen(
                launch = launch,
                session = session,
                tv = state.androidTvProfile,
                touchSettings = state.settings.androidTouch,
                model = this,
                onFirstFrame = {
                    _state.update { if (it.launch?.session === session) it.copy(connected = true) else it }
                },
                onInputChanged = { client, menuOpen ->
                    if (streamClient !== client || (menuOpen && !streamMenuOpen)) releaseInput()
                    streamClient = client
                    streamMenuOpen = menuOpen
                },
            )
        } else {
            BoosteroidLaunchStatus(launch, screen.stopping, ::stop)
        }
    }

    private fun boosteroidStreamSettings(settings: StreamSettings): StreamSettings {
        val (width, height) = streamResolutionPixels(settings)
        val resolution = when {
            width >= 3840 && height >= 2160 -> "3840x2160"
            width >= 2560 && height >= 1440 -> "2560x1440"
            width >= 1920 && height >= 1080 -> "1920x1080"
            else -> "1280x720"
        }
        return StreamSettings(
            resolution = resolution,
            fps = if (settings.fps >= 120) 120 else 60,
            maxBitrateMbps = settings.maxBitrateMbps,
            codec = if (settings.codec == VideoCodec.AV1) VideoCodec.AV1 else VideoCodec.H264,
            colorQuality = ColorQuality.EightBit420,
            mouseSensitivity = settings.mouseSensitivity,
            mouseAcceleration = settings.mouseAcceleration,
        )
    }

    private suspend fun refreshActiveSession(token: String) {
        boosteroidResult { sessions.getActiveSessions(token).firstOrNull() }
            .onSuccess { active -> _state.update { it.copy(activeSession = active) } }
    }

    private fun expireAccount() {
        launchJob?.cancel()
        sessions.closePendingLaunches()
        accountJob?.cancel()
        installedCatalogJob?.cancel()
        catalogJob?.cancel()
        libraryJob?.cancel()
        accountStore.isActive = false
        accountStore.save(null)
        _state.update {
            BoosteroidUiState(
                initialized = true,
                configuredStreamSettings = it.configuredStreamSettings,
                error = R.string.boosteroid_sign_in_expired,
            )
        }
    }

    private fun token(): String? = if (::accountStore.isInitialized) accountStore.account?.tokens?.accessToken else null
    private companion object {
        const val MAX_CATALOG_PAGES = 40
        val androidSystemKeys = setOf(
            KeyEvent.KEYCODE_BACK,
            KeyEvent.KEYCODE_MENU,
            KeyEvent.KEYCODE_VOLUME_UP,
            KeyEvent.KEYCODE_VOLUME_DOWN,
            KeyEvent.KEYCODE_VOLUME_MUTE,
        )
    }
}

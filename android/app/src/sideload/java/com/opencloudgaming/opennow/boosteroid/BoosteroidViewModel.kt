package com.opencloudgaming.opennow.boosteroid

import android.app.Application
import androidx.lifecycle.AndroidViewModel
import androidx.lifecycle.viewModelScope
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

internal data class BoosteroidUiState(
    val initialized: Boolean = false,
    val user: AuthUser? = null,
    val loginPrompt: DeviceLoginPrompt? = null,
    val signingIn: Boolean = false,
    val games: List<GameInfo> = emptyList(),
    val query: String = "",
    val library: Boolean = false,
    val loading: Boolean = false,
    val hasMore: Boolean = false,
    val settings: StreamSettings = StreamSettings(),
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

internal class BoosteroidViewModel(application: Application) : AndroidViewModel(application) {
    private lateinit var accountStore: BoosteroidAccountStore
    private lateinit var http: BoosteroidHttp
    private lateinit var catalog: BoosteroidCatalogRepository
    private lateinit var sessions: BoosteroidSessionRepository
    private lateinit var subscription: BoosteroidSubscriptionRepository
    private val _state = MutableStateFlow(BoosteroidUiState())
    val state = _state.asStateFlow()
    private var loginJob: Job? = null
    private var catalogJob: Job? = null
    private var accountJob: Job? = null
    private var libraryJob: Job? = null
    private var launchJob: Job? = null
    private var recoveryAttempted = false
    private var page = 0

    private data class LaunchResult(val session: BoosteroidSession?)

    init {
        viewModelScope.launch {
            accountStore = withContext(Dispatchers.IO) { BoosteroidAccountStore(application) }
            http = BoosteroidHttp(accountStore)
            catalog = BoosteroidCatalogRepository(http)
            subscription = BoosteroidSubscriptionRepository(http)
            sessions = BoosteroidSessionRepository(accountStore, http)
            _state.value = BoosteroidUiState(
                initialized = true,
                user = accountStore.account?.user,
                settings = withContext(Dispatchers.IO) { accountStore.loadSettings() },
            )
            refreshAccount()
        }
    }

    fun signIn() {
        if (!state.value.initialized || state.value.signingIn || state.value.user != null) return
        _state.update { it.copy(signingIn = true, error = null) }
        loginJob = viewModelScope.launch {
            boosteroidResult { http.login { prompt -> _state.update { it.copy(loginPrompt = prompt) } } }
                .onSuccess { account ->
                    _state.update { it.copy(user = account.user, signingIn = false, loginPrompt = null) }
                    refreshAccount()
                }
                .onFailure {
                    _state.update {
                        it.copy(signingIn = false, loginPrompt = null, error = R.string.boosteroid_sign_in_failed)
                    }
                }
        }
    }

    fun cancelSignIn() {
        loginJob?.cancel()
        _state.update { it.copy(signingIn = false, loginPrompt = null) }
    }

    fun signOut() {
        if (state.value.launch != null || state.value.stopping) return
        catalogJob?.cancel()
        accountJob?.cancel()
        libraryJob?.cancel()
        _state.update { BoosteroidUiState(settings = it.settings) }
        viewModelScope.launch {
            http.logout()
            _state.update { it.copy(initialized = true) }
        }
    }

    private fun refreshAccount() {
        val token = token() ?: return
        refreshCatalog(reset = true)
        accountJob?.cancel()
        accountJob = viewModelScope.launch {
            coroutineScope {
                launch {
                    val plan = boosteroidResult { subscription.fetchSubscription(token) }.getOrNull()
                    _state.update { it.copy(membership = plan?.membershipTier) }
                }
                launch { refreshActiveSession(token) }
            }
        }
    }

    fun refresh() = refreshAccount()

    fun selectLibrary(library: Boolean) {
        if (library == state.value.library) return
        _state.update { it.copy(library = library, query = "", games = emptyList(), hasMore = false) }
        refreshCatalog(reset = true)
    }

    fun search(query: String) {
        if (state.value.library) {
            _state.update { it.copy(query = query) }
        } else {
            _state.update { it.copy(query = query, games = emptyList(), hasMore = false) }
            refreshCatalog(reset = true, debounce = true)
        }
    }

    fun loadMore() {
        if (!state.value.loading && state.value.hasMore) refreshCatalog(reset = false)
    }

    private fun refreshCatalog(reset: Boolean, debounce: Boolean = false) {
        val token = token() ?: return
        catalogJob?.cancel()
        if (reset) page = 0
        val snapshot = state.value
        val requestedPage = page + 1
        _state.update { it.copy(loading = true, error = null) }
        catalogJob = viewModelScope.launch {
            if (debounce) delay(300)
            boosteroidResult { catalog.games(token, snapshot.query, requestedPage, snapshot.library) }
                .onSuccess { results ->
                    page = requestedPage
                    _state.update {
                        val existing = if (reset) emptyList() else it.games
                        val combined = (existing + results).distinctBy(GameInfo::id)
                        it.copy(
                            games = combined,
                            loading = false,
                            hasMore = combined.size > existing.size && results.size >= BOOSTEROID_DEFAULT_PAGE_SIZE,
                        )
                    }
                }
                .onFailure { error ->
                    if (error is BoosteroidApiException && error.statusCode == 401) expireAccount()
                    else _state.update { it.copy(loading = false, error = R.string.boosteroid_catalog_failed) }
                }
        }
    }

    fun setInstalled(game: GameInfo, installed: Boolean) {
        val id = game.launchAppId?.toIntOrNull() ?: return
        updateLibrary { token -> catalog.setInstalled(token, id, installed) }
    }

    fun syncLibrary(platform: String) = updateLibrary { token -> catalog.synchronizePlatform(token, platform) }

    private fun updateLibrary(action: suspend (String) -> Unit) {
        val token = token() ?: return
        if (libraryJob?.isActive == true) return
        libraryJob = viewModelScope.launch {
            boosteroidResult { action(token) }
                .onSuccess { refreshCatalog(reset = true) }
                .onFailure { _state.update { it.copy(error = R.string.boosteroid_library_failed) } }
        }
    }

    fun setProfile(resolution: String, fps: Int, codec: VideoCodec) {
        if (state.value.launch != null) return
        val settings = StreamSettings(
            resolution = resolution,
            fps = fps,
            codec = codec,
            colorQuality = ColorQuality.EightBit420,
        )
        _state.update { it.copy(settings = settings) }
        accountStore.saveSettings(settings)
    }

    fun play(game: GameInfo) {
        val id = game.launchAppId?.toIntOrNull()?.takeIf { it > 0 } ?: return
        val token = token() ?: return
        if (state.value.launch != null || state.value.stopping) return
        recoveryAttempted = false
        launch(game.title, state.value.settings) {
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

    fun resume() {
        val active = state.value.activeSession ?: return
        val token = token() ?: return
        if (state.value.launch != null || state.value.stopping) return
        recoveryAttempted = false
        val title = state.value.games.firstOrNull { it.launchAppId == active.appId.toString() }?.title
        launch(title, state.value.settings) { sessions.claimSession(token, active) }
    }

    private fun launch(
        title: String?,
        settings: StreamSettings,
        start: suspend () -> BoosteroidSession?,
    ) {
        val token = token() ?: return
        _state.update { it.copy(launch = BoosteroidLaunch(title, settings), error = null) }
        launchJob = viewModelScope.launch {
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
        _state.update { it.copy(launch = null, error = R.string.boosteroid_session_failed) }
        token()?.let { token -> viewModelScope.launch { refreshActiveSession(token) } }
    }

    fun stop() {
        val token = token() ?: return
        if (state.value.stopping) return
        _state.update { it.copy(stopping = true, launch = null, error = null) }
        viewModelScope.launch {
            launchJob?.cancelAndJoin()
            boosteroidResult { sessions.stopSession(token) }
                .onSuccess { _state.update { it.copy(activeSession = null) } }
                .onFailure { _state.update { it.copy(error = R.string.boosteroid_stop_failed) } }
            refreshActiveSession(token)
            _state.update { it.copy(stopping = false) }
        }
    }

    override fun onCleared() {
        if (::sessions.isInitialized) sessions.closePendingLaunches()
        super.onCleared()
    }

    fun dismissError() = _state.update { it.copy(error = null) }

    private suspend fun refreshActiveSession(token: String) {
        boosteroidResult { sessions.getActiveSessions(token).firstOrNull() }
            .onSuccess { active -> _state.update { it.copy(activeSession = active) } }
    }

    private fun expireAccount() {
        launchJob?.cancel()
        sessions.closePendingLaunches()
        accountJob?.cancel()
        libraryJob?.cancel()
        accountStore.save(null)
        _state.update {
            BoosteroidUiState(
                initialized = true,
                settings = it.settings,
                error = R.string.boosteroid_sign_in_expired,
            )
        }
    }

    private fun token(): String? = if (::accountStore.isInitialized) accountStore.account?.tokens?.accessToken else null
}

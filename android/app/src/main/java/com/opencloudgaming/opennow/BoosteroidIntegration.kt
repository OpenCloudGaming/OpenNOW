package com.opencloudgaming.opennow

import android.view.KeyEvent
import android.view.MotionEvent
import androidx.compose.runtime.Composable
import kotlinx.coroutines.flow.Flow

internal const val BOOSTEROID_PROVIDER_CODE = "BOOSTEROID"
internal const val BOOSTEROID_GAME_PREFIX = "boosteroid:"
internal val BOOSTEROID_RESOLUTIONS = setOf("1280x720", "1920x1080", "2560x1440", "3840x2160")

internal val boosteroidLoginProvider = LoginProvider(
    idpId = "boosteroid",
    code = BOOSTEROID_PROVIDER_CODE,
    displayName = "Boosteroid",
    streamingServiceUrl = "",
)

data class BoosteroidPresentation(
    val active: Boolean = false,
    val account: SavedAccount? = null,
    val loginPrompt: DeviceLoginPrompt? = null,
    val signingIn: Boolean = false,
    val error: Int? = null,
    val store: List<GameInfo> = emptyList(),
    val library: List<GameInfo> = emptyList(),
    val search: String = "",
    val loading: Boolean = false,
    val selectedGame: GameInfo? = null,
    val resumable: ActiveSessionInfo? = null,
    val streamStatus: String = "idle",
    val streamSettings: StreamSettings? = null,
    val configuredStreamSettings: StreamSettings? = null,
)

internal interface BoosteroidIntegration {
    val presentation: Flow<BoosteroidPresentation>
    val active: Boolean
    val busy: Boolean

    fun signInOrSwitch()
    fun cancelSignIn()
    fun switchAway()
    fun signOut()
    fun refresh()
    fun search(query: String)
    fun updateStreamSettings(settings: StreamSettings)
    fun selectGame(game: GameInfo?)
    fun setInstalled(game: GameInfo, installed: Boolean)
    fun syncLibrary(platform: String)
    fun play(game: GameInfo, settings: StreamSettings)
    fun resume(settings: StreamSettings)
    fun stop()
    fun dismissError()
    fun dispatchKey(event: KeyEvent): Boolean
    fun dispatchMotion(event: MotionEvent): Boolean
    fun releaseInput()

    @Composable
    fun Stream(state: OpenNowUiState)
}

/** The shared UI can show provider data without putting it into NVIDIA's auth or session state. */
internal fun OpenNowUiState.withBoosteroidPresentation(boosteroid: BoosteroidPresentation): OpenNowUiState {
    if (!boosteroid.active) {
        return copy(
            boosteroid = boosteroid,
            savedAccounts = savedAccounts + listOfNotNull(boosteroid.account),
        )
    }

    val streaming = boosteroid.streamStatus != "idle"
    return copy(
        boosteroid = boosteroid,
        authSession = null,
        savedAccounts = savedAccounts + listOfNotNull(boosteroid.account),
        page = if (streaming) AppPage.Stream else page,
        settings = boosteroid.configuredStreamSettings?.let {
            settings.copy(stream = it, streamPreset = StreamPreset.Custom)
        } ?: settings,
        subscriptionInfo = null,
        accountConnectors = emptyList(),
        loadingAccountConnectors = false,
        connectorActionStore = null,
        regions = emptyList(),
        games = boosteroid.store,
        newlyAddedGames = emptyList(),
        libraryGames = boosteroid.library,
        queuedGameKeys = emptyList(),
        dismissedContinuePlaying = emptyMap(),
        catalogResult = CatalogBrowseResult(boosteroid.store, searchQuery = boosteroid.search),
        catalogSearch = boosteroid.search,
        catalogSortId = CATALOG_SORT_DEFAULT,
        catalogFilterIds = emptyList(),
        loadingGames = boosteroid.loading,
        catalogQueryLoading = boosteroid.loading && boosteroid.search.isNotBlank(),
        selectedGame = boosteroid.selectedGame,
        selectedGameRail = null,
        removingLibraryGameId = null,
        activeSession = boosteroid.resumable,
        activeSessionDecision = null,
        streamSession = null,
        activeStreamSettings = boosteroid.streamSettings,
        streamStatus = boosteroid.streamStatus,
        launchPhase = "",
        error = null,
        pendingStoreChoiceGame = null,
        pendingMembershipNotice = null,
        pendingGfnMembershipActivation = false,
        pendingBatteryOptimizationLaunch = null,
        pendingLaunchRecovery = null,
        pendingPrintedWasteGame = null,
        deviceLoginPrompt = null,
        queuePosition = null,
        localTvConnector = LocalTvConnectorState(),
        streamLaunchMinimized = false,
        sessionReport = null,
    )
}

internal val GameInfo.isBoosteroidGame: Boolean
    get() = id.startsWith(BOOSTEROID_GAME_PREFIX)

internal val OpenNowUiState.activeCloudAccount: SavedAccount?
    get() = if (boosteroid.active) boosteroid.account else authSession?.let { session ->
        SavedAccount(
            userId = session.user.userId,
            displayName = session.user.displayName,
            email = session.user.email,
            avatarUrl = session.user.avatarUrl,
            membershipTier = session.user.membershipTier,
            providerCode = session.provider.code,
        )
    }

internal val OpenNowUiState.hasCloudAccount: Boolean
    get() = if (boosteroid.active) boosteroid.account != null else authSession != null

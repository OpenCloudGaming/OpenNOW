package com.opencloudgaming.opennow

import android.app.Activity
import android.app.ActivityOptions
import android.content.Context
import android.content.Intent
import android.hardware.display.DisplayManager
import android.os.Build
import android.os.Handler
import android.os.Looper
import android.util.Log
import android.view.Display
import androidx.compose.runtime.Composable
import androidx.compose.runtime.DisposableEffect
import androidx.compose.runtime.collectAsState
import androidx.compose.runtime.getValue
import androidx.compose.runtime.mutableStateOf
import androidx.compose.runtime.remember
import androidx.compose.runtime.rememberUpdatedState
import androidx.lifecycle.Lifecycle
import androidx.lifecycle.lifecycleScope
import androidx.lifecycle.repeatOnLifecycle
import kotlinx.coroutines.channels.BufferOverflow
import kotlinx.coroutines.flow.MutableSharedFlow
import kotlinx.coroutines.flow.MutableStateFlow
import kotlinx.coroutines.flow.SharedFlow
import kotlinx.coroutines.flow.StateFlow
import kotlinx.coroutines.flow.asSharedFlow
import kotlinx.coroutines.flow.asStateFlow
import kotlinx.coroutines.flow.combine
import kotlinx.coroutines.launch
import java.lang.ref.WeakReference

private const val DUAL_SCREEN_LOG_TAG = "OpenNOWDualScreen"
private const val DECK_RELAUNCH_GUARD_MS = 2_000L

enum class DualScreenPhase {
    SignedOut,
    Browse,
    Launching,
    Streaming,
}

/** Everything the bottom screen renders. Derived from the ViewModel; never written by the deck. */
data class DualScreenSnapshot(
    val phase: DualScreenPhase,
    val accountName: String?,
    val membershipTier: String?,
    val remainingHours: Double?,
    val unlimitedHours: Boolean,
    val page: AppPage,
    val catalogSearch: String,
    val librarySearch: String,
    val streamPreset: StreamPreset,
    val playMode: BottomScreenPlayMode,
    val gameTitle: String?,
    val launchPhase: String,
    val queuePosition: Int?,
    val targetFps: Int?,
    val stats: StreamRuntimeStats?,
    val settings: AppSettings,
    /** Game whose details sheet is open on the top screen. */
    val selectedGame: GameInfo?,
)

internal fun dualScreenPhase(state: OpenNowUiState): DualScreenPhase = when {
    state.authSession == null -> DualScreenPhase.SignedOut
    state.streamStatus == "streaming" -> DualScreenPhase.Streaming
    state.streamStatus != "idle" -> DualScreenPhase.Launching
    else -> DualScreenPhase.Browse
}

internal fun dualScreenSnapshot(state: OpenNowUiState, stats: StreamRuntimeStats?): DualScreenSnapshot {
    val phase = dualScreenPhase(state)
    val subscription = state.subscriptionInfo
    return DualScreenSnapshot(
        phase = phase,
        accountName = state.authSession?.user?.displayName,
        membershipTier = subscription?.membershipTier ?: state.authSession?.user?.membershipTier,
        remainingHours = subscription?.remainingHours,
        unlimitedHours = subscription?.isUnlimited == true,
        page = state.page,
        catalogSearch = state.catalogSearch,
        librarySearch = state.librarySearch,
        streamPreset = state.settings.streamPreset,
        playMode = state.settings.bottomScreenPlayMode,
        gameTitle = state.streamGame?.title,
        launchPhase = state.launchPhase,
        queuePosition = state.queuePosition,
        targetFps = (state.activeStreamSettings ?: state.settings.stream).fps,
        stats = stats.takeIf { phase == DualScreenPhase.Streaming },
        settings = state.settings,
        selectedGame = state.selectedGame.takeIf { phase == DualScreenPhase.Browse && state.page != AppPage.Settings },
    )
}

/** Requests the deck sends back; MainActivity applies them through the ViewModel on the main thread. */
sealed interface DualScreenAction {
    data class Navigate(val page: AppPage) : DualScreenAction
    data class Search(val query: String) : DualScreenAction
    data class ApplyPreset(val preset: StreamPreset) : DualScreenAction
    data class SetPlayMode(val mode: BottomScreenPlayMode) : DualScreenAction
    data object CancelLaunch : DualScreenAction
    data object OpenStreamMenu : DualScreenAction
    data object ToggleStatsOverlay : DualScreenAction
    data object EndSession : DualScreenAction
    data class Play(val game: GameInfo) : DualScreenAction
    data class ChooseStore(val game: GameInfo) : DualScreenAction
    data class ToggleFavorite(val gameId: String) : DualScreenAction
    data object CloseGameDetails : DualScreenAction
}

/**
 * A large touch version of the setting that holds controller focus on the top screen. The
 * callbacks are the setting's own, so the bottom screen writes through exactly the same path.
 */
internal sealed interface SettingInspection {
    val label: String
    val description: String?

    data class Switch(
        override val label: String,
        override val description: String?,
        val checked: Boolean,
        val enabled: Boolean,
        val onCheckedChange: (Boolean) -> Unit,
    ) : SettingInspection

    data class Choice(
        override val label: String,
        override val description: String?,
        val options: List<ChoiceMenuOption>,
        val selectedLabel: String,
        val onSelect: (String) -> Unit,
    ) : SettingInspection

    data class Slider(
        override val label: String,
        override val description: String?,
        val value: Float,
        val min: Float,
        val max: Float,
        val step: Float,
        val valueText: String,
        val onChange: (Float) -> Unit,
    ) : SettingInspection
}

/** Top-screen UI whose rendering currently lives on the bottom screen. */
internal class HostedBottomContent(val owner: Any, val scale: Float, val content: @Composable () -> Unit)

/**
 * Process-wide link between MainActivity (top screen, owns the ViewModel) and
 * [DualScreenActivity] (bottom screen). Both activities live in one process, so a bounded
 * in-memory channel is enough; nothing here is persisted or crosses a process boundary.
 */
object DualScreenBridge {
    private val _snapshot = MutableStateFlow<DualScreenSnapshot?>(null)
    val snapshot: StateFlow<DualScreenSnapshot?> = _snapshot.asStateFlow()

    private val _actions = MutableSharedFlow<DualScreenAction>(
        extraBufferCapacity = 16,
        onBufferOverflow = BufferOverflow.DROP_OLDEST,
    )
    val actions: SharedFlow<DualScreenAction> = _actions.asSharedFlow()

    private val _deckReady = MutableStateFlow(false)
    /** True while the deck activity exists, so top-screen UI may move onto the bottom screen. */
    val deckReady: StateFlow<Boolean> = _deckReady.asStateFlow()

    /** Compose state, read by the deck's composition and written by the top screen's. */
    internal val hostedContent = mutableStateOf<HostedBottomContent?>(null)
    internal val inspection = mutableStateOf<Pair<Any, SettingInspection>?>(null)

    private var deck: WeakReference<Activity>? = null
    private var mainDispatcher: WeakReference<Activity>? = null

    internal fun publish(snapshot: DualScreenSnapshot) {
        _snapshot.value = snapshot
    }

    fun dispatch(action: DualScreenAction) {
        _actions.tryEmit(action)
    }

    internal fun attachDeck(activity: Activity) {
        deck = WeakReference(activity)
        _deckReady.value = true
    }

    internal fun detachDeck(activity: Activity) {
        if (deck?.get() === activity) {
            deck = null
            _deckReady.value = false
        }
    }

    internal fun host(owner: Any, scale: Float, content: @Composable () -> Unit) {
        hostedContent.value = HostedBottomContent(owner, scale, content)
    }

    internal fun release(owner: Any) {
        if (hostedContent.value?.owner === owner) hostedContent.value = null
    }

    internal fun inspect(owner: Any, inspection: SettingInspection) {
        this.inspection.value = owner to inspection
    }

    internal fun clearInspection(owner: Any) {
        if (inspection.value?.first === owner) inspection.value = null
    }

    internal fun runningDeck(): Activity? = deck?.get()?.takeUnless { it.isFinishing || it.isDestroyed }

    internal fun attachMain(activity: Activity) {
        mainDispatcher = WeakReference(activity)
    }

    internal fun detachMain(activity: Activity) {
        if (mainDispatcher?.get() === activity) mainDispatcher = null
    }

    /** Hardware buttons that land on the bottom screen are handed to the game window instead. */
    internal fun forwardToMain(dispatch: (Activity) -> Boolean): Boolean {
        val main = mainDispatcher?.get()?.takeUnless { it.isFinishing || it.isDestroyed } ?: return false
        return dispatch(main)
    }
}

/**
 * Picks the built-in second screen. The AYN Thor reports its 3.92" bottom panel as a public,
 * non-default display; virtual, private, and switched-off displays are never used.
 */
internal fun selectBottomScreenDisplay(displays: Array<Display>): Display? =
    displays.firstOrNull { display ->
        display.displayId != Display.DEFAULT_DISPLAY &&
            display.state != Display.STATE_OFF &&
            display.flags and Display.FLAG_PRIVATE == 0
    }

internal fun Context.hasBottomScreenDisplay(): Boolean {
    val displayManager = getSystemService(Context.DISPLAY_SERVICE) as? DisplayManager ?: return false
    return selectBottomScreenDisplay(displayManager.displays) != null
}

/**
 * Owned by MainActivity. Shows the deck on the second display while the app is in the
 * foreground, applies deck actions to the ViewModel, and removes the deck when it is no longer
 * wanted (setting off, display gone, picture-in-picture, or the app leaving the foreground).
 */
class DualScreenController(
    private val activity: MainActivity,
    private val viewModel: OpenNowViewModel,
) {
    private val displayManager = activity.getSystemService(Context.DISPLAY_SERVICE) as DisplayManager
    private val mainHandler = Handler(Looper.getMainLooper())
    private val bottomDisplay = MutableStateFlow(currentBottomDisplayId())
    private var launchFailedForDisplay: Int? = null
    private var lastLaunchRequestMs = 0L

    private val displayListener = object : DisplayManager.DisplayListener {
        override fun onDisplayAdded(displayId: Int) = refreshDisplay()
        override fun onDisplayRemoved(displayId: Int) = refreshDisplay()
        override fun onDisplayChanged(displayId: Int) = refreshDisplay()
    }

    fun start() {
        DualScreenBridge.attachMain(activity)
        activity.lifecycleScope.launch {
            activity.repeatOnLifecycle(Lifecycle.State.STARTED) {
                displayManager.registerDisplayListener(displayListener, mainHandler)
                refreshDisplay()
                try {
                    launch {
                        DualScreenBridge.actions.collect(::apply)
                    }
                    combine(viewModel.state, viewModel.streamRuntimeStats, bottomDisplay) { state, stats, displayId ->
                        Triple(state, stats, displayId)
                    }.collect { (state, stats, displayId) ->
                        DualScreenBridge.publish(dualScreenSnapshot(state, stats))
                        val wanted = displayId != null &&
                            state.settings.bottomScreenEnabled &&
                            !state.androidPictureInPictureActive &&
                            !state.androidTvProfile
                        if (wanted) showDeck(displayId!!) else finishDeck()
                    }
                } finally {
                    displayManager.unregisterDisplayListener(displayListener)
                    // Leaving the foreground hides the deck; returning relaunches it.
                    if (!activity.isChangingConfigurations) finishDeck()
                }
            }
        }
    }

    fun destroy() {
        DualScreenBridge.detachMain(activity)
    }

    private fun refreshDisplay() {
        bottomDisplay.value = currentBottomDisplayId()
    }

    private fun currentBottomDisplayId(): Int? =
        selectBottomScreenDisplay(displayManager.displays)?.displayId

    private fun showDeck(displayId: Int) {
        val running = DualScreenBridge.runningDeck()
        if (running != null) {
            @Suppress("DEPRECATION")
            if (running.windowManager.defaultDisplay.displayId == displayId) return
            running.finish()
        }
        if (launchFailedForDisplay == displayId || Build.VERSION.SDK_INT < Build.VERSION_CODES.O) return
        // State emits several times a second while streaming; the deck attaches asynchronously.
        val now = android.os.SystemClock.elapsedRealtime()
        if (now - lastLaunchRequestMs < DECK_RELAUNCH_GUARD_MS) return
        lastLaunchRequestMs = now
        val intent = Intent(activity, DualScreenActivity::class.java)
            .addFlags(Intent.FLAG_ACTIVITY_NEW_TASK or Intent.FLAG_ACTIVITY_NO_ANIMATION)
        val options = ActivityOptions.makeBasic().setLaunchDisplayId(displayId)
        try {
            activity.startActivity(intent, options.toBundle())
        } catch (error: RuntimeException) {
            // Some firmware refuses third-party activities on secondary panels; keep the
            // single-screen app fully usable and do not retry on every state emission.
            launchFailedForDisplay = displayId
            Log.w(DUAL_SCREEN_LOG_TAG, "Could not open the bottom screen deck on display $displayId", error)
        }
    }

    private fun finishDeck() {
        DualScreenBridge.runningDeck()?.finish()
    }

    private fun apply(action: DualScreenAction) {
        when (action) {
            is DualScreenAction.Navigate -> viewModel.setPage(action.page)
            is DualScreenAction.Search -> {
                if (viewModel.state.value.page == AppPage.Library) {
                    viewModel.setLibrarySearch(action.query)
                } else {
                    if (viewModel.state.value.page != AppPage.Home) viewModel.setPage(AppPage.Home)
                    viewModel.setCatalogSearch(action.query)
                }
            }
            is DualScreenAction.ApplyPreset -> viewModel.applyStreamPreset(action.preset)
            is DualScreenAction.SetPlayMode -> {
                val settings = viewModel.state.value.settings
                viewModel.updateSettings(settings.copy(bottomScreenPlayMode = action.mode))
            }
            DualScreenAction.CancelLaunch -> viewModel.stopStream()
            DualScreenAction.OpenStreamMenu -> viewModel.requestStreamMenu()
            DualScreenAction.ToggleStatsOverlay -> viewModel.toggleStreamStatsOverlay()
            DualScreenAction.EndSession -> viewModel.stopStream()
            is DualScreenAction.Play -> viewModel.play(action.game)
            is DualScreenAction.ChooseStore -> viewModel.chooseStore(action.game)
            is DualScreenAction.ToggleFavorite -> viewModel.updateFavorites(action.gameId)
            DualScreenAction.CloseGameDetails -> viewModel.clearSelectedGame()
        }
    }
}

/** Whether top-screen UI should currently render on the bottom screen instead. */
@Composable
internal fun rememberBottomScreenHosting(settings: AppSettings): Boolean {
    val deckReady by DualScreenBridge.deckReady.collectAsState()
    return deckReady && settings.bottomScreenEnabled
}

/** Panels laid out for the 6" screen get a denser scale so they fit the 3.92" one unchanged. */
internal const val BOTTOM_SCREEN_PANEL_SCALE = 0.85f
internal const val BOTTOM_SCREEN_PICKER_SCALE = 0.72f

/**
 * Renders [content] on the bottom screen while this call stays in the composition. The content
 * keeps reading the caller's state, so both screens see one source of truth.
 */
@Composable
internal fun HostOnBottomScreen(scale: Float = BOTTOM_SCREEN_PANEL_SCALE, content: @Composable () -> Unit) {
    val latest by rememberUpdatedState(content)
    val owner = remember { Any() }
    DisposableEffect(owner, scale) {
        DualScreenBridge.host(owner, scale) { latest() }
        onDispose { DualScreenBridge.release(owner) }
    }
}

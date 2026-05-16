package com.opencloudgaming.opennow

import android.app.Activity
import android.content.Intent
import android.graphics.BitmapFactory
import android.os.Build
import android.speech.RecognizerIntent
import android.view.KeyEvent
import android.view.MotionEvent
import android.view.View
import androidx.activity.compose.BackHandler
import androidx.activity.compose.rememberLauncherForActivityResult
import androidx.activity.result.contract.ActivityResultContracts
import androidx.compose.animation.AnimatedVisibility
import androidx.compose.animation.fadeIn
import androidx.compose.animation.fadeOut
import androidx.compose.animation.scaleIn
import androidx.compose.animation.scaleOut
import androidx.compose.animation.slideInVertically
import androidx.compose.animation.slideOutVertically
import androidx.compose.animation.core.MutableTransitionState
import androidx.compose.foundation.gestures.detectVerticalDragGestures
import androidx.compose.foundation.text.BasicTextField
import androidx.compose.foundation.Image
import androidx.compose.foundation.background
import androidx.compose.foundation.border
import androidx.compose.foundation.clickable
import androidx.compose.foundation.layout.Arrangement
import androidx.compose.foundation.layout.Box
import androidx.compose.foundation.layout.Column
import androidx.compose.foundation.layout.ColumnScope
import androidx.compose.foundation.layout.ExperimentalLayoutApi
import androidx.compose.foundation.layout.FlowRow
import androidx.compose.foundation.layout.PaddingValues
import androidx.compose.foundation.layout.Row
import androidx.compose.foundation.layout.RowScope
import androidx.compose.foundation.layout.Spacer
import androidx.compose.foundation.layout.aspectRatio
import androidx.compose.foundation.layout.fillMaxHeight
import androidx.compose.foundation.layout.fillMaxSize
import androidx.compose.foundation.layout.fillMaxWidth
import androidx.compose.foundation.layout.height
import androidx.compose.foundation.layout.padding
import androidx.compose.foundation.layout.size
import androidx.compose.foundation.layout.width
import androidx.compose.foundation.lazy.LazyColumn
import androidx.compose.foundation.lazy.items
import androidx.compose.foundation.lazy.grid.GridCells
import androidx.compose.foundation.lazy.grid.LazyVerticalGrid
import androidx.compose.foundation.lazy.grid.items as gridItems
import androidx.compose.foundation.rememberScrollState
import androidx.compose.foundation.shape.CircleShape
import androidx.compose.foundation.shape.RoundedCornerShape
import androidx.compose.foundation.verticalScroll
import androidx.compose.material3.AssistChip
import androidx.compose.material3.Button
import androidx.compose.material3.Card
import androidx.compose.material3.CardDefaults
import androidx.compose.material3.CircularProgressIndicator
import androidx.compose.material3.DropdownMenu
import androidx.compose.material3.DropdownMenuItem
import androidx.compose.material3.ElevatedButton
import androidx.compose.material3.ExperimentalMaterial3Api
import androidx.compose.material3.FilterChip
import androidx.compose.material3.Icon
import androidx.compose.material3.IconButton
import androidx.compose.material3.LinearProgressIndicator
import androidx.compose.material3.MaterialTheme
import androidx.compose.material3.NavigationBar
import androidx.compose.material3.NavigationBarItem
import androidx.compose.material3.OutlinedButton
import androidx.compose.material3.OutlinedTextField
import androidx.compose.material3.Scaffold
import androidx.compose.material3.Slider
import androidx.compose.material3.Surface
import androidx.compose.material3.Switch
import androidx.compose.material3.Text
import androidx.compose.material3.TextButton
import androidx.compose.material3.darkColorScheme
import androidx.compose.material3.dynamicDarkColorScheme
import androidx.compose.runtime.Composable
import androidx.compose.runtime.DisposableEffect
import androidx.compose.runtime.LaunchedEffect
import androidx.compose.runtime.getValue
import androidx.compose.runtime.mutableFloatStateOf
import androidx.compose.runtime.mutableStateOf
import androidx.compose.runtime.produceState
import androidx.compose.runtime.remember
import androidx.compose.runtime.setValue
import androidx.compose.ui.Alignment
import androidx.compose.ui.Modifier
import androidx.compose.ui.draw.clip
import androidx.compose.ui.geometry.Offset
import androidx.compose.ui.graphics.Color
import androidx.compose.ui.graphics.ImageBitmap
import androidx.compose.ui.graphics.SolidColor
import androidx.compose.ui.graphics.asImageBitmap
import androidx.compose.ui.graphics.graphicsLayer
import androidx.compose.ui.input.pointer.pointerInput
import androidx.compose.ui.layout.ContentScale
import androidx.compose.ui.platform.LocalClipboardManager
import androidx.compose.ui.platform.LocalContext
import androidx.compose.ui.res.painterResource
import androidx.compose.ui.res.stringResource
import androidx.compose.ui.text.AnnotatedString
import androidx.compose.ui.text.font.FontWeight
import androidx.compose.ui.text.style.TextOverflow
import androidx.compose.ui.unit.dp
import androidx.compose.ui.viewinterop.AndroidView
import androidx.lifecycle.compose.collectAsStateWithLifecycle
import androidx.media3.common.MediaItem
import androidx.media3.common.Player
import androidx.media3.exoplayer.ExoPlayer
import androidx.media3.ui.PlayerView
import kotlinx.coroutines.Dispatchers
import kotlinx.coroutines.withContext
import java.util.Locale
import java.net.URL
import kotlin.math.abs
import kotlin.math.min
import kotlin.math.roundToInt
import kotlin.math.sqrt

private val Green = Color(0xff6af0a0)
private val Background = Color(0xff07100b)
private val Panel = Color(0xff0d1a12)
private val PanelAlt = Color(0xff13251a)
private val TextPrimary = Color(0xffe5fff0)
private val TextMuted = Color(0xff9fbea9)
private val SettingsBackground = Color(0xff090b0d)
private val SettingsPanel = Color(0xff11161a)
private val SettingsPanelAlt = Color(0xff171d22)
private val SettingsText = Color(0xffeef3f5)
private val SettingsTextMuted = Color(0xff98a4aa)
private const val TOUCH_MOUSE_TAP_SLOP_PX = 18f
private const val TOUCH_MOUSE_DOUBLE_TAP_TIMEOUT_MS = 320L
private const val TOUCH_MOUSE_DOUBLE_TAP_SLOP_PX = 36f
private val UiAccent.color: Color
    get() = when (this) {
        UiAccent.OpenNow -> Green
        UiAccent.Pixel -> Color(0xff8ab4f8)
        UiAccent.Lime -> Color(0xffc7ef6b)
        UiAccent.Coral -> Color(0xffff8d7a)
        UiAccent.Violet -> Color(0xffc7a4ff)
    }

@Composable
private fun uiAccentLabel(accent: UiAccent): String = when (accent) {
    UiAccent.OpenNow -> stringResource(R.string.accent_opennow)
    UiAccent.Pixel -> stringResource(R.string.accent_pixel)
    UiAccent.Lime -> stringResource(R.string.accent_lime)
    UiAccent.Coral -> stringResource(R.string.accent_coral)
    UiAccent.Violet -> stringResource(R.string.accent_violet)
}

private class TouchMouseState {
    private var activePointerId = -1
    private var downX = 0f
    private var downY = 0f
    private var lastX = 0f
    private var lastY = 0f
    private var moved = false
    private var selecting = false
    private var lastTapTimeMs = Long.MIN_VALUE
    private var lastTapX = Float.NaN
    private var lastTapY = Float.NaN

    fun handle(view: View, event: MotionEvent, enabled: Boolean, client: NativeStreamClient): Boolean {
        if (!enabled) {
            if (selecting) client.setTouchMouseButton(false)
            activePointerId = -1
            selecting = false
            return false
        }

        when (event.actionMasked) {
            MotionEvent.ACTION_DOWN -> {
                activePointerId = event.getPointerId(0)
                downX = event.x
                downY = event.y
                lastX = event.x
                lastY = event.y
                moved = false
                selecting = isDoubleTap(event)
                if (selecting) {
                    client.setTouchMouseButton(true)
                    lastTapTimeMs = Long.MIN_VALUE
                }
                return true
            }
            MotionEvent.ACTION_POINTER_DOWN -> {
                if (selecting) client.setTouchMouseButton(false)
                activePointerId = -1
                moved = true
                selecting = false
                return true
            }
            MotionEvent.ACTION_MOVE -> {
                val index = event.findPointerIndex(activePointerId)
                if (index < 0) return true
                val x = event.getX(index)
                val y = event.getY(index)
                val dx = x - lastX
                val dy = y - lastY
                if (abs(x - downX) > TOUCH_MOUSE_TAP_SLOP_PX || abs(y - downY) > TOUCH_MOUSE_TAP_SLOP_PX) {
                    moved = true
                }
                sendMouseDelta(dx, dy, client)
                lastX = x
                lastY = y
                return true
            }
            MotionEvent.ACTION_UP -> {
                val wasTap = activePointerId >= 0 && !moved
                activePointerId = -1
                if (selecting) {
                    client.setTouchMouseButton(false)
                    selecting = false
                    return true
                }
                if (wasTap) {
                    client.sendTouchMouseClick()
                    lastTapTimeMs = event.eventTime
                    lastTapX = event.x
                    lastTapY = event.y
                }
                return true
            }
            MotionEvent.ACTION_CANCEL -> {
                if (selecting) client.setTouchMouseButton(false)
                activePointerId = -1
                selecting = false
                return true
            }
        }
        return true
    }

    private fun isDoubleTap(event: MotionEvent): Boolean {
        if (lastTapTimeMs == Long.MIN_VALUE) return false
        if (event.eventTime - lastTapTimeMs > TOUCH_MOUSE_DOUBLE_TAP_TIMEOUT_MS) return false
        if (!lastTapX.isFinite() || !lastTapY.isFinite()) return false
        return abs(event.x - lastTapX) <= TOUCH_MOUSE_DOUBLE_TAP_SLOP_PX &&
            abs(event.y - lastTapY) <= TOUCH_MOUSE_DOUBLE_TAP_SLOP_PX
    }

    private fun sendMouseDelta(dx: Float, dy: Float, client: NativeStreamClient) {
        val ix = dx.roundToInt()
        val iy = dy.roundToInt()
        if (ix != 0 || iy != 0) {
            client.sendTouchMouseMove(ix, iy)
        }
    }
}

@Composable
fun OpenNowTheme(settings: AppSettings, content: @Composable () -> Unit) {
    val context = LocalContext.current
    val accent = settings.uiAccent.color
    val fallbackScheme = darkColorScheme(
        primary = accent,
        onPrimary = Color(0xff031008),
        background = Background,
        surface = Panel,
        surfaceVariant = PanelAlt,
        onBackground = TextPrimary,
        onSurface = TextPrimary,
        onSurfaceVariant = TextMuted,
    )
    val colorScheme = if (settings.dynamicColor && Build.VERSION.SDK_INT >= Build.VERSION_CODES.S) {
        dynamicDarkColorScheme(context)
    } else {
        fallbackScheme
    }
    MaterialTheme(
        colorScheme = colorScheme,
        content = content,
    )
}

@Composable
fun OpenNowApp(viewModel: OpenNowViewModel) {
    val state by viewModel.state.collectAsStateWithLifecycle()

    OpenNowTheme(state.settings) {
        Surface(Modifier.fillMaxSize(), color = MaterialTheme.colorScheme.background) {
            val startingText = stringResource(R.string.status_starting_opennow)
            when {
                state.initializing -> LoadingScreen(state.launchPhase.ifBlank { startingText })
                state.authSession == null -> LoginScreen(state, viewModel)
                else -> MainShell(state, viewModel)
            }
        }
    }
}

@Composable
private fun LoadingScreen(text: String) {
    Box(Modifier.fillMaxSize(), contentAlignment = Alignment.Center) {
        Column(horizontalAlignment = Alignment.CenterHorizontally, verticalArrangement = Arrangement.spacedBy(16.dp)) {
            OpenNowMark(72.dp)
            CircularProgressIndicator(color = Green)
            Text(text, color = TextMuted)
        }
    }
}

@Composable
private fun LoginScreen(state: OpenNowUiState, viewModel: OpenNowViewModel) {
    Column(
        Modifier
            .fillMaxSize()
            .padding(24.dp),
        horizontalAlignment = Alignment.CenterHorizontally,
        verticalArrangement = Arrangement.Center,
    ) {
        OpenNowMark(88.dp)
        Spacer(Modifier.height(20.dp))
        Text("OpenNOW", style = MaterialTheme.typography.displaySmall, fontWeight = FontWeight.Bold)
        Text("Native Android GeForce NOW client", color = TextMuted)
        Spacer(Modifier.height(28.dp))
        ProviderPicker(state.providers, state.selectedProvider, viewModel::selectProvider)
        Spacer(Modifier.height(16.dp))
        Button(onClick = { viewModel.login() }) {
            Text(if (state.launchPhase.isBlank()) "Sign in with ${state.selectedProvider.displayName}" else state.launchPhase)
        }
        if (state.error != null) {
            Spacer(Modifier.height(14.dp))
            Text(state.error.orEmpty(), color = Color(0xffff9f9f))
        }
    }
}

@Composable
private fun MainShell(state: OpenNowUiState, viewModel: OpenNowViewModel) {
    val inStream = state.page == AppPage.Stream
    val modalPickerOpen = state.pendingPrintedWasteGame != null || state.pendingStoreChoiceGame != null
    BackHandler(enabled = state.selectedGame != null && !inStream) {
        viewModel.clearSelectedGame()
    }
    Scaffold(
        bottomBar = {
            if (!inStream) {
                NavigationBar(
                    containerColor = if (state.page == AppPage.Settings) SettingsBackground else MaterialTheme.colorScheme.background,
                    tonalElevation = 0.dp,
                ) {
                    BottomNavItem(
                        selected = state.page == AppPage.Home,
                        onClick = { viewModel.setPage(AppPage.Home) },
                        iconRes = R.drawable.ic_tab_store,
                        label = stringResource(R.string.nav_store),
                    )
                    BottomNavItem(
                        selected = state.page == AppPage.Library,
                        onClick = { viewModel.setPage(AppPage.Library) },
                        iconRes = R.drawable.ic_tab_library,
                        label = stringResource(R.string.nav_library),
                    )
                    BottomNavItem(
                        selected = state.page == AppPage.Settings,
                        onClick = { viewModel.setPage(AppPage.Settings) },
                        iconRes = R.drawable.ic_tab_settings,
                        label = stringResource(R.string.nav_settings),
                    )
                }
            }
        },
    ) { padding ->
        Box(Modifier.fillMaxSize().padding(padding)) {
            Column(Modifier.fillMaxSize()) {
                if (!inStream) {
                    TopStatusBar(state)
                }
                Box(Modifier.fillMaxSize()) {
                    when (state.page) {
                        AppPage.Home -> HomeScreen(state, viewModel)
                        AppPage.Library -> LibraryScreen(state, viewModel)
                        AppPage.Settings -> SettingsScreen(state, viewModel)
                        AppPage.Stream -> StreamScreen(state, viewModel)
                    }
                }
            }
            AnimatedVisibility(
                visible = state.selectedGame != null && !inStream && !modalPickerOpen,
                enter = fadeIn() + slideInVertically(initialOffsetY = { it / 3 }) + scaleIn(initialScale = 0.96f),
                exit = fadeOut() + slideOutVertically(targetOffsetY = { it / 3 }) + scaleOut(targetScale = 0.96f),
                modifier = Modifier.align(Alignment.Center),
            ) {
                state.selectedGame?.let { game ->
                    GameDetailsSheet(
                        game = game,
                        favorite = game.id in state.settings.favoriteGameIds,
                        onPlay = viewModel::play,
                        onFavorite = viewModel::updateFavorites,
                        onDismiss = viewModel::clearSelectedGame,
                    )
                }
            }
            state.pendingPrintedWasteGame?.let { game ->
                AnimatedLaunchOverlay(Modifier.align(Alignment.Center)) {
                    PrintedWasteSelector(state, game, viewModel)
                }
            }
            state.pendingStoreChoiceGame?.let { game ->
                AnimatedLaunchOverlay(Modifier.align(Alignment.Center)) {
                    StoreLaunchSelector(
                        game = game,
                        onLaunch = viewModel::playVariant,
                        onDismiss = viewModel::dismissStoreChoice,
                    )
                }
            }
            if (state.streamLaunchMinimized && shouldShowQueueLaunchStatus(state)) {
                MinimizedQueuePill(
                    state = state,
                    onRestore = viewModel::restoreStreamLaunch,
                    onCancel = viewModel::stopStream,
                    modifier = Modifier.align(Alignment.BottomCenter),
                )
            }
        }
    }
}

@Composable
private fun RowScope.BottomNavItem(selected: Boolean, onClick: () -> Unit, iconRes: Int, label: String) {
    NavigationBarItem(
        selected = selected,
        onClick = onClick,
        icon = {
            Icon(
                painter = painterResource(iconRes),
                contentDescription = null,
                modifier = Modifier.size(24.dp),
            )
        },
        label = { Text(label, maxLines = 1, overflow = TextOverflow.Ellipsis) },
    )
}

@Composable
private fun TopStatusBar(state: OpenNowUiState) {
    Surface(
        modifier = Modifier.fillMaxWidth(),
        color = MaterialTheme.colorScheme.background,
        tonalElevation = 0.dp,
        shadowElevation = 0.dp,
    ) {
        Row(
            Modifier.padding(horizontal = 14.dp, vertical = 10.dp),
            verticalAlignment = Alignment.CenterVertically,
        ) {
            OpenNowMark(34.dp)
            Spacer(Modifier.width(10.dp))
            Column(Modifier.weight(1f)) {
                Text(state.authSession?.user?.displayName ?: "OpenNOW", fontWeight = FontWeight.Bold, maxLines = 1, overflow = TextOverflow.Ellipsis)
                val tier = state.subscriptionInfo?.membershipTier ?: state.authSession?.user?.membershipTier ?: "GFN"
                Text("$tier ${state.activeSession?.let { "  Active session ${it.sessionId.take(8)}" } ?: ""}", color = MaterialTheme.colorScheme.onSurfaceVariant, style = MaterialTheme.typography.bodySmall)
            }
        }
    }
}

@Composable
private fun NativeSearchField(
    query: String,
    onQueryChange: (String) -> Unit,
    placeholder: String,
    searching: Boolean = false,
    modifier: Modifier = Modifier,
) {
    val speechLauncher = rememberLauncherForActivityResult(ActivityResultContracts.StartActivityForResult()) { result ->
        if (result.resultCode == Activity.RESULT_OK) {
            val spoken = result.data
                ?.getStringArrayListExtra(RecognizerIntent.EXTRA_RESULTS)
                ?.firstOrNull()
            if (!spoken.isNullOrBlank()) onQueryChange(spoken)
        }
    }
    val voiceSearchIntent = remember(placeholder) {
        Intent(RecognizerIntent.ACTION_RECOGNIZE_SPEECH)
            .putExtra(RecognizerIntent.EXTRA_LANGUAGE_MODEL, RecognizerIntent.LANGUAGE_MODEL_FREE_FORM)
            .putExtra(RecognizerIntent.EXTRA_LANGUAGE, Locale.getDefault())
            .putExtra(RecognizerIntent.EXTRA_PROMPT, placeholder)
    }
    Surface(
        modifier = modifier.height(56.dp),
        shape = RoundedCornerShape(28.dp),
        color = MaterialTheme.colorScheme.surfaceVariant.copy(alpha = 0.72f),
        tonalElevation = 2.dp,
    ) {
        Row(
            Modifier
                .fillMaxSize()
                .padding(start = 18.dp, end = if (query.isBlank()) 18.dp else 6.dp),
            verticalAlignment = Alignment.CenterVertically,
            horizontalArrangement = Arrangement.spacedBy(12.dp),
        ) {
            Icon(
                painter = painterResource(R.drawable.ic_search),
                contentDescription = null,
                tint = MaterialTheme.colorScheme.onSurfaceVariant,
                modifier = Modifier.size(22.dp),
            )
            BasicTextField(
                value = query,
                onValueChange = onQueryChange,
                singleLine = true,
                textStyle = MaterialTheme.typography.bodyLarge.copy(color = MaterialTheme.colorScheme.onSurface),
                cursorBrush = SolidColor(MaterialTheme.colorScheme.primary),
                modifier = Modifier.weight(1f),
                decorationBox = { innerTextField ->
                    Box(Modifier.fillMaxWidth()) {
                        if (query.isBlank()) {
                            Text(
                                placeholder,
                                color = MaterialTheme.colorScheme.onSurfaceVariant,
                                maxLines = 1,
                                overflow = TextOverflow.Ellipsis,
                            )
                        }
                        innerTextField()
                    }
                },
            )
            if (query.isNotBlank()) {
                if (searching) {
                    CircularProgressIndicator(
                        modifier = Modifier.size(18.dp),
                        strokeWidth = 2.dp,
                        color = MaterialTheme.colorScheme.primary,
                    )
                }
                IconButton(onClick = { onQueryChange("") }) {
                    Icon(
                        painter = painterResource(R.drawable.ic_clear),
                        contentDescription = stringResource(R.string.search_clear),
                        tint = MaterialTheme.colorScheme.onSurfaceVariant,
                        modifier = Modifier.size(26.dp),
                    )
                }
            } else {
                if (searching) {
                    CircularProgressIndicator(
                        modifier = Modifier.size(18.dp),
                        strokeWidth = 2.dp,
                        color = MaterialTheme.colorScheme.primary,
                    )
                }
                IconButton(onClick = { runCatching { speechLauncher.launch(voiceSearchIntent) } }) {
                    Icon(
                        painter = painterResource(R.drawable.ic_mic),
                        contentDescription = stringResource(R.string.search_voice),
                        tint = MaterialTheme.colorScheme.onSurfaceVariant,
                        modifier = Modifier.size(22.dp),
                    )
                }
            }
        }
    }
}

@Composable
private fun HomeScreen(state: OpenNowUiState, viewModel: OpenNowViewModel) {
    val visibleGames = state.games.ifEmpty { state.catalogResult.games }
    val searchingCatalog = state.loadingGames && state.catalogSearch.isNotBlank()
    SwipeToRefreshContainer(
        refreshing = state.loadingGames,
        showRefreshIndicator = !searchingCatalog,
        onRefresh = viewModel::refreshGames,
        modifier = Modifier.fillMaxSize(),
    ) {
        Column(Modifier.fillMaxSize().padding(12.dp), verticalArrangement = Arrangement.spacedBy(10.dp)) {
            NativeSearchField(
                modifier = Modifier.fillMaxWidth(),
                query = state.catalogSearch,
                onQueryChange = viewModel::setCatalogSearch,
                placeholder = stringResource(R.string.search_games),
                searching = searchingCatalog,
            )
            SortPicker(state.catalogResult.sortOptions, state.catalogSortId, viewModel::setCatalogSort)
            FilterRow(state.catalogResult.filterGroups, state.catalogFilterIds, viewModel::toggleCatalogFilter)
            AnimatedVisibility(state.error != null) {
                Text(state.error.orEmpty(), color = Color(0xffff9f9f), modifier = Modifier.padding(horizontal = 4.dp))
            }
            if (state.loadingGames && visibleGames.isEmpty()) {
                RefreshingGamesPlaceholder(Modifier.weight(1f))
            } else {
                GameGrid(
                    games = visibleGames,
                    favoriteIds = state.settings.favoriteGameIds,
                    settings = state.settings,
                    onSelect = viewModel::selectGame,
                    onFavorite = viewModel::updateFavorites,
                    onPlay = viewModel::play,
                    modifier = Modifier.weight(1f),
                )
            }
        }
    }
}

@Composable
private fun LibraryScreen(state: OpenNowUiState, viewModel: OpenNowViewModel) {
    val favorites = state.libraryGames.filter { it.id in state.settings.favoriteGameIds }
    val orderedGames = if (favorites.isNotEmpty()) favorites + state.libraryGames.filterNot { it.id in state.settings.favoriteGameIds } else state.libraryGames
    val games = orderedGames.filter { gameMatchesSearch(it, state.librarySearch) }
    SwipeToRefreshContainer(
        refreshing = state.loadingGames,
        onRefresh = viewModel::refreshGames,
        modifier = Modifier.fillMaxSize(),
    ) {
        Column(Modifier.fillMaxSize().padding(12.dp), verticalArrangement = Arrangement.spacedBy(10.dp)) {
            Row(verticalAlignment = Alignment.CenterVertically) {
                Column(Modifier.weight(1f)) {
                    Text(stringResource(R.string.nav_library), style = MaterialTheme.typography.titleLarge, fontWeight = FontWeight.Bold)
                    Text(stringResource(R.string.library_count, state.libraryGames.size), color = TextMuted)
                }
                state.activeSession?.let { active ->
                    ElevatedButton(
                        onClick = {
                            val game = (state.games + state.libraryGames).firstOrNull { it.launchAppId == active.appId.toString() }
                            if (game != null) viewModel.play(game)
                        },
                    ) { Text(stringResource(R.string.action_resume)) }
                }
            }
            NativeSearchField(
                modifier = Modifier.fillMaxWidth(),
                query = state.librarySearch,
                onQueryChange = viewModel::setLibrarySearch,
                placeholder = "Search library",
            )
            GameGrid(
                games,
                state.settings.favoriteGameIds,
                state.settings,
                viewModel::selectGame,
                viewModel::updateFavorites,
                viewModel::play,
                modifier = Modifier.weight(1f),
            )
        }
    }
}

@Composable
private fun RefreshingGamesPlaceholder(modifier: Modifier = Modifier) {
    Box(modifier.fillMaxSize(), contentAlignment = Alignment.Center) {
        Text("Loading games", color = TextMuted)
    }
}

@Composable
private fun SwipeToRefreshContainer(
    refreshing: Boolean,
    onRefresh: () -> Unit,
    modifier: Modifier = Modifier,
    showRefreshIndicator: Boolean = true,
    content: @Composable () -> Unit,
) {
    var dragDistance by remember { mutableFloatStateOf(0f) }
    Box(
        modifier.pointerInput(refreshing) {
            detectVerticalDragGestures(
                onDragEnd = {
                    if (!refreshing && dragDistance > 140f) onRefresh()
                    dragDistance = 0f
                },
                onDragCancel = { dragDistance = 0f },
                onVerticalDrag = { change, dragAmount ->
                    if (dragAmount > 0) {
                        dragDistance += dragAmount
                        change.consume()
                    }
                },
            )
        },
    ) {
        content()
        AnimatedVisibility(
            visible = showRefreshIndicator && (refreshing || dragDistance > 40f),
            modifier = Modifier.align(Alignment.TopCenter).padding(top = 8.dp),
        ) {
            Surface(
                shape = RoundedCornerShape(999.dp),
                color = PanelAlt.copy(alpha = 0.94f),
                tonalElevation = 3.dp,
            ) {
                Row(Modifier.padding(horizontal = 12.dp, vertical = 8.dp), verticalAlignment = Alignment.CenterVertically) {
                    if (refreshing) CircularProgressIndicator(Modifier.size(16.dp), strokeWidth = 2.dp)
                    Text(
                        if (refreshing) "Refreshing" else "Release to refresh",
                        color = TextMuted,
                        style = MaterialTheme.typography.labelMedium,
                        modifier = Modifier.padding(start = if (refreshing) 8.dp else 0.dp),
                    )
                }
            }
        }
    }
}

@Composable
private fun GameGrid(
    games: List<GameInfo>,
    favoriteIds: List<String>,
    settings: AppSettings,
    onSelect: (GameInfo) -> Unit,
    onFavorite: (String) -> Unit,
    onPlay: (GameInfo) -> Unit,
    modifier: Modifier = Modifier,
) {
    if (games.isEmpty()) {
        Box(modifier.fillMaxSize(), contentAlignment = Alignment.Center) {
            Text(stringResource(R.string.no_games_loaded), color = TextMuted)
        }
        return
    }
    val scale = settings.posterSizeScale.coerceIn(0.82f, 1.08f)
    val compact = settings.compactGameCards
    val cardHeight = if (compact) 252.dp else 286.dp
    LazyVerticalGrid(
        modifier = modifier.fillMaxSize(),
        columns = GridCells.Fixed(2),
        contentPadding = PaddingValues(4.dp),
        horizontalArrangement = Arrangement.spacedBy(if (compact) 8.dp else 10.dp),
        verticalArrangement = Arrangement.spacedBy(if (compact) 10.dp else 12.dp),
    ) {
        gridItems(games, key = { it.id }) { game ->
            GameCard(game, game.id in favoriteIds, settings, cardHeight * scale, onSelect, onFavorite, onPlay)
        }
    }
}

@Composable
private fun GameCard(
    game: GameInfo,
    favorite: Boolean,
    settings: AppSettings,
    cardHeight: androidx.compose.ui.unit.Dp,
    onSelect: (GameInfo) -> Unit,
    onFavorite: (String) -> Unit,
    onPlay: (GameInfo) -> Unit,
) {
    Card(
        modifier = Modifier
            .fillMaxWidth()
            .height(cardHeight),
        colors = CardDefaults.cardColors(
            containerColor = if (settings.expressiveUi) MaterialTheme.colorScheme.surfaceVariant.copy(alpha = 0.72f) else Panel,
        ),
        shape = RoundedCornerShape(if (settings.expressiveUi) 12.dp else 8.dp),
    ) {
        Box(
            Modifier
                .weight(1f)
                .clickable { onSelect(game) },
        ) {
            UrlImage(game.imageUrl, Modifier.fillMaxSize())
            TextButton(
                onClick = { onFavorite(game.id) },
                modifier = Modifier.align(Alignment.TopEnd),
            ) { Text(if (favorite) stringResource(R.string.action_saved) else stringResource(R.string.action_save)) }
        }
        Column(
            Modifier
                .clickable { onSelect(game) }
                .padding(9.dp),
            verticalArrangement = Arrangement.spacedBy(5.dp),
        ) {
            Text(
                game.title,
                fontWeight = FontWeight.Bold,
                minLines = 2,
                maxLines = 2,
                overflow = TextOverflow.Ellipsis,
                style = MaterialTheme.typography.bodyMedium,
            )
            if (settings.showGameStoreLabels) {
                Text(displayStoresForGame(game), color = TextMuted, maxLines = 1, overflow = TextOverflow.Ellipsis, style = MaterialTheme.typography.labelMedium)
            }
        }
        Box(Modifier.padding(start = 9.dp, end = 9.dp, bottom = 9.dp)) {
            Button(onClick = { onPlay(game) }, modifier = Modifier.fillMaxWidth(), contentPadding = PaddingValues(vertical = 6.dp)) {
                Text(stringResource(R.string.action_play), maxLines = 1, overflow = TextOverflow.Ellipsis)
            }
        }
    }
}

private fun displayStoresForGame(game: GameInfo): String {
    val stores = displayStoresForVariants(game.variants).ifEmpty {
        game.availableStores.map(::gameStoreDisplayName)
    }.distinctBy { normalizeGameStore(it) }
    return stores.joinToString(", ").ifBlank { "GeForce NOW" }
}

@Composable
private fun AnimatedLaunchOverlay(modifier: Modifier = Modifier, content: @Composable () -> Unit) {
    val visibleState = remember {
        MutableTransitionState(false).apply {
            targetState = true
        }
    }
    AnimatedVisibility(
        visibleState = visibleState,
        enter = fadeIn() + slideInVertically(initialOffsetY = { it / 4 }) + scaleIn(initialScale = 0.94f),
        exit = fadeOut() + slideOutVertically(targetOffsetY = { it / 4 }) + scaleOut(targetScale = 0.94f),
        modifier = modifier,
    ) {
        content()
    }
}

@OptIn(ExperimentalLayoutApi::class)
@Composable
private fun GameDetailsSheet(
    game: GameInfo,
    favorite: Boolean,
    onPlay: (GameInfo) -> Unit,
    onFavorite: (String) -> Unit,
    onDismiss: () -> Unit,
) {
    Box(
        Modifier
            .fillMaxSize()
            .background(Color.Black.copy(alpha = 0.72f))
            .clickable(onClick = onDismiss),
        contentAlignment = Alignment.BottomCenter,
    ) {
        Surface(
            modifier = Modifier
                .fillMaxWidth()
                .fillMaxHeight(0.92f)
                .clickable(onClick = {}),
            shape = RoundedCornerShape(topStart = 28.dp, topEnd = 28.dp),
            color = Panel,
            tonalElevation = 8.dp,
        ) {
            Column(Modifier.fillMaxSize()) {
                LazyColumn(
                    modifier = Modifier.weight(1f),
                    contentPadding = PaddingValues(bottom = 18.dp),
                    verticalArrangement = Arrangement.spacedBy(14.dp),
                ) {
                    item {
                        Box(
                            Modifier
                                .fillMaxWidth()
                                .height(220.dp),
                        ) {
                            UrlImage(
                                game.screenshotUrl ?: game.imageUrl,
                                Modifier.fillMaxSize(),
                            )
                            Box(Modifier.fillMaxSize().background(Color.Black.copy(alpha = 0.28f)))
                            FavoriteIconButton(
                                favorite = favorite,
                                onClick = { onFavorite(game.id) },
                                modifier = Modifier
                                    .align(Alignment.TopEnd)
                                    .padding(10.dp),
                            )
                            Column(
                                Modifier
                                    .align(Alignment.BottomStart)
                                    .padding(18.dp),
                                verticalArrangement = Arrangement.spacedBy(6.dp),
                            ) {
                                Text(
                                    game.title,
                                    style = MaterialTheme.typography.headlineSmall,
                                    fontWeight = FontWeight.Bold,
                                    maxLines = 2,
                                    overflow = TextOverflow.Ellipsis,
                                )
                                Text(displayStoresForGame(game), color = TextMuted, maxLines = 1, overflow = TextOverflow.Ellipsis)
                            }
                        }
                    }
                    item {
                        Column(Modifier.padding(horizontal = 18.dp), verticalArrangement = Arrangement.spacedBy(12.dp)) {
                            val description = game.longDescription?.takeIf { it.isNotBlank() }
                                ?: game.description?.takeIf { it.isNotBlank() }
                            if (description != null) {
                                Text(description, color = TextPrimary, style = MaterialTheme.typography.bodyMedium)
                            } else {
                                Text("No description is available for this game yet.", color = TextMuted)
                            }
                            val chips = (game.featureLabels + game.genres + listOfNotNull(game.playType, game.membershipTierLabel))
                                .map { it.trim() }
                                .filter { it.isNotBlank() }
                                .distinct()
                            if (chips.isNotEmpty()) {
                                FlowRow(horizontalArrangement = Arrangement.spacedBy(7.dp), verticalArrangement = Arrangement.spacedBy(7.dp)) {
                                    chips.take(12).forEach { label ->
                                        AssistChip(onClick = {}, label = { Text(label, maxLines = 1, overflow = TextOverflow.Ellipsis) })
                                    }
                                }
                            }
                            DetailRows(game)
                            if (game.variants.isNotEmpty()) {
                                Column(verticalArrangement = Arrangement.spacedBy(8.dp)) {
                                    Text("Launch options", color = TextMuted, style = MaterialTheme.typography.labelMedium, fontWeight = FontWeight.Bold)
                                    launchableGameVariants(game.variants).forEach { variant ->
                                        Surface(
                                            modifier = Modifier.fillMaxWidth(),
                                            shape = RoundedCornerShape(14.dp),
                                            color = PanelAlt,
                                        ) {
                                            Row(
                                                Modifier.padding(horizontal = 12.dp, vertical = 10.dp),
                                                verticalAlignment = Alignment.CenterVertically,
                                                horizontalArrangement = Arrangement.spacedBy(10.dp),
                                            ) {
                                                Column(Modifier.weight(1f)) {
                                                    Text(gameStoreDisplayName(variant.store), fontWeight = FontWeight.SemiBold, maxLines = 1, overflow = TextOverflow.Ellipsis)
                                                    val details = listOfNotNull(
                                                        variant.libraryStatus?.takeIf { it.isNotBlank() },
                                                        variant.supportedControls.takeIf { it.isNotEmpty() }?.joinToString(", "),
                                                        variant.lastPlayedDate?.takeIf { it.isNotBlank() }?.let { "Last played $it" },
                                                    ).joinToString(" · ")
                                                    if (details.isNotBlank()) {
                                                        Text(details, color = TextMuted, style = MaterialTheme.typography.bodySmall, maxLines = 2, overflow = TextOverflow.Ellipsis)
                                                    }
                                                }
                                            }
                                        }
                                    }
                                }
                            }
                        }
                    }
                }
                Surface(color = Panel.copy(alpha = 0.98f), tonalElevation = 8.dp) {
                    Row(
                        Modifier
                            .fillMaxWidth()
                            .padding(horizontal = 14.dp, vertical = 12.dp),
                        horizontalArrangement = Arrangement.spacedBy(10.dp),
                        verticalAlignment = Alignment.CenterVertically,
                    ) {
                        OutlinedButton(onClick = onDismiss, modifier = Modifier.weight(0.8f)) {
                            Text("Dismiss", maxLines = 1, overflow = TextOverflow.Ellipsis)
                        }
                        Button(
                            onClick = {
                                onDismiss()
                                onPlay(game)
                            },
                            modifier = Modifier.weight(1.2f),
                        ) {
                            Text(stringResource(R.string.action_play), maxLines = 1, overflow = TextOverflow.Ellipsis)
                        }
                    }
                }
            }
        }
    }
}

@Composable
private fun FavoriteIconButton(favorite: Boolean, onClick: () -> Unit, modifier: Modifier = Modifier) {
    Surface(
        modifier = modifier.size(44.dp),
        shape = CircleShape,
        color = Color.Black.copy(alpha = 0.52f),
        tonalElevation = 3.dp,
    ) {
        IconButton(onClick = onClick) {
            Text(
                if (favorite) "★" else "☆",
                color = if (favorite) Green else TextPrimary,
                style = MaterialTheme.typography.titleLarge,
                fontWeight = FontWeight.Bold,
            )
        }
    }
}

@Composable
private fun DetailRows(game: GameInfo) {
    val rows = listOfNotNull(
        game.publisherName?.takeIf { it.isNotBlank() }?.let { "Publisher" to it },
        game.playabilityState?.takeIf { it.isNotBlank() }?.let { "Status" to it },
        game.contentRatings.takeIf { it.isNotEmpty() }?.joinToString(", ")?.let { "Rating" to it },
        game.lastPlayed?.takeIf { it.isNotBlank() }?.let { "Last played" to it },
        game.availableStores.takeIf { it.isNotEmpty() }?.map(::gameStoreDisplayName)?.distinct()?.joinToString(", ")?.let { "Stores" to it },
    )
    if (rows.isEmpty()) return
    Column(verticalArrangement = Arrangement.spacedBy(8.dp)) {
        rows.forEach { (label, value) ->
            Row(
                Modifier
                    .fillMaxWidth()
                    .clip(RoundedCornerShape(12.dp))
                    .background(PanelAlt)
                    .padding(horizontal = 12.dp, vertical = 10.dp),
                verticalAlignment = Alignment.Top,
                horizontalArrangement = Arrangement.spacedBy(12.dp),
            ) {
                Text(label, color = TextMuted, style = MaterialTheme.typography.bodySmall, modifier = Modifier.width(92.dp))
                Text(value, color = TextPrimary, style = MaterialTheme.typography.bodySmall, modifier = Modifier.weight(1f))
            }
        }
    }
}

private fun gameMatchesSearch(game: GameInfo, query: String): Boolean {
    val normalized = query.trim().lowercase()
    if (normalized.isBlank()) return true
    val haystack = buildString {
        append(game.title).append(' ')
        append(game.description.orEmpty()).append(' ')
        append(game.longDescription.orEmpty()).append(' ')
        append(game.publisherName.orEmpty()).append(' ')
        append(game.genres.joinToString(" ")).append(' ')
        append(game.featureLabels.joinToString(" ")).append(' ')
        append(displayStoresForGame(game))
    }.lowercase()
    return normalized.split(Regex("\\s+")).all { it in haystack }
}

@Composable
private fun StoreLaunchSelector(
    game: GameInfo,
    onLaunch: (GameInfo, GameVariant) -> Unit,
    onDismiss: () -> Unit,
    modifier: Modifier = Modifier,
) {
    val variants = remember(game) { launchableGameVariants(game.variants) }
    Box(
        Modifier
            .fillMaxSize()
            .background(Color.Black.copy(alpha = 0.72f))
            .clickable(enabled = false) {},
        contentAlignment = Alignment.Center,
    ) {
        Card(
            modifier = modifier
                .fillMaxWidth(0.92f)
                .fillMaxHeight(0.58f),
            colors = CardDefaults.cardColors(containerColor = Panel),
            shape = RoundedCornerShape(22.dp),
        ) {
            Column(Modifier.fillMaxSize().padding(18.dp), verticalArrangement = Arrangement.spacedBy(12.dp)) {
                Row(verticalAlignment = Alignment.CenterVertically) {
                    UrlImage(
                        game.imageUrl,
                        Modifier
                            .width(58.dp)
                            .height(76.dp)
                            .clip(RoundedCornerShape(12.dp)),
                    )
                    Spacer(Modifier.width(12.dp))
                    Column(Modifier.weight(1f)) {
                        Text(game.title, fontWeight = FontWeight.Bold, maxLines = 1, overflow = TextOverflow.Ellipsis)
                        Text("Choose launcher", color = TextMuted, style = MaterialTheme.typography.bodySmall)
                    }
                }

                LazyColumn(
                    modifier = Modifier.weight(1f),
                    verticalArrangement = Arrangement.spacedBy(8.dp),
                ) {
                    items(variants, key = { it.id }) { variant ->
                        Surface(
                            modifier = Modifier
                                .fillMaxWidth()
                                .clickable { onLaunch(game, variant) },
                            shape = RoundedCornerShape(14.dp),
                            color = PanelAlt,
                        ) {
                            Row(
                                Modifier.padding(14.dp),
                                verticalAlignment = Alignment.CenterVertically,
                                horizontalArrangement = Arrangement.spacedBy(12.dp),
                            ) {
                                Column(Modifier.weight(1f), verticalArrangement = Arrangement.spacedBy(2.dp)) {
                                    Text(gameStoreDisplayName(variant.store), fontWeight = FontWeight.Bold)
                                    val details = listOfNotNull(
                                        variant.libraryStatus?.takeIf { it.isNotBlank() },
                                        variant.supportedControls.takeIf { it.isNotEmpty() }?.joinToString(", "),
                                    ).joinToString(" · ")
                                    if (details.isNotBlank()) {
                                        Text(details, color = TextMuted, style = MaterialTheme.typography.bodySmall, maxLines = 1, overflow = TextOverflow.Ellipsis)
                                    }
                                }
                                Button(onClick = { onLaunch(game, variant) }) { Text("Launch") }
                            }
                        }
                    }
                }

                TextButton(onClick = onDismiss, modifier = Modifier.align(Alignment.End)) {
                    Text("Cancel")
                }
            }
        }
    }
}

@OptIn(ExperimentalMaterial3Api::class)
@Composable
private fun SettingsScreen(state: OpenNowUiState, viewModel: OpenNowViewModel) {
    val settings = state.settings
    LazyColumn(
        Modifier
            .fillMaxSize()
            .background(SettingsBackground),
        contentPadding = PaddingValues(14.dp),
        verticalArrangement = Arrangement.spacedBy(12.dp),
    ) {
        item {
            SettingsSection(stringResource(R.string.settings_section_stream)) {
                ChoiceRow(stringResource(R.string.settings_resolution), listOf("1280x720", "1920x1080", "2560x1440", "3840x2160"), settings.stream.resolution) {
                    viewModel.updateStreamSettings { s -> s.copy(resolution = it) }
                }
                ChoiceRow(stringResource(R.string.settings_aspect_ratio), listOf("16:9", "16:10", "21:9", "32:9"), settings.stream.aspectRatio) {
                    viewModel.updateStreamSettings { s -> s.copy(aspectRatio = it) }
                }
                NumberSlider(stringResource(R.string.settings_fps), settings.stream.fps.toFloat(), 30f, 240f, 30f) {
                    viewModel.updateStreamSettings { s -> s.copy(fps = it.roundToInt()) }
                }
                NumberSlider(stringResource(R.string.settings_bitrate), settings.stream.maxBitrateMbps.toFloat(), 1f, 150f, 1f) {
                    viewModel.updateStreamSettings { s -> s.copy(maxBitrateMbps = it.roundToInt()) }
                }
                ChoiceRow(stringResource(R.string.settings_codec), VideoCodec.entries.map { it.name }, settings.stream.codec.name) {
                    viewModel.updateStreamSettings { s -> s.copy(codec = VideoCodec.valueOf(it)) }
                }
                ChoiceRow(stringResource(R.string.settings_color), ColorQuality.entries.map { it.label }, settings.stream.colorQuality.label) { label ->
                    viewModel.updateStreamSettings { s -> s.copy(colorQuality = ColorQuality.entries.first { it.label == label }) }
                }
                ChoiceRow(stringResource(R.string.settings_region), listOf(stringResource(R.string.option_auto)) + state.regions.map { it.name }, state.regions.firstOrNull { it.url == settings.stream.region }?.name ?: stringResource(R.string.option_auto)) { label ->
                    val url = state.regions.firstOrNull { it.name == label }?.url.orEmpty()
                    viewModel.updateStreamSettings { s -> s.copy(region = url) }
                }
                SettingSwitch(stringResource(R.string.settings_l4s), settings.stream.enableL4S) { viewModel.updateStreamSettings { s -> s.copy(enableL4S = it) } }
                SettingSwitch(stringResource(R.string.settings_cloud_gsync), settings.stream.enableCloudGsync) { viewModel.updateStreamSettings { s -> s.copy(enableCloudGsync = it) } }
            }
        }
        item {
            SettingsSection("Input") {
                NumberSlider("Mouse sensitivity", settings.stream.mouseSensitivity, 0.25f, 3f, 0.05f) {
                    viewModel.updateStreamSettings { s -> s.copy(mouseSensitivity = it) }
                }
                NumberSlider("Mouse acceleration", settings.stream.mouseAcceleration.toFloat(), 1f, 150f, 1f) {
                    viewModel.updateStreamSettings { s -> s.copy(mouseAcceleration = it.roundToInt()) }
                }
                ChoiceRow("Keyboard layout", listOf("en-US", "en-GB", "de-DE", "fr-FR", "es-ES", "pt-BR", "ja-JP", "ko-KR", "zh-CN"), settings.stream.keyboardLayout) {
                    viewModel.updateStreamSettings { s -> s.copy(keyboardLayout = it) }
                }
                ChoiceRow("Game language", listOf("en_US", "en_GB", "de_DE", "fr_FR", "es_ES", "pt_BR", "ja_JP", "ko_KR", "zh_CN"), settings.stream.gameLanguage) {
                    viewModel.updateStreamSettings { s -> s.copy(gameLanguage = it) }
                }
                SettingSwitch("Clipboard paste", settings.clipboardPaste) { enabled -> viewModel.updateSettings(settings.copy(clipboardPaste = enabled)) }
                SettingSwitch("Native mouse capture", settings.mouseCapture) { enabled -> viewModel.updateSettings(settings.copy(mouseCapture = enabled)) }
                SettingSwitch("Touch controls", settings.androidTouch.enabled) { enabled -> viewModel.updateSettings(settings.copy(androidTouch = settings.androidTouch.copy(enabled = enabled))) }
                SettingSwitch("Finger mouse", settings.androidTouch.mousePad) { enabled -> viewModel.updateSettings(settings.copy(androidTouch = settings.androidTouch.copy(mousePad = enabled))) }
                NumberSlider("Touch layout scale", settings.androidTouch.scale, 0.6f, 1.4f, 0.05f) { value -> viewModel.updateSettings(settings.copy(androidTouch = settings.androidTouch.copy(scale = value))) }
                NumberSlider("Touch button size", settings.androidTouch.buttonScale, 0.65f, 1.5f, 0.05f) { value -> viewModel.updateSettings(settings.copy(androidTouch = settings.androidTouch.copy(buttonScale = value))) }
                NumberSlider("Touch stick size", settings.androidTouch.stickScale, 0.65f, 1.5f, 0.05f) { value -> viewModel.updateSettings(settings.copy(androidTouch = settings.androidTouch.copy(stickScale = value))) }
                NumberSlider("Touch opacity", settings.androidTouch.opacity, 0.15f, 1f, 0.05f) { value -> viewModel.updateSettings(settings.copy(androidTouch = settings.androidTouch.copy(opacity = value))) }
            }
        }
        item {
            SettingsSection(stringResource(R.string.settings_section_interface)) {
                val accentOptions = UiAccent.entries.map { it to uiAccentLabel(it) }
                SettingSwitch(stringResource(R.string.settings_dynamic_color), settings.dynamicColor) { viewModel.updateSettings(settings.copy(dynamicColor = it)) }
                ChoiceRow(stringResource(R.string.settings_accent), accentOptions.map { it.second }, accentOptions.firstOrNull { it.first == settings.uiAccent }?.second ?: accentOptions.first().second) { label ->
                    accentOptions.firstOrNull { it.second == label }?.first?.let { accent ->
                        viewModel.updateSettings(settings.copy(uiAccent = accent))
                    }
                }
                SettingSwitch(stringResource(R.string.settings_expressive_ui), settings.expressiveUi) { viewModel.updateSettings(settings.copy(expressiveUi = it)) }
                SettingSwitch(stringResource(R.string.settings_compact_cards), settings.compactGameCards) { viewModel.updateSettings(settings.copy(compactGameCards = it)) }
                SettingSwitch(stringResource(R.string.settings_show_store_labels), settings.showGameStoreLabels) { viewModel.updateSettings(settings.copy(showGameStoreLabels = it)) }
                NumberSlider(stringResource(R.string.settings_card_size), settings.posterSizeScale, 0.82f, 1.08f, 0.02f) { value -> viewModel.updateSettings(settings.copy(posterSizeScale = value)) }
                SettingSwitch(stringResource(R.string.settings_hide_stream_buttons), settings.hideStreamButtons) { viewModel.updateSettings(settings.copy(hideStreamButtons = it)) }
                SettingSwitch(stringResource(R.string.settings_show_stats), settings.showStatsOnLaunch) { viewModel.updateSettings(settings.copy(showStatsOnLaunch = it)) }
                SettingSwitch(stringResource(R.string.settings_hide_server_selector), settings.hideServerSelector) { viewModel.updateSettings(settings.copy(hideServerSelector = it)) }
                SettingSwitch(stringResource(R.string.settings_controller_mode), settings.controllerMode) { viewModel.updateSettings(settings.copy(controllerMode = it)) }
                SettingSwitch(stringResource(R.string.settings_controller_sounds), settings.controllerUiSounds) { viewModel.updateSettings(settings.copy(controllerUiSounds = it)) }
                SettingSwitch(stringResource(R.string.settings_controller_animations), settings.controllerBackgroundAnimations) { viewModel.updateSettings(settings.copy(controllerBackgroundAnimations = it)) }
                SettingSwitch(stringResource(R.string.settings_controller_backdrop), settings.controllerLibraryGameBackdrop) { viewModel.updateSettings(settings.copy(controllerLibraryGameBackdrop = it)) }
                SettingSwitch(stringResource(R.string.settings_auto_load_library), settings.autoLoadControllerLibrary) { viewModel.updateSettings(settings.copy(autoLoadControllerLibrary = it)) }
                SettingSwitch(stringResource(R.string.settings_auto_fullscreen), settings.autoFullScreen) { viewModel.updateSettings(settings.copy(autoFullScreen = it)) }
                SettingSwitch(stringResource(R.string.settings_session_counter), settings.sessionCounterEnabled) { viewModel.updateSettings(settings.copy(sessionCounterEnabled = it)) }
            }
        }
        item {
            SettingsSection("Account") {
                Text(state.authSession?.user?.email ?: state.authSession?.user?.displayName.orEmpty(), color = SettingsTextMuted)
                Row(horizontalArrangement = Arrangement.spacedBy(8.dp)) {
                    OutlinedButton(onClick = viewModel::logout) { Text("Sign out") }
                    OutlinedButton(onClick = viewModel::logoutAll) { Text("Sign out all") }
                }
            }
        }
        item {
            SettingsSection("Codec Diagnostics") {
                state.codecReport?.capabilities?.forEach { cap ->
                    Text("${cap.codec.name}: decode=${cap.decoderName ?: "no"} encode=${cap.encoderName ?: "no"}", color = SettingsTextMuted)
                }
                Text("Native: ${state.codecReport?.nativeRuntimeSummary ?: "unknown"}", color = SettingsTextMuted)
            }
        }
        item {
            SettingsSection("Debug Logs") {
                val clipboard = LocalClipboardManager.current
                var copied by remember { mutableStateOf(false) }
                Text("Includes launch state, queue state, ads, stream settings, input settings, and codec capabilities.", color = SettingsTextMuted)
                Row(horizontalArrangement = Arrangement.spacedBy(8.dp)) {
                    Button(onClick = {
                        clipboard.setText(AnnotatedString(viewModel.debugLogText()))
                        copied = true
                    }) {
                        Text(if (copied) "Copied logs" else "Copy logs")
                    }
                    state.error?.let { error ->
                        OutlinedButton(onClick = {
                            clipboard.setText(AnnotatedString(error))
                            copied = true
                        }) {
                            Text("Copy error")
                        }
                    }
                }
            }
        }
    }
}

@Composable
private fun StreamScreen(state: OpenNowUiState, viewModel: OpenNowViewModel) {
    val context = LocalContext.current
    val activity = context as? Activity
    val session = state.streamSession
    val game = state.streamGame
    var streamState by remember { mutableStateOf("Preparing") }
    var controlsOpen by remember { mutableStateOf(false) }
    var exitConfirmOpen by remember { mutableStateOf(false) }
    var keyboardOpen by remember { mutableStateOf(false) }
    var keyboardText by remember { mutableStateOf("") }
    var audioMuted by remember { mutableStateOf(false) }
    var statsVisible by remember(state.settings.showStatsOnLaunch) { mutableStateOf(state.settings.showStatsOnLaunch) }
    val touchMouseState = remember { TouchMouseState() }
    val client = remember {
        NativeStreamClient(
            context = context.applicationContext,
            onState = {
                streamState = it
                if (it == "Streaming") viewModel.markStreamConnected()
            },
            onError = {
                streamState = it
                viewModel.markStreamError(it)
            },
        )
    }

    DisposableEffect(Unit) {
        val decor = activity?.window?.decorView
        val oldFlags = decor?.systemUiVisibility ?: 0
        NativeStreamInputRouter.attach(client)
        decor?.systemUiVisibility = View.SYSTEM_UI_FLAG_IMMERSIVE_STICKY or View.SYSTEM_UI_FLAG_FULLSCREEN or View.SYSTEM_UI_FLAG_HIDE_NAVIGATION or View.SYSTEM_UI_FLAG_LAYOUT_FULLSCREEN or View.SYSTEM_UI_FLAG_LAYOUT_HIDE_NAVIGATION or View.SYSTEM_UI_FLAG_LAYOUT_STABLE
        onDispose {
            decor?.systemUiVisibility = oldFlags
            NativeStreamInputRouter.detach(client)
            client.release()
        }
    }

    val streamReady = session?.status in setOf(2, 3)
    LaunchedEffect(session?.sessionId, session?.status) {
        if (session != null && streamReady) {
            client.start(session, state.settings.stream)
        }
    }

    Box(Modifier.fillMaxSize().background(Color.Black)) {
        if (session == null) {
            LoadingScreen(state.launchPhase.ifBlank { "No active stream" })
        } else if (!streamReady) {
            QueueLoadingScreen(state, viewModel)
        } else {
            AndroidView(
                modifier = Modifier.fillMaxSize(),
                factory = { ctx ->
                    client.createRenderer(ctx).apply {
                        isFocusable = true
                        isFocusableInTouchMode = true
                        requestFocus()
                    }
                },
                update = { renderer ->
                    renderer.isFocusable = true
                    renderer.isFocusableInTouchMode = true
                    renderer.setOnKeyListener { _, _, event -> client.dispatchKey(event) }
                    renderer.setOnGenericMotionListener { _, event -> client.dispatchMotion(event) }
                    renderer.setOnTouchListener { view, event ->
                        touchMouseState.handle(view, event, state.settings.androidTouch.mousePad, client)
                    }
                    renderer.requestFocus()
                },
            )
            if (statsVisible) {
                StreamStatsPill(
                    gameTitle = game?.title ?: "Stream",
                    status = streamState,
                    settings = state.settings,
                    audioMuted = audioMuted,
                    modifier = Modifier.align(Alignment.TopStart),
                )
            }
            if (state.settings.androidTouch.enabled) {
                TouchOverlay(
                    client = client,
                    touch = state.settings.androidTouch,
                    modifier = Modifier.align(Alignment.BottomCenter),
                )
            }
            if (!state.settings.hideStreamButtons) {
                StreamControlLauncher(
                    controlsOpen = controlsOpen,
                    status = (state.queuePosition?.let { "Queue $it" } ?: streamState).takeUnless(::shouldHideStreamStatusText),
                    onToggle = { controlsOpen = !controlsOpen },
                    onExit = { exitConfirmOpen = true },
                    modifier = Modifier.align(Alignment.TopEnd),
                )
                AnimatedVisibility(
                    visible = controlsOpen,
                    enter = fadeIn() + slideInVertically(initialOffsetY = { it / 4 }) + scaleIn(initialScale = 0.96f),
                    exit = fadeOut() + slideOutVertically(targetOffsetY = { it / 4 }) + scaleOut(targetScale = 0.96f),
                    modifier = Modifier.align(Alignment.BottomEnd),
                ) {
                    StreamControlsPanel(
                        gameTitle = game?.title ?: "Stream",
                        status = (state.queuePosition?.let { "Queue $it" } ?: streamState).takeUnless(::shouldHideStreamStatusText),
                        settings = state.settings,
                        audioMuted = audioMuted,
                        statsVisible = statsVisible,
                        keyboardOpen = keyboardOpen,
                        onAudioToggle = {
                            audioMuted = !audioMuted
                            client.setAudioMuted(audioMuted)
                        },
                        onStatsToggle = {
                            statsVisible = !statsVisible
                            viewModel.updateSettings(state.settings.copy(showStatsOnLaunch = statsVisible))
                        },
                        onKeyboardToggle = { keyboardOpen = !keyboardOpen },
                        onEsc = { client.sendKeyCode(KeyEvent.KEYCODE_ESCAPE) },
                        onEnter = { client.sendKeyCode(KeyEvent.KEYCODE_ENTER) },
                        onBackspace = { client.sendKeyCode(KeyEvent.KEYCODE_DEL) },
                        onTouchControlsToggle = {
                            viewModel.updateSettings(
                                state.settings.copy(
                                    androidTouch = state.settings.androidTouch.copy(enabled = !state.settings.androidTouch.enabled),
                                ),
                            )
                        },
                        onMousePadToggle = {
                            viewModel.updateSettings(
                                state.settings.copy(
                                    androidTouch = state.settings.androidTouch.copy(mousePad = !state.settings.androidTouch.mousePad),
                                ),
                            )
                        },
                        onTouchScaleChange = { value ->
                            viewModel.updateSettings(state.settings.copy(androidTouch = state.settings.androidTouch.copy(scale = value)))
                        },
                        onButtonScaleChange = { value ->
                            viewModel.updateSettings(state.settings.copy(androidTouch = state.settings.androidTouch.copy(buttonScale = value)))
                        },
                        onStickScaleChange = { value ->
                            viewModel.updateSettings(state.settings.copy(androidTouch = state.settings.androidTouch.copy(stickScale = value)))
                        },
                        onOpacityChange = { value ->
                            viewModel.updateSettings(state.settings.copy(androidTouch = state.settings.androidTouch.copy(opacity = value)))
                        },
                        onClose = { controlsOpen = false },
                    )
                }
                if (keyboardOpen) {
                    AnimatedLaunchOverlay(Modifier.align(Alignment.BottomCenter)) {
                        StreamKeyboardBar(
                            text = keyboardText,
                            onTextChange = { keyboardText = it },
                            onSend = {
                                client.sendText(keyboardText)
                                keyboardText = ""
                            },
                            onBackspace = { client.sendKeyCode(KeyEvent.KEYCODE_DEL) },
                            onEnter = { client.sendKeyCode(KeyEvent.KEYCODE_ENTER) },
                            onEsc = { client.sendKeyCode(KeyEvent.KEYCODE_ESCAPE) },
                            onDone = { keyboardOpen = false },
                        )
                    }
                }
                if (exitConfirmOpen) {
                    AnimatedLaunchOverlay(Modifier.align(Alignment.Center)) {
                        StreamExitConfirmation(
                            gameTitle = game?.title ?: "this game",
                            onKeepPlaying = { exitConfirmOpen = false },
                            onExit = {
                                exitConfirmOpen = false
                                viewModel.stopStream()
                            },
                        )
                    }
                }
            }
        }
    }
}

@Composable
private fun StreamControlLauncher(
    controlsOpen: Boolean,
    status: String?,
    onToggle: () -> Unit,
    onExit: () -> Unit,
    modifier: Modifier = Modifier,
) {
    Row(
        modifier.padding(top = 10.dp, end = 10.dp),
        horizontalArrangement = Arrangement.spacedBy(8.dp),
        verticalAlignment = Alignment.CenterVertically,
    ) {
        if (status != null) {
            Surface(
                shape = RoundedCornerShape(999.dp),
                color = Color(0xcc061009),
                tonalElevation = 3.dp,
            ) {
                Text(
                    status,
                    color = TextMuted,
                    style = MaterialTheme.typography.labelMedium,
                    maxLines = 1,
                    overflow = TextOverflow.Ellipsis,
                    modifier = Modifier.padding(horizontal = 10.dp, vertical = 8.dp),
                )
            }
        }
        Button(onClick = onToggle, contentPadding = PaddingValues(horizontal = 12.dp, vertical = 8.dp)) {
            Text(if (controlsOpen) "Close" else "Controls")
        }
        OutlinedButton(onClick = onExit, contentPadding = PaddingValues(horizontal = 12.dp, vertical = 8.dp)) {
            Text("Exit")
        }
    }
}

@Composable
private fun StreamControlsPanel(
    gameTitle: String,
    status: String?,
    settings: AppSettings,
    audioMuted: Boolean,
    statsVisible: Boolean,
    keyboardOpen: Boolean,
    onAudioToggle: () -> Unit,
    onStatsToggle: () -> Unit,
    onKeyboardToggle: () -> Unit,
    onEsc: () -> Unit,
    onEnter: () -> Unit,
    onBackspace: () -> Unit,
    onTouchControlsToggle: () -> Unit,
    onMousePadToggle: () -> Unit,
    onTouchScaleChange: (Float) -> Unit,
    onButtonScaleChange: (Float) -> Unit,
    onStickScaleChange: (Float) -> Unit,
    onOpacityChange: (Float) -> Unit,
    onClose: () -> Unit,
) {
    Surface(
        modifier = Modifier
            .padding(14.dp)
            .fillMaxWidth(0.94f)
            .fillMaxHeight(0.72f),
        shape = RoundedCornerShape(18.dp),
        color = Color(0xee08150d),
        tonalElevation = 6.dp,
    ) {
        LazyColumn(
            contentPadding = PaddingValues(14.dp),
            verticalArrangement = Arrangement.spacedBy(12.dp),
        ) {
            item {
                Row(verticalAlignment = Alignment.CenterVertically, horizontalArrangement = Arrangement.spacedBy(10.dp)) {
                    Column(Modifier.weight(1f)) {
                        Text("Stream Controls", fontWeight = FontWeight.Bold)
                        Text(gameTitle, color = TextMuted, maxLines = 1, overflow = TextOverflow.Ellipsis)
                    }
                    if (status != null) {
                        Text(status, color = TextMuted, style = MaterialTheme.typography.labelMedium)
                    }
                    OutlinedButton(onClick = onClose, contentPadding = PaddingValues(horizontal = 10.dp, vertical = 6.dp)) {
                        Text("Done")
                    }
                }
            }
            item {
                StreamPanelSection("Display") {
                    StreamControlSwitch("Audio", if (audioMuted) "Muted" else "On", !audioMuted, onAudioToggle)
                    StreamControlSwitch("Stream stats", if (statsVisible) "On" else "Off", statsVisible, onStatsToggle)
                }
            }
            item {
                StreamPanelSection("Input") {
                    StreamControlSwitch("Keyboard bar", if (keyboardOpen) "Visible" else "Hidden", keyboardOpen, onKeyboardToggle)
                    Row(horizontalArrangement = Arrangement.spacedBy(8.dp), modifier = Modifier.fillMaxWidth()) {
                        OutlinedButton(onClick = onEsc, modifier = Modifier.weight(1f)) { Text("Esc") }
                        OutlinedButton(onClick = onEnter, modifier = Modifier.weight(1f)) { Text("Enter") }
                        OutlinedButton(onClick = onBackspace, modifier = Modifier.weight(1f)) { Text("⌫") }
                    }
                    StreamControlSwitch("Finger mouse", if (settings.androidTouch.mousePad) "On" else "Off", settings.androidTouch.mousePad, onMousePadToggle)
                    StreamControlSwitch("Touch controller", if (settings.androidTouch.enabled) "Visible" else "Hidden", settings.androidTouch.enabled, onTouchControlsToggle)
                }
            }
            item {
                StreamPanelSection("Touch Layout") {
                    CompactSlider("Layout scale", settings.androidTouch.scale, 0.6f, 1.4f, onTouchScaleChange)
                    CompactSlider("Button size", settings.androidTouch.buttonScale, 0.65f, 1.5f, onButtonScaleChange)
                    CompactSlider("Stick size", settings.androidTouch.stickScale, 0.65f, 1.5f, onStickScaleChange)
                    CompactSlider("Opacity", settings.androidTouch.opacity, 0.15f, 1f, onOpacityChange)
                }
            }
        }
    }
}

@Composable
private fun StreamPanelSection(title: String, content: @Composable ColumnScope.() -> Unit) {
    Column(verticalArrangement = Arrangement.spacedBy(8.dp)) {
        Text(title, color = TextMuted, style = MaterialTheme.typography.labelMedium, fontWeight = FontWeight.Bold)
        content()
    }
}

@Composable
private fun StreamControlSwitch(label: String, value: String, checked: Boolean, onClick: () -> Unit) {
    Row(
        Modifier
            .fillMaxWidth()
            .clip(RoundedCornerShape(12.dp))
            .background(Color.White.copy(alpha = 0.06f))
            .clickable(onClick = onClick)
            .padding(horizontal = 12.dp, vertical = 10.dp),
        verticalAlignment = Alignment.CenterVertically,
    ) {
        Column(Modifier.weight(1f)) {
            Text(label, fontWeight = FontWeight.SemiBold)
            Text(value, color = TextMuted, style = MaterialTheme.typography.labelSmall)
        }
        Switch(checked = checked, onCheckedChange = { onClick() })
    }
}

@Composable
private fun CompactSlider(label: String, value: Float, min: Float, max: Float, onChange: (Float) -> Unit) {
    var local by remember(value) { mutableFloatStateOf(value) }
    Column {
        Row(Modifier.fillMaxWidth(), verticalAlignment = Alignment.CenterVertically) {
            Text(label, Modifier.weight(1f))
            Text("${(local * 100).roundToInt()}%", color = TextMuted)
        }
        Slider(
            value = local,
            onValueChange = {
                local = it.coerceIn(min, max)
                onChange(local)
            },
            valueRange = min..max,
        )
    }
}

@Composable
private fun StreamKeyboardBar(
    text: String,
    onTextChange: (String) -> Unit,
    onSend: () -> Unit,
    onBackspace: () -> Unit,
    onEnter: () -> Unit,
    onEsc: () -> Unit,
    onDone: () -> Unit,
    modifier: Modifier = Modifier,
) {
    Surface(
        modifier = modifier.fillMaxWidth(),
        color = Color(0xf207100b),
        tonalElevation = 8.dp,
    ) {
        Column(Modifier.padding(12.dp), verticalArrangement = Arrangement.spacedBy(8.dp)) {
            OutlinedTextField(
                value = text,
                onValueChange = onTextChange,
                modifier = Modifier.fillMaxWidth(),
                singleLine = true,
                placeholder = { Text("Type into stream", color = TextMuted) },
            )
            Row(horizontalArrangement = Arrangement.spacedBy(8.dp), modifier = Modifier.fillMaxWidth()) {
                Button(onClick = onSend, enabled = text.isNotBlank(), modifier = Modifier.weight(1f)) { Text("Send") }
                OutlinedButton(onClick = onBackspace, modifier = Modifier.weight(1f)) { Text("⌫") }
                OutlinedButton(onClick = onEnter, modifier = Modifier.weight(1f)) { Text("Enter") }
                OutlinedButton(onClick = onEsc, modifier = Modifier.weight(1f)) { Text("Esc") }
                TextButton(onClick = onDone) { Text("Done") }
            }
        }
    }
}

@Composable
private fun StreamStatsPill(
    gameTitle: String,
    status: String,
    settings: AppSettings,
    audioMuted: Boolean,
    modifier: Modifier = Modifier,
) {
    Surface(
        modifier = modifier.padding(10.dp),
        shape = RoundedCornerShape(12.dp),
        color = Color(0xcc061009),
        tonalElevation = 4.dp,
    ) {
        Column(Modifier.padding(horizontal = 12.dp, vertical = 9.dp), verticalArrangement = Arrangement.spacedBy(2.dp)) {
            val statusParts = listOfNotNull(
                status.takeUnless(::shouldHideStreamStatusText),
                if (audioMuted) "audio muted" else "audio on",
            )
            Text(gameTitle, fontWeight = FontWeight.Bold, maxLines = 1, overflow = TextOverflow.Ellipsis)
            Text(
                "${settings.stream.resolution} · ${settings.stream.fps}fps · ${settings.stream.maxBitrateMbps} Mbps",
                color = TextMuted,
                style = MaterialTheme.typography.labelSmall,
            )
            Text(
                statusParts.joinToString(" · "),
                color = TextMuted,
                style = MaterialTheme.typography.labelSmall,
                maxLines = 1,
                overflow = TextOverflow.Ellipsis,
            )
        }
    }
}

private fun shouldHideStreamStatusText(status: String): Boolean =
    status.trim().equals("ICE CONNECTED", ignoreCase = true)

@Composable
private fun StreamExitConfirmation(
    gameTitle: String,
    onKeepPlaying: () -> Unit,
    onExit: () -> Unit,
    modifier: Modifier = Modifier,
) {
    Box(
        Modifier
            .fillMaxSize()
            .background(Color.Black.copy(alpha = 0.58f))
            .clickable(onClick = onKeepPlaying),
        contentAlignment = Alignment.Center,
    ) {
        Surface(
            modifier = modifier
                .padding(24.dp)
                .fillMaxWidth(),
            shape = RoundedCornerShape(20.dp),
            color = Color(0xf20d1a12),
            tonalElevation = 8.dp,
        ) {
            Column(Modifier.padding(18.dp), verticalArrangement = Arrangement.spacedBy(12.dp)) {
                Text("Session Control", color = TextMuted, style = MaterialTheme.typography.labelMedium, fontWeight = FontWeight.Bold)
                Text("Exit Stream?", style = MaterialTheme.typography.titleLarge, fontWeight = FontWeight.Bold)
                Text("Do you really want to exit $gameTitle?", color = TextMuted)
                Text("Your current cloud gaming session will be closed.", color = TextMuted, style = MaterialTheme.typography.bodySmall)
                Row(horizontalArrangement = Arrangement.spacedBy(10.dp), modifier = Modifier.fillMaxWidth()) {
                    OutlinedButton(onClick = onKeepPlaying, modifier = Modifier.weight(1f)) { Text("Keep Playing") }
                    Button(onClick = onExit, modifier = Modifier.weight(1f)) { Text("Exit Stream") }
                }
            }
        }
    }
}

@Composable
private fun QueueLoadingScreen(state: OpenNowUiState, viewModel: OpenNowViewModel) {
    val session = state.streamSession
    val game = state.streamGame
    val ad = session?.adState?.sessionAds?.firstOrNull()
    val mediaUrl = ad?.adMediaFiles?.firstOrNull { !it.mediaFileUrl.isNullOrBlank() }?.mediaFileUrl
        ?: ad?.adUrl
        ?: ad?.mediaUrl
    val queueCopy = queueLaunchStatusText(state)
    Column(
        Modifier
            .fillMaxSize()
            .padding(18.dp),
        horizontalAlignment = Alignment.CenterHorizontally,
        verticalArrangement = Arrangement.Center,
    ) {
        UrlImage(
            game?.imageUrl,
            Modifier
                .width(96.dp)
                .height(128.dp)
                .clip(RoundedCornerShape(14.dp)),
        )
        Spacer(Modifier.height(16.dp))
        Text(game?.title ?: "Starting stream", style = MaterialTheme.typography.headlineSmall, fontWeight = FontWeight.Bold)
        Text(
            queueCopy,
            color = TextMuted,
        )
        Spacer(Modifier.height(18.dp))
        LinearProgressIndicator(Modifier.fillMaxWidth(0.7f))
        Spacer(Modifier.height(12.dp))
        Row(horizontalArrangement = Arrangement.spacedBy(10.dp)) {
            OutlinedButton(onClick = viewModel::minimizeStreamLaunch) { Text("Minimize") }
            OutlinedButton(onClick = viewModel::stopStream) { Text("Cancel launch") }
        }
        if (ad != null && mediaUrl != null) {
            Spacer(Modifier.height(18.dp))
            QueueAdPlayer(
                url = mediaUrl,
                onStarted = { viewModel.reportQueueAd(ad.adId, "start") },
                onFinished = { viewModel.reportQueueAd(ad.adId, "finish") },
            )
        } else if (session?.adState?.isAdsRequired == true) {
            Spacer(Modifier.height(12.dp))
            Text(session.adState.message ?: "Ad is required before the queue can continue.", color = TextMuted)
        }
        state.error?.let {
            Spacer(Modifier.height(12.dp))
            Text(it, color = Color(0xffff9f9f))
        }
    }
}

@Composable
private fun MinimizedQueuePill(
    state: OpenNowUiState,
    onRestore: () -> Unit,
    onCancel: () -> Unit,
    modifier: Modifier = Modifier,
) {
    Surface(
        modifier = modifier
            .padding(horizontal = 12.dp, vertical = 86.dp)
            .fillMaxWidth(),
        shape = RoundedCornerShape(18.dp),
        color = Color(0xf20a160f),
        tonalElevation = 8.dp,
    ) {
        Row(
            Modifier.padding(horizontal = 14.dp, vertical = 12.dp),
            verticalAlignment = Alignment.CenterVertically,
            horizontalArrangement = Arrangement.spacedBy(12.dp),
        ) {
            CircularProgressIndicator(Modifier.size(24.dp), strokeWidth = 2.dp, color = Green)
            Column(Modifier.weight(1f)) {
                Text(
                    state.streamGame?.title ?: "Starting stream",
                    fontWeight = FontWeight.Bold,
                    maxLines = 1,
                    overflow = TextOverflow.Ellipsis,
                )
                Text(queueLaunchStatusText(state), color = TextMuted, style = MaterialTheme.typography.bodySmall)
            }
            TextButton(onClick = onRestore) { Text("View") }
            OutlinedButton(onClick = onCancel, contentPadding = PaddingValues(horizontal = 10.dp, vertical = 6.dp)) {
                Text("Cancel")
            }
        }
    }
}

@Composable
private fun QueueAdPlayer(url: String, onStarted: () -> Unit, onFinished: () -> Unit) {
    val context = LocalContext.current
    val player = remember(url) {
        ExoPlayer.Builder(context).build().apply {
            setMediaItem(MediaItem.fromUri(url))
            prepare()
            playWhenReady = true
        }
    }
    var reportedStart by remember(url) { mutableStateOf(false) }
    var reportedFinish by remember(url) { mutableStateOf(false) }
    var playing by remember(url) { mutableStateOf(player.playWhenReady) }
    var muted by remember(url) { mutableStateOf(false) }
    DisposableEffect(player) {
        val listener = object : Player.Listener {
            override fun onIsPlayingChanged(isPlaying: Boolean) {
                playing = isPlaying
            }

            override fun onPlaybackStateChanged(playbackState: Int) {
                if (playbackState == Player.STATE_READY && !reportedStart) {
                    reportedStart = true
                    onStarted()
                }
                if (playbackState == Player.STATE_ENDED && !reportedFinish) {
                    reportedFinish = true
                    onFinished()
                }
            }
        }
        player.addListener(listener)
        onDispose {
            player.removeListener(listener)
            player.release()
        }
    }
    Column(horizontalAlignment = Alignment.CenterHorizontally, verticalArrangement = Arrangement.spacedBy(10.dp)) {
        AndroidView(
            modifier = Modifier
                .fillMaxWidth(0.8f)
                .height(220.dp)
                .clip(RoundedCornerShape(8.dp)),
            factory = { ctx -> PlayerView(ctx).apply { this.player = player; useController = false } },
            update = { it.player = player; it.useController = false },
        )
        Row(horizontalArrangement = Arrangement.spacedBy(10.dp)) {
            Button(onClick = {
                if (playing) {
                    player.pause()
                    playing = false
                } else {
                    player.play()
                    playing = true
                }
            }) {
                Text(if (playing) "Pause" else "Play")
            }
            OutlinedButton(onClick = {
                muted = !muted
                player.volume = if (muted) 0f else 1f
            }) {
                Text(if (muted) "Unmute" else "Mute")
            }
        }
    }
}

@Composable
private fun TouchOverlay(client: NativeStreamClient, touch: AndroidTouchSettings, modifier: Modifier = Modifier) {
    val opacity = touch.opacity
    val layoutScale = touch.scale
    val buttonScale = touch.buttonScale
    val stickScale = touch.stickScale
    Row(
        modifier
            .fillMaxWidth()
            .padding(18.dp),
        horizontalArrangement = Arrangement.SpaceBetween,
        verticalAlignment = Alignment.Bottom,
    ) {
        Column(verticalArrangement = Arrangement.spacedBy(10.dp), horizontalAlignment = Alignment.Start) {
            if (touch.enabled) {
                Row(horizontalArrangement = Arrangement.spacedBy(8.dp)) {
                    GamepadButton("LB", 0x0100, client, opacity, 48.dp * buttonScale * layoutScale)
                    GamepadTriggerButton("LT", left = true, client = client, opacity = opacity, size = 48.dp * buttonScale * layoutScale)
                }
                VirtualLeftStick(client, opacity, 116.dp * stickScale * layoutScale)
                DpadCluster(client, opacity, buttonScale * layoutScale)
            }
        }
        if (touch.enabled) {
            Column(verticalArrangement = Arrangement.spacedBy(8.dp), horizontalAlignment = Alignment.End) {
                Row(horizontalArrangement = Arrangement.spacedBy(8.dp)) {
                    GamepadTriggerButton("RT", left = false, client = client, opacity = opacity, size = 48.dp * buttonScale * layoutScale)
                    GamepadButton("RB", 0x0200, client, opacity, 48.dp * buttonScale * layoutScale)
                }
                Row(horizontalArrangement = Arrangement.spacedBy(8.dp)) {
                    GamepadButton("View", 0x0020, client, opacity, 44.dp * buttonScale * layoutScale)
                    GamepadButton("Menu", 0x0010, client, opacity, 44.dp * buttonScale * layoutScale)
                }
                Row(horizontalArrangement = Arrangement.spacedBy(8.dp)) {
                    GamepadButton("Y", 0x8000, client, opacity, 58.dp * buttonScale * layoutScale)
                }
                Row(horizontalArrangement = Arrangement.spacedBy(8.dp)) {
                    GamepadButton("X", 0x4000, client, opacity, 58.dp * buttonScale * layoutScale)
                    GamepadButton("B", 0x2000, client, opacity, 58.dp * buttonScale * layoutScale)
                }
                Row(horizontalArrangement = Arrangement.spacedBy(8.dp)) {
                    GamepadButton("A", 0x1000, client, opacity, 58.dp * buttonScale * layoutScale)
                }
            }
        }
    }
}

private fun clampStickOffset(offset: Offset, maxRadius: Float): Offset {
    val distance = sqrt(offset.x * offset.x + offset.y * offset.y)
    if (distance <= maxRadius || distance == 0f) return offset
    val scale = maxRadius / distance
    return Offset(offset.x * scale, offset.y * scale)
}

@Composable
private fun VirtualLeftStick(client: NativeStreamClient, opacity: Float, diameter: androidx.compose.ui.unit.Dp) {
    var knobOffset by remember { mutableStateOf(Offset.Zero) }

    DisposableEffect(client) {
        onDispose {
            client.setVirtualLeftStick(0f, 0f)
        }
    }

    Box(
        Modifier
            .size(diameter)
            .clip(CircleShape)
            .background(Color(0xff102918).copy(alpha = opacity))
            .border(1.dp, Green.copy(alpha = opacity), CircleShape)
            .pointerInput(client) {
                awaitPointerEventScope {
                    while (true) {
                        val event = awaitPointerEvent()
                        val change = event.changes.firstOrNull { it.pressed }
                        val maxRadius = min(size.width, size.height) * 0.34f
                        if (change == null) {
                            if (knobOffset != Offset.Zero) {
                                knobOffset = Offset.Zero
                                client.setVirtualLeftStick(0f, 0f)
                            }
                            continue
                        }
                        val center = Offset(size.width / 2f, size.height / 2f)
                        val clamped = clampStickOffset(change.position - center, maxRadius)
                        knobOffset = clamped
                        client.setVirtualLeftStick(
                            x = (clamped.x / maxRadius).coerceIn(-1f, 1f),
                            y = (clamped.y / maxRadius).coerceIn(-1f, 1f),
                        )
                        change.consume()
                    }
                }
            },
        contentAlignment = Alignment.Center,
    ) {
        Box(
            Modifier
                .size(diameter * 0.44f)
                .graphicsLayer {
                    translationX = knobOffset.x
                    translationY = knobOffset.y
                }
                .clip(CircleShape)
                .background(Green.copy(alpha = opacity))
                .border(1.dp, Color.White.copy(alpha = opacity * 0.65f), CircleShape),
            contentAlignment = Alignment.Center,
        ) {
            Text("L", fontWeight = FontWeight.Bold, color = Color.Black)
        }
    }
}

@Composable
private fun DpadCluster(client: NativeStreamClient, opacity: Float, scale: Float) {
    Column(horizontalAlignment = Alignment.CenterHorizontally, verticalArrangement = Arrangement.spacedBy(5.dp)) {
        GamepadButton("↑", 0x0001, client, opacity, 44.dp * scale)
        Row(horizontalArrangement = Arrangement.spacedBy(5.dp)) {
            GamepadButton("←", 0x0004, client, opacity, 44.dp * scale)
            GamepadButton("↓", 0x0002, client, opacity, 44.dp * scale)
            GamepadButton("→", 0x0008, client, opacity, 44.dp * scale)
        }
    }
}

@Composable
private fun GamepadTriggerButton(label: String, left: Boolean, client: NativeStreamClient, opacity: Float, size: androidx.compose.ui.unit.Dp) {
    var pressed by remember { mutableStateOf(false) }
    Box(
        Modifier
            .width(size * 1.24f)
            .height(size * 0.78f)
            .clip(RoundedCornerShape(999.dp))
            .background((if (pressed) Green else Color(0xff16331f)).copy(alpha = opacity))
            .border(1.dp, Green.copy(alpha = opacity), RoundedCornerShape(999.dp))
            .pointerInput(left) {
                awaitPointerEventScope {
                    while (true) {
                        val event = awaitPointerEvent()
                        val down = event.changes.any { it.pressed }
                        if (down != pressed) {
                            pressed = down
                            client.setVirtualTrigger(left, down)
                        }
                    }
                }
            },
        contentAlignment = Alignment.Center,
    ) {
        Text(label, fontWeight = FontWeight.Bold, color = if (pressed) Color.Black else TextPrimary)
    }
}

@Composable
private fun GamepadButton(label: String, mask: Int, client: NativeStreamClient, opacity: Float, size: androidx.compose.ui.unit.Dp) {
    var pressed by remember { mutableStateOf(false) }
    Box(
        Modifier
            .size(size)
            .clip(CircleShape)
            .background((if (pressed) Green else Color(0xff16331f)).copy(alpha = opacity))
            .border(1.dp, Green.copy(alpha = opacity), CircleShape)
            .pointerInput(mask) {
                awaitPointerEventScope {
                    while (true) {
                        val event = awaitPointerEvent()
                        val down = event.changes.any { it.pressed }
                        if (down != pressed) {
                            pressed = down
                            client.setVirtualButton(mask, down)
                        }
                    }
                }
            },
        contentAlignment = Alignment.Center,
    ) {
        Text(label, fontWeight = FontWeight.Bold, color = if (pressed) Color.Black else TextPrimary)
    }
}

@Composable
private fun SettingsSection(title: String, content: @Composable ColumnScope.() -> Unit) {
    val sectionShape = RoundedCornerShape(14.dp)
    Card(
        modifier = Modifier.fillMaxWidth(),
        colors = CardDefaults.cardColors(containerColor = SettingsPanel),
        shape = sectionShape,
    ) {
        Column(Modifier.fillMaxWidth().padding(14.dp), verticalArrangement = Arrangement.spacedBy(10.dp)) {
            Text(title, color = SettingsText, style = MaterialTheme.typography.titleMedium, fontWeight = FontWeight.Bold)
            content()
        }
    }
}

@Composable
private fun SettingSwitch(label: String, checked: Boolean, onCheckedChange: (Boolean) -> Unit) {
    Row(
        Modifier
            .fillMaxWidth()
            .clip(RoundedCornerShape(14.dp))
            .background(SettingsPanelAlt)
            .padding(horizontal = 12.dp, vertical = 8.dp),
        verticalAlignment = Alignment.CenterVertically,
    ) {
        Text(label, Modifier.weight(1f), color = SettingsText, maxLines = 2, overflow = TextOverflow.Ellipsis)
        Switch(checked = checked, onCheckedChange = onCheckedChange)
    }
}

@Composable
private fun NumberSlider(label: String, value: Float, min: Float, max: Float, step: Float, onChange: (Float) -> Unit) {
    var local by remember(value) { mutableFloatStateOf(value) }
    Column(
        Modifier
            .fillMaxWidth()
            .clip(RoundedCornerShape(14.dp))
            .background(SettingsPanelAlt)
            .padding(horizontal = 12.dp, vertical = 10.dp),
    ) {
        Row(Modifier.fillMaxWidth(), verticalAlignment = Alignment.CenterVertically) {
            Text(label, Modifier.weight(1f), color = SettingsText, maxLines = 2, overflow = TextOverflow.Ellipsis)
            Text(if (step < 1f) "%.2f".format(local) else local.roundToInt().toString(), color = SettingsTextMuted)
        }
        Slider(
            value = local,
            onValueChange = { local = ((it / step).roundToInt() * step).coerceIn(min, max) },
            onValueChangeFinished = { onChange(local) },
            valueRange = min..max,
        )
    }
}

@Composable
private fun ChoiceRow(label: String, options: List<String>, selected: String, onSelect: (String) -> Unit) {
    var expanded by remember { mutableStateOf(false) }
    val autoLabel = stringResource(R.string.option_auto)
    Row(
        Modifier
            .fillMaxWidth()
            .clip(RoundedCornerShape(14.dp))
            .background(SettingsPanelAlt)
            .padding(horizontal = 12.dp, vertical = 8.dp),
        verticalAlignment = Alignment.CenterVertically,
    ) {
        Text(label, Modifier.weight(1f), color = SettingsText, maxLines = 2, overflow = TextOverflow.Ellipsis)
        Box {
            OutlinedButton(onClick = { expanded = true }) { Text(selected.ifBlank { autoLabel }, maxLines = 1, overflow = TextOverflow.Ellipsis) }
            DropdownMenu(expanded = expanded, onDismissRequest = { expanded = false }) {
                options.forEach { option ->
                    DropdownMenuItem(
                        text = { Text(option) },
                        onClick = {
                            expanded = false
                            onSelect(option)
                        },
                    )
                }
            }
        }
    }
}

@Composable
private fun SortPicker(options: List<CatalogSortOption>, selected: String, onSelect: (String) -> Unit) {
    val labels = options.ifEmpty { listOf(CatalogSortOption("relevance", "Relevance", "")) }
    ChoiceRow("Sort", labels.map { it.label }, labels.firstOrNull { it.id == selected }?.label ?: labels.first().label) { label ->
        labels.firstOrNull { it.label == label }?.id?.let(onSelect)
    }
}

@OptIn(ExperimentalLayoutApi::class)
@Composable
private fun FilterRow(groups: List<CatalogFilterGroup>, selectedIds: List<String>, onToggle: (String) -> Unit) {
    val visible = groups.filter { it.id in setOf("digital_store", "genre", "subscriptions") }
    if (visible.isEmpty()) return
    var expanded by remember { mutableStateOf(false) }
    val options = visible.flatMap { group -> group.options.take(if (group.id == "genre") 10 else group.options.size) }
    Column(verticalArrangement = Arrangement.spacedBy(8.dp)) {
        Row(Modifier.fillMaxWidth(), verticalAlignment = Alignment.CenterVertically) {
            Box {
                OutlinedButton(onClick = { expanded = true }) {
                    Text(if (selectedIds.isEmpty()) "Filters" else "Filters (${selectedIds.size})")
                }
                DropdownMenu(expanded = expanded, onDismissRequest = { expanded = false }) {
                    options.forEach { option ->
                        DropdownMenuItem(
                            text = {
                                Row(verticalAlignment = Alignment.CenterVertically) {
                                    Text(if (option.id in selectedIds) "✓" else "", modifier = Modifier.width(24.dp))
                                    Text(option.label)
                                }
                            },
                            onClick = { onToggle(option.id) },
                        )
                    }
                }
            }
            Spacer(Modifier.width(10.dp))
            Text(
                "${options.size} available",
                color = TextMuted,
                style = MaterialTheme.typography.bodySmall,
            )
        }
        val selectedOptions = options.filter { it.id in selectedIds }
        if (selectedOptions.isNotEmpty()) {
            FlowRow(horizontalArrangement = Arrangement.spacedBy(6.dp), verticalArrangement = Arrangement.spacedBy(6.dp)) {
                selectedOptions.take(4).forEach { option ->
                    AssistChip(onClick = { onToggle(option.id) }, label = { Text(option.label, maxLines = 1, overflow = TextOverflow.Ellipsis) })
                }
                if (selectedOptions.size > 4) {
                    AssistChip(onClick = { expanded = true }, label = { Text("+${selectedOptions.size - 4}") })
                }
            }
        }
    }
}

@Composable
private fun PrintedWasteSelector(
    state: OpenNowUiState,
    game: GameInfo,
    viewModel: OpenNowViewModel,
    modifier: Modifier = Modifier,
) {
    val zones = remember(state.printedWasteQueue, state.printedWasteMapping, state.printedWastePings) {
        state.printedWasteQueue
            .filter { (zoneId, _) -> isStandardPrintedWasteZone(zoneId) && state.printedWasteMapping[zoneId]?.nuked != true }
            .map { (zoneId, zone) ->
                val routingUrl = printedWasteZoneUrl(zoneId)
                PrintedWasteZoneOption(
                    zoneId = zoneId,
                    zone = zone,
                    routingUrl = routingUrl,
                    pingMs = state.printedWastePings[routingUrl],
                )
            }
    }
    val autoZone = remember(zones) { recommendedPrintedWasteZone(zones) }
    val sortedZones = remember(zones, autoZone) {
        val maxPing = zones.mapNotNull { it.pingMs }.maxOrNull()?.coerceAtLeast(1) ?: 1
        val maxQueue = zones.maxOfOrNull { it.zone.QueuePosition }?.coerceAtLeast(1) ?: 1
        zones.sortedWith(
            compareByDescending<PrintedWasteZoneOption> { it.zoneId == autoZone?.zoneId }
                .thenBy { printedWasteScore(it, maxPing, maxQueue) }
                .thenBy { it.zoneId },
        )
    }
    var selectedZoneId by remember(game.id, sortedZones) { mutableStateOf<String?>(autoZone?.zoneId) }
    val selectedZone = sortedZones.firstOrNull { it.zoneId == selectedZoneId } ?: autoZone

    Box(
        Modifier
            .fillMaxSize()
            .background(Color.Black.copy(alpha = 0.72f))
            .clickable(enabled = false) {},
        contentAlignment = Alignment.Center,
    ) {
        Card(
            modifier = modifier
                .fillMaxWidth(0.94f)
                .fillMaxHeight(0.82f),
            colors = CardDefaults.cardColors(containerColor = Panel),
            shape = RoundedCornerShape(22.dp),
        ) {
            Column(Modifier.fillMaxSize().padding(18.dp), verticalArrangement = Arrangement.spacedBy(12.dp)) {
                Row(verticalAlignment = Alignment.CenterVertically) {
                    UrlImage(
                        game.imageUrl,
                        Modifier
                            .width(58.dp)
                            .height(76.dp)
                            .clip(RoundedCornerShape(12.dp)),
                    )
                    Spacer(Modifier.width(12.dp))
                    Column(Modifier.weight(1f)) {
                        Text(game.title, fontWeight = FontWeight.Bold, maxLines = 1, overflow = TextOverflow.Ellipsis)
                        Text("Free tier queue routing", color = TextMuted, style = MaterialTheme.typography.bodySmall)
                    }
                }

                if (state.printedWasteLoading) {
                    Box(Modifier.weight(1f).fillMaxWidth(), contentAlignment = Alignment.Center) {
                        Column(horizontalAlignment = Alignment.CenterHorizontally, verticalArrangement = Arrangement.spacedBy(12.dp)) {
                            CircularProgressIndicator(color = Green)
                            Text("Checking PrintedWaste queues and latency", color = TextMuted)
                        }
                    }
                } else if (state.printedWasteError != null) {
                    Box(Modifier.weight(1f).fillMaxWidth(), contentAlignment = Alignment.Center) {
                        Column(horizontalAlignment = Alignment.CenterHorizontally, verticalArrangement = Arrangement.spacedBy(12.dp)) {
                            Text(state.printedWasteError, color = Color(0xffff9f9f))
                            OutlinedButton(onClick = viewModel::refreshPrintedWasteQueues) { Text("Retry") }
                        }
                    }
                } else {
                    autoZone?.let { zoneOption ->
                        Surface(
                            modifier = Modifier.fillMaxWidth(),
                            shape = RoundedCornerShape(16.dp),
                            color = Green.copy(alpha = 0.12f),
                        ) {
                            Column(Modifier.padding(14.dp), verticalArrangement = Arrangement.spacedBy(4.dp)) {
                                Text("Recommended: ${zoneOption.zoneId}", color = Green, fontWeight = FontWeight.Bold)
                                Text(
                                    listOfNotNull(
                                        zoneOption.pingMs?.let { "${it}ms" },
                                        "Queue ${zoneOption.zone.QueuePosition}",
                                        zoneOption.zone.eta?.let { formatPrintedWasteWait(it) },
                                    ).joinToString(" · "),
                                    color = TextMuted,
                                )
                            }
                        }
                    }
                    LazyColumn(
                        modifier = Modifier.weight(1f),
                        verticalArrangement = Arrangement.spacedBy(8.dp),
                    ) {
                        items(sortedZones, key = { it.zoneId }) { zoneOption ->
                            val zoneId = zoneOption.zoneId
                            val zone = zoneOption.zone
                            val selected = zoneId == selectedZoneId
                            Surface(
                                modifier = Modifier
                                    .fillMaxWidth()
                                    .clip(RoundedCornerShape(12.dp))
                                    .clickable { selectedZoneId = zoneId },
                                shape = RoundedCornerShape(12.dp),
                                color = if (selected) Green.copy(alpha = 0.16f) else PanelAlt,
                                tonalElevation = if (selected) 2.dp else 0.dp,
                            ) {
                                Row(Modifier.padding(12.dp), verticalAlignment = Alignment.CenterVertically) {
                                    Column(Modifier.weight(1f)) {
                                        Text(zoneId, fontWeight = FontWeight.Bold, color = if (selected) Green else TextPrimary)
                                        Text(regionLabel(zone.Region), color = TextMuted, style = MaterialTheme.typography.bodySmall)
                                    }
                                    Text(zoneOption.pingMs?.let { "${it}ms" } ?: "--", color = zoneOption.pingMs?.let(::pingColor) ?: TextMuted, fontWeight = FontWeight.Bold)
                                    Spacer(Modifier.width(10.dp))
                                    Text("Q ${zone.QueuePosition}", color = queueColor(zone.QueuePosition), fontWeight = FontWeight.Bold)
                                    zone.eta?.let {
                                        Spacer(Modifier.width(10.dp))
                                        Text(formatPrintedWasteWait(it), color = TextMuted)
                                    }
                                }
                            }
                        }
                    }
                }

                Row(Modifier.fillMaxWidth(), horizontalArrangement = Arrangement.spacedBy(10.dp), verticalAlignment = Alignment.CenterVertically) {
                    TextButton(onClick = viewModel::dismissPrintedWasteSelector) { Text("Cancel") }
                    OutlinedButton(onClick = { viewModel.launchWithPrintedWaste(null) }, modifier = Modifier.weight(1f)) {
                        Text("Default")
                    }
                    Button(
                        onClick = { viewModel.launchWithPrintedWaste(selectedZone?.routingUrl) },
                        enabled = !state.printedWasteLoading && selectedZone != null,
                        modifier = Modifier.weight(1f),
                    ) {
                        Text("Launch")
                    }
                }
            }
        }
    }
}

private fun isStandardPrintedWasteZone(zoneId: String): Boolean =
    zoneId.startsWith("NP-") && !zoneId.startsWith("NPA-")

private data class PrintedWasteZoneOption(
    val zoneId: String,
    val zone: PrintedWasteZone,
    val routingUrl: String,
    val pingMs: Long?,
)

private fun recommendedPrintedWasteZone(zones: List<PrintedWasteZoneOption>): PrintedWasteZoneOption? {
    if (zones.isEmpty()) return null
    val pool = zones.filter { it.pingMs != null }.ifEmpty { zones }
    val maxPing = pool.mapNotNull { it.pingMs }.maxOrNull()?.coerceAtLeast(1) ?: 1
    val maxQueue = pool.maxOfOrNull { it.zone.QueuePosition }?.coerceAtLeast(1) ?: 1
    return pool.minWithOrNull(
        compareBy<PrintedWasteZoneOption> { printedWasteScore(it, maxPing, maxQueue) }
            .thenBy { it.pingMs ?: Long.MAX_VALUE }
            .thenBy { it.zone.QueuePosition },
    )
}

private fun printedWasteScore(zone: PrintedWasteZoneOption, maxPing: Long, maxQueue: Int): Double {
    val pingScore = ((zone.pingMs ?: maxPing).toDouble() / maxPing.toDouble()) * 0.75
    val queueScore = (zone.zone.QueuePosition.toDouble() / maxQueue.toDouble()) * 0.25
    return pingScore + queueScore
}

private fun printedWasteZoneUrl(zoneId: String): String =
    "https://${zoneId.lowercase()}.cloudmatchbeta.nvidiagrid.net/"

private fun formatPrintedWasteWait(etaMs: Long): String {
    val minutes = ((etaMs + 59_999L) / 60_000L).coerceAtLeast(1L)
    return if (minutes < 60L) "${minutes}m" else "${minutes / 60L}h ${minutes % 60L}m"
}

private fun queueColor(queue: Int): Color = when {
    queue <= 5 -> Green
    queue <= 20 -> Color(0xffc7ef6b)
    queue <= 45 -> Color(0xffffc95a)
    else -> Color(0xffff8d8d)
}

private fun pingColor(pingMs: Long): Color = when {
    pingMs <= 60L -> Green
    pingMs <= 120L -> Color(0xffc7ef6b)
    pingMs <= 180L -> Color(0xffffc95a)
    else -> Color(0xffff8d8d)
}

private fun regionLabel(region: String): String = when (region) {
    "US" -> "North America"
    "CA" -> "Canada"
    "EU" -> "Europe"
    "JP" -> "Japan"
    "KR" -> "South Korea"
    "THAI" -> "Southeast Asia"
    "MY" -> "Malaysia"
    else -> region
}

@Composable
private fun ProviderPicker(providers: List<LoginProvider>, selected: LoginProvider, onSelect: (LoginProvider) -> Unit) {
    var expanded by remember { mutableStateOf(false) }
    Box {
        OutlinedButton(onClick = { expanded = true }) { Text(selected.displayName) }
        DropdownMenu(expanded = expanded, onDismissRequest = { expanded = false }) {
            providers.forEach { provider ->
                DropdownMenuItem(
                    text = { Text(provider.displayName) },
                    onClick = {
                        expanded = false
                        onSelect(provider)
                    },
                )
            }
        }
    }
}

@Composable
private fun UrlImage(url: String?, modifier: Modifier = Modifier) {
    val image by produceState<ImageBitmap?>(initialValue = null, key1 = url) {
        value = withContext(Dispatchers.IO) {
            runCatching {
                if (url.isNullOrBlank()) null else URL(url).openStream().use { BitmapFactory.decodeStream(it)?.asImageBitmap() }
            }.getOrNull()
        }
    }
    Box(modifier.background(Color(0xff102015)), contentAlignment = Alignment.Center) {
        if (image != null) {
            Image(bitmap = image!!, contentDescription = null, modifier = Modifier.fillMaxSize(), contentScale = ContentScale.Crop)
        } else {
            OpenNowMark(42.dp)
        }
    }
}

@Composable
private fun OpenNowMark(size: androidx.compose.ui.unit.Dp) {
    Image(
        painter = painterResource(R.drawable.opennow_logo_mark),
        contentDescription = "OpenNOW",
        modifier = Modifier
            .width(size * 1.85f)
            .height(size),
        contentScale = ContentScale.Fit,
    )
}

private val ColorQuality.label: String
    get() = when (this) {
        ColorQuality.EightBit420 -> "8-bit 4:2:0"
        ColorQuality.EightBit444 -> "8-bit 4:4:4"
        ColorQuality.TenBit420 -> "10-bit 4:2:0"
        ColorQuality.TenBit444 -> "10-bit 4:4:4"
    }

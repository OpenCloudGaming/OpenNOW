package com.opencloudgaming.opennow

import android.app.Activity
import android.graphics.BitmapFactory
import android.view.KeyEvent
import android.view.MotionEvent
import android.view.View
import androidx.compose.foundation.gestures.detectVerticalDragGestures
import androidx.compose.animation.AnimatedVisibility
import androidx.compose.foundation.Image
import androidx.compose.foundation.background
import androidx.compose.foundation.border
import androidx.compose.foundation.clickable
import androidx.compose.foundation.gestures.detectDragGestures
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
import androidx.compose.ui.graphics.asImageBitmap
import androidx.compose.ui.graphics.graphicsLayer
import androidx.compose.ui.input.pointer.pointerInput
import androidx.compose.ui.layout.ContentScale
import androidx.compose.ui.platform.LocalContext
import androidx.compose.ui.platform.LocalClipboardManager
import androidx.compose.ui.res.painterResource
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
private const val TOUCH_MOUSE_TAP_SLOP_PX = 18f

private class TouchMouseState {
    private var activePointerId = -1
    private var downX = 0f
    private var downY = 0f
    private var lastX = 0f
    private var lastY = 0f
    private var cursorX = Float.NaN
    private var cursorY = Float.NaN
    private var moved = false

    fun handle(view: View, event: MotionEvent, enabled: Boolean, client: NativeStreamClient): Boolean {
        if (!enabled) {
            activePointerId = -1
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
                moveCursorTo(event.x, event.y, view.width, view.height, client)
                return true
            }
            MotionEvent.ACTION_POINTER_DOWN -> {
                activePointerId = -1
                moved = true
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
                cursorX = (cursorX + dx).coerceIn(0f, view.width.toFloat())
                cursorY = (cursorY + dy).coerceIn(0f, view.height.toFloat())
                lastX = x
                lastY = y
                return true
            }
            MotionEvent.ACTION_UP -> {
                val wasTap = activePointerId >= 0 && !moved
                activePointerId = -1
                if (wasTap) {
                    client.sendTouchMouseClick()
                }
                return true
            }
            MotionEvent.ACTION_CANCEL -> {
                activePointerId = -1
                return true
            }
        }
        return true
    }

    private fun moveCursorTo(x: Float, y: Float, width: Int, height: Int, client: NativeStreamClient) {
        if (!cursorX.isFinite() || !cursorY.isFinite()) {
            cursorX = width / 2f
            cursorY = height / 2f
        }
        val targetX = x.coerceIn(0f, width.toFloat())
        val targetY = y.coerceIn(0f, height.toFloat())
        sendMouseDelta(targetX - cursorX, targetY - cursorY, client)
        cursorX = targetX
        cursorY = targetY
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
fun OpenNowTheme(content: @Composable () -> Unit) {
    MaterialTheme(
        colorScheme = darkColorScheme(
            primary = Green,
            onPrimary = Color(0xff031008),
            background = Background,
            surface = Panel,
            surfaceVariant = PanelAlt,
            onBackground = TextPrimary,
            onSurface = TextPrimary,
            onSurfaceVariant = TextMuted,
        ),
        content = content,
    )
}

@Composable
fun OpenNowApp(viewModel: OpenNowViewModel) {
    val state by viewModel.state.collectAsStateWithLifecycle()

    Surface(Modifier.fillMaxSize(), color = Background) {
        when {
            state.initializing -> LoadingScreen(state.launchPhase.ifBlank { "Starting OpenNOW" })
            state.authSession == null -> LoginScreen(state, viewModel)
            else -> MainShell(state, viewModel)
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
    Scaffold(
        bottomBar = {
            if (!inStream) {
                NavigationBar(containerColor = Color(0xff08150d), tonalElevation = 3.dp) {
                    BottomNavItem(
                        selected = state.page == AppPage.Home,
                        onClick = { viewModel.setPage(AppPage.Home) },
                        iconRes = R.drawable.ic_tab_store,
                        label = "Store",
                    )
                    BottomNavItem(
                        selected = state.page == AppPage.Library,
                        onClick = { viewModel.setPage(AppPage.Library) },
                        iconRes = R.drawable.ic_tab_library,
                        label = "Library",
                    )
                    BottomNavItem(
                        selected = state.page == AppPage.Settings,
                        onClick = { viewModel.setPage(AppPage.Settings) },
                        iconRes = R.drawable.ic_tab_settings,
                        label = "Settings",
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
            state.pendingPrintedWasteGame?.let { game ->
                PrintedWasteSelector(state, game, viewModel, Modifier.align(Alignment.Center))
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
    Row(
        Modifier
            .fillMaxWidth()
            .background(Color(0xff061009))
            .padding(horizontal = 14.dp, vertical = 10.dp),
        verticalAlignment = Alignment.CenterVertically,
    ) {
        OpenNowMark(34.dp)
        Spacer(Modifier.width(10.dp))
        Column(Modifier.weight(1f)) {
            Text(state.authSession?.user?.displayName ?: "OpenNOW", fontWeight = FontWeight.Bold, maxLines = 1, overflow = TextOverflow.Ellipsis)
            val tier = state.subscriptionInfo?.membershipTier ?: state.authSession?.user?.membershipTier ?: "GFN"
            Text("$tier ${state.activeSession?.let { "  Active session ${it.sessionId.take(8)}" } ?: ""}", color = TextMuted, style = MaterialTheme.typography.bodySmall)
        }
        if (state.loadingGames) {
            CircularProgressIndicator(Modifier.size(24.dp), strokeWidth = 2.dp)
        }
    }
}

@Composable
private fun HomeScreen(state: OpenNowUiState, viewModel: OpenNowViewModel) {
    SwipeToRefreshContainer(
        refreshing = state.loadingGames,
        onRefresh = viewModel::refreshGames,
        modifier = Modifier.fillMaxSize(),
    ) {
    Column(Modifier.fillMaxSize().padding(12.dp), verticalArrangement = Arrangement.spacedBy(10.dp)) {
        OutlinedTextField(
            modifier = Modifier.fillMaxWidth(),
            value = state.catalogSearch,
            onValueChange = viewModel::setCatalogSearch,
            singleLine = true,
            placeholder = { Text("Search games", maxLines = 1, overflow = TextOverflow.Ellipsis) },
        )
        SortPicker(state.catalogResult.sortOptions, state.catalogSortId, viewModel::setCatalogSort)
        FilterRow(state.catalogResult.filterGroups, state.catalogFilterIds, viewModel::toggleCatalogFilter)
        AnimatedVisibility(state.error != null) {
            Text(state.error.orEmpty(), color = Color(0xffff9f9f), modifier = Modifier.padding(horizontal = 4.dp))
        }
        if (state.loadingGames && state.games.isEmpty()) {
            LoadingScreen("Loading games")
        } else {
            GameGrid(
                games = state.games.ifEmpty { state.catalogResult.games },
                favoriteIds = state.settings.favoriteGameIds,
                onSelect = viewModel::selectGame,
                onFavorite = viewModel::updateFavorites,
                onPlay = viewModel::play,
            )
        }
    }
    }
}

@Composable
private fun LibraryScreen(state: OpenNowUiState, viewModel: OpenNowViewModel) {
    val favorites = state.libraryGames.filter { it.id in state.settings.favoriteGameIds }
    val games = if (favorites.isNotEmpty()) favorites + state.libraryGames.filterNot { it.id in state.settings.favoriteGameIds } else state.libraryGames
    SwipeToRefreshContainer(
        refreshing = state.loadingGames,
        onRefresh = viewModel::refreshGames,
        modifier = Modifier.fillMaxSize(),
    ) {
    Column(Modifier.fillMaxSize().padding(12.dp), verticalArrangement = Arrangement.spacedBy(10.dp)) {
        Row(verticalAlignment = Alignment.CenterVertically) {
            Column(Modifier.weight(1f)) {
                Text("Library", style = MaterialTheme.typography.titleLarge, fontWeight = FontWeight.Bold)
                Text("${state.libraryGames.size} games", color = TextMuted)
            }
            state.activeSession?.let { active ->
                ElevatedButton(
                    onClick = {
                        val game = (state.games + state.libraryGames).firstOrNull { it.launchAppId == active.appId.toString() }
                        if (game != null) viewModel.play(game)
                    },
                ) { Text("Resume") }
            }
        }
        GameGrid(games, state.settings.favoriteGameIds, viewModel::selectGame, viewModel::updateFavorites, viewModel::play)
    }
    }
}

@Composable
private fun SwipeToRefreshContainer(
    refreshing: Boolean,
    onRefresh: () -> Unit,
    modifier: Modifier = Modifier,
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
            visible = refreshing || dragDistance > 40f,
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
    onSelect: (GameInfo) -> Unit,
    onFavorite: (String) -> Unit,
    onPlay: (GameInfo) -> Unit,
) {
    if (games.isEmpty()) {
        Box(Modifier.fillMaxSize(), contentAlignment = Alignment.Center) {
            Text("No games loaded", color = TextMuted)
        }
        return
    }
    LazyVerticalGrid(
        columns = GridCells.Adaptive(136.dp),
        contentPadding = PaddingValues(4.dp),
        horizontalArrangement = Arrangement.spacedBy(10.dp),
        verticalArrangement = Arrangement.spacedBy(12.dp),
    ) {
        gridItems(games, key = { it.id }) { game ->
            GameCard(game, game.id in favoriteIds, onSelect, onFavorite, onPlay)
        }
    }
}

@Composable
private fun GameCard(
    game: GameInfo,
    favorite: Boolean,
    onSelect: (GameInfo) -> Unit,
    onFavorite: (String) -> Unit,
    onPlay: (GameInfo) -> Unit,
) {
    Card(
        modifier = Modifier
            .fillMaxWidth()
            .clickable { onSelect(game) },
        colors = CardDefaults.cardColors(containerColor = Panel),
        shape = RoundedCornerShape(8.dp),
    ) {
        Box {
            UrlImage(game.imageUrl, Modifier.fillMaxWidth().aspectRatio(0.66f))
            TextButton(
                onClick = { onFavorite(game.id) },
                modifier = Modifier.align(Alignment.TopEnd),
            ) { Text(if (favorite) "Saved" else "Save") }
        }
        Column(Modifier.padding(10.dp), verticalArrangement = Arrangement.spacedBy(6.dp)) {
            Text(game.title, fontWeight = FontWeight.Bold, maxLines = 2, overflow = TextOverflow.Ellipsis)
            Text(game.availableStores.joinToString(", ").ifBlank { "GeForce NOW" }, color = TextMuted, maxLines = 1, overflow = TextOverflow.Ellipsis)
            Button(onClick = { onPlay(game) }, modifier = Modifier.fillMaxWidth()) { Text("Play") }
        }
    }
}

@OptIn(ExperimentalMaterial3Api::class)
@Composable
private fun SettingsScreen(state: OpenNowUiState, viewModel: OpenNowViewModel) {
    val settings = state.settings
    LazyColumn(
        Modifier.fillMaxSize(),
        contentPadding = PaddingValues(14.dp),
        verticalArrangement = Arrangement.spacedBy(12.dp),
    ) {
        item {
            SettingsSection("Stream") {
                ChoiceRow("Resolution", listOf("1280x720", "1920x1080", "2560x1440", "3840x2160"), settings.stream.resolution) {
                    viewModel.updateStreamSettings { s -> s.copy(resolution = it) }
                }
                ChoiceRow("Aspect ratio", listOf("16:9", "16:10", "21:9", "32:9"), settings.stream.aspectRatio) {
                    viewModel.updateStreamSettings { s -> s.copy(aspectRatio = it) }
                }
                NumberSlider("FPS", settings.stream.fps.toFloat(), 30f, 240f, 30f) {
                    viewModel.updateStreamSettings { s -> s.copy(fps = it.roundToInt()) }
                }
                NumberSlider("Bitrate Mbps", settings.stream.maxBitrateMbps.toFloat(), 1f, 150f, 1f) {
                    viewModel.updateStreamSettings { s -> s.copy(maxBitrateMbps = it.roundToInt()) }
                }
                ChoiceRow("Codec", VideoCodec.entries.map { it.name }, settings.stream.codec.name) {
                    viewModel.updateStreamSettings { s -> s.copy(codec = VideoCodec.valueOf(it)) }
                }
                ChoiceRow("Color", ColorQuality.entries.map { it.label }, settings.stream.colorQuality.label) { label ->
                    viewModel.updateStreamSettings { s -> s.copy(colorQuality = ColorQuality.entries.first { it.label == label }) }
                }
                ChoiceRow("Region", listOf("Auto") + state.regions.map { it.name }, state.regions.firstOrNull { it.url == settings.stream.region }?.name ?: "Auto") { label ->
                    val url = state.regions.firstOrNull { it.name == label }?.url.orEmpty()
                    viewModel.updateStreamSettings { s -> s.copy(region = url) }
                }
                SettingSwitch("L4S", settings.stream.enableL4S) { viewModel.updateStreamSettings { s -> s.copy(enableL4S = it) } }
                SettingSwitch("Cloud G-Sync / VRR request", settings.stream.enableCloudGsync) { viewModel.updateStreamSettings { s -> s.copy(enableCloudGsync = it) } }
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
                SettingSwitch("Finger mouse pad", settings.androidTouch.mousePad) { enabled -> viewModel.updateSettings(settings.copy(androidTouch = settings.androidTouch.copy(mousePad = enabled))) }
                NumberSlider("Touch layout scale", settings.androidTouch.scale, 0.6f, 1.4f, 0.05f) { value -> viewModel.updateSettings(settings.copy(androidTouch = settings.androidTouch.copy(scale = value))) }
                NumberSlider("Touch button size", settings.androidTouch.buttonScale, 0.65f, 1.5f, 0.05f) { value -> viewModel.updateSettings(settings.copy(androidTouch = settings.androidTouch.copy(buttonScale = value))) }
                NumberSlider("Touch stick size", settings.androidTouch.stickScale, 0.65f, 1.5f, 0.05f) { value -> viewModel.updateSettings(settings.copy(androidTouch = settings.androidTouch.copy(stickScale = value))) }
                NumberSlider("Touch opacity", settings.androidTouch.opacity, 0.15f, 1f, 0.05f) { value -> viewModel.updateSettings(settings.copy(androidTouch = settings.androidTouch.copy(opacity = value))) }
            }
        }
        item {
            SettingsSection("Interface") {
                SettingSwitch("Hide stream buttons", settings.hideStreamButtons) { viewModel.updateSettings(settings.copy(hideStreamButtons = it)) }
                SettingSwitch("Show stats on launch", settings.showStatsOnLaunch) { viewModel.updateSettings(settings.copy(showStatsOnLaunch = it)) }
                SettingSwitch("Hide server selector", settings.hideServerSelector) { viewModel.updateSettings(settings.copy(hideServerSelector = it)) }
                SettingSwitch("Controller mode", settings.controllerMode) { viewModel.updateSettings(settings.copy(controllerMode = it)) }
                SettingSwitch("Controller UI sounds", settings.controllerUiSounds) { viewModel.updateSettings(settings.copy(controllerUiSounds = it)) }
                SettingSwitch("Controller background animations", settings.controllerBackgroundAnimations) { viewModel.updateSettings(settings.copy(controllerBackgroundAnimations = it)) }
                SettingSwitch("Controller library game backdrop", settings.controllerLibraryGameBackdrop) { viewModel.updateSettings(settings.copy(controllerLibraryGameBackdrop = it)) }
                SettingSwitch("Auto-load controller library", settings.autoLoadControllerLibrary) { viewModel.updateSettings(settings.copy(autoLoadControllerLibrary = it)) }
                SettingSwitch("Auto fullscreen", settings.autoFullScreen) { viewModel.updateSettings(settings.copy(autoFullScreen = it)) }
                SettingSwitch("Session counter", settings.sessionCounterEnabled) { viewModel.updateSettings(settings.copy(sessionCounterEnabled = it)) }
            }
        }
        item {
            SettingsSection("Account") {
                Text(state.authSession?.user?.email ?: state.authSession?.user?.displayName.orEmpty(), color = TextMuted)
                Row(horizontalArrangement = Arrangement.spacedBy(8.dp)) {
                    OutlinedButton(onClick = viewModel::logout) { Text("Sign out") }
                    OutlinedButton(onClick = viewModel::logoutAll) { Text("Sign out all") }
                }
            }
        }
        item {
            SettingsSection("Codec Diagnostics") {
                state.codecReport?.capabilities?.forEach { cap ->
                    Text("${cap.codec.name}: decode=${cap.decoderName ?: "no"} encode=${cap.encoderName ?: "no"}", color = TextMuted)
                }
                Text("Native: ${state.codecReport?.nativeRuntimeSummary ?: "unknown"}", color = TextMuted)
            }
        }
        item {
            SettingsSection("Debug Logs") {
                val clipboard = LocalClipboardManager.current
                var copied by remember { mutableStateOf(false) }
                Text("Includes launch state, queue state, ads, stream settings, input settings, and codec capabilities.", color = TextMuted)
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
            if (state.settings.androidTouch.enabled || state.settings.androidTouch.mousePad) {
                TouchOverlay(
                    client = client,
                    touch = state.settings.androidTouch,
                    modifier = Modifier.align(Alignment.BottomCenter),
                )
            }
            if (!state.settings.hideStreamButtons) {
                StreamControlLauncher(
                    controlsOpen = controlsOpen,
                    status = state.queuePosition?.let { "Queue $it" } ?: streamState,
                    onToggle = { controlsOpen = !controlsOpen },
                    onExit = { exitConfirmOpen = true },
                    modifier = Modifier.align(Alignment.TopEnd),
                )
                AnimatedVisibility(
                    visible = controlsOpen,
                    modifier = Modifier.align(Alignment.BottomEnd),
                ) {
                    StreamControlsPanel(
                        gameTitle = game?.title ?: "Stream",
                        status = state.queuePosition?.let { "Queue $it" } ?: streamState,
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
                        modifier = Modifier.align(Alignment.BottomCenter),
                    )
                }
                if (exitConfirmOpen) {
                    StreamExitConfirmation(
                        gameTitle = game?.title ?: "this game",
                        onKeepPlaying = { exitConfirmOpen = false },
                        onExit = {
                            exitConfirmOpen = false
                            viewModel.stopStream()
                        },
                        modifier = Modifier.align(Alignment.Center),
                    )
                }
            }
        }
    }
}

@Composable
private fun StreamControlLauncher(
    controlsOpen: Boolean,
    status: String,
    onToggle: () -> Unit,
    onExit: () -> Unit,
    modifier: Modifier = Modifier,
) {
    Row(
        modifier.padding(top = 10.dp, end = 10.dp),
        horizontalArrangement = Arrangement.spacedBy(8.dp),
        verticalAlignment = Alignment.CenterVertically,
    ) {
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
    status: String,
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
                    Text(status, color = TextMuted, style = MaterialTheme.typography.labelMedium)
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
                    StreamControlSwitch("Finger mouse pad", if (settings.androidTouch.mousePad) "On" else "Off", settings.androidTouch.mousePad, onMousePadToggle)
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
            Text(gameTitle, fontWeight = FontWeight.Bold, maxLines = 1, overflow = TextOverflow.Ellipsis)
            Text(
                "${settings.stream.resolution} · ${settings.stream.fps}fps · ${settings.stream.maxBitrateMbps} Mbps",
                color = TextMuted,
                style = MaterialTheme.typography.labelSmall,
            )
            Text(
                listOf(status, if (audioMuted) "audio muted" else "audio on").joinToString(" · "),
                color = TextMuted,
                style = MaterialTheme.typography.labelSmall,
                maxLines = 1,
                overflow = TextOverflow.Ellipsis,
            )
        }
    }
}

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
    val queueCopy = when {
        state.queuePosition != null -> "Queue position ${state.queuePosition}"
        state.launchPhase.equals("Queue", ignoreCase = true) || state.launchPhase.equals("Checking queue", ignoreCase = true) -> "Checking live queue"
        session?.seatSetupStep == 1 -> "Waiting for a rig"
        else -> state.launchPhase.ifBlank { "Preparing rig" }
    }
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
        OutlinedButton(onClick = viewModel::stopStream) { Text("Cancel launch") }
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
            if (touch.mousePad) {
                Box(
                    Modifier
                        .size(142.dp * stickScale * layoutScale)
                        .clip(CircleShape)
                        .background(Color(0xff16331f).copy(alpha = opacity))
                        .border(1.dp, Green.copy(alpha = opacity), CircleShape)
                        .pointerInput(client) {
                            detectDragGestures { change, drag ->
                                change.consume()
                                client.sendTouchMouseMove(drag.x.roundToInt(), drag.y.roundToInt())
                            }
                        },
                    contentAlignment = Alignment.Center,
                ) {
                    Text("Mouse", color = TextMuted, style = MaterialTheme.typography.labelMedium)
                }
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
    Card(colors = CardDefaults.cardColors(containerColor = Panel), shape = RoundedCornerShape(8.dp)) {
        Column(Modifier.fillMaxWidth().padding(14.dp), verticalArrangement = Arrangement.spacedBy(10.dp)) {
            Text(title, style = MaterialTheme.typography.titleMedium, fontWeight = FontWeight.Bold)
            content()
        }
    }
}

@Composable
private fun SettingSwitch(label: String, checked: Boolean, onCheckedChange: (Boolean) -> Unit) {
    Row(Modifier.fillMaxWidth(), verticalAlignment = Alignment.CenterVertically) {
        Text(label, Modifier.weight(1f))
        Switch(checked = checked, onCheckedChange = onCheckedChange)
    }
}

@Composable
private fun NumberSlider(label: String, value: Float, min: Float, max: Float, step: Float, onChange: (Float) -> Unit) {
    var local by remember(value) { mutableFloatStateOf(value) }
    Column {
        Row(Modifier.fillMaxWidth(), verticalAlignment = Alignment.CenterVertically) {
            Text(label, Modifier.weight(1f))
            Text(if (step < 1f) "%.2f".format(local) else local.roundToInt().toString(), color = TextMuted)
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
    Row(Modifier.fillMaxWidth(), verticalAlignment = Alignment.CenterVertically) {
        Text(label, Modifier.weight(1f))
        Box {
            OutlinedButton(onClick = { expanded = true }) { Text(selected.ifBlank { "Auto" }) }
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
                        Text("Default routing")
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

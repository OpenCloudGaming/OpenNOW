package com.opencloudgaming.opennow.boosteroid

import androidx.activity.compose.BackHandler
import androidx.compose.foundation.background
import androidx.compose.foundation.layout.Arrangement
import androidx.compose.foundation.layout.Box
import androidx.compose.foundation.layout.Column
import androidx.compose.foundation.layout.FlowRow
import androidx.compose.foundation.layout.Row
import androidx.compose.foundation.layout.WindowInsets
import androidx.compose.foundation.layout.aspectRatio
import androidx.compose.foundation.layout.fillMaxSize
import androidx.compose.foundation.layout.fillMaxWidth
import androidx.compose.foundation.layout.padding
import androidx.compose.foundation.layout.safeDrawing
import androidx.compose.foundation.layout.size
import androidx.compose.foundation.layout.widthIn
import androidx.compose.foundation.layout.windowInsetsPadding
import androidx.compose.foundation.lazy.grid.GridCells
import androidx.compose.foundation.lazy.grid.GridItemSpan
import androidx.compose.foundation.lazy.grid.LazyVerticalGrid
import androidx.compose.foundation.lazy.grid.items
import androidx.compose.foundation.rememberScrollState
import androidx.compose.foundation.verticalScroll
import androidx.compose.material3.AlertDialog
import androidx.compose.material3.Button
import androidx.compose.material3.Card
import androidx.compose.material3.CircularProgressIndicator
import androidx.compose.material3.FilterChip
import androidx.compose.material3.MaterialTheme
import androidx.compose.material3.OutlinedTextField
import androidx.compose.material3.Surface
import androidx.compose.material3.Text
import androidx.compose.material3.TextButton
import androidx.compose.runtime.Composable
import androidx.compose.runtime.LaunchedEffect
import androidx.compose.runtime.getValue
import androidx.compose.runtime.mutableStateOf
import androidx.compose.runtime.remember
import androidx.compose.runtime.setValue
import androidx.compose.ui.Alignment
import androidx.compose.ui.Modifier
import androidx.compose.ui.draw.drawWithCache
import androidx.compose.ui.graphics.BlendMode
import androidx.compose.ui.graphics.Brush
import androidx.compose.ui.graphics.Color
import androidx.compose.ui.graphics.CompositingStrategy
import androidx.compose.ui.graphics.graphicsLayer
import androidx.compose.ui.layout.ContentScale
import androidx.compose.ui.platform.LocalContext
import androidx.compose.ui.res.stringResource
import androidx.compose.ui.text.style.TextOverflow
import androidx.compose.ui.unit.dp
import coil3.compose.AsyncImage
import com.opencloudgaming.opennow.CodecProbe
import com.opencloudgaming.opennow.DeviceLoginPanel
import com.opencloudgaming.opennow.GameInfo
import com.opencloudgaming.opennow.LoginService
import com.opencloudgaming.opennow.LoginServiceSelector
import com.opencloudgaming.opennow.OpenNowMark
import com.opencloudgaming.opennow.R
import com.opencloudgaming.opennow.StreamSettings
import com.opencloudgaming.opennow.VideoCodec
import com.opencloudgaming.opennow.gameStoreDisplayName
import com.opencloudgaming.opennow.streamingHardwareDecoderAvailable
import kotlinx.coroutines.Dispatchers
import kotlinx.coroutines.withContext

@Composable
internal fun BoosteroidScreen(
    state: BoosteroidUiState,
    model: BoosteroidViewModel,
    tv: Boolean,
    onNvidia: () -> Unit,
    onSwitchService: () -> Unit,
    onStreamInputChanged: (BoosteroidStreamClient?, Boolean) -> Unit,
) {
    val launch = state.launch
    val session = launch?.session
    if (launch != null && session?.isReadyForStream() == true) {
        BoosteroidStreamScreen(
            launch = launch,
            session = session,
            tv = tv,
            model = model,
            onInputChanged = onStreamInputChanged,
        )
        return
    }
    Surface(
        color = MaterialTheme.colorScheme.background,
        modifier = Modifier.fillMaxSize(),
    ) {
        Box(Modifier.fillMaxSize().windowInsetsPadding(WindowInsets.safeDrawing)) {
            when {
                launch != null || state.stopping -> {
                    BackHandler(enabled = !state.stopping, onBack = model::stop)
                    BoosteroidLoading(launch, state.stopping, model::stop)
                }
                !state.initialized || state.user == null -> {
                    BackHandler(enabled = state.signingIn, onBack = model::cancelSignIn)
                    BoosteroidSignIn(state, tv, model, onNvidia)
                }
                else -> BoosteroidCatalog(state, model, onSwitchService)
            }
        }
    }
    state.error?.let { message ->
        AlertDialog(
            onDismissRequest = model::dismissError,
            title = { Text(stringResource(R.string.app_name)) },
            text = { Text(stringResource(message)) },
            confirmButton = {
                TextButton(onClick = model::dismissError) { Text(stringResource(R.string.action_ok)) }
            },
        )
    }
}

@Composable
internal fun CloudServicePicker(
    usingBoosteroid: Boolean,
    onNvidia: () -> Unit,
    onBoosteroid: () -> Unit,
) {
    Surface(Modifier.fillMaxSize(), color = MaterialTheme.colorScheme.background) {
        Box(Modifier.fillMaxSize().windowInsetsPadding(WindowInsets.safeDrawing), contentAlignment = Alignment.Center) {
            Column(
                Modifier.padding(24.dp),
                horizontalAlignment = Alignment.CenterHorizontally,
                verticalArrangement = Arrangement.spacedBy(16.dp),
            ) {
                if (usingBoosteroid) BoosteroidMark(70.dp) else OpenNowMark(70.dp)
                Text(stringResource(R.string.app_name), style = MaterialTheme.typography.headlineLarge)
                LoginServiceSelector(
                    selected = if (usingBoosteroid) LoginService.Boosteroid else LoginService.Nvidia,
                    onNvidia = onNvidia,
                    onBoosteroid = onBoosteroid,
                )
            }
        }
    }
}

@Composable
private fun BoosteroidMark(size: androidx.compose.ui.unit.Dp) {
    OpenNowMark(
        size = size,
        modifier = Modifier
            .graphicsLayer { compositingStrategy = CompositingStrategy.Offscreen }
            .drawWithCache {
                val tint = Brush.linearGradient(listOf(Color(0xFF73C5FF), Color(0xFF9562FF)))
                onDrawWithContent {
                    drawContent()
                    drawRect(tint, blendMode = BlendMode.SrcIn)
                }
            },
    )
}

@Composable
private fun BoosteroidSignIn(
    state: BoosteroidUiState,
    tv: Boolean,
    model: BoosteroidViewModel,
    onNvidia: () -> Unit,
) {
    val context = LocalContext.current
    val prompt = state.loginPrompt
    LaunchedEffect(prompt?.verificationUri, tv) {
        if (!tv && prompt != null) com.opencloudgaming.opennow.openExternalUrl(context, prompt.verificationUri)
    }
    Box(Modifier.fillMaxSize(), contentAlignment = Alignment.Center) {
        Column(
            Modifier.widthIn(max = 740.dp).verticalScroll(rememberScrollState()).padding(24.dp),
            horizontalAlignment = Alignment.CenterHorizontally,
            verticalArrangement = Arrangement.spacedBy(16.dp),
        ) {
            BoosteroidMark(70.dp)
            Text(stringResource(R.string.app_name), style = MaterialTheme.typography.headlineLarge)
            if (prompt == null && !state.signingIn) {
                LoginServiceSelector(
                    selected = LoginService.Boosteroid,
                    onNvidia = onNvidia,
                    onBoosteroid = {},
                )
            }
            if (prompt != null) {
                DeviceLoginPanel(
                    prompt = prompt,
                    phase = stringResource(R.string.boosteroid_waiting_for_sign_in),
                    onCancel = model::cancelSignIn,
                    preferLandscapeLayout = tv,
                    qrMaxSize = 200.dp,
                )
            } else if (state.initialized) {
                Button(onClick = model::signIn, enabled = !state.signingIn) {
                    Text(stringResource(R.string.action_sign_in))
                }
            } else {
                CircularProgressIndicator(Modifier.size(24.dp))
            }

        }
    }
}

@Composable
private fun BoosteroidLoading(launch: BoosteroidLaunch?, stopping: Boolean, onStop: () -> Unit) {
    Box(Modifier.fillMaxSize(), contentAlignment = Alignment.Center) {
        Column(
            Modifier.padding(24.dp),
            horizontalAlignment = Alignment.CenterHorizontally,
            verticalArrangement = Arrangement.spacedBy(14.dp),
        ) {
            BoosteroidMark(58.dp)
            launch?.title?.let { Text(it, style = MaterialTheme.typography.titleLarge) }
            CircularProgressIndicator(Modifier.size(30.dp))
            val position = launch?.session?.queuePosition
            Text(
                when {
                    stopping -> stringResource(R.string.boosteroid_ending)
                    position != null -> stringResource(R.string.boosteroid_queue_position, position)
                    else -> stringResource(R.string.boosteroid_launching)
                },
            )
            if (!stopping) TextButton(onClick = onStop) { Text(stringResource(R.string.action_cancel)) }
        }
    }
}

@Composable
private fun BoosteroidCatalog(
    state: BoosteroidUiState,
    model: BoosteroidViewModel,
    onSwitchService: () -> Unit,
) {
    var game by remember { mutableStateOf<GameInfo?>(null) }
    var settingsOpen by remember { mutableStateOf(false) }
    val displayedGames = if (state.library && state.query.isNotBlank()) {
        state.games.filter { it.title.contains(state.query.trim(), ignoreCase = true) }
    } else state.games
    LazyVerticalGrid(
        columns = GridCells.Adaptive(148.dp),
        modifier = Modifier.fillMaxSize().padding(horizontal = 16.dp),
        horizontalArrangement = Arrangement.spacedBy(12.dp),
        verticalArrangement = Arrangement.spacedBy(12.dp),
    ) {
        item(span = { GridItemSpan(maxLineSpan) }) {
            Column(verticalArrangement = Arrangement.spacedBy(8.dp)) {
                Row(
                    Modifier.fillMaxWidth(),
                    horizontalArrangement = Arrangement.SpaceBetween,
                    verticalAlignment = Alignment.CenterVertically,
                ) {
                    Row(Modifier.weight(1f), verticalAlignment = Alignment.CenterVertically) {
                        BoosteroidMark(30.dp)
                        Column(Modifier.padding(start = 8.dp)) {
                            Text(stringResource(R.string.app_name), style = MaterialTheme.typography.titleLarge)
                            val identity = listOfNotNull(state.user?.displayName, state.membership).joinToString(" · ")
                            Text(identity, maxLines = 1, overflow = TextOverflow.Ellipsis)
                        }
                    }
                    TextButton(onClick = onSwitchService) { Text(stringResource(R.string.login_switch_service)) }
                }
                FlowRow(horizontalArrangement = Arrangement.spacedBy(8.dp)) {
                    FilterChip(!state.library, { model.selectLibrary(false) }, { Text(stringResource(R.string.nav_store)) })
                    FilterChip(state.library, { model.selectLibrary(true) }, { Text(stringResource(R.string.nav_library)) })
                    TextButton(onClick = model::refresh) { Text(stringResource(R.string.action_refresh)) }
                    TextButton(onClick = { settingsOpen = true }) { Text(stringResource(R.string.nav_settings)) }
                }
                OutlinedTextField(
                    value = state.query,
                    onValueChange = model::search,
                    label = { Text(stringResource(R.string.search_games)) },
                    modifier = Modifier.fillMaxWidth(),
                    singleLine = true,
                )
                if (state.activeSession != null) {
                    Text(stringResource(R.string.boosteroid_active_session), style = MaterialTheme.typography.titleMedium)
                    Row(horizontalArrangement = Arrangement.spacedBy(12.dp)) {
                        Button(onClick = model::resume) { Text(stringResource(R.string.action_resume)) }
                        TextButton(onClick = model::stop) { Text(stringResource(R.string.boosteroid_end_session)) }
                    }
                }
            }
        }
        items(displayedGames, key = GameInfo::id) { item ->
            Card(onClick = { game = item }) {
                if (!item.imageUrl.isNullOrBlank()) {
                    AsyncImage(
                        model = item.imageUrl,
                        contentDescription = null,
                        modifier = Modifier.fillMaxWidth().aspectRatio(16f / 9f),
                        contentScale = ContentScale.Crop,
                    )
                }
                Column(Modifier.padding(10.dp), verticalArrangement = Arrangement.spacedBy(3.dp)) {
                    Text(item.title, maxLines = 2, overflow = TextOverflow.Ellipsis)
                    item.availableStores.firstOrNull()?.let {
                        Text(gameStoreDisplayName(it), style = MaterialTheme.typography.labelSmall)
                    }
                }
            }
        }
        item(span = { GridItemSpan(maxLineSpan) }) {
            Box(Modifier.fillMaxWidth().padding(bottom = 16.dp), contentAlignment = Alignment.Center) {
                when {
                    state.loading -> CircularProgressIndicator(Modifier.size(26.dp))
                    state.hasMore -> TextButton(onClick = model::loadMore) {
                        Text(stringResource(R.string.boosteroid_load_more))
                    }
                    displayedGames.isEmpty() -> Text(stringResource(R.string.boosteroid_no_games))
                }
            }
        }
    }

    game?.let { item ->
        AlertDialog(
            onDismissRequest = { game = null },
            title = { Text(item.title) },
            text = {
                Column(verticalArrangement = Arrangement.spacedBy(8.dp)) {
                    Text(item.availableStores.joinToString(transform = ::gameStoreDisplayName))
                    item.description?.let { Text(it, maxLines = 5, overflow = TextOverflow.Ellipsis) }
                    TextButton(onClick = {
                        game = null
                        model.setInstalled(item, !item.isInLibrary)
                    }) {
                        Text(stringResource(if (item.isInLibrary) R.string.boosteroid_remove_game else R.string.boosteroid_add_game))
                    }
                }
            },
            confirmButton = {
                Button(onClick = { game = null; model.play(item) }) { Text(stringResource(R.string.action_play)) }
            },
            dismissButton = {
                TextButton(onClick = { game = null }) { Text(stringResource(R.string.action_cancel)) }
            },
        )
    }
    if (settingsOpen) {
        BoosteroidSettings(state.settings, model, onClose = { settingsOpen = false })
    }
}

@Composable
private fun BoosteroidSettings(settings: StreamSettings, model: BoosteroidViewModel, onClose: () -> Unit) {
    val context = LocalContext.current
    var av1Available by remember { mutableStateOf(settings.codec == VideoCodec.AV1) }
    LaunchedEffect(context) {
        av1Available = withContext(Dispatchers.Default) {
            boosteroidResult {
                CodecProbe.report(context.applicationContext).capabilities.any {
                    it.codec == VideoCodec.AV1 && it.streamingHardwareDecoderAvailable()
                }
            }.getOrDefault(false)
        }
        if (!av1Available && settings.codec == VideoCodec.AV1) {
            model.setProfile(settings.resolution, settings.fps, VideoCodec.H264)
        }
    }
    AlertDialog(
        onDismissRequest = onClose,
        title = { Text(stringResource(R.string.boosteroid_quality)) },
        text = {
            Column(Modifier.verticalScroll(rememberScrollState()), verticalArrangement = Arrangement.spacedBy(6.dp)) {
                Text(stringResource(R.string.settings_resolution))
                FlowRow(horizontalArrangement = Arrangement.spacedBy(8.dp)) {
                    for ((label, resolution) in streamResolutions) {
                        FilterChip(settings.resolution == resolution, {
                            model.setProfile(resolution, settings.fps, settings.codec)
                        }, { Text(label) })
                    }
                }
                FlowRow(horizontalArrangement = Arrangement.spacedBy(8.dp)) {
                    for (fps in listOf(60, 120)) {
                        FilterChip(settings.fps == fps, {
                            model.setProfile(settings.resolution, fps, settings.codec)
                        }, { Text(stringResource(R.string.boosteroid_fps, fps)) })
                    }
                }
                if (av1Available) {
                    FlowRow(horizontalArrangement = Arrangement.spacedBy(8.dp)) {
                        FilterChip(settings.codec == VideoCodec.H264, {
                            model.setProfile(settings.resolution, settings.fps, VideoCodec.H264)
                        }, { Text("H.264") })
                        FilterChip(settings.codec == VideoCodec.AV1, {
                            model.setProfile(settings.resolution, settings.fps, VideoCodec.AV1)
                        }, { Text("AV1") })
                    }
                }
                Text(stringResource(R.string.boosteroid_library_sync), style = MaterialTheme.typography.titleMedium)
                FlowRow {
                    for ((platform, label) in syncPlatforms) {
                        TextButton(onClick = { onClose(); model.syncLibrary(platform) }) { Text(label) }
                    }
                }
                TextButton(onClick = { onClose(); model.signOut() }) {
                    Text(stringResource(R.string.account_sign_out))
                }
            }
        },
        confirmButton = { TextButton(onClick = onClose) { Text(stringResource(R.string.action_close)) } },
    )
}

private val streamResolutions = listOf(
    "720p" to "1280x720",
    "1080p" to "1920x1080",
    "1440p" to "2560x1440",
    "4K" to "3840x2160",
)

private val syncPlatforms = listOf("steam" to "Steam", "epic" to "Epic Games", "battle-net" to "Battle.net")

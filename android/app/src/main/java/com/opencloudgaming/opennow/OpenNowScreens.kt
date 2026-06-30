package com.opencloudgaming.opennow

import android.app.Activity
import android.content.res.Configuration
import android.content.Intent
import android.graphics.Color as AndroidColor
import android.graphics.BitmapFactory
import android.net.Uri
import android.os.Build
import android.speech.RecognizerIntent
import android.view.KeyEvent
import android.view.MotionEvent
import android.view.PointerIcon
import android.view.View
import android.view.ViewGroup
import android.widget.Toast
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
import androidx.compose.animation.core.RepeatMode
import androidx.compose.animation.core.animateFloat
import androidx.compose.animation.core.infiniteRepeatable
import androidx.compose.animation.core.rememberInfiniteTransition
import androidx.compose.animation.core.tween
import androidx.compose.foundation.ExperimentalFoundationApi
import androidx.compose.foundation.Canvas
import androidx.compose.foundation.gestures.awaitEachGesture
import androidx.compose.foundation.gestures.awaitFirstDown
import androidx.compose.foundation.gestures.detectDragGestures
import androidx.compose.foundation.gestures.detectVerticalDragGestures
import androidx.compose.foundation.text.BasicTextField
import androidx.compose.foundation.Image
import androidx.compose.foundation.background
import androidx.compose.foundation.border
import androidx.compose.foundation.clickable
import androidx.compose.foundation.combinedClickable
import androidx.compose.foundation.focusable
import androidx.compose.foundation.interaction.MutableInteractionSource
import androidx.compose.foundation.layout.Arrangement
import androidx.compose.foundation.layout.Box
import androidx.compose.foundation.layout.BoxScope
import androidx.compose.foundation.layout.BoxWithConstraints
import androidx.compose.foundation.layout.Column
import androidx.compose.foundation.layout.ColumnScope
import androidx.compose.foundation.layout.ExperimentalLayoutApi
import androidx.compose.foundation.layout.FlowRow
import androidx.compose.foundation.layout.PaddingValues
import androidx.compose.foundation.layout.Row
import androidx.compose.foundation.layout.RowScope
import androidx.compose.foundation.layout.Spacer
import androidx.compose.foundation.layout.WindowInsets
import androidx.compose.foundation.layout.aspectRatio
import androidx.compose.foundation.layout.fillMaxHeight
import androidx.compose.foundation.layout.fillMaxSize
import androidx.compose.foundation.layout.fillMaxWidth
import androidx.compose.foundation.layout.height
import androidx.compose.foundation.layout.offset
import androidx.compose.foundation.layout.padding
import androidx.compose.foundation.layout.size
import androidx.compose.foundation.layout.width
import androidx.compose.foundation.lazy.LazyColumn
import androidx.compose.foundation.lazy.items
import androidx.compose.foundation.lazy.grid.GridCells
import androidx.compose.foundation.lazy.grid.GridItemSpan
import androidx.compose.foundation.lazy.grid.LazyVerticalGrid
import androidx.compose.foundation.lazy.grid.items as gridItems
import androidx.compose.foundation.lazy.grid.rememberLazyGridState
import androidx.compose.foundation.rememberScrollState
import androidx.compose.foundation.shape.CircleShape
import androidx.compose.foundation.shape.RoundedCornerShape
import androidx.compose.foundation.verticalScroll
import androidx.compose.material3.AlertDialog
import androidx.compose.material3.AssistChip
import androidx.compose.material3.Button
import androidx.compose.material3.ButtonDefaults
import androidx.compose.material3.Card
import androidx.compose.material3.CardDefaults
import androidx.compose.material3.Checkbox
import androidx.compose.material3.CircularProgressIndicator
import androidx.compose.material3.DropdownMenu
import androidx.compose.material3.DropdownMenuItem
import androidx.compose.material3.ElevatedButton
import androidx.compose.material3.Icon
import androidx.compose.material3.IconButton
import androidx.compose.material3.LinearProgressIndicator
import androidx.compose.material3.MaterialTheme
import androidx.compose.material3.NavigationBar
import androidx.compose.material3.NavigationBarItem
import androidx.compose.material3.NavigationRailItem
import androidx.compose.material3.OutlinedButton
import androidx.compose.material3.OutlinedTextField
import androidx.compose.material3.Scaffold
import androidx.compose.material3.ScaffoldDefaults
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
import androidx.compose.runtime.rememberUpdatedState
import androidx.compose.runtime.rememberCoroutineScope
import androidx.compose.runtime.key
import androidx.compose.runtime.setValue
import androidx.compose.ui.Alignment
import androidx.compose.ui.ExperimentalComposeUiApi
import androidx.compose.ui.Modifier
import androidx.compose.ui.draw.clip
import androidx.compose.ui.draw.clipToBounds
import androidx.compose.ui.focus.FocusDirection
import androidx.compose.ui.focus.FocusManager
import androidx.compose.ui.focus.FocusRequester
import androidx.compose.ui.focus.focusRequester
import androidx.compose.ui.focus.onFocusChanged
import androidx.compose.ui.geometry.CornerRadius
import androidx.compose.ui.geometry.Offset
import androidx.compose.ui.geometry.Size
import androidx.compose.ui.graphics.Color
import androidx.compose.ui.graphics.ImageBitmap
import androidx.compose.ui.graphics.Path
import androidx.compose.ui.graphics.SolidColor
import androidx.compose.ui.graphics.asImageBitmap
import androidx.compose.ui.graphics.graphicsLayer
import androidx.compose.ui.input.pointer.pointerInput
import androidx.compose.ui.input.pointer.PointerEventPass
import androidx.compose.ui.input.pointer.pointerInteropFilter
import androidx.compose.ui.input.key.Key
import androidx.compose.ui.input.key.KeyEventType
import androidx.compose.ui.input.key.key
import androidx.compose.ui.input.key.onPreviewKeyEvent
import androidx.compose.ui.input.key.type
import androidx.compose.ui.layout.ContentScale
import androidx.compose.ui.layout.boundsInRoot
import androidx.compose.ui.layout.onGloballyPositioned
import androidx.compose.ui.layout.onSizeChanged
import androidx.compose.ui.platform.LocalClipboardManager
import androidx.compose.ui.platform.LocalConfiguration
import androidx.compose.ui.platform.LocalContext
import androidx.compose.ui.platform.LocalDensity
import androidx.compose.ui.platform.LocalFocusManager
import androidx.compose.ui.platform.LocalSoftwareKeyboardController
import androidx.compose.ui.platform.LocalView
import androidx.compose.ui.res.painterResource
import androidx.compose.ui.res.stringResource
import androidx.compose.ui.semantics.contentDescription
import androidx.compose.ui.semantics.semantics
import androidx.compose.ui.text.AnnotatedString
import androidx.compose.ui.text.font.FontWeight
import androidx.compose.ui.text.style.TextAlign
import androidx.compose.ui.text.style.TextOverflow
import androidx.compose.ui.unit.dp
import androidx.compose.ui.unit.Dp
import androidx.compose.ui.unit.IntSize
import androidx.compose.ui.viewinterop.AndroidView
import androidx.lifecycle.compose.collectAsStateWithLifecycle
import androidx.media3.common.MediaItem
import androidx.media3.common.Player
import androidx.media3.exoplayer.ExoPlayer
import androidx.media3.ui.PlayerView
import kotlinx.coroutines.Dispatchers
import kotlinx.coroutines.delay
import kotlinx.coroutines.launch
import kotlinx.coroutines.withContext
import java.text.DateFormat
import java.util.Date
import java.util.Locale
import java.net.URL
import kotlin.math.min
import kotlin.math.floor
import kotlin.math.roundToInt
import kotlin.math.sqrt

private val Green = Color(0xff6af0a0)
private val Background = Color(0xff090b0d)
private val Panel = Color(0xff11161a)
private val PanelAlt = Color(0xff171d22)
private val TextPrimary = Color(0xffeef3f5)
private val TextMuted = Color(0xff98a4aa)
private val UiAccent.color: Color
    get() = when (this) {
        UiAccent.OpenNow -> Green
        UiAccent.Pixel -> Color(0xff8ab4f8)
        UiAccent.HotPink -> Color(0xffff4fb8)
        UiAccent.Lime -> Color(0xffc7ef6b)
        UiAccent.Coral -> Color(0xffff8d7a)
        UiAccent.Violet -> Color(0xffc7a4ff)
    }

@Composable
internal fun uiAccentLabel(accent: UiAccent): String = when (accent) {
    UiAccent.OpenNow -> stringResource(R.string.accent_opennow)
    UiAccent.Pixel -> stringResource(R.string.accent_pixel)
    UiAccent.HotPink -> stringResource(R.string.accent_hot_pink)
    UiAccent.Lime -> stringResource(R.string.accent_lime)
    UiAccent.Coral -> stringResource(R.string.accent_coral)
    UiAccent.Violet -> stringResource(R.string.accent_violet)
}

@Composable
fun OpenNowTheme(settings: AppSettings, content: @Composable () -> Unit) {
    val context = LocalContext.current
    val accent = settings.uiAccent.color
    val fallbackScheme = darkColorScheme(
        primary = accent,
        onPrimary = Color(0xff08090c),
        background = Background,
        surface = Panel,
        surfaceVariant = PanelAlt,
        onBackground = TextPrimary,
        onSurface = TextPrimary,
        onSurfaceVariant = TextMuted,
    )
    val colorScheme = if (settings.dynamicColor && Build.VERSION.SDK_INT >= Build.VERSION_CODES.S) {
        dynamicDarkColorScheme(context).copy(
            primary = accent,
            onPrimary = Color(0xff08090c),
            secondary = accent,
            tertiary = Green,
        )
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
    val context = LocalContext.current
    val launchAudioController = remember(context) { AndroidNerdAudioController(context.applicationContext) }
    val playIntroOnAppLaunch = remember { state.settings.streamIntroMusic }
    var launchIntroStarted by remember { mutableStateOf(false) }
    var musicCuePlaying by remember { mutableStateOf(false) }
    var activeMusicCue by remember { mutableStateOf<MusicCueUiPurpose?>(null) }
    var previousStreamStatus by remember { mutableStateOf(state.streamStatus) }
    var queuedForStartCue by remember { mutableStateOf(false) }
    var lastStartCueSessionId by remember { mutableStateOf<String?>(null) }
    fun updateMusicCue(purpose: MusicCueUiPurpose, playing: Boolean) {
        musicCuePlaying = playing
        activeMusicCue = when {
            playing -> purpose
            activeMusicCue == purpose -> null
            else -> activeMusicCue
        }
    }
    fun clearMusicCue(playing: Boolean) {
        musicCuePlaying = playing
        if (!playing) activeMusicCue = null
    }

    DisposableEffect(launchAudioController) {
        onDispose {
            launchAudioController.release()
        }
    }
    LaunchedEffect(
        playIntroOnAppLaunch,
        state.settings.streamIntroMusic,
        state.settings.queueReadyMusic,
        state.page,
        state.streamStatus,
        state.launchPhase,
        state.queuePosition,
        state.streamSession?.sessionId,
        state.streamSession?.queuePosition,
    ) {
        val streamActive = state.page == AppPage.Stream || state.streamStatus != "idle"
        val sessionId = state.streamSession?.sessionId
        val queueReadyForStream =
            previousStreamStatus == "queue" &&
                state.streamStatus == "connecting" &&
                sessionId != null &&
                sessionId != lastStartCueSessionId
        previousStreamStatus = state.streamStatus
        if (state.streamStatus == "queue") {
            queuedForStartCue = queuedForStartCue ||
                state.queuePosition != null ||
                state.streamSession?.queuePosition?.takeIf { it > 0 } != null ||
                state.launchPhase.equals("Queue", ignoreCase = true)
        }

        if (!state.settings.streamIntroMusic) {
            launchAudioController.stopIntro { updateMusicCue(MusicCueUiPurpose.Intro, it) }
        }
        if (!state.settings.queueReadyMusic) {
            launchAudioController.stopQueueReadyReminder { updateMusicCue(MusicCueUiPurpose.QueueReady, it) }
        }

        if (!state.settings.streamIntroMusic && !state.settings.queueReadyMusic) {
            launchAudioController.stopAll(::clearMusicCue)
        } else if (queueReadyForStream && queuedForStartCue) {
            lastStartCueSessionId = sessionId
            queuedForStartCue = false
            launchAudioController.startQueueReadyReminder(enabled = state.settings.queueReadyMusic) { updateMusicCue(MusicCueUiPurpose.QueueReady, it) }
        } else if (playIntroOnAppLaunch && state.settings.streamIntroMusic && !streamActive) {
            if (!launchIntroStarted) {
                launchIntroStarted = true
                launchAudioController.startIntro(enabled = true) { updateMusicCue(MusicCueUiPurpose.Intro, it) }
            }
        } else {
            launchAudioController.stopIntro { updateMusicCue(MusicCueUiPurpose.Intro, it) }
        }
        if (state.streamStatus == "idle") {
            queuedForStartCue = false
        }
    }

    OpenNowTheme(state.settings) {
        Box(Modifier.fillMaxSize()) {
            Surface(Modifier.fillMaxSize(), color = MaterialTheme.colorScheme.background) {
                val startingText = stringResource(R.string.status_starting_opennow)
                when {
                    state.initializing -> LoadingScreen(state.launchPhase.ifBlank { startingText })
                    state.authSession == null -> LoginScreen(state, viewModel)
                    else -> MainShell(state, viewModel)
                }
            }
            AnimatedVisibility(
                visible = musicCuePlaying && when (activeMusicCue) {
                    MusicCueUiPurpose.Intro -> state.settings.streamIntroMusic
                    MusicCueUiPurpose.QueueReady -> state.settings.queueReadyMusic
                    null -> false
                },
                modifier = Modifier
                    .align(Alignment.TopEnd)
                    .padding(top = 14.dp, end = 14.dp),
            ) {
                MusicCueMuteButton {
                    val cue = activeMusicCue
                    launchAudioController.stopAll(::clearMusicCue)
                    viewModel.updateSettings(
                        when (cue) {
                            MusicCueUiPurpose.QueueReady -> state.settings.copy(queueReadyMusic = false)
                            MusicCueUiPurpose.Intro -> state.settings.copy(streamIntroMusic = false)
                            null -> state.settings.copy(streamIntroMusic = false, queueReadyMusic = false)
                        },
                    )
                }
            }
        }
    }
}

private enum class MusicCueUiPurpose {
    Intro,
    QueueReady,
}

@Composable
private fun MusicCueMuteButton(onClick: () -> Unit) {
    Surface(
        shape = CircleShape,
        color = PanelAlt.copy(alpha = 0.9f),
        tonalElevation = 4.dp,
        shadowElevation = 4.dp,
    ) {
        IconButton(onClick = onClick, modifier = Modifier.size(44.dp)) {
            Icon(
                painter = painterResource(R.drawable.ic_volume_off),
                contentDescription = stringResource(R.string.action_mute_music),
                tint = TextPrimary,
                modifier = Modifier.size(22.dp),
            )
        }
    }
}

@Composable
private fun LoadingScreen(text: String) {
    Box(Modifier.fillMaxSize(), contentAlignment = Alignment.Center) {
        Column(horizontalAlignment = Alignment.CenterHorizontally, verticalArrangement = Arrangement.spacedBy(16.dp)) {
            OpenNowMark(72.dp)
            CircularProgressIndicator(color = MaterialTheme.colorScheme.primary)
            Text(text, color = TextMuted)
        }
    }
}

@Composable
private fun LoginScreen(state: OpenNowUiState, viewModel: OpenNowViewModel) {
    val signInFocusRequester = remember { FocusRequester() }
    val tvLogin = state.codecReport?.androidTvProfile == true
    val deviceCodeLoginAvailable = state.selectedProvider.supportsDeviceCodeLogin
    val preferDeviceCodeLogin = tvLogin && deviceCodeLoginAvailable
    val deviceLoginPrompt = state.deviceLoginPrompt.takeIf { deviceCodeLoginAvailable }
    LaunchedEffect(preferDeviceCodeLogin, deviceLoginPrompt == null) {
        if (preferDeviceCodeLogin && deviceLoginPrompt == null) {
            runCatching { signInFocusRequester.requestFocus() }
        }
    }
    if (preferDeviceCodeLogin && deviceLoginPrompt != null) {
        TvDeviceLoginScreen(
            prompt = deviceLoginPrompt,
            phase = state.launchPhase,
            onCancel = viewModel::cancelLogin,
        )
        return
    }
    Column(
        Modifier
            .fillMaxSize()
            .verticalScroll(rememberScrollState())
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
        deviceLoginPrompt?.let { prompt ->
            DeviceLoginPanel(prompt = prompt, phase = state.launchPhase, onCancel = viewModel::cancelLogin)
        } ?: Column(horizontalAlignment = Alignment.CenterHorizontally, verticalArrangement = Arrangement.spacedBy(10.dp)) {
            Button(
                onClick = { viewModel.login() },
                modifier = Modifier.focusRequester(signInFocusRequester),
            ) {
                Text(
                    when {
                        state.launchPhase.isNotBlank() -> state.launchPhase
                        preferDeviceCodeLogin -> stringResource(R.string.login_tv_start, state.selectedProvider.displayName)
                        else -> stringResource(R.string.login_with_provider, state.selectedProvider.displayName)
                    },
                )
            }
            if (!tvLogin && deviceCodeLoginAvailable) {
                TextButton(onClick = { viewModel.loginWithCode() }) {
                    Text("Use code sign-in")
                }
            }
        }
        if (state.error != null) {
            Spacer(Modifier.height(14.dp))
            Text(state.error.orEmpty(), color = Color(0xffff9f9f))
        }
    }
}

@Composable
private fun TvDeviceLoginScreen(prompt: DeviceLoginPrompt, phase: String, onCancel: () -> Unit) {
    BoxWithConstraints(
        modifier = Modifier.fillMaxSize().padding(horizontal = 48.dp, vertical = 36.dp),
        contentAlignment = Alignment.Center,
    ) {
        val landscape = maxWidth >= 720.dp
        val qrMaxSize = minOf(
            maxWidth * if (landscape) 0.28f else 0.68f,
            maxHeight * if (landscape) 0.58f else 0.38f,
            340.dp,
        )
        DeviceLoginPanel(
            prompt = prompt,
            phase = phase,
            onCancel = onCancel,
            modifier = Modifier.fillMaxWidth(if (landscape) 0.86f else 1f),
            qrMaxSize = qrMaxSize,
            preferLandscapeLayout = landscape,
            focusCancelOnPrompt = false,
        )
    }
}

@Composable
private fun DeviceLoginPanel(
    prompt: DeviceLoginPrompt,
    phase: String,
    onCancel: () -> Unit,
    modifier: Modifier = Modifier.fillMaxWidth().padding(horizontal = 24.dp),
    qrMaxSize: androidx.compose.ui.unit.Dp = 360.dp,
    preferLandscapeLayout: Boolean = false,
    focusCancelOnPrompt: Boolean = true,
) {
    val context = LocalContext.current
    val clipboardManager = LocalClipboardManager.current
    val initialFocusRequester = remember { FocusRequester() }
    val launchUrl = remember(prompt.verificationUriComplete, prompt.verificationUri) {
        prompt.verificationUriComplete ?: prompt.verificationUri
    }
    val qrContent = launchUrl
    var urlActionMessage by remember(launchUrl) { mutableStateOf<String?>(null) }
    val qrCode = remember(qrContent, prompt.verificationUri) {
        QrCode.encodeText(qrContent) ?: QrCode.encodeText(prompt.verificationUri)
    }
    val remainingSeconds by produceState(initialValue = secondsUntil(prompt.expiresAt), prompt.expiresAt) {
        while (value > 0) {
            delay(1000L)
            value = secondsUntil(prompt.expiresAt)
        }
    }
    LaunchedEffect(prompt.userCode, focusCancelOnPrompt) {
        runCatching { initialFocusRequester.requestFocus() }
    }
    Card(
        colors = CardDefaults.cardColors(containerColor = PanelAlt, contentColor = TextPrimary),
        shape = RoundedCornerShape(14.dp),
        modifier = modifier,
    ) {
        if (preferLandscapeLayout) {
            Row(
                Modifier.fillMaxWidth().padding(24.dp),
                horizontalArrangement = Arrangement.spacedBy(24.dp),
                verticalAlignment = Alignment.CenterVertically,
            ) {
                DeviceLoginQr(
                    qrCode = qrCode,
                    qrMaxSize = qrMaxSize,
                    modifier = Modifier.weight(0.9f),
                )
                DeviceLoginControls(
                    launchUrl = launchUrl,
                    prompt = prompt,
                    phase = phase,
                    remainingSeconds = remainingSeconds,
                    urlActionMessage = urlActionMessage,
                    onUrlActionMessage = { urlActionMessage = it },
                    onCancel = onCancel,
                    focusRequester = initialFocusRequester,
                    focusCancel = focusCancelOnPrompt,
                    context = context,
                    clipboardManager = clipboardManager,
                    modifier = Modifier.weight(1.1f),
                    showTitle = true,
                )
            }
        } else {
            Column(
                Modifier.padding(20.dp),
                horizontalAlignment = Alignment.CenterHorizontally,
                verticalArrangement = Arrangement.spacedBy(12.dp),
            ) {
                Text(stringResource(R.string.login_tv_title), style = MaterialTheme.typography.titleMedium, fontWeight = FontWeight.Bold)
                DeviceLoginQr(qrCode = qrCode, qrMaxSize = qrMaxSize)
                DeviceLoginControls(
                    launchUrl = launchUrl,
                    prompt = prompt,
                    phase = phase,
                    remainingSeconds = remainingSeconds,
                    urlActionMessage = urlActionMessage,
                    onUrlActionMessage = { urlActionMessage = it },
                    onCancel = onCancel,
                    focusRequester = initialFocusRequester,
                    focusCancel = focusCancelOnPrompt,
                    context = context,
                    clipboardManager = clipboardManager,
                    showTitle = false,
                )
            }
        }
    }
}

@Composable
private fun DeviceLoginQr(qrCode: QrCode?, qrMaxSize: androidx.compose.ui.unit.Dp, modifier: Modifier = Modifier) {
    qrCode?.let {
        BoxWithConstraints(
            modifier = modifier.fillMaxWidth(),
            contentAlignment = Alignment.Center,
        ) {
            val qrDisplaySize = minOf(maxWidth * 0.92f, qrMaxSize)
            QrCodeView(it, Modifier.size(qrDisplaySize))
        }
    }
}

@Composable
private fun DeviceLoginControls(
    launchUrl: String,
    prompt: DeviceLoginPrompt,
    phase: String,
    remainingSeconds: Int,
    urlActionMessage: String?,
    onUrlActionMessage: (String) -> Unit,
    onCancel: () -> Unit,
    focusRequester: FocusRequester,
    focusCancel: Boolean,
    context: android.content.Context,
    clipboardManager: androidx.compose.ui.platform.ClipboardManager,
    modifier: Modifier = Modifier,
    showTitle: Boolean = true,
) {
    Column(
        modifier = modifier,
        horizontalAlignment = Alignment.CenterHorizontally,
        verticalArrangement = Arrangement.spacedBy(12.dp),
    ) {
        if (showTitle) {
            Text(stringResource(R.string.login_tv_title), style = MaterialTheme.typography.titleMedium, fontWeight = FontWeight.Bold)
        }
        TextButton(
            onClick = {
                val opened = openExternalUrl(context, launchUrl)
                if (opened) {
                    onUrlActionMessage("Opening sign-in URL")
                } else {
                    clipboardManager.setText(AnnotatedString(launchUrl))
                    onUrlActionMessage("URL copied")
                }
            },
            modifier = if (focusCancel) Modifier else Modifier.focusRequester(focusRequester),
        ) {
            Text(
                launchUrl,
                color = MaterialTheme.colorScheme.primary,
                style = MaterialTheme.typography.titleSmall,
                textAlign = TextAlign.Center,
                maxLines = 2,
                overflow = TextOverflow.Ellipsis,
            )
        }
        Text(prompt.userCode, style = MaterialTheme.typography.displaySmall, fontWeight = FontWeight.Bold, color = Color.White)
        Text(stringResource(R.string.login_tv_status, phase.ifBlank { stringResource(R.string.login_tv_waiting) }), color = TextMuted)
        urlActionMessage?.let {
            Text(it, color = TextMuted, style = MaterialTheme.typography.bodySmall)
        }
        Text(stringResource(R.string.login_tv_expires, remainingSeconds / 60, remainingSeconds % 60), color = TextMuted)
        OutlinedButton(
            onClick = onCancel,
            modifier = if (focusCancel) Modifier.focusRequester(focusRequester) else Modifier,
        ) {
            Text(stringResource(R.string.action_cancel))
        }
    }
}

internal fun openExternalUrl(context: android.content.Context, url: String): Boolean {
    val intent = Intent(Intent.ACTION_VIEW, Uri.parse(url)).addFlags(Intent.FLAG_ACTIVITY_NEW_TASK)
    return runCatching {
        context.startActivity(intent)
        true
    }.getOrDefault(false)
}

@Composable
private fun QrCodeView(qrCode: QrCode, modifier: Modifier = Modifier) {
    Canvas(
        modifier
            .clip(RoundedCornerShape(12.dp))
            .background(Color.White)
            .padding(14.dp),
    ) {
        val quiet = 4
        val cells = qrCode.size + quiet * 2
        val cellSize = floor(min(size.width, size.height) / cells).coerceAtLeast(1f)
        val qrSize = cellSize * cells
        val originX = floor((size.width - qrSize) / 2f)
        val originY = floor((size.height - qrSize) / 2f)
        for (y in 0 until qrCode.size) {
            for (x in 0 until qrCode.size) {
                if (!qrCode.isDark(x, y)) continue
                drawRect(
                    color = Color.Black,
                    topLeft = Offset(originX + (x + quiet) * cellSize, originY + (y + quiet) * cellSize),
                    size = Size(cellSize, cellSize),
                )
            }
        }
    }
}

private fun secondsUntil(deadlineMs: Long): Int =
    ((deadlineMs - System.currentTimeMillis()).coerceAtLeast(0L) / 1000L).toInt()

private fun isPhoneLandscape(width: androidx.compose.ui.unit.Dp, height: androidx.compose.ui.unit.Dp): Boolean =
    width > height && minOf(width, height) < PHONE_NAV_RAIL_MAX_SMALLEST_WIDTH

private fun isPhonePortrait(width: androidx.compose.ui.unit.Dp, height: androidx.compose.ui.unit.Dp): Boolean =
    height >= width && minOf(width, height) < PHONE_NAV_RAIL_MAX_SMALLEST_WIDTH

@Composable
private fun MainShell(state: OpenNowUiState, viewModel: OpenNowViewModel) {
    val inStream = state.page == AppPage.Stream
    val streamingActive = inStream && state.streamStatus != "idle"
    val modalPickerOpen = state.pendingPrintedWasteGame != null || state.pendingStoreChoiceGame != null
    val tvProfile = state.codecReport?.androidTvProfile == true
    var visibleSearchTarget by remember { mutableStateOf<SearchTarget?>(null) }
    var settingsSearchQuery by remember { mutableStateOf("") }
    fun revealSearch(
        target: SearchTarget = when (state.page) {
            AppPage.Library -> SearchTarget.Library
            AppPage.Settings -> SearchTarget.Settings
            else -> SearchTarget.Store
        },
    ) {
        visibleSearchTarget = target
        if (target == SearchTarget.Store && state.page != AppPage.Home) {
            viewModel.setPage(AppPage.Home)
        } else if (target == SearchTarget.Library && state.page != AppPage.Library) {
            viewModel.setPage(AppPage.Library)
        } else if (target == SearchTarget.Settings && state.page != AppPage.Settings) {
            viewModel.setPage(AppPage.Settings)
        }
    }
    BackHandler(enabled = state.selectedGame != null && !inStream) {
        viewModel.clearSelectedGame()
    }
    BackHandler(enabled = tvProfile && !inStream && state.selectedGame == null && state.page != AppPage.Home) {
        viewModel.setPage(AppPage.Home)
    }
    BoxWithConstraints(Modifier.fillMaxSize()) {
        var phoneLandscapeScrollChromeHidden by remember { mutableStateOf(false) }
        val horizontalChrome = maxWidth > maxHeight
        val phoneLandscapeChrome = !tvProfile && !inStream && isPhoneLandscape(maxWidth, maxHeight)
        val portraitChrome = !inStream && maxHeight >= maxWidth
        val showNavigationRail = !inStream && (tvProfile || phoneLandscapeChrome)
        val scrollChromePage = state.page == AppPage.Home || state.page == AppPage.Library
        LaunchedEffect(phoneLandscapeChrome, scrollChromePage) {
            if (!phoneLandscapeChrome || !scrollChromePage) {
                phoneLandscapeScrollChromeHidden = false
            }
        }

        Scaffold(
            contentWindowInsets = if (streamingActive) WindowInsets(0, 0, 0, 0) else ScaffoldDefaults.contentWindowInsets,
            bottomBar = {
                if (!inStream && !showNavigationRail) {
                    NavigationBar(
                        containerColor = if (state.page == AppPage.Settings) SettingsBackground else MaterialTheme.colorScheme.background,
                        tonalElevation = 0.dp,
                    ) {
                        BottomNavItem(
                            selected = state.page == AppPage.Home && visibleSearchTarget != SearchTarget.Store,
                            onClick = {
                                visibleSearchTarget = null
                                viewModel.setPage(AppPage.Home)
                            },
                            iconRes = R.drawable.ic_tab_store,
                            label = stringResource(R.string.nav_store),
                        )
                        BottomNavItem(
                            selected = visibleSearchTarget != null,
                            onClick = { revealSearch() },
                            iconRes = R.drawable.ic_search,
                            label = stringResource(R.string.nav_search),
                        )
                        BottomNavItem(
                            selected = state.page == AppPage.Library && visibleSearchTarget != SearchTarget.Library,
                            onClick = {
                                visibleSearchTarget = null
                                viewModel.setPage(AppPage.Library)
                            },
                            iconRes = R.drawable.ic_tab_library,
                            label = stringResource(R.string.nav_library),
                        )
                        BottomNavItem(
                            selected = state.page == AppPage.Settings && visibleSearchTarget != SearchTarget.Settings,
                            onClick = {
                                visibleSearchTarget = null
                                viewModel.setPage(AppPage.Settings)
                            },
                            iconRes = R.drawable.ic_tab_settings,
                            label = stringResource(R.string.nav_settings),
                        )
                    }
                }
            },
        ) { padding ->
            Box(Modifier.fillMaxSize().padding(padding)) {
                Row(Modifier.fillMaxSize()) {
                    if (showNavigationRail) {
                        AppNavigationRail(
                            state = state,
                            activeSearchTarget = visibleSearchTarget,
                            showAppIcon = showNavigationRail && horizontalChrome,
                            onNavigate = { page ->
                                visibleSearchTarget = null
                                viewModel.setPage(page)
                            },
                            onSearch = { revealSearch(it) },
                        )
                    }
                    Column(
                        Modifier
                            .weight(1f)
                            .fillMaxHeight(),
                    ) {
                        AnimatedVisibility(
                            visible = portraitChrome,
                        ) {
                            if (!inStream) {
                                TopStatusBar(
                                    state = state,
                                    onResumeActiveSession = viewModel::resumeActiveSession,
                                )
                            }
                        }
                        Box(Modifier.fillMaxSize()) {
                            when (state.page) {
                                AppPage.Home -> HomeScreen(
                                    state = state,
                                    viewModel = viewModel,
                                    tvProfile = tvProfile,
                                    hideChromeWhenScrolled = phoneLandscapeChrome,
                                    searchRequested = visibleSearchTarget == SearchTarget.Store,
                                    onSearchDismissed = {
                                        if (visibleSearchTarget == SearchTarget.Store) visibleSearchTarget = null
                                    },
                                    onScrollChromeHiddenChange = { phoneLandscapeScrollChromeHidden = it },
                                )
                                AppPage.Library -> LibraryScreen(
                                    state = state,
                                    viewModel = viewModel,
                                    tvProfile = tvProfile,
                                    hideChromeWhenScrolled = phoneLandscapeChrome,
                                    searchRequested = visibleSearchTarget == SearchTarget.Library,
                                    onSearchDismissed = {
                                        if (visibleSearchTarget == SearchTarget.Library) visibleSearchTarget = null
                                    },
                                    onScrollChromeHiddenChange = { phoneLandscapeScrollChromeHidden = it },
                                )
                                AppPage.Settings -> SettingsScreen(
                                    state = state,
                                    viewModel = viewModel,
                                    tvProfile = tvProfile,
                                    searchRequested = visibleSearchTarget == SearchTarget.Settings,
                                    searchQuery = settingsSearchQuery,
                                    onSearchQueryChange = { next ->
                                        settingsSearchQuery = next
                                        if (next.isBlank() && visibleSearchTarget == SearchTarget.Settings) {
                                            visibleSearchTarget = null
                                        }
                                    },
                                )
                                AppPage.Stream -> StreamScreen(state, viewModel)
                            }
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
                            defaultVariantId = state.settings.defaultGameVariantIds[game.id],
                            onPlay = viewModel::play,
                            onChooseStore = viewModel::chooseStore,
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
                            defaultVariantId = state.settings.defaultGameVariantIds[game.id],
                            onLaunch = viewModel::playVariant,
                            onSetDefaultStore = viewModel::setDefaultGameVariant,
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
}

@Composable
private fun AppNavigationRail(
    state: OpenNowUiState,
    activeSearchTarget: SearchTarget?,
    showAppIcon: Boolean,
    onNavigate: (AppPage) -> Unit,
    onSearch: (SearchTarget) -> Unit,
) {
    Surface(
        modifier = Modifier
            .width(APP_NAV_RAIL_WIDTH)
            .fillMaxHeight(),
        color = if (state.page == AppPage.Settings) SettingsBackground else MaterialTheme.colorScheme.background,
        tonalElevation = 0.dp,
    ) {
        Box(Modifier.fillMaxSize()) {
            if (showAppIcon) {
                Box(
                    modifier = Modifier
                        .align(Alignment.TopCenter)
                        .fillMaxWidth()
                        .padding(top = 12.dp, bottom = 8.dp),
                    contentAlignment = Alignment.Center,
                ) {
                    OpenNowAppIcon(34.dp)
                }
            }
            Column(
                modifier = Modifier
                    .align(Alignment.Center)
                    .fillMaxWidth(),
                horizontalAlignment = Alignment.CenterHorizontally,
            ) {
                if (!showAppIcon) {
                    Spacer(Modifier.height(8.dp))
                }
                AppNavigationRailItem(
                    selected = state.page == AppPage.Home && activeSearchTarget != SearchTarget.Store,
                    onClick = { onNavigate(AppPage.Home) },
                    iconRes = R.drawable.ic_tab_store,
                    label = stringResource(R.string.nav_store),
                )
                AppNavigationRailItem(
                    selected = activeSearchTarget != null,
                    onClick = {
                        onSearch(
                            when (state.page) {
                                AppPage.Library -> SearchTarget.Library
                                AppPage.Settings -> SearchTarget.Settings
                                else -> SearchTarget.Store
                            },
                        )
                    },
                    iconRes = R.drawable.ic_search,
                    label = stringResource(R.string.nav_search),
                )
                AppNavigationRailItem(
                    selected = state.page == AppPage.Library && activeSearchTarget != SearchTarget.Library,
                    onClick = { onNavigate(AppPage.Library) },
                    iconRes = R.drawable.ic_tab_library,
                    label = stringResource(R.string.nav_library),
                )
                AppNavigationRailItem(
                    selected = state.page == AppPage.Settings && activeSearchTarget != SearchTarget.Settings,
                    onClick = { onNavigate(AppPage.Settings) },
                    iconRes = R.drawable.ic_tab_settings,
                    label = stringResource(R.string.nav_settings),
                )
            }
        }
    }
}

@Composable
private fun AppNavigationRailItem(selected: Boolean, onClick: () -> Unit, iconRes: Int, label: String) {
    NavigationRailItem(
        selected = selected,
        onClick = onClick,
        icon = {
            Icon(
                painter = painterResource(iconRes),
                contentDescription = label,
                modifier = Modifier.size(24.dp),
            )
        },
        label = { Text(label, maxLines = 1, overflow = TextOverflow.Ellipsis) },
    )
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
private fun TopStatusBar(
    state: OpenNowUiState,
    onResumeActiveSession: () -> Unit,
) {
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
            if (state.settings.nerdMode) {
                Spacer(Modifier.width(6.dp))
                TopStatusDetails(state)
            }
            if (state.activeSession != null) {
                Spacer(Modifier.width(6.dp))
                ElevatedButton(onClick = onResumeActiveSession) {
                    Text(stringResource(R.string.action_resume))
                }
            }
        }
    }
}

@Composable
private fun TopStatusDetails(state: OpenNowUiState) {
    val stream = state.activeStreamSettings ?: state.settings.stream
    Row(
        horizontalArrangement = Arrangement.spacedBy(5.dp),
        verticalAlignment = Alignment.CenterVertically,
    ) {
        Surface(
            shape = RoundedCornerShape(999.dp),
            color = PanelAlt.copy(alpha = 0.78f),
            tonalElevation = 0.dp,
        ) {
            Text(
                "Codec ${stream.codec.name}",
                modifier = Modifier.padding(horizontal = 8.dp, vertical = 4.dp),
                color = TextMuted,
                style = MaterialTheme.typography.labelSmall,
                maxLines = 1,
                overflow = TextOverflow.Ellipsis,
            )
        }
    }
}

@Composable
internal fun NativeSearchField(
    query: String,
    onQueryChange: (String) -> Unit,
    placeholder: String,
    searching: Boolean = false,
    modifier: Modifier = Modifier,
    focusRequester: FocusRequester? = null,
    onOpen: (() -> Unit)? = null,
) {
    val focusManager = LocalFocusManager.current
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
                modifier = Modifier
                    .weight(1f)
                    .then(focusRequester?.let { Modifier.focusRequester(it) } ?: Modifier)
                    .onFocusChanged { if (it.isFocused) onOpen?.invoke() }
                    .onPreviewKeyEvent { handleDpadFocusMove(it, focusManager) },
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

private fun handleDpadFocusMove(event: androidx.compose.ui.input.key.KeyEvent, focusManager: FocusManager): Boolean {
    if (event.type != KeyEventType.KeyDown) return false
    val direction = when (event.key) {
        Key.DirectionUp -> FocusDirection.Up
        Key.DirectionDown -> FocusDirection.Down
        Key.DirectionLeft -> FocusDirection.Left
        Key.DirectionRight -> FocusDirection.Right
        else -> return false
    }
    return focusManager.moveFocus(direction)
}

internal fun handleVerticalDpadFocusMove(event: androidx.compose.ui.input.key.KeyEvent, focusManager: FocusManager): Boolean {
    if (event.type != KeyEventType.KeyDown) return false
    val direction = when (event.key) {
        Key.DirectionUp -> FocusDirection.Up
        Key.DirectionDown -> FocusDirection.Down
        else -> return false
    }
    focusManager.moveFocus(direction)
    return true
}

internal fun isTvActivateKey(event: androidx.compose.ui.input.key.KeyEvent): Boolean =
    event.type == KeyEventType.KeyUp &&
        event.key in setOf(
            Key.DirectionCenter,
            Key.Enter,
            Key.NumPadEnter,
        )

@OptIn(ExperimentalComposeUiApi::class)
@Composable
private fun HomeScreen(
    state: OpenNowUiState,
    viewModel: OpenNowViewModel,
    tvProfile: Boolean,
    hideChromeWhenScrolled: Boolean,
    searchRequested: Boolean,
    onSearchDismissed: () -> Unit,
    onScrollChromeHiddenChange: (Boolean) -> Unit,
) {
    val visibleGames = state.games.ifEmpty { state.catalogResult.games }
    val searchingCatalog = state.loadingGames && state.catalogSearch.isNotBlank()
    val gridState = rememberLazyGridState()
    val searchFocusRequester = remember { FocusRequester() }
    val scope = rememberCoroutineScope()
    val focusManager = LocalFocusManager.current
    val keyboardController = LocalSoftwareKeyboardController.current
    val showSearch = searchRequested || state.catalogSearch.isNotBlank()
    val showScrollActions = gridState.firstVisibleItemIndex > 0 || gridState.firstVisibleItemScrollOffset > 80
    val scrolledAwayFromTop = gridState.firstVisibleItemIndex > 0 || gridState.firstVisibleItemScrollOffset > 0
    val hideScrollChrome = hideChromeWhenScrolled && scrolledAwayFromTop
    LaunchedEffect(hideScrollChrome) {
        onScrollChromeHiddenChange(hideScrollChrome)
    }
    DisposableEffect(Unit) {
        onDispose { onScrollChromeHiddenChange(false) }
    }
    LaunchedEffect(searchRequested) {
        if (searchRequested) {
            delay(90)
            runCatching { searchFocusRequester.requestFocus() }
            keyboardController?.show()
        }
    }
    SwipeToRefreshContainer(
        refreshing = state.loadingGames,
        showRefreshIndicator = !searchingCatalog,
        onRefresh = viewModel::refreshGames,
        modifier = Modifier.fillMaxSize(),
    ) {
        BoxWithConstraints(Modifier.fillMaxSize()) {
            Column(
                Modifier
                    .fillMaxSize()
                    .padding(12.dp),
                verticalArrangement = Arrangement.spacedBy(10.dp),
            ) {
                AnimatedVisibility(visible = showSearch) {
                    NativeSearchField(
                        modifier = Modifier.fillMaxWidth(),
                        query = state.catalogSearch,
                        onQueryChange = { next ->
                            viewModel.setCatalogSearch(next)
                            if (next.isBlank()) onSearchDismissed()
                        },
                        placeholder = stringResource(R.string.search_games),
                        searching = searchingCatalog,
                        focusRequester = searchFocusRequester,
                        onOpen = {
                            if (gridState.firstVisibleItemIndex > 0 || gridState.firstVisibleItemScrollOffset > 0) {
                                scope.launch { gridState.animateScrollToItem(0) }
                            }
                        },
                    )
                }
                Box(
                    Modifier
                        .weight(1f)
                        .pointerInput(Unit) {
                            awaitEachGesture {
                                awaitFirstDown(requireUnconsumed = false, pass = PointerEventPass.Initial)
                                focusManager.clearFocus()
                                keyboardController?.hide()
                            }
                        },
                ) {
                    if (state.loadingGames && visibleGames.isEmpty()) {
                        Column(Modifier.fillMaxSize(), verticalArrangement = Arrangement.spacedBy(10.dp)) {
                            StoreScrollableControls(
                                state = state,
                                onSortChange = viewModel::setCatalogSort,
                                onFilterToggle = viewModel::toggleCatalogFilter,
                            )
                            RefreshingGamesPlaceholder(Modifier.weight(1f))
                        }
                    } else {
                        StoreGameGrid(
                            games = visibleGames,
                            favoriteIds = state.settings.favoriteGameIds,
                            settings = state.settings,
                            tvProfile = tvProfile,
                            state = state,
                            onSelect = viewModel::selectGame,
                            onFavorite = viewModel::updateFavorites,
                            onPlay = viewModel::play,
                            onChooseStore = viewModel::chooseStore,
                            onSortChange = viewModel::setCatalogSort,
                            onFilterToggle = viewModel::toggleCatalogFilter,
                            onClearSearch = {
                                viewModel.setCatalogSearch("")
                                onSearchDismissed()
                            },
                            onClearFilters = viewModel::clearCatalogFilters,
                            gridState = gridState,
                            modifier = Modifier.fillMaxSize(),
                        )
                    }
                    if (showScrollActions) {
                        Box(Modifier.align(Alignment.BottomEnd).padding(2.dp)) {
                            StoreScrollActionButton(
                                iconRes = R.drawable.ic_arrow_up,
                                contentDescription = stringResource(R.string.action_scroll_top),
                            ) {
                                scope.launch { gridState.animateScrollToItem(0) }
                            }
                        }
                    }
                }
            }
        }
    }
}

@Composable
private fun StoreScrollableControls(
    state: OpenNowUiState,
    onSortChange: (String) -> Unit,
    onFilterToggle: (String) -> Unit,
) {
    val filterGroups = catalogVisibleFilterGroups(state.catalogResult.filterGroups)
    val filterOptions = catalogFilterOptions(filterGroups)
    Column(verticalArrangement = Arrangement.spacedBy(10.dp)) {
        Row(
            Modifier.fillMaxWidth(),
            horizontalArrangement = Arrangement.spacedBy(8.dp),
            verticalAlignment = Alignment.CenterVertically,
        ) {
            SortPicker(
                options = state.catalogResult.sortOptions,
                selected = state.catalogSortId,
                onSelect = onSortChange,
                modifier = Modifier.weight(1f),
            )
            if (filterOptions.isNotEmpty()) {
                FilterMenu(options = filterOptions, selectedIds = state.catalogFilterIds, onToggle = onFilterToggle)
            }
        }
        SelectedFilterChips(options = filterOptions, selectedIds = state.catalogFilterIds, onToggle = onFilterToggle)
        InlineErrorNotice(error = state.error)
    }
}

@Composable
private fun InlineErrorNotice(error: String?) {
    if (error.isNullOrBlank()) return
    Surface(
        modifier = Modifier.fillMaxWidth(),
        shape = RoundedCornerShape(12.dp),
        color = Color(0xff33181c),
        tonalElevation = 0.dp,
    ) {
        Column(Modifier.padding(horizontal = 12.dp, vertical = 10.dp)) {
            Text(
                compactErrorTitle(error),
                color = Color(0xffffb8bf),
                style = MaterialTheme.typography.labelLarge,
                fontWeight = FontWeight.Bold,
                maxLines = 1,
                overflow = TextOverflow.Ellipsis,
            )
            Text(
                compactErrorBody(error),
                color = Color(0xffffb8bf),
                style = MaterialTheme.typography.bodySmall,
                maxLines = 2,
                overflow = TextOverflow.Ellipsis,
            )
        }
    }
}

private fun compactErrorTitle(error: String): String =
    when {
        error.contains("DNS lookup failed", ignoreCase = true) -> "Network lookup failed"
        error.contains("Unable to resolve host", ignoreCase = true) -> "Network lookup failed"
        else -> "Something went wrong"
    }

private fun compactErrorBody(error: String): String =
    error
        .replace('\n', ' ')
        .replace(Regex("\\s+"), " ")
        .let { if (it.length > 180) "${it.take(177)}..." else it }

@Composable
private fun StoreScrollActionButton(iconRes: Int, contentDescription: String, onClick: () -> Unit) {
    Surface(
        shape = CircleShape,
        color = PanelAlt.copy(alpha = 0.96f),
        tonalElevation = 4.dp,
        shadowElevation = 4.dp,
    ) {
        IconButton(onClick = onClick, modifier = Modifier.size(44.dp)) {
            Icon(
                painter = painterResource(iconRes),
                contentDescription = contentDescription,
                tint = MaterialTheme.colorScheme.primary,
                modifier = Modifier.size(22.dp),
            )
        }
    }
}

@Composable
private fun LibraryScreen(
    state: OpenNowUiState,
    viewModel: OpenNowViewModel,
    tvProfile: Boolean,
    hideChromeWhenScrolled: Boolean,
    searchRequested: Boolean,
    onSearchDismissed: () -> Unit,
    onScrollChromeHiddenChange: (Boolean) -> Unit,
) {
    val orderedGames = remember(state.libraryGames, state.settings.favoriteGameIds) {
        favoriteOrderedGames(state.libraryGames, state.settings.favoriteGameIds)
    }
    val filterOptions = remember(orderedGames) {
        libraryStoreFilterOptions(orderedGames)
    }
    val games = remember(orderedGames, state.librarySearch, state.libraryFilterIds) {
        orderedGames.filter { game ->
            gameMatchesSearch(game, state.librarySearch) && gameMatchesLibraryFilters(game, state.libraryFilterIds)
        }
    }
    val gridState = rememberLazyGridState()
    val searchFocusRequester = remember { FocusRequester() }
    val keyboardController = LocalSoftwareKeyboardController.current
    val showSearch = searchRequested || state.librarySearch.isNotBlank()
    val scrolledAwayFromTop = gridState.firstVisibleItemIndex > 0 || gridState.firstVisibleItemScrollOffset > 0
    val hideScrollChrome = hideChromeWhenScrolled && scrolledAwayFromTop
    LaunchedEffect(hideScrollChrome) {
        onScrollChromeHiddenChange(hideScrollChrome)
    }
    DisposableEffect(Unit) {
        onDispose { onScrollChromeHiddenChange(false) }
    }
    LaunchedEffect(searchRequested) {
        if (searchRequested) {
            delay(90)
            runCatching { searchFocusRequester.requestFocus() }
            keyboardController?.show()
        }
    }
    SwipeToRefreshContainer(
        refreshing = state.loadingGames,
        onRefresh = viewModel::refreshGames,
        modifier = Modifier.fillMaxSize(),
    ) {
        BoxWithConstraints(Modifier.fillMaxSize()) {
            Column(Modifier.fillMaxSize().padding(12.dp), verticalArrangement = Arrangement.spacedBy(10.dp)) {
                AnimatedVisibility(visible = showSearch) {
                    NativeSearchField(
                        modifier = Modifier.fillMaxWidth(),
                        query = state.librarySearch,
                        onQueryChange = { next ->
                            viewModel.setLibrarySearch(next)
                            if (next.isBlank()) onSearchDismissed()
                        },
                        placeholder = "Search library",
                        focusRequester = searchFocusRequester,
                    )
                }
                LibraryFilterControls(
                    gameCount = games.size,
                    totalCount = state.libraryGames.size,
                    options = filterOptions,
                    selectedIds = state.libraryFilterIds,
                    onToggle = viewModel::toggleLibraryFilter,
                )
                GameGrid(
                    games,
                    state.settings.favoriteGameIds,
                    state.settings,
                    tvProfile,
                    viewModel::selectGame,
                    viewModel::updateFavorites,
                    viewModel::play,
                    viewModel::chooseStore,
                    modifier = Modifier.weight(1f),
                    gridState = gridState,
                    emptyContent = {
                        val hasSearch = state.librarySearch.isNotBlank()
                        val hasFilters = state.libraryFilterIds.isNotEmpty()
                        if ((hasSearch || hasFilters) && state.libraryGames.isNotEmpty()) {
                            SearchEmptyState(
                                title = stringResource(R.string.library_empty_search_title),
                                message = when {
                                    hasSearch && hasFilters -> stringResource(R.string.library_empty_search_filters_body)
                                    hasSearch -> stringResource(R.string.library_empty_search_body)
                                    else -> stringResource(R.string.library_empty_filters_body)
                                },
                                onClearSearch = if (hasSearch) {
                                    {
                                        viewModel.setLibrarySearch("")
                                        onSearchDismissed()
                                    }
                                } else {
                                    null
                                },
                                onClearFilters = if (hasFilters) {
                                    viewModel::clearLibraryFilters
                                } else {
                                    null
                                },
                            )
                        } else {
                            Text(stringResource(R.string.no_games_loaded), color = TextMuted)
                        }
                    },
                )
            }
        }
    }
}

@Composable
private fun LibraryFilterControls(
    gameCount: Int,
    totalCount: Int,
    options: List<CatalogFilterOption>,
    selectedIds: List<String>,
    onToggle: (String) -> Unit,
) {
    Column(verticalArrangement = Arrangement.spacedBy(8.dp)) {
        Row(
            Modifier.fillMaxWidth(),
            horizontalArrangement = Arrangement.spacedBy(8.dp),
            verticalAlignment = Alignment.CenterVertically,
        ) {
            Text(
                text = if (gameCount == totalCount) {
                    stringResource(R.string.library_count, totalCount)
                } else {
                    "$gameCount / ${stringResource(R.string.library_count, totalCount)}"
                },
                color = TextMuted,
                style = MaterialTheme.typography.labelMedium,
                maxLines = 1,
                overflow = TextOverflow.Ellipsis,
                modifier = Modifier.weight(1f),
            )
            if (options.isNotEmpty()) {
                FilterMenu(options = options, selectedIds = selectedIds, onToggle = onToggle)
            }
        }
        SelectedFilterChips(options = options, selectedIds = selectedIds, onToggle = onToggle)
    }
}

private fun libraryStoreFilterOptions(games: List<GameInfo>): List<CatalogFilterOption> {
    val labelsById = linkedMapOf<String, String>()
    games.forEach { game ->
        libraryStoreFilterIds(game).forEach { (id, label) ->
            labelsById.putIfAbsent(id, label)
        }
    }
    return labelsById.entries
        .sortedBy { it.value.lowercase(Locale.US) }
        .map { (id, label) ->
            CatalogFilterOption(
                id = id,
                rawId = id.removePrefix(LIBRARY_STORE_FILTER_PREFIX),
                label = label,
                groupId = "library_store",
                groupLabel = "Launcher",
            )
        }
}

private fun gameMatchesLibraryFilters(game: GameInfo, selectedIds: List<String>): Boolean {
    if (selectedIds.isEmpty()) return true
    val gameFilterIds = libraryStoreFilterIds(game).map { it.first }.toSet()
    return selectedIds.any { it in gameFilterIds }
}

private fun libraryStoreFilterIds(game: GameInfo): List<Pair<String, String>> {
    val labels = libraryStoreDisplayNames(game)
    return labels
        .mapNotNull { label ->
            val normalized = normalizeGameStore(label)
            if (normalized.isBlank()) return@mapNotNull null
            LIBRARY_STORE_FILTER_PREFIX + normalized to label
        }
        .distinctBy { it.first }
}

private const val LIBRARY_STORE_FILTER_PREFIX = "library_store:"

@Composable
private fun ActiveSessionResumeCard(
    state: OpenNowUiState,
    onResumeActiveSession: () -> Unit,
    modifier: Modifier = Modifier,
) {
    val active = state.activeSession ?: return
    val game = activeSessionGame(state, active)
    Surface(
        modifier = modifier.fillMaxWidth(),
        shape = RoundedCornerShape(18.dp),
        color = PanelAlt.copy(alpha = 0.92f),
        tonalElevation = 3.dp,
    ) {
        Row(
            Modifier.padding(horizontal = 14.dp, vertical = 12.dp),
            verticalAlignment = Alignment.CenterVertically,
            horizontalArrangement = Arrangement.spacedBy(12.dp),
        ) {
            UrlImage(
                game?.imageUrl,
                Modifier
                    .width(44.dp)
                    .height(58.dp)
                    .clip(RoundedCornerShape(10.dp)),
            )
            Column(Modifier.weight(1f), verticalArrangement = Arrangement.spacedBy(2.dp)) {
                Text("Resume cloud session", fontWeight = FontWeight.Bold, maxLines = 1, overflow = TextOverflow.Ellipsis)
                Text(
                    game?.title ?: "App ${active.appId}",
                    color = TextMuted,
                    maxLines = 1,
                    overflow = TextOverflow.Ellipsis,
                )
                Text(
                    activeSessionSummary(active),
                    color = TextMuted,
                    style = MaterialTheme.typography.labelSmall,
                    maxLines = 1,
                    overflow = TextOverflow.Ellipsis,
                )
            }
            Button(onClick = onResumeActiveSession, contentPadding = PaddingValues(horizontal = 14.dp, vertical = 8.dp)) {
                Text(stringResource(R.string.action_resume), maxLines = 1, overflow = TextOverflow.Ellipsis)
            }
        }
    }
}

private fun activeSessionGame(state: OpenNowUiState, active: ActiveSessionInfo): GameInfo? =
    (state.games + state.libraryGames).firstOrNull { game ->
        game.launchAppId == active.appId.toString() ||
            game.variants.any { variant -> variant.id == active.appId.toString() }
    }

private fun activeSessionSummary(active: ActiveSessionInfo): String =
    listOfNotNull(
        when (active.status) {
            1 -> active.queuePosition?.takeIf { it > 0 }?.let { "Queue $it" } ?: "Starting"
            2, 3 -> "Ready"
            else -> "Active"
        },
        active.resolution,
        active.fps?.let { "${it} FPS" },
        active.gpuType,
        active.sessionId.take(8).takeIf { it.isNotBlank() }?.let { "Session $it" },
    ).joinToString(" - ")

@Composable
private fun SearchEmptyState(
    title: String,
    message: String,
    onClearSearch: (() -> Unit)? = null,
    onClearFilters: (() -> Unit)? = null,
) {
    Column(
        Modifier
            .fillMaxWidth()
            .padding(horizontal = 28.dp),
        horizontalAlignment = Alignment.CenterHorizontally,
        verticalArrangement = Arrangement.spacedBy(10.dp),
    ) {
        Text(
            title,
            color = TextPrimary,
            style = MaterialTheme.typography.titleMedium,
            fontWeight = FontWeight.Bold,
            textAlign = TextAlign.Center,
        )
        Text(
            message,
            color = TextMuted,
            style = MaterialTheme.typography.bodyMedium,
            textAlign = TextAlign.Center,
        )
        Row(
            horizontalArrangement = Arrangement.spacedBy(8.dp),
            verticalAlignment = Alignment.CenterVertically,
        ) {
            onClearSearch?.let { clearSearch ->
                OutlinedButton(onClick = clearSearch) {
                    Text(stringResource(R.string.search_clear), maxLines = 1, overflow = TextOverflow.Ellipsis)
                }
            }
            onClearFilters?.let { clearFilters ->
                OutlinedButton(onClick = clearFilters) {
                    Text(stringResource(R.string.action_clear_filters), maxLines = 1, overflow = TextOverflow.Ellipsis)
                }
            }
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
    tvProfile: Boolean,
    onSelect: (GameInfo) -> Unit,
    onFavorite: (String) -> Unit,
    onPlay: (GameInfo) -> Unit,
    onChooseStore: (GameInfo) -> Unit,
    modifier: Modifier = Modifier,
    gridState: androidx.compose.foundation.lazy.grid.LazyGridState = rememberLazyGridState(),
    emptyContent: (@Composable () -> Unit)? = null,
) {
    if (games.isEmpty()) {
        Box(modifier.fillMaxSize(), contentAlignment = Alignment.Center) {
            if (emptyContent != null) {
                emptyContent()
            } else {
                Text(stringResource(R.string.no_games_loaded), color = TextMuted)
            }
        }
        return
    }
    val scale = settings.posterSizeScale.coerceIn(0.82f, 1.08f)
    val compact = settings.compactGameCards
    val landscapeLayout = LocalConfiguration.current.orientation == Configuration.ORIENTATION_LANDSCAPE
    BoxWithConstraints(modifier.fillMaxSize()) {
        val gridSpec = gameGridSpec(maxWidth, compact, landscapeLayout)
        LazyVerticalGrid(
            modifier = Modifier.fillMaxSize(),
            state = gridState,
            columns = GridCells.Fixed(gridSpec.columns),
            contentPadding = PaddingValues(4.dp),
            horizontalArrangement = Arrangement.spacedBy(if (compact) 8.dp else 10.dp),
            verticalArrangement = Arrangement.spacedBy(if (compact) 10.dp else 12.dp),
        ) {
            gridItems(games, key = { it.id }) { game ->
                GameCard(
                    game = game,
                    favorite = game.id in favoriteIds,
                    settings = settings,
                    tvProfile = tvProfile,
                    cardHeight = gridSpec.cardHeight * scale,
                    thumbnailPlayOverlay = !tvProfile,
                    onSelect = onSelect,
                    onFavorite = onFavorite,
                    onPlay = onPlay,
                    onChooseStore = onChooseStore,
                )
            }
        }
    }
}

@Composable
private fun StoreGameGrid(
    games: List<GameInfo>,
    favoriteIds: List<String>,
    settings: AppSettings,
    tvProfile: Boolean,
    state: OpenNowUiState,
    onSelect: (GameInfo) -> Unit,
    onFavorite: (String) -> Unit,
    onPlay: (GameInfo) -> Unit,
    onChooseStore: (GameInfo) -> Unit,
    onSortChange: (String) -> Unit,
    onFilterToggle: (String) -> Unit,
    onClearSearch: () -> Unit,
    onClearFilters: () -> Unit,
    gridState: androidx.compose.foundation.lazy.grid.LazyGridState,
    modifier: Modifier = Modifier,
) {
    if (games.isEmpty()) {
        Column(modifier.fillMaxSize(), verticalArrangement = Arrangement.spacedBy(10.dp)) {
            StoreScrollableControls(state, onSortChange, onFilterToggle)
            Box(Modifier.fillMaxSize(), contentAlignment = Alignment.Center) {
                val hasSearch = state.catalogSearch.isNotBlank()
                val hasFilters = state.catalogFilterIds.isNotEmpty()
                if (hasSearch || hasFilters) {
                    SearchEmptyState(
                        title = stringResource(R.string.store_empty_search_title),
                        message = when {
                            hasSearch && hasFilters -> stringResource(R.string.store_empty_search_filters_body)
                            hasSearch -> stringResource(R.string.store_empty_search_body)
                            else -> stringResource(R.string.store_empty_filters_body)
                        },
                        onClearSearch = if (hasSearch) onClearSearch else null,
                        onClearFilters = if (hasFilters) onClearFilters else null,
                    )
                } else {
                    Text(stringResource(R.string.no_games_loaded), color = TextMuted)
                }
            }
        }
        return
    }
    val scale = settings.posterSizeScale.coerceIn(0.82f, 1.08f)
    val compact = settings.compactGameCards
    val landscapeLayout = LocalConfiguration.current.orientation == Configuration.ORIENTATION_LANDSCAPE
    BoxWithConstraints(modifier.fillMaxSize()) {
        val gridSpec = gameGridSpec(maxWidth, compact, landscapeLayout)
        LazyVerticalGrid(
            modifier = Modifier.fillMaxSize(),
            state = gridState,
            columns = GridCells.Fixed(gridSpec.columns),
            contentPadding = PaddingValues(0.dp),
            horizontalArrangement = Arrangement.spacedBy(if (compact) 8.dp else 10.dp),
            verticalArrangement = Arrangement.spacedBy(if (compact) 10.dp else 12.dp),
        ) {
            item(span = { GridItemSpan(maxLineSpan) }) {
                StoreScrollableControls(state, onSortChange, onFilterToggle)
            }
            gridItems(games, key = { it.id }) { game ->
                GameCard(
                    game = game,
                    favorite = game.id in favoriteIds,
                    settings = settings,
                    tvProfile = tvProfile,
                    cardHeight = gridSpec.cardHeight * scale,
                    thumbnailPlayOverlay = !tvProfile,
                    onSelect = onSelect,
                    onFavorite = onFavorite,
                    onPlay = onPlay,
                    onChooseStore = onChooseStore,
                )
            }
        }
    }
}

private data class GameGridSpec(
    val columns: Int,
    val cardHeight: Dp,
)

private fun gameGridSpec(
    maxWidth: androidx.compose.ui.unit.Dp,
    compact: Boolean,
    landscapeLayout: Boolean,
): GameGridSpec {
    return when {
        landscapeLayout -> GameGridSpec(
            columns = landscapePosterColumnCount(maxWidth),
            cardHeight = if (compact) 188.dp else 214.dp,
        )
        compact -> GameGridSpec(columns = gameGridColumnCount(maxWidth), cardHeight = 218.dp)
        else -> GameGridSpec(columns = gameGridColumnCount(maxWidth), cardHeight = 246.dp)
    }
}

private fun landscapePosterColumnCount(maxWidth: androidx.compose.ui.unit.Dp): Int =
    when {
        maxWidth >= 1440.dp -> 8
        maxWidth >= 1050.dp -> 7
        maxWidth >= 520.dp -> 6
        else -> 5
    }

private fun gameGridColumnCount(maxWidth: androidx.compose.ui.unit.Dp): Int =
    when {
        maxWidth >= 1100.dp -> 5
        maxWidth >= 840.dp -> 4
        maxWidth >= 600.dp -> 3
        else -> 2
    }

@Composable
private fun GameCard(
    game: GameInfo,
    favorite: Boolean,
    settings: AppSettings,
    tvProfile: Boolean,
    cardHeight: androidx.compose.ui.unit.Dp,
    thumbnailPlayOverlay: Boolean,
    onSelect: (GameInfo) -> Unit,
    onFavorite: (String) -> Unit,
    onPlay: (GameInfo) -> Unit,
    onChooseStore: (GameInfo) -> Unit,
) {
    var focused by remember { mutableStateOf(false) }
    val cardShape = RoundedCornerShape(if (settings.expressiveUi) 12.dp else 8.dp)
    Card(
        modifier = Modifier
            .fillMaxWidth()
            .height(cardHeight)
            .onFocusChanged { focused = it.isFocused || it.hasFocus }
            .then(
                if (tvProfile) {
                    Modifier
                        .border(
                            width = if (focused) 2.dp else 1.dp,
                            color = if (focused) MaterialTheme.colorScheme.primary else Color.Transparent,
                            shape = cardShape,
                        )
                        .onPreviewKeyEvent { event ->
                            if (isTvActivateKey(event)) {
                                onSelect(game)
                                true
                            } else {
                                false
                            }
                        }
                        .focusable()
                } else {
                    Modifier
                },
            ),
        colors = CardDefaults.cardColors(
            containerColor = if (settings.expressiveUi) MaterialTheme.colorScheme.surfaceVariant.copy(alpha = 0.72f) else Panel,
        ),
        shape = cardShape,
    ) {
        Box(
            Modifier
                .weight(1f)
                .clickable { onSelect(game) },
        ) {
            UrlImage(game.imageUrl, Modifier.fillMaxSize())
            FavoriteIconButton(
                favorite = favorite,
                onClick = { onFavorite(game.id) },
                modifier = Modifier
                    .align(Alignment.TopEnd)
                    .padding(8.dp),
            )
            if (thumbnailPlayOverlay) {
                ThumbnailStoreButton(
                    badge = launcherBadgeForGame(game),
                    onClick = { onChooseStore(game) },
                    modifier = Modifier
                        .align(Alignment.BottomStart)
                        .padding(8.dp),
                )
                ThumbnailPlayButton(
                    onClick = { onPlay(game) },
                    onLongClick = { onChooseStore(game) },
                    modifier = Modifier
                        .align(Alignment.BottomEnd)
                        .padding(8.dp),
                )
            }
        }
        if (!thumbnailPlayOverlay) {
            Column(
                Modifier
                    .clickable { onSelect(game) }
                    .padding(9.dp),
                verticalArrangement = Arrangement.spacedBy(5.dp),
            ) {
                /*
                Text(
                    game.title,
                    fontWeight = FontWeight.Bold,
                    minLines = 2,
                    maxLines = 2,
                    overflow = TextOverflow.Ellipsis,
                    style = MaterialTheme.typography.bodyMedium,
                )
                */
                if (settings.showGameStoreLabels) {
                    Text(displayStoresForGame(game), color = TextMuted, maxLines = 1, overflow = TextOverflow.Ellipsis, style = MaterialTheme.typography.labelMedium)
                }
            }
        }
        if (!thumbnailPlayOverlay) {
            Box(Modifier.padding(start = 9.dp, end = 9.dp, bottom = 9.dp)) {
                LongPressPlayButton(
                    onClick = { onPlay(game) },
                    onLongClick = { onChooseStore(game) },
                    modifier = Modifier.fillMaxWidth(),
                )
            }
        }
    }
}

@Composable
private fun ThumbnailStoreButton(badge: LauncherBadge, onClick: () -> Unit, modifier: Modifier = Modifier) {
    val chooseLauncherLabel = stringResource(R.string.store_selector_choose_launcher)
    Surface(
        modifier = modifier.size(44.dp),
        shape = RoundedCornerShape(12.dp),
        color = badge.background.copy(alpha = 0.94f),
        tonalElevation = 3.dp,
        shadowElevation = 3.dp,
    ) {
        IconButton(
            onClick = onClick,
            modifier = Modifier.semantics {
                contentDescription = "${badge.name} $chooseLauncherLabel"
            },
        ) {
            Icon(
                painter = painterResource(badge.iconRes),
                contentDescription = null,
                tint = badge.foreground,
                modifier = Modifier.size(24.dp),
            )
        }
    }
}

private fun launcherBadgeForGame(game: GameInfo): LauncherBadge {
    val candidateStores = mutableListOf<String>()
    game.variants.getOrNull(game.selectedVariantIndex)?.let { selectedVariant ->
        candidateStores += splitGameStoreKeys(selectedVariant.store)
    }
    game.variants.forEach { variant ->
        candidateStores += splitGameStoreKeys(variant.store)
    }
    game.availableStores.forEach { store ->
        candidateStores += splitGameStoreKeys(store)
    }
    val key = candidateStores.firstOrNull { it.isNotBlank() }
    return launcherBadgeForStoreKey(key)
}

internal fun launcherBadgeForStoreKey(storeKey: String?): LauncherBadge =
    when (storeKey) {
        "STEAM" -> LauncherBadge(R.drawable.ic_store_steam, "Steam", Color(0xff17324d))
        "EPIC", "EGS", "EPIC_GAMES_STORE" -> LauncherBadge(R.drawable.ic_store_epic, "Epic", Color(0xff111111))
        "HOYO", "HOYOVERSE", "HOYOPLAY", "HOYO_PLAY", "MIHOYO" -> LauncherBadge(R.drawable.ic_store_hoyo, "HoYo", Color(0xff2b62d9))
        "XBOX", "XBOX_GAME_PASS", "GAME_PASS" -> LauncherBadge(R.drawable.ic_store_xbox, "Xbox", Color(0xff107c10))
        "MICROSOFT", "MICROSOFT_STORE" -> LauncherBadge(R.drawable.ic_store_microsoft, "Microsoft Store", Color(0xff0067b8))
        "UBISOFT", "UBISOFT_CONNECT" -> LauncherBadge(R.drawable.ic_store_ubisoft, "Ubisoft Connect", Color(0xff006efc))
        "EA", "EA_APP", "ORIGIN" -> LauncherBadge(R.drawable.ic_store_ea, "EA app", Color(0xffff4747))
        "GOG", "GOG.COM", "GOG_COM" -> LauncherBadge(R.drawable.ic_store_gog, "GOG", Color(0xff6a35a8))
        "BATTLENET", "BATTLE.NET", "BATTLE_NET", "BLIZZARD" -> LauncherBadge(R.drawable.ic_store_battlenet, "Battle.net", Color(0xff148eff))
        "RIOT", "RIOT_CLIENT", "RIOT_GAMES" -> LauncherBadge(R.drawable.ic_store_riot, "Riot", Color(0xffd13639))
        "ROCKSTAR", "ROCKSTAR_GAMES", "ROCKSTAR_GAMES_LAUNCHER" -> LauncherBadge(R.drawable.ic_store_rockstar, "Rockstar", Color(0xffffc400), Color(0xff111111))
        "NCSOFT", "NC_SOFT", "PURPLE" -> LauncherBadge(R.drawable.ic_tab_store, "NCSOFT", Color(0xffb4822d), Color(0xff111111))
        "GOOGLE_PLAY", "PLAY_STORE", "ANDROID" -> LauncherBadge(R.drawable.ic_store_google_play, "Google Play", Color(0xff0f9d58))
        "AMAZON", "AMAZON_GAMES" -> LauncherBadge(R.drawable.ic_store_amazon, "Amazon Games", Color(0xffff9900), Color(0xff111111))
        else -> LauncherBadge(R.drawable.ic_tab_store, "GeForce NOW", Color.Black.copy(alpha = 0.72f))
    }

private fun displayStoresForGame(game: GameInfo): String {
    val stores = displayStoresForVariants(game.variants).ifEmpty {
        game.availableStores.map(::gameStoreDisplayName)
    }.distinctBy { normalizeGameStore(it) }
    return stores.joinToString(", ").ifBlank { "GeForce NOW" }
}

@OptIn(ExperimentalFoundationApi::class)
@Composable
private fun ThumbnailPlayButton(onClick: () -> Unit, onLongClick: () -> Unit, modifier: Modifier = Modifier) {
    val playColor = MaterialTheme.colorScheme.onPrimary
    Surface(
        modifier = modifier
            .size(44.dp)
            .combinedClickable(
                onClick = onClick,
                onLongClick = onLongClick,
                onLongClickLabel = stringResource(R.string.store_selector_play_long_press),
            ),
        shape = CircleShape,
        color = MaterialTheme.colorScheme.primary.copy(alpha = 0.94f),
        tonalElevation = 3.dp,
        shadowElevation = 3.dp,
    ) {
        Canvas(Modifier.fillMaxSize().padding(14.dp)) {
            val path = Path().apply {
                moveTo(size.width * 0.32f, size.height * 0.18f)
                lineTo(size.width * 0.32f, size.height * 0.82f)
                lineTo(size.width * 0.84f, size.height * 0.5f)
                close()
            }
            drawPath(path, playColor)
        }
    }
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
    defaultVariantId: String?,
    onPlay: (GameInfo) -> Unit,
    onChooseStore: (GameInfo) -> Unit,
    onFavorite: (String) -> Unit,
    onDismiss: () -> Unit,
) {
    val playFocusRequester = remember(game.id) { FocusRequester() }
    LaunchedEffect(game.id) {
        runCatching { playFocusRequester.requestFocus() }
    }
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
            BoxWithConstraints(Modifier.fillMaxSize()) {
                val aspect = if (maxHeight.value > 0f) maxWidth.value / maxHeight.value else 1f
                val landscapeTvLayout = maxWidth >= 720.dp && aspect >= 1.35f
                val phoneLandscapeLayout = landscapeTvLayout && minOf(maxWidth, maxHeight) < PHONE_NAV_RAIL_MAX_SMALLEST_WIDTH
                if (landscapeTvLayout) {
                    GameDetailsLandscapeContent(
                        game = game,
                        favorite = favorite,
                        defaultVariantId = defaultVariantId,
                        onPlay = onPlay,
                        onChooseStore = onChooseStore,
                        onFavorite = onFavorite,
                        onDismiss = onDismiss,
                        playFocusRequester = playFocusRequester,
                        shortHeight = maxHeight <= 620.dp,
                        imageActionsOverlay = phoneLandscapeLayout,
                    )
                } else {
                    GameDetailsScrollableContent(
                        game = game,
                        favorite = favorite,
                        defaultVariantId = defaultVariantId,
                        onPlay = onPlay,
                        onChooseStore = onChooseStore,
                        onFavorite = onFavorite,
                        onDismiss = onDismiss,
                        playFocusRequester = playFocusRequester,
                    )
                }
            }
        }
    }
}

@OptIn(ExperimentalLayoutApi::class)
@Composable
private fun GameDetailsLandscapeContent(
    game: GameInfo,
    favorite: Boolean,
    defaultVariantId: String?,
    onPlay: (GameInfo) -> Unit,
    onChooseStore: (GameInfo) -> Unit,
    onFavorite: (String) -> Unit,
    onDismiss: () -> Unit,
    playFocusRequester: FocusRequester,
    shortHeight: Boolean,
    imageActionsOverlay: Boolean,
) {
    val description = gameDescriptionForDetails(game)
    val chips = gameDetailChips(game)
    val launchStores = displayStoresForVariants(game.variants).ifEmpty { game.availableStores }.map(::gameStoreDisplayName).distinct()
    val sideScrollState = rememberScrollState()
    Row(
        Modifier
            .fillMaxSize()
            .padding(horizontal = if (shortHeight) 18.dp else 24.dp, vertical = if (shortHeight) 16.dp else 22.dp),
        horizontalArrangement = Arrangement.spacedBy(if (shortHeight) 16.dp else 22.dp),
        verticalAlignment = Alignment.CenterVertically,
    ) {
        Box(
            Modifier
                .weight(0.92f)
                .fillMaxHeight()
                .clip(RoundedCornerShape(20.dp)),
        ) {
            UrlImage(game.screenshotUrl ?: game.imageUrl, Modifier.fillMaxSize())
            Box(Modifier.fillMaxSize().background(Color.Black.copy(alpha = 0.3f)))
            if (imageActionsOverlay) {
                ImageCloseButton(
                    onClick = onDismiss,
                    modifier = Modifier
                        .align(Alignment.TopStart)
                        .padding(10.dp),
                )
            }
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
                    .fillMaxWidth()
                    .padding(
                        start = if (shortHeight) 16.dp else 20.dp,
                        top = if (shortHeight) 16.dp else 20.dp,
                        end = if (imageActionsOverlay) 152.dp else if (shortHeight) 16.dp else 20.dp,
                        bottom = if (shortHeight) 16.dp else 20.dp,
                    ),
                verticalArrangement = Arrangement.spacedBy(6.dp),
            ) {
                Text(
                    game.title,
                    style = if (shortHeight) MaterialTheme.typography.titleLarge else MaterialTheme.typography.headlineSmall,
                    fontWeight = FontWeight.Bold,
                    maxLines = 2,
                    overflow = TextOverflow.Ellipsis,
                )
                Text(displayStoresForGame(game), color = TextMuted, maxLines = 1, overflow = TextOverflow.Ellipsis)
            }
            if (imageActionsOverlay) {
                LongPressPlayButton(
                    onClick = {
                        onDismiss()
                        onPlay(game)
                    },
                    onLongClick = {
                        onDismiss()
                        onChooseStore(game)
                    },
                    modifier = Modifier
                        .align(Alignment.BottomEnd)
                        .padding(14.dp)
                        .width(126.dp)
                        .focusRequester(playFocusRequester),
                )
            }
        }

        Column(
            Modifier
                .weight(1.08f)
                .fillMaxHeight()
                .then(if (imageActionsOverlay) Modifier.verticalScroll(sideScrollState) else Modifier),
            verticalArrangement = Arrangement.spacedBy(if (shortHeight) 8.dp else 10.dp),
        ) {
            Text(
                description ?: "No description is available for this game yet.",
                color = if (description == null) TextMuted else TextPrimary,
                style = MaterialTheme.typography.bodyMedium,
                lineHeight = if (shortHeight) MaterialTheme.typography.bodyMedium.lineHeight * 0.92f else MaterialTheme.typography.bodyMedium.lineHeight,
                maxLines = if (shortHeight) 6 else 8,
                overflow = TextOverflow.Ellipsis,
            )
            if (chips.isNotEmpty()) {
                FlowRow(horizontalArrangement = Arrangement.spacedBy(6.dp), verticalArrangement = Arrangement.spacedBy(4.dp)) {
                    chips.take(if (shortHeight) 8 else 12).forEach { label ->
                        AssistChip(onClick = {}, label = { Text(label, maxLines = 1, overflow = TextOverflow.Ellipsis) })
                    }
                }
            }
            CompactDetailRows(game)
            if (launchStores.isNotEmpty()) {
                Text(
                    "Launchers: ${launchStores.joinToString(", ")}",
                    color = TextMuted,
                    style = MaterialTheme.typography.bodySmall,
                    maxLines = 2,
                    overflow = TextOverflow.Ellipsis,
                )
            }
            LaunchOptionsList(
                game = game,
                defaultVariantId = defaultVariantId,
                compact = true,
            )
            if (!imageActionsOverlay) {
                Spacer(Modifier.weight(1f))
                Row(
                    Modifier.fillMaxWidth(),
                    horizontalArrangement = Arrangement.spacedBy(10.dp),
                    verticalAlignment = Alignment.CenterVertically,
                ) {
                    OutlinedButton(onClick = onDismiss, modifier = Modifier.weight(0.8f)) {
                        Text("Dismiss", maxLines = 1, overflow = TextOverflow.Ellipsis)
                    }
                    LongPressPlayButton(
                        onClick = {
                            onDismiss()
                            onPlay(game)
                        },
                        onLongClick = {
                            onDismiss()
                            onChooseStore(game)
                        },
                        modifier = Modifier
                            .weight(1.2f)
                            .focusRequester(playFocusRequester),
                    )
                }
            }
        }
    }
}

@OptIn(ExperimentalLayoutApi::class)
@Composable
private fun GameDetailsScrollableContent(
    game: GameInfo,
    favorite: Boolean,
    defaultVariantId: String?,
    onPlay: (GameInfo) -> Unit,
    onChooseStore: (GameInfo) -> Unit,
    onFavorite: (String) -> Unit,
    onDismiss: () -> Unit,
    playFocusRequester: FocusRequester,
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
                    val description = gameDescriptionForDetails(game)
                    if (description != null) {
                        Text(description, color = TextPrimary, style = MaterialTheme.typography.bodyMedium)
                    } else {
                        Text("No description is available for this game yet.", color = TextMuted)
                    }
                    val chips = gameDetailChips(game)
                    if (chips.isNotEmpty()) {
                        FlowRow(horizontalArrangement = Arrangement.spacedBy(7.dp), verticalArrangement = Arrangement.spacedBy(7.dp)) {
                            chips.take(12).forEach { label ->
                                AssistChip(onClick = {}, label = { Text(label, maxLines = 1, overflow = TextOverflow.Ellipsis) })
                            }
                        }
                    }
                    DetailRows(game)
                    LaunchOptionsList(
                        game = game,
                        defaultVariantId = defaultVariantId,
                        compact = false,
                    )
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
                LongPressPlayButton(
                    onClick = {
                        onDismiss()
                        onPlay(game)
                    },
                    onLongClick = {
                        onDismiss()
                        onChooseStore(game)
                    },
                    modifier = Modifier
                        .weight(1.2f)
                        .focusRequester(playFocusRequester),
                )
            }
        }
    }
}

@Composable
private fun LaunchOptionsList(
    game: GameInfo,
    defaultVariantId: String?,
    compact: Boolean,
) {
    val variants = launchableGameVariants(game.variants)
    if (variants.size <= 1) return
    Column(verticalArrangement = Arrangement.spacedBy(if (compact) 6.dp else 8.dp)) {
        Text(
            stringResource(R.string.store_selector_launchers),
            color = TextMuted,
            style = MaterialTheme.typography.labelMedium,
            fontWeight = FontWeight.Bold,
        )
        variants.take(if (compact) 3 else variants.size).forEach { variant ->
            val isDefault = variant.id == defaultVariantId
            Surface(
                modifier = Modifier.fillMaxWidth(),
                shape = RoundedCornerShape(if (compact) 12.dp else 14.dp),
                color = if (isDefault) MaterialTheme.colorScheme.primary.copy(alpha = 0.18f) else PanelAlt,
            ) {
                Row(
                    Modifier.padding(horizontal = if (compact) 10.dp else 12.dp, vertical = if (compact) 8.dp else 10.dp),
                    verticalAlignment = Alignment.CenterVertically,
                    horizontalArrangement = Arrangement.spacedBy(10.dp),
                ) {
                    Column(Modifier.weight(1f)) {
                        Text(gameStoreDisplayName(variant.store), fontWeight = FontWeight.SemiBold, maxLines = 1, overflow = TextOverflow.Ellipsis)
                        val details = variantDetailsText(variant)
                        Text(
                            if (isDefault) {
                                listOf(stringResource(R.string.store_selector_default), details).filter { it.isNotBlank() }.joinToString(" - ")
                            } else {
                                details.ifBlank { stringResource(R.string.store_selector_available_launcher) }
                            },
                            color = TextMuted,
                            style = MaterialTheme.typography.bodySmall,
                            maxLines = if (compact) 1 else 2,
                            overflow = TextOverflow.Ellipsis,
                        )
                    }
                }
            }
        }
    }
}

@OptIn(ExperimentalFoundationApi::class)
@Composable
private fun LongPressPlayButton(
    onClick: () -> Unit,
    onLongClick: () -> Unit,
    modifier: Modifier = Modifier,
) {
    var focused by remember { mutableStateOf(false) }
    val shape = RoundedCornerShape(999.dp)
    Surface(
        modifier = modifier
            .height(48.dp)
            .onFocusChanged { focused = it.isFocused }
            .onPreviewKeyEvent { event ->
                if (isTvActivateKey(event)) {
                    onClick()
                    true
                } else {
                    false
                }
            }
            .focusable()
            .combinedClickable(
                onClick = onClick,
                onLongClick = onLongClick,
                onLongClickLabel = stringResource(R.string.store_selector_play_long_press),
            )
            .border(
                width = if (focused) 2.dp else 0.dp,
                color = if (focused) TextPrimary else Color.Transparent,
                shape = shape,
            ),
        shape = shape,
        color = MaterialTheme.colorScheme.primary,
        tonalElevation = 2.dp,
    ) {
        Box(Modifier.fillMaxSize(), contentAlignment = Alignment.Center) {
            Text(
                stringResource(R.string.action_play),
                color = MaterialTheme.colorScheme.onPrimary,
                fontWeight = FontWeight.SemiBold,
                maxLines = 1,
                overflow = TextOverflow.Ellipsis,
            )
        }
    }
}

private fun variantDetailsText(variant: GameVariant): String =
    listOfNotNull(
        variant.libraryStatus?.takeIf { it.isNotBlank() }?.let(::formatGameMetadataLabel),
        variant.supportedControls.takeIf { it.isNotEmpty() }?.joinToString(", ") { formatGameMetadataLabel(it) },
        variant.lastPlayedDate?.takeIf { it.isNotBlank() }?.let { "Last played $it" },
    ).joinToString(" - ")

@Composable
private fun ImageCloseButton(onClick: () -> Unit, modifier: Modifier = Modifier) {
    Surface(
        modifier = modifier.size(44.dp),
        shape = CircleShape,
        color = Color.Black.copy(alpha = 0.58f),
        tonalElevation = 3.dp,
    ) {
        IconButton(onClick = onClick) {
            Icon(
                painter = painterResource(R.drawable.ic_clear),
                contentDescription = stringResource(R.string.action_cancel),
                tint = TextPrimary,
                modifier = Modifier.size(20.dp),
            )
        }
    }
}

@Composable
private fun FavoriteIconButton(favorite: Boolean, onClick: () -> Unit, modifier: Modifier = Modifier) {
    val label = stringResource(if (favorite) R.string.action_saved else R.string.action_save)
    Surface(
        modifier = modifier.size(44.dp),
        shape = CircleShape,
        color = Color.Black.copy(alpha = 0.52f),
        tonalElevation = 3.dp,
    ) {
        IconButton(onClick = onClick) {
            Icon(
                painter = painterResource(if (favorite) R.drawable.ic_save_filled else R.drawable.ic_save),
                contentDescription = label,
                tint = if (favorite) MaterialTheme.colorScheme.primary else TextPrimary,
                modifier = Modifier.size(22.dp),
            )
        }
    }
}

private fun gameDescriptionForDetails(game: GameInfo): String? =
    game.longDescription?.takeIf { it.isNotBlank() }
        ?: game.description?.takeIf { it.isNotBlank() }

private fun gameDetailChips(game: GameInfo): List<String> =
    (game.featureLabels + game.genres + listOfNotNull(game.playType, game.membershipTierLabel))
        .map { it.trim() }
        .filter { it.isNotBlank() }
        .map(::formatGameMetadataLabel)
        .filterNot(::isNoisyGameTag)
        .distinctBy { it.lowercase(Locale.US) }
        .take(10)

private fun formatGameMetadataLabel(raw: String): String {
    val compact = raw.trim()
        .removePrefix("GFN_")
        .removePrefix("GAME_")
        .replace(Regex("[_-]+"), " ")
        .replace(Regex("\\s+"), " ")
        .trim()
    if (compact.isBlank()) return ""
    val lower = compact.lowercase(Locale.US)
    return when (lower) {
        "full game" -> "Full game"
        "single player" -> "Single-player"
        "multi player", "multiplayer" -> "Multiplayer"
        "controller", "gamepad" -> "Controller"
        "keyboard mouse", "mouse keyboard" -> "Mouse and keyboard"
        else -> compact.split(" ").joinToString(" ") { word ->
            if (word.length <= 3 && word.all { it.isUpperCase() || it.isDigit() }) {
                word
            } else {
                word.lowercase(Locale.US).replaceFirstChar { char -> char.titlecase(Locale.US) }
            }
        }
    }
}

private fun isNoisyGameTag(label: String): Boolean {
    val normalized = label.trim().lowercase(Locale.US)
    return normalized.isBlank() ||
        normalized == "unknown" ||
        normalized == "gfn" ||
        normalized == "nvidia" ||
        normalized.contains("sku based tag") ||
        normalized.contains("catalog")
}

@Composable
private fun CompactDetailRows(game: GameInfo) {
    val rows = listOfNotNull(
        game.publisherName?.takeIf { it.isNotBlank() }?.let { "Publisher" to it },
        game.playabilityState?.takeIf { it.isNotBlank() }?.let { "Status" to it },
        game.contentRatings.takeIf { it.isNotEmpty() }?.joinToString(", ")?.let { "Rating" to it },
        game.lastPlayed?.takeIf { it.isNotBlank() }?.let { "Last played" to it },
        game.availableStores.takeIf { it.isNotEmpty() }?.map(::gameStoreDisplayName)?.distinct()?.joinToString(", ")?.let { "Stores" to it },
    ).take(3)
    if (rows.isEmpty()) return
    Column(verticalArrangement = Arrangement.spacedBy(6.dp)) {
        rows.forEach { (label, value) ->
            Row(
                Modifier
                    .fillMaxWidth()
                    .clip(RoundedCornerShape(10.dp))
                    .background(PanelAlt)
                    .padding(horizontal = 10.dp, vertical = 7.dp),
                verticalAlignment = Alignment.Top,
                horizontalArrangement = Arrangement.spacedBy(10.dp),
            ) {
                Text(label, color = TextMuted, style = MaterialTheme.typography.bodySmall, modifier = Modifier.width(82.dp), maxLines = 1, overflow = TextOverflow.Ellipsis)
                Text(value, color = TextPrimary, style = MaterialTheme.typography.bodySmall, modifier = Modifier.weight(1f), maxLines = 1, overflow = TextOverflow.Ellipsis)
            }
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

private fun favoriteOrderedGames(games: List<GameInfo>, favoriteIds: List<String>): List<GameInfo> {
    val favorites = games.filter { it.id in favoriteIds }
    return if (favorites.isNotEmpty()) favorites + games.filterNot { it.id in favoriteIds } else games
}

@Composable
@OptIn(ExperimentalLayoutApi::class)
private fun StoreLaunchSelector(
    game: GameInfo,
    defaultVariantId: String?,
    onLaunch: (GameInfo, GameVariant) -> Unit,
    onSetDefaultStore: (String, String?) -> Unit,
    onDismiss: () -> Unit,
    modifier: Modifier = Modifier,
) {
    val variants = remember(game) { launchableGameVariants(game.variants) }
    val context = LocalContext.current
    val initialVariantId = remember(game.id, defaultVariantId, variants) {
        defaultVariantId?.takeIf { savedId -> variants.any { it.id == savedId } }
            ?: variants.firstOrNull()?.id
    }
    var selectedVariantId by remember(game.id, initialVariantId) { mutableStateOf(initialVariantId) }
    var rememberDefaultStore by remember(game.id, defaultVariantId) { mutableStateOf(defaultVariantId != null) }
    val selectedVariant = variants.firstOrNull { it.id == selectedVariantId }
    val continueFocusRequester = remember(game.id) { FocusRequester() }
    BackHandler(onBack = onDismiss)
    LaunchedEffect(game.id, variants.size) {
        if (variants.isNotEmpty()) {
            runCatching { continueFocusRequester.requestFocus() }
        }
    }
    BoxWithConstraints(
        Modifier
            .fillMaxSize()
            .background(Color.Black.copy(alpha = 0.72f))
            .clickable(enabled = false) {},
    ) {
        val phoneLandscape = isPhoneLandscape(maxWidth, maxHeight)
        val landscape = maxWidth > maxHeight
        Box(
            Modifier.fillMaxSize(),
            contentAlignment = if (phoneLandscape) Alignment.CenterEnd else Alignment.Center,
        ) {
            Card(
                modifier = modifier
                    .then(
                        if (phoneLandscape) {
                            Modifier
                                .padding(end = 12.dp)
                                .fillMaxWidth(0.9f)
                                .fillMaxHeight(0.9f)
                        } else {
                            Modifier
                                .fillMaxWidth(if (landscape) 0.78f else 0.92f)
                                .fillMaxHeight(if (landscape) 0.86f else 0.64f)
                        },
                    ),
                colors = CardDefaults.cardColors(containerColor = Panel),
                shape = RoundedCornerShape(22.dp),
            ) {
                if (phoneLandscape) {
                    Row(
                        Modifier.fillMaxSize().padding(14.dp),
                        horizontalArrangement = Arrangement.spacedBy(14.dp),
                        verticalAlignment = Alignment.CenterVertically,
                    ) {
                        LaunchGameSummary(
                            game = game,
                            subtitle = stringResource(R.string.store_selector_choose_launcher),
                            modifier = Modifier
                                .width(190.dp)
                                .fillMaxHeight(),
                        )
                        StoreLaunchOptionsColumn(
                            variants = variants,
                            selectedVariantId = selectedVariantId,
                            defaultVariantId = defaultVariantId,
                            rememberDefaultStore = rememberDefaultStore,
                            selectedVariant = selectedVariant,
                            continueFocusRequester = continueFocusRequester,
                            onSelectVariant = { selectedVariantId = it },
                            onRememberDefaultStoreChange = { rememberDefaultStore = it },
                            onDismiss = onDismiss,
                            onContinue = { variant ->
                                if (rememberDefaultStore || defaultVariantId != null) {
                                    onSetDefaultStore(game.id, if (rememberDefaultStore) variant.id else null)
                                }
                                if (rememberDefaultStore) {
                                    Toast.makeText(context, context.getString(R.string.store_selector_long_press_tip), Toast.LENGTH_LONG).show()
                                }
                                onLaunch(game, variant)
                            },
                            modifier = Modifier
                                .weight(1f)
                                .fillMaxHeight(),
                        )
                    }
                } else {
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
                                Text(stringResource(R.string.store_selector_choose_launcher), color = TextMuted, style = MaterialTheme.typography.bodySmall)
                            }
                        }
                        StoreLaunchOptionsColumn(
                            variants = variants,
                            selectedVariantId = selectedVariantId,
                            defaultVariantId = defaultVariantId,
                            rememberDefaultStore = rememberDefaultStore,
                            selectedVariant = selectedVariant,
                            continueFocusRequester = continueFocusRequester,
                            onSelectVariant = { selectedVariantId = it },
                            onRememberDefaultStoreChange = { rememberDefaultStore = it },
                            onDismiss = onDismiss,
                            onContinue = { variant ->
                                if (rememberDefaultStore || defaultVariantId != null) {
                                    onSetDefaultStore(game.id, if (rememberDefaultStore) variant.id else null)
                                }
                                if (rememberDefaultStore) {
                                    Toast.makeText(context, context.getString(R.string.store_selector_long_press_tip), Toast.LENGTH_LONG).show()
                                }
                                onLaunch(game, variant)
                            },
                            modifier = Modifier.weight(1f),
                        )
                    }
                }
            }
        }
    }
}

@Composable
private fun LaunchGameSummary(game: GameInfo, subtitle: String, modifier: Modifier = Modifier) {
    Column(modifier, verticalArrangement = Arrangement.spacedBy(10.dp)) {
        UrlImage(
            game.imageUrl,
            Modifier
                .fillMaxWidth()
                .weight(1f)
                .clip(RoundedCornerShape(16.dp)),
        )
        Column(verticalArrangement = Arrangement.spacedBy(3.dp)) {
            Text(game.title, fontWeight = FontWeight.Bold, maxLines = 2, overflow = TextOverflow.Ellipsis)
            Text(subtitle, color = TextMuted, style = MaterialTheme.typography.bodySmall, maxLines = 2, overflow = TextOverflow.Ellipsis)
        }
    }
}

@Composable
private fun StoreLaunchOptionsColumn(
    variants: List<GameVariant>,
    selectedVariantId: String?,
    defaultVariantId: String?,
    rememberDefaultStore: Boolean,
    selectedVariant: GameVariant?,
    continueFocusRequester: FocusRequester,
    onSelectVariant: (String) -> Unit,
    onRememberDefaultStoreChange: (Boolean) -> Unit,
    onDismiss: () -> Unit,
    onContinue: (GameVariant) -> Unit,
    modifier: Modifier = Modifier,
) {
    Column(modifier, verticalArrangement = Arrangement.spacedBy(10.dp)) {
        LazyColumn(
            modifier = Modifier.weight(1f),
            verticalArrangement = Arrangement.spacedBy(8.dp),
        ) {
            items(variants, key = { it.id }) { variant ->
                StoreLaunchVariantRow(
                    variant = variant,
                    selected = variant.id == selectedVariantId,
                    savedDefault = variant.id == defaultVariantId,
                    onClick = { onSelectVariant(variant.id) },
                )
            }
        }
        Row(
            modifier = Modifier
                .fillMaxWidth()
                .clip(RoundedCornerShape(14.dp))
                .clickable { onRememberDefaultStoreChange(!rememberDefaultStore) }
                .padding(vertical = 2.dp),
            verticalAlignment = Alignment.CenterVertically,
            horizontalArrangement = Arrangement.spacedBy(8.dp),
        ) {
            Checkbox(
                checked = rememberDefaultStore,
                onCheckedChange = onRememberDefaultStoreChange,
            )
            Text(
                stringResource(R.string.store_selector_default_checkbox),
                color = TextPrimary,
                style = MaterialTheme.typography.bodySmall,
                modifier = Modifier.weight(1f),
            )
        }
        Row(
            Modifier.fillMaxWidth(),
            horizontalArrangement = Arrangement.spacedBy(10.dp),
            verticalAlignment = Alignment.CenterVertically,
        ) {
            TextButton(onClick = onDismiss, modifier = Modifier.weight(0.85f)) {
                Text(stringResource(R.string.action_cancel))
            }
            Button(
                onClick = {
                    val variant = selectedVariant ?: return@Button
                    onContinue(variant)
                },
                enabled = selectedVariant != null,
                modifier = Modifier
                    .weight(1.15f)
                    .focusRequester(continueFocusRequester),
            ) {
                Text(stringResource(R.string.action_continue), maxLines = 1, overflow = TextOverflow.Ellipsis)
            }
        }
    }
}

@Composable
private fun StoreLaunchVariantRow(
    variant: GameVariant,
    selected: Boolean,
    savedDefault: Boolean,
    onClick: () -> Unit,
) {
    val badge = launcherBadgeForStoreKey(splitGameStoreKeys(variant.store).firstOrNull())
    Surface(
        modifier = Modifier
            .fillMaxWidth()
            .clickable { onClick() },
        shape = RoundedCornerShape(14.dp),
        color = if (selected) MaterialTheme.colorScheme.primary.copy(alpha = 0.18f) else PanelAlt,
    ) {
        Row(
            Modifier.padding(14.dp),
            verticalAlignment = Alignment.CenterVertically,
            horizontalArrangement = Arrangement.spacedBy(12.dp),
        ) {
            ConnectorStoreIcon(badge)
            Column(Modifier.weight(1f), verticalArrangement = Arrangement.spacedBy(2.dp)) {
                Text(gameStoreDisplayName(variant.store), fontWeight = FontWeight.Bold, maxLines = 1, overflow = TextOverflow.Ellipsis)
                val details = listOf(
                    if (savedDefault) stringResource(R.string.store_selector_default) else "",
                    variantDetailsText(variant),
                ).filter { it.isNotBlank() }.joinToString(" - ")
                if (details.isNotBlank()) {
                    Text(details, color = TextMuted, style = MaterialTheme.typography.bodySmall, maxLines = 1, overflow = TextOverflow.Ellipsis)
                }
            }
            if (selected) {
                Text(
                    stringResource(R.string.store_selector_selected),
                    color = MaterialTheme.colorScheme.primary,
                    style = MaterialTheme.typography.labelLarge,
                    fontWeight = FontWeight.Bold,
                    maxLines = 1,
                )
            }
        }
    }
}

@Composable
private fun StreamScreen(state: OpenNowUiState, viewModel: OpenNowViewModel) {
    val context = LocalContext.current
    val activity = context as? Activity
    val audioController = remember(context) { AndroidNerdAudioController(context.applicationContext) }
    val session = state.streamSession
    val game = state.streamGame
    var streamState by remember { mutableStateOf("Preparing") }
    var controlsOpen by remember { mutableStateOf(false) }
    var exitConfirmOpen by remember { mutableStateOf(false) }
    var keyboardOpen by remember { mutableStateOf(false) }
    var keyboardText by remember { mutableStateOf("") }
    var audioMuted by remember { mutableStateOf(false) }
    var touchLayoutEditing by remember { mutableStateOf(false) }
    var streamGuideOpen by remember(session?.sessionId) { mutableStateOf(false) }
    var statsVisible by remember(state.settings.showStatsOnLaunch) { mutableStateOf(state.settings.showStatsOnLaunch) }
    var streamStats by remember { mutableStateOf(StreamRuntimeStats()) }
    val streamReady = session?.isReadyForStream() == true
    val nerdMode = state.settings.nerdMode
    val buttonToneEnabled = nerdMode && state.settings.controllerUiSounds
    val stretchToFill = state.settings.stretchStreamToFill
    val playButtonTone = {
        audioController.playButtonTone(buttonToneEnabled)
    }
    val launchStreamSettings = state.activeStreamSettings ?: state.settings.stream
    val streamSettings = launchStreamSettings.copy(
        mouseSensitivity = state.settings.stream.mouseSensitivity,
        mouseAcceleration = state.settings.stream.mouseAcceleration,
        streamSharpeningEnabled = state.settings.stream.streamSharpeningEnabled,
        streamSharpeningAmount = state.settings.stream.streamSharpeningAmount,
    )
    val dismissStreamGuide = {
        streamGuideOpen = false
        if (!state.settings.androidStreamGuideDismissed) {
            viewModel.updateSettings(state.settings.copy(androidStreamGuideDismissed = true))
        }
    }
    val streamOverlayOpen = controlsOpen || exitConfirmOpen || keyboardOpen || streamGuideOpen
    val externalMousePassthroughActive = streamReady && !streamOverlayOpen
    val handleStreamBack = {
        when {
            streamGuideOpen -> dismissStreamGuide()
            exitConfirmOpen -> exitConfirmOpen = false
            keyboardOpen -> keyboardOpen = false
            controlsOpen -> controlsOpen = false
            else -> controlsOpen = true
        }
    }
    BackHandler(enabled = streamReady) {
        handleStreamBack()
    }
    val client = remember {
        NativeStreamClient(
            context = context.applicationContext,
            onState = {
                streamState = it
                viewModel.recordNativeStreamState(it)
                if (it == "Streaming") viewModel.markStreamConnected()
            },
            onError = {
                streamState = it
                viewModel.markStreamError(it)
            },
            onSafeVideoFallbackRequired = {
                streamState = it
                viewModel.restartStreamWithSafeVideoProfile(it)
            },
            onSessionRecoveryRequired = {
                streamState = it
                viewModel.recoverStreamSession(it)
            },
            onStats = { streamStats = it },
        )
    }

    DisposableEffect(Unit) {
        val window = activity?.window
        val decor = activity?.window?.decorView
        val oldStatusBarColor = window?.statusBarColor
        val oldNavigationBarColor = window?.navigationBarColor
        window?.statusBarColor = AndroidColor.BLACK
        window?.navigationBarColor = AndroidColor.BLACK
        NativeStreamInputRouter.attach(client)
        onDispose {
            if (oldStatusBarColor != null) window.statusBarColor = oldStatusBarColor
            if (oldNavigationBarColor != null) window.navigationBarColor = oldNavigationBarColor
            if (Build.VERSION.SDK_INT >= 26) {
                decor?.releasePointerCapture()
            }
            NativeStreamInputRouter.clearUiTouchPassthroughBounds()
            NativeStreamInputRouter.clearStreamPanelTouchPassthroughBounds()
            NativeStreamInputRouter.setSystemMenuHandler(null)
            NativeStreamInputRouter.setSystemBackHandler(null)
            NativeStreamInputRouter.setStreamUiActive(false)
            NativeStreamInputRouter.detach(client)
            client.release()
        }
    }
    DisposableEffect(audioController) {
        onDispose {
            audioController.release()
        }
    }

    LaunchedEffect(streamReady, streamOverlayOpen) {
        NativeStreamInputRouter.setStreamUiActive(streamReady && streamOverlayOpen)
        NativeStreamInputRouter.setSystemMenuHandler {
            keyboardOpen = false
            exitConfirmOpen = false
            if (streamGuideOpen) {
                dismissStreamGuide()
            } else {
                streamGuideOpen = false
            }
            controlsOpen = true
        }
        NativeStreamInputRouter.setSystemBackHandler {
            handleStreamBack()
        }
    }

    LaunchedEffect(streamReady, state.settings.androidStreamGuideDismissed, session?.sessionId) {
        streamGuideOpen = streamReady && !state.settings.androidStreamGuideDismissed
    }

    LaunchedEffect(streamReady, state.settings.androidTouch.mousePad) {
        NativeStreamInputRouter.setTouchMouseEnabled(streamReady && state.settings.androidTouch.mousePad)
    }
    LaunchedEffect(streamReady, state.settings.androidTouch.mousePad, controlsOpen, exitConfirmOpen, keyboardOpen, streamGuideOpen, state.settings.androidTouch.enabled) {
        NativeStreamInputRouter.setCaptureAllTouch(
            streamReady &&
                state.settings.androidTouch.mousePad &&
                !controlsOpen &&
                !exitConfirmOpen &&
                !keyboardOpen &&
                !streamGuideOpen,
        )
    }
    DisposableEffect(Unit) {
        onDispose {
            NativeStreamInputRouter.setCaptureAllTouch(false)
        }
    }
    LaunchedEffect(state.settings.phoneRumbleFallback) {
        client.updateHapticsSettings(state.settings.phoneRumbleFallback)
    }
    LaunchedEffect(session?.sessionId, session?.status, launchStreamSettings) {
        if (session != null && streamReady) {
            client.start(session, launchStreamSettings)
        }
    }

    Box(Modifier.fillMaxSize().background(Color.Black)) {
        if (session == null && state.streamStatus != "idle") {
            QueueLoadingScreen(state, viewModel)
        } else if (session == null) {
            NoActiveStreamScreen(
                canResumeSession = state.activeSession != null,
                canEndSession = state.authSession != null,
                onBack = { viewModel.setPage(AppPage.Home) },
                onResumeSession = viewModel::resumeActiveSession,
                onEndSession = viewModel::stopStream,
            )
        } else if (!streamReady) {
            QueueLoadingScreen(state, viewModel)
        } else {
            StreamVideoSurface(
                client = client,
                settings = streamSettings,
                hideExternalMousePointer = externalMousePassthroughActive,
                touchMouseEnabled = state.settings.androidTouch.mousePad,
                externalMouseRoot = activity?.window?.decorView,
                onMouseCaptureInput = { (activity as? MainActivity)?.enforceStreamSystemUiFromInput() },
                stretchToFill = stretchToFill,
            )
            if (statsVisible) {
                StreamStatsPill(
                    gameTitle = game?.title ?: "Stream",
                    status = streamState,
                    streamStats = streamStats,
                    audioMuted = audioMuted,
                    style = state.settings.streamStatsStyle,
                    modifier = Modifier.align(Alignment.TopStart),
                )
            }
            if (nerdMode) {
                NerdStreamStatusOverlay(
                    codecLabel = streamStats.codec ?: streamSettings.codec.name,
                    stretchToFill = stretchToFill,
                    modifier = Modifier.align(Alignment.TopEnd),
                )
            }
            if (state.settings.androidTouch.enabled) {
                TouchOverlay(
                    client = client,
                    touch = state.settings.androidTouch,
                    onButtonTone = playButtonTone,
                    layoutEditing = touchLayoutEditing,
                    onLeftOffsetChange = { x, y ->
                        viewModel.updateSettings(
                            state.settings.copy(
                                androidTouch = state.settings.androidTouch.copy(leftOffsetXDp = x, leftOffsetYDp = y),
                            ),
                        )
                    },
                    onRightOffsetChange = { x, y ->
                        viewModel.updateSettings(
                            state.settings.copy(
                                androidTouch = state.settings.androidTouch.copy(rightOffsetXDp = x, rightOffsetYDp = y),
                            ),
                        )
                    },
                    modifier = Modifier.align(Alignment.BottomCenter),
                )
            }
            if (streamGuideOpen) {
                AnimatedLaunchOverlay(Modifier.align(Alignment.Center)) {
                    StreamFirstLaunchGuide(
                        gameTitle = game?.title ?: "your game",
                        touchControlsEnabled = state.settings.androidTouch.enabled,
                        onOpenControls = {
                            playButtonTone()
                            dismissStreamGuide()
                            controlsOpen = true
                        },
                        onDismiss = {
                            playButtonTone()
                            dismissStreamGuide()
                        },
                    )
                }
            }
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
                    touchLayoutEditing = touchLayoutEditing,
                    keyboardOpen = keyboardOpen,
                    onAudioToggle = {
                        audioMuted = !audioMuted
                        client.setAudioMuted(audioMuted)
                    },
                    onStatsToggle = {
                        statsVisible = !statsVisible
                        viewModel.updateSettings(state.settings.copy(showStatsOnLaunch = statsVisible))
                    },
                    onStatsStyleCycle = {
                        viewModel.updateSettings(state.settings.copy(streamStatsStyle = state.settings.streamStatsStyle.next()))
                    },
                    onPhoneRumbleFallbackToggle = {
                        viewModel.updateSettings(state.settings.copy(phoneRumbleFallback = !state.settings.phoneRumbleFallback))
                    },
                    onTouchLayoutEditingToggle = {
                        touchLayoutEditing = !touchLayoutEditing
                    },
                    onKeyboardToggle = { keyboardOpen = !keyboardOpen },
                    onEsc = { client.sendKeyCode(KeyEvent.KEYCODE_ESCAPE) },
                    onEnter = { client.sendKeyCode(KeyEvent.KEYCODE_ENTER) },
                    onBackspace = { client.sendKeyCode(KeyEvent.KEYCODE_DEL) },
                    onExit = {
                        controlsOpen = false
                        exitConfirmOpen = true
                    },
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
                    onSharpeningToggle = {
                        viewModel.updateStreamSettings { settings ->
                            settings.copy(streamSharpeningEnabled = !settings.streamSharpeningEnabled)
                        }
                    },
                    onSharpeningAmountChange = { value ->
                        viewModel.updateStreamSettings { settings ->
                            settings.copy(streamSharpeningAmount = value)
                        }
                    },
                    onStretchToFillToggle = {
                        viewModel.updateSettings(state.settings.copy(stretchStreamToFill = !state.settings.stretchStreamToFill))
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
                    onTouchEdgePaddingChange = { value ->
                        viewModel.updateSettings(state.settings.copy(androidTouch = state.settings.androidTouch.copy(edgePaddingDp = value)))
                    },
                    onTouchBottomPaddingChange = { value ->
                        viewModel.updateSettings(state.settings.copy(androidTouch = state.settings.androidTouch.copy(bottomPaddingDp = value)))
                    },
                    onTouchLeftOffsetChange = { value ->
                        viewModel.updateSettings(state.settings.copy(androidTouch = state.settings.androidTouch.copy(leftOffsetYDp = value)))
                    },
                    onTouchRightOffsetChange = { value ->
                        viewModel.updateSettings(state.settings.copy(androidTouch = state.settings.androidTouch.copy(rightOffsetYDp = value)))
                    },
                    onButtonTone = playButtonTone,
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

@Composable
private fun NerdStreamStatusOverlay(
    codecLabel: String,
    stretchToFill: Boolean,
    modifier: Modifier = Modifier,
) {
    Column(
        modifier.padding(top = 8.dp, end = 8.dp),
        horizontalAlignment = Alignment.End,
        verticalArrangement = Arrangement.spacedBy(6.dp),
    ) {
        Surface(
            shape = RoundedCornerShape(999.dp),
            color = Panel.copy(alpha = 0.52f),
            tonalElevation = 0.dp,
        ) {
            Row(
                Modifier.padding(horizontal = 9.dp, vertical = 5.dp),
                horizontalArrangement = Arrangement.spacedBy(7.dp),
                verticalAlignment = Alignment.CenterVertically,
            ) {
                Text(
                    "Codec ${codecLabel.uppercase(Locale.US)}",
                    color = TextPrimary,
                    style = MaterialTheme.typography.labelSmall,
                    maxLines = 1,
                    overflow = TextOverflow.Ellipsis,
                )
                if (stretchToFill) {
                    Text(
                        "Fill",
                        color = MaterialTheme.colorScheme.primary,
                        style = MaterialTheme.typography.labelSmall,
                        fontWeight = FontWeight.Bold,
                    )
                }
            }
        }
    }
}

@Composable
private fun StreamVideoSurface(
    client: NativeStreamClient,
    settings: StreamSettings,
    hideExternalMousePointer: Boolean,
    touchMouseEnabled: Boolean,
    externalMouseRoot: android.view.View?,
    onMouseCaptureInput: () -> Unit,
    stretchToFill: Boolean,
    modifier: Modifier = Modifier,
) {
    val rootView = LocalView.current
    val configuration = LocalConfiguration.current
    val pointerRootView = externalMouseRoot ?: rootView
    val (streamWidth, streamHeight) = streamResolutionPixels(settings)
    val streamAspect = (streamWidth.toFloat() / streamHeight.toFloat()).takeIf { it.isFinite() && it > 0f } ?: (16f / 9f)
    val currentOnMouseCaptureInput by rememberUpdatedState(onMouseCaptureInput)
    var zoomScale by remember { mutableFloatStateOf(1f) }
    var zoomOffset by remember { mutableStateOf(Offset.Zero) }
    var viewportSize by remember { mutableStateOf(IntSize.Zero) }
    LaunchedEffect(
        settings.resolution,
        settings.aspectRatio,
        stretchToFill,
        configuration.orientation,
        configuration.screenWidthDp,
        configuration.screenHeightDp,
    ) {
        zoomScale = 1f
        zoomOffset = Offset.Zero
    }
    DisposableEffect(client, rootView, pointerRootView, hideExternalMousePointer) {
        pointerRootView.configureAndroidMousePointerCapture(hideExternalMousePointer, { currentOnMouseCaptureInput() }) { event ->
            client.dispatchMotion(event)
        }
        if (hideExternalMousePointer) {
            pointerRootView.hideAndroidPointerTree()
        } else {
            pointerRootView.showAndroidPointerTree()
        }
        onDispose {
            pointerRootView.clearAndroidMousePointerCapture()
            pointerRootView.showAndroidPointerTree()
        }
    }
    BoxWithConstraints(
        modifier
            .fillMaxSize()
            .background(Color.Black),
        contentAlignment = Alignment.Center,
    ) {
        val containerAspect = if (maxHeight.value > 0f) maxWidth.value / maxHeight.value else streamAspect
        val cropUltrawideToPhoneViewport =
            !stretchToFill &&
                isPhoneLandscape(maxWidth, maxHeight) &&
                streamAspect >= PHONE_ULTRAWIDE_MIN_STREAM_ASPECT &&
                containerAspect >= PHONE_ULTRAWIDE_MIN_VIEWPORT_ASPECT &&
                streamAspect > containerAspect
        val viewportModifier = when {
            stretchToFill -> Modifier.fillMaxSize()
            cropUltrawideToPhoneViewport || containerAspect > streamAspect -> Modifier
                .fillMaxHeight()
                .aspectRatio(streamAspect)
            else -> Modifier
                .fillMaxWidth()
                .aspectRatio(streamAspect)
        }
        Box(
            viewportModifier
                .onSizeChanged {
                    if (viewportSize != it) {
                        viewportSize = it
                        zoomScale = 1f
                        zoomOffset = Offset.Zero
                    } else {
                        zoomOffset = clampStreamZoomOffset(zoomOffset, zoomScale, it)
                    }
                }
                .clipToBounds()
                .background(Color.Black),
        ) {
            val stretchedRendererModifier = when {
                !stretchToFill -> Modifier.matchParentSize()
                containerAspect > streamAspect -> Modifier
                    .fillMaxHeight()
                    .aspectRatio(streamAspect)
                    .align(Alignment.Center)
                    .graphicsLayer {
                        scaleX = (containerAspect / streamAspect).takeIf { it.isFinite() && it > 0f } ?: 1f
                    }
                else -> Modifier
                    .fillMaxWidth()
                    .aspectRatio(streamAspect)
                    .align(Alignment.Center)
                    .graphicsLayer {
                        scaleY = (streamAspect / containerAspect).takeIf { it.isFinite() && it > 0f } ?: 1f
                    }
            }
            Box(
                Modifier
                    .matchParentSize()
                    .graphicsLayer {
                        scaleX = zoomScale
                        scaleY = zoomScale
                        translationX = zoomOffset.x
                        translationY = zoomOffset.y
                    },
            ) {
                key(settings.streamSharpeningEnabled, viewportSize.width, viewportSize.height, stretchToFill) {
                    AndroidView(
                        modifier = stretchedRendererModifier,
                        factory = { ctx ->
                            client.createRenderer(ctx, settings, stretchToFill).apply {
                                isFocusable = false
                                isFocusableInTouchMode = false
                                hideAndroidPointerTree()
                            }
                        },
                        update = { renderer ->
                            client.updateRendererSettings(settings, stretchToFill)
                            renderer.isFocusable = false
                            renderer.isFocusableInTouchMode = false
                            pointerRootView.configureAndroidMousePointerCapture(hideExternalMousePointer, { currentOnMouseCaptureInput() }) { event ->
                                client.dispatchMotion(event)
                            }
                            if (hideExternalMousePointer) {
                                pointerRootView.hideAndroidPointerTree()
                                renderer.hideAndroidPointerTree()
                            } else {
                                pointerRootView.showAndroidPointerTree()
                                renderer.showAndroidPointerTree()
                            }
                            renderer.setOnKeyListener(null)
                            renderer.setOnGenericMotionListener { _, event ->
                                if (hideExternalMousePointer) pointerRootView.hideAndroidPointerTree()
                                client.dispatchMotion(event)
                            }
                            renderer.setOnTouchListener { view, event ->
                                NativeStreamInputRouter.dispatchTouch(event, view.width, view.height)
                            }
                        },
                    )
                }
            }
            FingerMouseInputLayer(
                enabled = touchMouseEnabled,
                onZoomGesture = { scaleChange, pan ->
                    val nextScale = (zoomScale * scaleChange).coerceIn(1f, 3f)
                    zoomScale = nextScale
                    zoomOffset = if (nextScale <= 1.001f) {
                        Offset.Zero
                    } else {
                        clampStreamZoomOffset(zoomOffset + pan, nextScale, viewportSize)
                    }
                },
                modifier = Modifier.matchParentSize(),
            )
        }
    }
}

private fun clampStreamZoomOffset(offset: Offset, zoomScale: Float, viewportSize: IntSize): Offset {
    if (zoomScale <= 1.001f || viewportSize.width <= 0 || viewportSize.height <= 0) return Offset.Zero
    val maxX = viewportSize.width * (zoomScale - 1f) / 2f
    val maxY = viewportSize.height * (zoomScale - 1f) / 2f
    return Offset(
        x = offset.x.coerceIn(-maxX, maxX),
        y = offset.y.coerceIn(-maxY, maxY),
    )
}

private fun androidNullPointerIcon(view: android.view.View): PointerIcon? =
    if (Build.VERSION.SDK_INT >= 24) {
        PointerIcon.getSystemIcon(view.context, PointerIcon.TYPE_NULL)
    } else {
        null
    }

private fun View.configureAndroidMousePointerCapture(enabled: Boolean, onCaptureInput: () -> Unit = {}, onMotion: (MotionEvent) -> Boolean) {
    if (Build.VERSION.SDK_INT < 26) return
    if (!enabled) {
        clearAndroidMousePointerCapture()
        return
    }
    setOnCapturedPointerListener { _, event ->
        onCaptureInput()
        onMotion(event)
    }
    post {
        if (isAttachedToWindow && hasWindowFocus() && !hasPointerCapture()) {
            isFocusable = true
            isFocusableInTouchMode = true
            requestFocus()
            onCaptureInput()
            requestPointerCapture()
        }
    }
}

private fun View.clearAndroidMousePointerCapture() {
    if (Build.VERSION.SDK_INT < 26) return
    setOnCapturedPointerListener(null)
    releasePointerCapture()
}

private fun android.view.View.hideAndroidPointerTree() {
    if (Build.VERSION.SDK_INT < 24) return
    val icon = androidNullPointerIcon(this)
    applyAndroidPointerIconTree(icon)
}

private fun android.view.View.showAndroidPointerTree() {
    if (Build.VERSION.SDK_INT < 24) return
    applyAndroidPointerIconTree(null)
}

private fun android.view.View.applyAndroidPointerIconTree(icon: PointerIcon?) {
    if (Build.VERSION.SDK_INT < 24) return
    pointerIcon = icon
    if (this is ViewGroup) {
        for (index in 0 until childCount) {
            getChildAt(index).applyAndroidPointerIconTree(icon)
        }
    }
}

@Composable
private fun FingerMouseInputLayer(
    enabled: Boolean,
    onZoomGesture: (scaleChange: Float, pan: Offset) -> Unit,
    modifier: Modifier = Modifier,
) {
    if (!enabled) return
    var width by remember { mutableStateOf(0) }
    var height by remember { mutableStateOf(0) }
    var pinchActive by remember { mutableStateOf(false) }
    var lastPinchDistance by remember { mutableFloatStateOf(0f) }
    var lastPinchCentroid by remember { mutableStateOf(Offset.Zero) }
    Box(
        modifier
            .onSizeChanged {
                width = it.width
                height = it.height
            }
            .pointerInteropFilter { event ->
                if (NativeStreamInputRouter.isNativeUiTouchGestureActive()) {
                    pinchActive = false
                    lastPinchDistance = 0f
                    lastPinchCentroid = Offset.Zero
                    return@pointerInteropFilter true
                }
                if (event.pointerCount >= 2) {
                    NativeStreamInputRouter.cancelTouchMouse()
                    val distance = event.firstTwoPointerDistance()
                    val centroid = event.firstTwoPointerCentroid()
                    if (pinchActive && lastPinchDistance > 0f && distance > 0f) {
                        onZoomGesture(
                            (distance / lastPinchDistance).coerceIn(0.82f, 1.22f),
                            centroid - lastPinchCentroid,
                        )
                    }
                    pinchActive = true
                    lastPinchDistance = distance
                    lastPinchCentroid = centroid
                    return@pointerInteropFilter true
                }
                if (pinchActive) {
                    if (event.actionMasked == MotionEvent.ACTION_UP || event.actionMasked == MotionEvent.ACTION_CANCEL) {
                        pinchActive = false
                        lastPinchDistance = 0f
                        lastPinchCentroid = Offset.Zero
                    }
                    return@pointerInteropFilter true
                }
                if (event.actionMasked == MotionEvent.ACTION_DOWN) {
                    NativeInputDiagnostics.add("compose finger layer down size=${width}x$height")
                }
                NativeStreamInputRouter.dispatchTouch(event, width, height)
            },
    )
}

private fun MotionEvent.firstTwoPointerDistance(): Float {
    if (pointerCount < 2) return 0f
    val dx = getX(1) - getX(0)
    val dy = getY(1) - getY(0)
    return sqrt(dx * dx + dy * dy)
}

private fun MotionEvent.firstTwoPointerCentroid(): Offset =
    if (pointerCount >= 2) {
        Offset((getX(0) + getX(1)) / 2f, (getY(0) + getY(1)) / 2f)
    } else {
        Offset.Zero
    }

@Composable
private fun NoActiveStreamScreen(
    canResumeSession: Boolean,
    canEndSession: Boolean,
    onBack: () -> Unit,
    onResumeSession: () -> Unit,
    onEndSession: () -> Unit,
) {
    Column(
        Modifier
            .fillMaxSize()
            .padding(24.dp),
        horizontalAlignment = Alignment.CenterHorizontally,
        verticalArrangement = Arrangement.Center,
    ) {
        Text("No active stream", style = MaterialTheme.typography.headlineSmall, fontWeight = FontWeight.Bold)
        Spacer(Modifier.height(8.dp))
        Text(
            "OpenNOW does not have a local stream attached right now.",
            color = TextMuted,
            textAlign = TextAlign.Center,
        )
        Spacer(Modifier.height(18.dp))
        Row(horizontalArrangement = Arrangement.spacedBy(10.dp)) {
            OutlinedButton(onClick = onBack) { Text("Back to library") }
            if (canResumeSession) {
                Button(onClick = onResumeSession) { Text(stringResource(R.string.action_resume)) }
            }
            if (canEndSession) {
                Button(onClick = onEndSession) { Text("End cloud session") }
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
        modifier
            .padding(top = 10.dp, end = 10.dp)
            .onGloballyPositioned { coordinates ->
                val bounds = coordinates.boundsInRoot()
                NativeStreamInputRouter.setUiTouchPassthroughBounds(
                    bounds.left.roundToInt(),
                    bounds.top.roundToInt(),
                    bounds.right.roundToInt(),
                    bounds.bottom.roundToInt(),
                )
            },
        horizontalArrangement = Arrangement.spacedBy(8.dp),
        verticalAlignment = Alignment.CenterVertically,
    ) {
        if (status != null) {
            Surface(
                shape = RoundedCornerShape(999.dp),
                color = Panel.copy(alpha = 0.8f),
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

    DisposableEffect(Unit) {
        onDispose {
            NativeStreamInputRouter.clearUiTouchPassthroughBounds()
        }
    }
}

@Composable
private fun StreamFirstLaunchGuide(
    gameTitle: String,
    touchControlsEnabled: Boolean,
    onOpenControls: () -> Unit,
    onDismiss: () -> Unit,
) {
    val primaryFocusRequester = remember { FocusRequester() }
    val overlayInteraction = remember { MutableInteractionSource() }
    LaunchedEffect(Unit) {
        delay(80)
        runCatching { primaryFocusRequester.requestFocus() }
    }
    BoxWithConstraints(
        Modifier
            .fillMaxSize()
            .background(Color.Black.copy(alpha = 0.62f))
            .clickable(
                interactionSource = overlayInteraction,
                indication = null,
                onClick = {},
            ),
        contentAlignment = Alignment.Center,
    ) {
        val landscape = maxWidth > maxHeight
        val panelWidth = if (landscape) 0.58f else 0.92f
        Surface(
            modifier = Modifier
                .padding(18.dp)
                .fillMaxWidth(panelWidth)
                .then(if (landscape) Modifier.fillMaxHeight(0.9f) else Modifier),
            shape = RoundedCornerShape(20.dp),
            color = Panel.copy(alpha = 0.96f),
            tonalElevation = 8.dp,
        ) {
            Column(
                Modifier
                    .padding(18.dp)
                    .verticalScroll(rememberScrollState()),
                verticalArrangement = Arrangement.spacedBy(14.dp),
            ) {
                Column(verticalArrangement = Arrangement.spacedBy(4.dp)) {
                    Text("Native stream guide", style = MaterialTheme.typography.titleLarge, fontWeight = FontWeight.Bold)
                    Text(
                        "Quick controls for $gameTitle",
                        color = TextMuted,
                        maxLines = 2,
                        overflow = TextOverflow.Ellipsis,
                    )
                }
                StreamGuidePoint(
                    title = "Open controls",
                    body = "Press Android Back once, Menu, controller Start hold, or D-pad Center/Enter. Back again closes panels.",
                )
                StreamGuidePoint(
                    title = "Change stream options",
                    body = "Controls include stats, audio, keyboard bar, finger mouse, touch controller, sharpening, and stretch to fill.",
                )
                StreamGuidePoint(
                    title = if (touchControlsEnabled) "Touch controls are active" else "Touch controls are hidden",
                    body = if (touchControlsEnabled) {
                        "Use the panel to resize, fade, move, or hide the on-screen controls."
                    } else {
                        "Use the panel to turn on the on-screen controller when you need it."
                    },
                )
                StreamGuidePoint(
                    title = "Exit is separate",
                    body = "Use Exit in the controls panel when you want to leave the stream. OpenNOW asks before ending the cloud session.",
                )
                Row(
                    Modifier.fillMaxWidth(),
                    horizontalArrangement = Arrangement.spacedBy(10.dp),
                ) {
                    OutlinedButton(
                        onClick = onDismiss,
                        modifier = Modifier.weight(1f),
                    ) {
                        Text("Got it", maxLines = 1, overflow = TextOverflow.Ellipsis)
                    }
                    Button(
                        onClick = onOpenControls,
                        modifier = Modifier
                            .weight(1f)
                            .focusRequester(primaryFocusRequester),
                    ) {
                        Text("Open controls", maxLines = 1, overflow = TextOverflow.Ellipsis)
                    }
                }
            }
        }
    }
}

@Composable
private fun StreamGuidePoint(title: String, body: String) {
    Row(
        Modifier
            .fillMaxWidth()
            .clip(RoundedCornerShape(12.dp))
            .background(Color.White.copy(alpha = 0.06f))
            .padding(horizontal = 12.dp, vertical = 10.dp),
        horizontalArrangement = Arrangement.spacedBy(10.dp),
        verticalAlignment = Alignment.Top,
    ) {
        Surface(
            modifier = Modifier.padding(top = 6.dp).size(8.dp),
            shape = CircleShape,
            color = MaterialTheme.colorScheme.primary,
        ) {}
        Column(Modifier.weight(1f), verticalArrangement = Arrangement.spacedBy(2.dp)) {
            Text(title, fontWeight = FontWeight.SemiBold)
            Text(body, color = TextMuted, style = MaterialTheme.typography.bodySmall)
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
    touchLayoutEditing: Boolean,
    keyboardOpen: Boolean,
    onAudioToggle: () -> Unit,
    onStatsToggle: () -> Unit,
    onStatsStyleCycle: () -> Unit,
    onPhoneRumbleFallbackToggle: () -> Unit,
    onTouchLayoutEditingToggle: () -> Unit,
    onKeyboardToggle: () -> Unit,
    onEsc: () -> Unit,
    onEnter: () -> Unit,
    onBackspace: () -> Unit,
    onExit: () -> Unit,
    onTouchControlsToggle: () -> Unit,
    onMousePadToggle: () -> Unit,
    onSharpeningToggle: () -> Unit,
    onSharpeningAmountChange: (Float) -> Unit,
    onStretchToFillToggle: () -> Unit,
    onTouchScaleChange: (Float) -> Unit,
    onButtonScaleChange: (Float) -> Unit,
    onStickScaleChange: (Float) -> Unit,
    onOpacityChange: (Float) -> Unit,
    onTouchEdgePaddingChange: (Float) -> Unit,
    onTouchBottomPaddingChange: (Float) -> Unit,
    onTouchLeftOffsetChange: (Float) -> Unit,
    onTouchRightOffsetChange: (Float) -> Unit,
    onButtonTone: () -> Unit,
    onClose: () -> Unit,
) {
    val doneFocusRequester = remember { FocusRequester() }
    val focusManager = LocalFocusManager.current
    LaunchedEffect(Unit) {
        delay(80)
        runCatching { doneFocusRequester.requestFocus() }
    }
    Surface(
        modifier = Modifier
            .padding(14.dp)
            .fillMaxWidth(0.94f)
            .fillMaxHeight(0.72f)
            .onGloballyPositioned { coordinates ->
                val bounds = coordinates.boundsInRoot()
                NativeStreamInputRouter.setStreamPanelTouchPassthroughBounds(
                    bounds.left.roundToInt(),
                    bounds.top.roundToInt(),
                    bounds.right.roundToInt(),
                    bounds.bottom.roundToInt(),
                )
            },
        shape = RoundedCornerShape(18.dp),
        color = Panel.copy(alpha = 0.93f),
        tonalElevation = 6.dp,
    ) {
        LazyColumn(
            modifier = Modifier.onPreviewKeyEvent { handleVerticalDpadFocusMove(it, focusManager) },
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
                    OutlinedButton(
                        onClick = {
                            onButtonTone()
                            onExit()
                        },
                        contentPadding = PaddingValues(horizontal = 10.dp, vertical = 6.dp),
                    ) {
                        Text("Exit")
                    }
                    OutlinedButton(
                        onClick = {
                            onButtonTone()
                            onClose()
                        },
                        modifier = Modifier.focusRequester(doneFocusRequester),
                        contentPadding = PaddingValues(horizontal = 10.dp, vertical = 6.dp),
                    ) {
                        Text("Done")
                    }
                }
            }
            item {
                StreamPanelSection("Display") {
                    StreamControlSwitch("Audio", if (audioMuted) "Muted" else "On", !audioMuted) {
                        onButtonTone()
                        onAudioToggle()
                    }
                    StreamControlSwitch("Stream stats", if (statsVisible) "On" else "Off", statsVisible) {
                        onButtonTone()
                        onStatsToggle()
                    }
                    StreamControlAction("Stats style", settings.streamStatsStyle.label) {
                        onButtonTone()
                        onStatsStyleCycle()
                    }
                    StreamControlSwitch("Stream sharpening", if (settings.stream.streamSharpeningEnabled) "On" else "Off", settings.stream.streamSharpeningEnabled) {
                        onButtonTone()
                        onSharpeningToggle()
                    }
                    if (settings.stream.streamSharpeningEnabled) {
                        CompactSlider("Sharpness amount", settings.stream.streamSharpeningAmount, 0f, 1f, onSharpeningAmountChange)
                    }
                    StreamControlSwitch("Stretch to fill", if (settings.stretchStreamToFill) "On" else "Off", settings.stretchStreamToFill) {
                        onButtonTone()
                        onStretchToFillToggle()
                    }
                }
            }
            item {
                StreamPanelSection("Input") {
                    StreamControlSwitch("Keyboard bar", if (keyboardOpen) "Visible" else "Hidden", keyboardOpen) {
                        onButtonTone()
                        onKeyboardToggle()
                    }
                    Row(horizontalArrangement = Arrangement.spacedBy(8.dp), modifier = Modifier.fillMaxWidth()) {
                        OutlinedButton(
                            onClick = {
                                onButtonTone()
                                onEsc()
                            },
                            modifier = Modifier.weight(1f),
                        ) { Text("Esc") }
                        OutlinedButton(
                            onClick = {
                                onButtonTone()
                                onEnter()
                            },
                            modifier = Modifier.weight(1f),
                        ) { Text("Enter") }
                        OutlinedButton(
                            onClick = {
                                onButtonTone()
                                onBackspace()
                            },
                            modifier = Modifier.weight(1f),
                        ) { Text("⌫") }
                    }
                    StreamControlSwitch("Finger mouse", if (settings.androidTouch.mousePad) "On" else "Off", settings.androidTouch.mousePad) {
                        onButtonTone()
                        onMousePadToggle()
                    }
                    StreamControlSwitch("Touch controller", if (settings.androidTouch.enabled) "Visible" else "Hidden", settings.androidTouch.enabled) {
                        onButtonTone()
                        onTouchControlsToggle()
                    }
                    StreamControlSwitch("Phone rumble fallback", if (settings.phoneRumbleFallback) "On" else "Off", settings.phoneRumbleFallback) {
                        onButtonTone()
                        onPhoneRumbleFallbackToggle()
                    }
                }
            }
            item {
                StreamPanelSection("Touch Layout") {
                    StreamControlSwitch("Drag edit mode", if (touchLayoutEditing) "On" else "Off", touchLayoutEditing) {
                        onButtonTone()
                        onTouchLayoutEditingToggle()
                    }
                    CompactSlider("Layout scale", settings.androidTouch.scale, 0.6f, 1.4f, onTouchScaleChange)
                    CompactSlider("Button size", settings.androidTouch.buttonScale, 0.65f, 1.5f, onButtonScaleChange)
                    CompactSlider("Stick size", settings.androidTouch.stickScale, 0.65f, 1.5f, onStickScaleChange)
                    CompactSlider("Opacity", settings.androidTouch.opacity, 0.15f, 1f, onOpacityChange)
                    CompactDpSlider("Edge padding", settings.androidTouch.edgePaddingDp, 0f, 72f, onTouchEdgePaddingChange)
                    CompactDpSlider("Bottom padding", settings.androidTouch.bottomPaddingDp, 0f, 120f, onTouchBottomPaddingChange)
                    CompactDpSlider("Left position", settings.androidTouch.leftOffsetYDp, -160f, 160f, onTouchLeftOffsetChange)
                    CompactDpSlider("Right position", settings.androidTouch.rightOffsetYDp, -160f, 160f, onTouchRightOffsetChange)
                }
            }
        }
    }
    DisposableEffect(Unit) {
        onDispose {
            NativeStreamInputRouter.clearStreamPanelTouchPassthroughBounds()
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
private fun StreamControlAction(label: String, value: String, onClick: () -> Unit) {
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
        Text("Change", color = MaterialTheme.colorScheme.primary, style = MaterialTheme.typography.labelMedium)
    }
}

@Composable
private fun CompactSlider(label: String, value: Float, min: Float, max: Float, onChange: (Float) -> Unit) {
    var local by remember(value) { mutableFloatStateOf(value) }
    val focusManager = LocalFocusManager.current
    Column {
        Row(Modifier.fillMaxWidth(), verticalAlignment = Alignment.CenterVertically) {
            Text(label, Modifier.weight(1f))
            Text("${(local * 100).roundToInt()}%", color = TextMuted)
        }
        Slider(
            modifier = Modifier.onPreviewKeyEvent { handleVerticalDpadFocusMove(it, focusManager) },
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
private fun CompactDpSlider(label: String, value: Float, min: Float, max: Float, onChange: (Float) -> Unit) {
    var local by remember(value) { mutableFloatStateOf(value) }
    val focusManager = LocalFocusManager.current
    Column {
        Row(Modifier.fillMaxWidth(), verticalAlignment = Alignment.CenterVertically) {
            Text(label, Modifier.weight(1f))
            Text("${local.roundToInt()} dp", color = TextMuted)
        }
        Slider(
            modifier = Modifier.onPreviewKeyEvent { handleVerticalDpadFocusMove(it, focusManager) },
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
        color = Panel.copy(alpha = 0.95f),
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
    streamStats: StreamRuntimeStats,
    audioMuted: Boolean,
    style: StreamStatsStyle,
    modifier: Modifier = Modifier,
) {
    if (style == StreamStatsStyle.Compact) {
        Surface(
            modifier = modifier.padding(8.dp),
            shape = RoundedCornerShape(999.dp),
            color = Panel.copy(alpha = 0.48f),
            tonalElevation = 0.dp,
        ) {
            Row(
                Modifier.padding(horizontal = 10.dp, vertical = 5.dp),
                horizontalArrangement = Arrangement.spacedBy(10.dp),
                verticalAlignment = Alignment.CenterVertically,
            ) {
                Text("FPS ${streamStats.fps?.toString() ?: "--"}", color = TextPrimary, style = MaterialTheme.typography.labelSmall, maxLines = 1)
                Text("Ping ${streamStats.pingMs?.let { "${it}ms" } ?: "--"}", color = TextPrimary, style = MaterialTheme.typography.labelSmall, maxLines = 1)
                Text("BW ${formatRuntimeBitrate(streamStats.bitrateKbps)}", color = TextPrimary, style = MaterialTheme.typography.labelSmall, maxLines = 1)
            }
        }
        return
    }
    Surface(
        modifier = modifier.padding(10.dp),
        shape = RoundedCornerShape(12.dp),
        color = Panel.copy(alpha = 0.8f),
        tonalElevation = 4.dp,
    ) {
        Column(Modifier.padding(horizontal = 12.dp, vertical = 9.dp), verticalArrangement = Arrangement.spacedBy(2.dp)) {
            val statusParts = listOfNotNull(
                status.takeUnless(::shouldHideStreamStatusText),
                if (audioMuted) "audio muted" else "audio on",
            )
            Text(gameTitle, fontWeight = FontWeight.Bold, maxLines = 1, overflow = TextOverflow.Ellipsis)
            Text(
                listOf(
                    "FPS ${streamStats.fps?.toString() ?: "--"}",
                    "Bitrate ${formatRuntimeBitrate(streamStats.bitrateKbps)}",
                    "Ping ${streamStats.pingMs?.let { "${it}ms" } ?: "--"}",
                    "Codec ${streamStats.codec ?: "--"}",
                ).joinToString(" · "),
                color = TextMuted,
                style = MaterialTheme.typography.labelSmall,
            )
            streamStats.resolution?.let { resolution ->
                Text(
                    resolution,
                    color = TextMuted,
                    style = MaterialTheme.typography.labelSmall,
                    maxLines = 1,
                    overflow = TextOverflow.Ellipsis,
                )
            }
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

private fun formatRuntimeBitrate(bitrateKbps: Int?): String {
    val kbps = bitrateKbps ?: return "--"
    return if (kbps >= 1000) {
        "${(kbps / 1000.0).let { kotlin.math.round(it * 10.0) / 10.0 }} Mbps"
    } else {
        "$kbps Kbps"
    }
}

private fun shouldHideStreamStatusText(status: String): Boolean =
    status.trim().replace('_', ' ').let {
        it.equals("Streaming", ignoreCase = true) ||
            it.equals("ICE CONNECTED", ignoreCase = true) ||
            it.equals("ICE COMPLETED", ignoreCase = true)
    }

@Composable
private fun StreamExitConfirmation(
    gameTitle: String,
    onKeepPlaying: () -> Unit,
    onExit: () -> Unit,
    modifier: Modifier = Modifier,
) {
    val keepPlayingFocusRequester = remember { FocusRequester() }
    LaunchedEffect(Unit) {
        delay(80)
        runCatching { keepPlayingFocusRequester.requestFocus() }
    }
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
            color = Panel.copy(alpha = 0.95f),
            tonalElevation = 8.dp,
        ) {
            Column(Modifier.padding(18.dp), verticalArrangement = Arrangement.spacedBy(12.dp)) {
                Text("Session Control", color = TextMuted, style = MaterialTheme.typography.labelMedium, fontWeight = FontWeight.Bold)
                Text("Exit Stream?", style = MaterialTheme.typography.titleLarge, fontWeight = FontWeight.Bold)
                Text("Do you really want to exit $gameTitle?", color = TextMuted)
                Text("Your current cloud gaming session will be closed.", color = TextMuted, style = MaterialTheme.typography.bodySmall)
                Row(horizontalArrangement = Arrangement.spacedBy(10.dp), modifier = Modifier.fillMaxWidth()) {
                    OutlinedButton(
                        onClick = onKeepPlaying,
                        modifier = Modifier
                            .weight(1f)
                            .focusRequester(keepPlayingFocusRequester),
                    ) { Text("Keep Playing") }
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
    val ads = sessionAdItems(session?.adState)
    val ad = ads.firstOrNull { it.adId == state.queueAdActiveId } ?: ads.firstOrNull()
    val mediaUrl = ad?.adMediaFiles?.firstOrNull { !it.mediaFileUrl.isNullOrBlank() }?.mediaFileUrl
        ?: ad?.adUrl
        ?: ad?.mediaUrl
    val queueCopy = queueLaunchStatusText(state)
    val hasPlayableAd = ad != null && mediaUrl != null

    BoxWithConstraints(
        Modifier
            .fillMaxSize()
            .padding(18.dp),
        contentAlignment = Alignment.Center,
    ) {
        val useLandscapeAdLayout = hasPlayableAd && maxWidth > maxHeight

        if (useLandscapeAdLayout) {
            Row(
                Modifier.fillMaxSize(),
                horizontalArrangement = Arrangement.spacedBy(18.dp),
                verticalAlignment = Alignment.CenterVertically,
            ) {
                QueueStatusPanel(
                    game = game,
                    queueCopy = queueCopy,
                    error = state.error,
                    compact = true,
                    onMinimize = viewModel::minimizeStreamLaunch,
                    onCancel = viewModel::stopStream,
                    modifier = Modifier.weight(1f),
                )
                QueueAdPanel(
                    ad = ad,
                    mediaUrl = mediaUrl,
                    viewModel = viewModel,
                    modifier = Modifier.weight(1f),
                    playerModifier = Modifier
                        .fillMaxWidth()
                        .aspectRatio(16f / 9f),
                )
            }
        } else {
            Column(
                Modifier.fillMaxSize(),
                horizontalAlignment = Alignment.CenterHorizontally,
                verticalArrangement = Arrangement.Center,
            ) {
                QueueStatusPanel(
                    game = game,
                    queueCopy = queueCopy,
                    error = state.error,
                    compact = false,
                    onMinimize = viewModel::minimizeStreamLaunch,
                    onCancel = viewModel::stopStream,
                    modifier = Modifier.fillMaxWidth(),
                )
                if (hasPlayableAd) {
                    Spacer(Modifier.height(18.dp))
                    QueueAdPanel(
                        ad = ad,
                        mediaUrl = mediaUrl,
                        viewModel = viewModel,
                        modifier = Modifier.fillMaxWidth(),
                        playerModifier = Modifier
                            .fillMaxWidth(0.8f)
                            .height(220.dp),
                    )
                } else if (isSessionAdsRequired(session?.adState)) {
                    Spacer(Modifier.height(12.dp))
                    Text(session?.adState?.message ?: "Ad is required before the queue can continue.", color = TextMuted)
                }
            }
        }
    }
}

@Composable
private fun QueueStatusPanel(
    game: GameInfo?,
    queueCopy: String,
    error: String?,
    compact: Boolean,
    onMinimize: () -> Unit,
    onCancel: () -> Unit,
    modifier: Modifier = Modifier,
) {
    Column(
        modifier,
        horizontalAlignment = Alignment.CenterHorizontally,
        verticalArrangement = Arrangement.Center,
    ) {
        val imageWidth = if (compact) 76.dp else 96.dp
        val imageHeight = if (compact) 102.dp else 128.dp
        UrlImage(
            game?.imageUrl,
            Modifier
                .width(imageWidth)
                .height(imageHeight)
                .clip(RoundedCornerShape(14.dp)),
        )
        Spacer(Modifier.height(if (compact) 12.dp else 16.dp))
        Text(
            game?.title ?: "Starting stream",
            style = if (compact) MaterialTheme.typography.titleLarge else MaterialTheme.typography.headlineSmall,
            fontWeight = FontWeight.Bold,
            maxLines = 2,
            overflow = TextOverflow.Ellipsis,
            textAlign = TextAlign.Center,
        )
        Text(
            queueCopy,
            color = TextMuted,
            textAlign = TextAlign.Center,
        )
        Spacer(Modifier.height(if (compact) 14.dp else 18.dp))
        LinearProgressIndicator(Modifier.fillMaxWidth(if (compact) 0.9f else 0.7f))
        Spacer(Modifier.height(12.dp))
        Row(
            Modifier.fillMaxWidth(if (compact) 0.92f else 0.7f),
            horizontalArrangement = Arrangement.spacedBy(10.dp),
        ) {
            OutlinedButton(onClick = onMinimize, modifier = Modifier.weight(1f)) {
                Text("Minimize", maxLines = 1, overflow = TextOverflow.Ellipsis)
            }
            OutlinedButton(onClick = onCancel, modifier = Modifier.weight(1f)) {
                Text("Cancel", maxLines = 1, overflow = TextOverflow.Ellipsis)
            }
        }
        error?.let {
            Spacer(Modifier.height(12.dp))
            Text(it, color = Color(0xffff9f9f), textAlign = TextAlign.Center)
        }
    }
}

@Composable
private fun QueueAdPanel(
    ad: SessionAdInfo,
    mediaUrl: String,
    viewModel: OpenNowViewModel,
    modifier: Modifier = Modifier,
    playerModifier: Modifier = Modifier,
) {
    Column(
        modifier,
        horizontalAlignment = Alignment.CenterHorizontally,
        verticalArrangement = Arrangement.Center,
    ) {
        QueueAdPlayer(
            adId = ad.adId,
            url = mediaUrl,
            modifier = playerModifier,
            onStarted = { viewModel.reportQueueAd(ad.adId, "start") },
            onPaused = { viewModel.reportQueueAd(ad.adId, "pause") },
            onResumed = { viewModel.reportQueueAd(ad.adId, "resume") },
            onFinished = { watchedTimeInMs ->
                viewModel.reportQueueAd(ad.adId, "finish", watchedTimeInMs = watchedTimeInMs)
            },
            onError = { watchedTimeInMs ->
                viewModel.reportQueueAd(
                    ad.adId,
                    "cancel",
                    watchedTimeInMs = watchedTimeInMs,
                    cancelReason = "error",
                    errorInfo = "Error loading url",
                )
            },
        )
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
        color = Panel.copy(alpha = 0.95f),
        tonalElevation = 8.dp,
    ) {
        Row(
            Modifier.padding(horizontal = 14.dp, vertical = 12.dp),
            verticalAlignment = Alignment.CenterVertically,
            horizontalArrangement = Arrangement.spacedBy(12.dp),
        ) {
            CircularProgressIndicator(Modifier.size(24.dp), strokeWidth = 2.dp, color = MaterialTheme.colorScheme.primary)
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
private fun QueueAdPlayer(
    adId: String,
    url: String,
    modifier: Modifier = Modifier,
    onStarted: () -> Unit,
    onPaused: () -> Unit,
    onResumed: () -> Unit,
    onFinished: (watchedTimeInMs: Long) -> Unit,
    onError: (watchedTimeInMs: Long) -> Unit,
) {
    val context = LocalContext.current
    var muted by remember { mutableStateOf(false) }
    val player = remember(adId, url) {
        ExoPlayer.Builder(context).build().apply {
            setMediaItem(MediaItem.fromUri(url))
            volume = if (muted) 0f else 1f
            prepare()
            playWhenReady = true
        }
    }
    var reportedStart by remember(adId, url) { mutableStateOf(false) }
    var reportedFinish by remember(adId, url) { mutableStateOf(false) }
    var reportedPause by remember(adId, url) { mutableStateOf(false) }
    var playing by remember(adId, url) { mutableStateOf(player.playWhenReady) }
    var controlsVisible by remember(adId, url) { mutableStateOf(false) }
    LaunchedEffect(controlsVisible, playing) {
        if (controlsVisible && playing) {
            delay(2400L)
            controlsVisible = false
        }
    }
    DisposableEffect(player) {
        val listener = object : Player.Listener {
            override fun onIsPlayingChanged(isPlaying: Boolean) {
                playing = isPlaying
                if (!isPlaying) controlsVisible = true
            }

            override fun onPlayWhenReadyChanged(playWhenReady: Boolean, reason: Int) {
                if (!reportedStart || reportedFinish) return
                if (playWhenReady && reportedPause) {
                    reportedPause = false
                    onResumed()
                } else if (!playWhenReady && player.playbackState != Player.STATE_ENDED && !reportedPause) {
                    reportedPause = true
                    onPaused()
                }
            }

            override fun onPlaybackStateChanged(playbackState: Int) {
                if (playbackState == Player.STATE_READY && player.playWhenReady && !reportedStart) {
                    reportedStart = true
                    onStarted()
                }
                if (playbackState == Player.STATE_ENDED && !reportedFinish) {
                    reportedFinish = true
                    onFinished(player.currentPosition.coerceAtLeast(0L))
                }
            }

            override fun onPlayerError(error: androidx.media3.common.PlaybackException) {
                if (!reportedFinish) {
                    reportedFinish = true
                    onError(player.currentPosition.coerceAtLeast(0L))
                }
            }
        }
        player.addListener(listener)
        listener.onIsPlayingChanged(player.isPlaying)
        listener.onPlaybackStateChanged(player.playbackState)
        onDispose {
            player.removeListener(listener)
            player.release()
        }
    }
    Box(
        modifier = modifier
            .clip(RoundedCornerShape(8.dp))
            .clickable { controlsVisible = true },
    ) {
        AndroidView(
            modifier = Modifier
                .fillMaxSize(),
            factory = { ctx -> PlayerView(ctx).apply { this.player = player; useController = false } },
            update = { it.player = player; it.useController = false },
        )
        AnimatedVisibility(
            visible = controlsVisible || !playing,
            enter = fadeIn(),
            exit = fadeOut(),
            modifier = Modifier.align(Alignment.BottomCenter),
        ) {
            Row(
                modifier = Modifier
                    .padding(bottom = 12.dp)
                    .clip(RoundedCornerShape(999.dp))
                    .background(Color.Black.copy(alpha = 0.58f))
                    .padding(horizontal = 8.dp, vertical = 6.dp),
                horizontalArrangement = Arrangement.spacedBy(6.dp),
                verticalAlignment = Alignment.CenterVertically,
            ) {
                QueueAdIconButton(
                    label = if (playing) "Pause ad" else "Play ad",
                    icon = if (playing) QueueAdControlIcon.Pause else QueueAdControlIcon.Play,
                    onClick = {
                        controlsVisible = true
                        if (playing) {
                            player.pause()
                            playing = false
                        } else {
                            player.play()
                            playing = true
                        }
                    },
                )
                QueueAdIconButton(
                    label = if (muted) "Unmute ad" else "Mute ad",
                    icon = if (muted) QueueAdControlIcon.Muted else QueueAdControlIcon.Volume,
                    onClick = {
                        controlsVisible = true
                        muted = !muted
                        player.volume = if (muted) 0f else 1f
                    },
                )
            }
        }
    }
}

private enum class QueueAdControlIcon { Play, Pause, Volume, Muted }

@Composable
private fun QueueAdIconButton(label: String, icon: QueueAdControlIcon, onClick: () -> Unit) {
    IconButton(
        onClick = onClick,
        modifier = Modifier
            .size(42.dp)
            .semantics { contentDescription = label },
    ) {
        QueueAdControlIconView(icon = icon, modifier = Modifier.size(22.dp))
    }
}

@Composable
private fun QueueAdControlIconView(icon: QueueAdControlIcon, modifier: Modifier = Modifier) {
    Canvas(modifier) {
        val w = size.width
        val h = size.height
        when (icon) {
            QueueAdControlIcon.Play -> {
                val path = Path().apply {
                    moveTo(w * 0.35f, h * 0.24f)
                    lineTo(w * 0.35f, h * 0.76f)
                    lineTo(w * 0.76f, h * 0.5f)
                    close()
                }
                drawPath(path, Color.White)
            }
            QueueAdControlIcon.Pause -> {
                drawRoundRect(Color.White, Offset(w * 0.28f, h * 0.24f), Size(w * 0.14f, h * 0.52f), CornerRadius(w * 0.04f, w * 0.04f))
                drawRoundRect(Color.White, Offset(w * 0.58f, h * 0.24f), Size(w * 0.14f, h * 0.52f), CornerRadius(w * 0.04f, w * 0.04f))
            }
            QueueAdControlIcon.Volume, QueueAdControlIcon.Muted -> {
                val body = Path().apply {
                    moveTo(w * 0.18f, h * 0.42f)
                    lineTo(w * 0.34f, h * 0.42f)
                    lineTo(w * 0.52f, h * 0.26f)
                    lineTo(w * 0.52f, h * 0.74f)
                    lineTo(w * 0.34f, h * 0.58f)
                    lineTo(w * 0.18f, h * 0.58f)
                    close()
                }
                drawPath(body, Color.White)
                if (icon == QueueAdControlIcon.Volume) {
                    drawLine(Color.White, Offset(w * 0.62f, h * 0.38f), Offset(w * 0.72f, h * 0.5f), strokeWidth = w * 0.08f)
                    drawLine(Color.White, Offset(w * 0.72f, h * 0.5f), Offset(w * 0.62f, h * 0.62f), strokeWidth = w * 0.08f)
                } else {
                    drawLine(Color.White, Offset(w * 0.64f, h * 0.36f), Offset(w * 0.84f, h * 0.64f), strokeWidth = w * 0.08f)
                    drawLine(Color.White, Offset(w * 0.84f, h * 0.36f), Offset(w * 0.64f, h * 0.64f), strokeWidth = w * 0.08f)
                }
            }
        }
    }
}

@Composable
private fun TouchOverlay(
    client: NativeStreamClient,
    touch: AndroidTouchSettings,
    onButtonTone: () -> Unit,
    layoutEditing: Boolean,
    onLeftOffsetChange: (Float, Float) -> Unit,
    onRightOffsetChange: (Float, Float) -> Unit,
    modifier: Modifier = Modifier,
) {
    val opacity = touch.opacity
    val layoutScale = touch.scale
    val buttonScale = touch.buttonScale
    val stickScale = touch.stickScale
    val leftOffsetX = touch.leftOffsetXDp.dp
    val leftOffsetY = touch.leftOffsetYDp.dp
    val rightOffsetX = touch.rightOffsetXDp.dp
    val rightOffsetY = touch.rightOffsetYDp.dp

    LaunchedEffect(client, touch.enabled) {
        client.setVirtualControllerVisible(touch.enabled)
        NativeStreamInputRouter.setTouchControllerVisible(touch.enabled)
    }
    DisposableEffect(client) {
        onDispose {
            client.setVirtualControllerVisible(false)
            NativeStreamInputRouter.setTouchControllerVisible(false)
            NativeStreamInputRouter.clearTouchControllerPassthroughBounds()
        }
    }

    BoxWithConstraints(
        modifier
            .fillMaxSize()
            .padding(
                start = touch.edgePaddingDp.dp,
                top = 10.dp,
                end = touch.edgePaddingDp.dp,
                bottom = touch.bottomPaddingDp.dp,
            ),
    ) {
        if (touch.enabled) {
            val landscape = maxWidth > maxHeight
            if (landscape) {
                LandscapeTouchControls(
                    client = client,
                    opacity = opacity,
                    layoutScale = layoutScale,
                    buttonScale = buttonScale,
                    stickScale = stickScale,
                    viewportHeight = maxHeight,
                    layoutEditing = layoutEditing,
                    leftOffsetX = leftOffsetX,
                    leftOffsetY = leftOffsetY,
                    rightOffsetX = rightOffsetX,
                    rightOffsetY = rightOffsetY,
                    onButtonTone = onButtonTone,
                    onLeftOffsetChange = onLeftOffsetChange,
                    onRightOffsetChange = onRightOffsetChange,
                )
            } else {
                PortraitTouchControls(
                    client = client,
                    opacity = opacity,
                    layoutScale = layoutScale,
                    buttonScale = buttonScale,
                    stickScale = stickScale,
                    layoutEditing = layoutEditing,
                    leftOffsetX = leftOffsetX,
                    leftOffsetY = leftOffsetY,
                    rightOffsetX = rightOffsetX,
                    rightOffsetY = rightOffsetY,
                    onButtonTone = onButtonTone,
                    onLeftOffsetChange = onLeftOffsetChange,
                    onRightOffsetChange = onRightOffsetChange,
                )
            }
        }
    }
}

@Composable
private fun PortraitTouchControls(
    client: NativeStreamClient,
    opacity: Float,
    layoutScale: Float,
    buttonScale: Float,
    stickScale: Float,
    layoutEditing: Boolean,
    leftOffsetX: Dp,
    leftOffsetY: Dp,
    rightOffsetX: Dp,
    rightOffsetY: Dp,
    onButtonTone: () -> Unit,
    onLeftOffsetChange: (Float, Float) -> Unit,
    onRightOffsetChange: (Float, Float) -> Unit,
) {
    Row(
        Modifier.fillMaxSize(),
        horizontalArrangement = Arrangement.SpaceBetween,
        verticalAlignment = Alignment.Bottom,
    ) {
        TouchControlGroup(
            id = "portrait-left",
            layoutEditing = layoutEditing,
            offsetX = leftOffsetX,
            offsetY = leftOffsetY,
            onOffsetChange = onLeftOffsetChange,
        ) {
            Column(
                verticalArrangement = Arrangement.spacedBy(8.dp),
                horizontalAlignment = Alignment.Start,
            ) {
                Row(horizontalArrangement = Arrangement.spacedBy(8.dp)) {
                    GamepadButton("LB", 0x0100, client, opacity, 48.dp * buttonScale * layoutScale, onButtonTone)
                    GamepadTriggerButton("LT", left = true, client = client, opacity = opacity, size = 48.dp * buttonScale * layoutScale, onPressTone = onButtonTone)
                }
                Spacer(Modifier.height(44.dp * buttonScale * layoutScale))
                Row(
                    horizontalArrangement = Arrangement.spacedBy(12.dp),
                    verticalAlignment = Alignment.Bottom,
                ) {
                    StickWithThumbButton(
                        stickLabel = "L",
                        thumbLabel = "L3",
                        thumbMask = GamepadButtonMapping.LEFT_THUMB,
                        client = client,
                        opacity = opacity,
                        diameter = 116.dp * stickScale * layoutScale,
                        buttonScale = buttonScale * layoutScale,
                        onButtonTone = onButtonTone,
                        onChange = client::setVirtualLeftStick,
                    )
                    DpadCluster(client, opacity, buttonScale * layoutScale, onButtonTone)
                }
            }
        }
        TouchControlGroup(
            id = "portrait-right",
            layoutEditing = layoutEditing,
            offsetX = rightOffsetX,
            offsetY = rightOffsetY,
            onOffsetChange = onRightOffsetChange,
        ) {
            Column(
                verticalArrangement = Arrangement.spacedBy(8.dp),
                horizontalAlignment = Alignment.End,
            ) {
                Row(horizontalArrangement = Arrangement.spacedBy(8.dp)) {
                    GamepadTriggerButton("RT", left = false, client = client, opacity = opacity, size = 48.dp * buttonScale * layoutScale, onPressTone = onButtonTone)
                    GamepadButton("RB", 0x0200, client, opacity, 48.dp * buttonScale * layoutScale, onButtonTone)
                }
                Row(horizontalArrangement = Arrangement.spacedBy(8.dp)) {
                    GamepadButton("View", 0x0020, client, opacity, 44.dp * buttonScale * layoutScale, onButtonTone)
                    GamepadButton("Menu", 0x0010, client, opacity, 44.dp * buttonScale * layoutScale, onButtonTone)
                }
                Row(
                    horizontalArrangement = Arrangement.spacedBy(12.dp),
                    verticalAlignment = Alignment.Bottom,
                ) {
                    StickWithThumbButton(
                        stickLabel = "R",
                        thumbLabel = "R3",
                        thumbMask = GamepadButtonMapping.RIGHT_THUMB,
                        client = client,
                        opacity = opacity,
                        diameter = 104.dp * stickScale * layoutScale,
                        buttonScale = buttonScale * layoutScale,
                        onButtonTone = onButtonTone,
                        onChange = client::setVirtualRightStick,
                    )
                    FaceButtonCluster(client, opacity, buttonScale * layoutScale, onButtonTone)
                }
            }
        }
    }
}

@Composable
private fun BoxScope.LandscapeTouchControls(
    client: NativeStreamClient,
    opacity: Float,
    layoutScale: Float,
    buttonScale: Float,
    stickScale: Float,
    viewportHeight: Dp,
    layoutEditing: Boolean,
    leftOffsetX: Dp,
    leftOffsetY: Dp,
    rightOffsetX: Dp,
    rightOffsetY: Dp,
    onButtonTone: () -> Unit,
    onLeftOffsetChange: (Float, Float) -> Unit,
    onRightOffsetChange: (Float, Float) -> Unit,
) {
    val controlScale = buttonScale * layoutScale
    val topControlClearance = landscapeTouchTopControlClearanceDp(viewportHeight.value, controlScale).dp
    TouchControlGroup(
        id = "landscape-top-left",
        layoutEditing = layoutEditing,
        offsetX = leftOffsetX,
        offsetY = leftOffsetY + topControlClearance,
        onOffsetChange = onLeftOffsetChange,
        modifier = Modifier.align(Alignment.TopStart),
    ) {
        Row(horizontalArrangement = Arrangement.spacedBy(8.dp)) {
            GamepadButton("LB", 0x0100, client, opacity, 46.dp * controlScale, onButtonTone)
            GamepadTriggerButton("LT", left = true, client = client, opacity = opacity, size = 50.dp * controlScale, onPressTone = onButtonTone)
        }
    }
    TouchControlGroup(
        id = "landscape-top-center",
        layoutEditing = false,
        offsetX = 0.dp,
        offsetY = topControlClearance,
        onOffsetChange = { _, _ -> },
        modifier = Modifier.align(Alignment.TopCenter),
    ) {
        Row(horizontalArrangement = Arrangement.spacedBy(10.dp)) {
            GamepadPillButton("View", 0x0020, client, opacity, width = 76.dp * controlScale, height = 42.dp * controlScale, onPressTone = onButtonTone)
            GamepadPillButton("Start", 0x0010, client, opacity, width = 84.dp * controlScale, height = 42.dp * controlScale, onPressTone = onButtonTone)
        }
    }
    TouchControlGroup(
        id = "landscape-top-right",
        layoutEditing = layoutEditing,
        offsetX = rightOffsetX,
        offsetY = rightOffsetY + topControlClearance,
        onOffsetChange = onRightOffsetChange,
        modifier = Modifier.align(Alignment.TopEnd),
    ) {
        Row(horizontalArrangement = Arrangement.spacedBy(8.dp)) {
            GamepadTriggerButton("RT", left = false, client = client, opacity = opacity, size = 50.dp * controlScale, onPressTone = onButtonTone)
            GamepadButton("RB", 0x0200, client, opacity, 46.dp * controlScale, onButtonTone)
        }
    }
    TouchControlGroup(
        id = "landscape-bottom-left",
        layoutEditing = layoutEditing,
        offsetX = leftOffsetX,
        offsetY = leftOffsetY,
        onOffsetChange = onLeftOffsetChange,
        modifier = Modifier.align(Alignment.BottomStart),
    ) {
        Row(
            horizontalArrangement = Arrangement.spacedBy(14.dp),
            verticalAlignment = Alignment.Bottom,
        ) {
            StickWithThumbButton(
                stickLabel = "L",
                thumbLabel = "L3",
                thumbMask = GamepadButtonMapping.LEFT_THUMB,
                client = client,
                opacity = opacity,
                diameter = 112.dp * stickScale * layoutScale,
                buttonScale = controlScale * 0.88f,
                onButtonTone = onButtonTone,
                onChange = client::setVirtualLeftStick,
            )
            DpadCluster(client, opacity, controlScale * 0.88f, onButtonTone)
        }
    }
    TouchControlGroup(
        id = "landscape-bottom-right",
        layoutEditing = layoutEditing,
        offsetX = rightOffsetX,
        offsetY = rightOffsetY,
        onOffsetChange = onRightOffsetChange,
        modifier = Modifier.align(Alignment.BottomEnd),
    ) {
        Row(
            horizontalArrangement = Arrangement.spacedBy(14.dp),
        verticalAlignment = Alignment.Bottom,
    ) {
            FaceButtonCluster(client, opacity, controlScale * 0.9f, onButtonTone)
            StickWithThumbButton(
                stickLabel = "R",
                thumbLabel = "R3",
                thumbMask = GamepadButtonMapping.RIGHT_THUMB,
                client = client,
                opacity = opacity,
                diameter = 98.dp * stickScale * layoutScale,
                buttonScale = controlScale * 0.9f,
                onButtonTone = onButtonTone,
                onChange = client::setVirtualRightStick,
            )
        }
    }
}

internal fun landscapeTouchTopControlClearanceDp(viewportHeightDp: Float, controlScale: Float): Float {
    val viewportBand = (viewportHeightDp * 0.11f).coerceIn(34f, 58f)
    val scaledBand = viewportBand * controlScale.coerceIn(0.75f, 1.35f)
    return scaledBand.coerceIn(30f, 76f)
}

@Composable
private fun TouchControlGroup(
    id: String,
    layoutEditing: Boolean,
    offsetX: Dp,
    offsetY: Dp,
    onOffsetChange: (Float, Float) -> Unit,
    modifier: Modifier = Modifier,
    content: @Composable BoxScope.() -> Unit,
) {
    val density = LocalDensity.current
    Box(
        modifier
            .offset(x = offsetX, y = offsetY)
            .onGloballyPositioned { coordinates ->
                val bounds = coordinates.boundsInRoot()
                NativeStreamInputRouter.setTouchControllerPassthroughBound(
                    id,
                    bounds.left.roundToInt(),
                    bounds.top.roundToInt(),
                    bounds.right.roundToInt(),
                    bounds.bottom.roundToInt(),
                )
            },
        contentAlignment = Alignment.Center,
    ) {
        content()
        if (layoutEditing) {
            Box(
                Modifier
                    .matchParentSize()
                    .clip(RoundedCornerShape(18.dp))
                    .background(MaterialTheme.colorScheme.primary.copy(alpha = 0.16f))
                    .border(1.dp, MaterialTheme.colorScheme.primary.copy(alpha = 0.72f), RoundedCornerShape(18.dp))
                    .pointerInput(offsetX, offsetY) {
                        detectDragGestures { change, dragAmount ->
                            change.consume()
                            val deltaXDp = with(density) { dragAmount.x.toDp().value }
                            val deltaYDp = with(density) { dragAmount.y.toDp().value }
                            onOffsetChange(
                                (offsetX.value + deltaXDp).coerceIn(-220f, 220f),
                                (offsetY.value + deltaYDp).coerceIn(-160f, 160f),
                            )
                        }
                    },
                contentAlignment = Alignment.TopCenter,
            ) {
                Surface(
                    color = MaterialTheme.colorScheme.primary.copy(alpha = 0.9f),
                    shape = RoundedCornerShape(999.dp),
                    modifier = Modifier.padding(top = 4.dp),
                ) {
                    Text(
                        "Drag",
                        color = MaterialTheme.colorScheme.onPrimary,
                        style = MaterialTheme.typography.labelSmall,
                        modifier = Modifier.padding(horizontal = 8.dp, vertical = 2.dp),
                    )
                }
            }
        }
    }
    DisposableEffect(id) {
        onDispose {
            NativeStreamInputRouter.clearTouchControllerPassthroughBound(id)
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
private fun StickWithThumbButton(
    stickLabel: String,
    thumbLabel: String,
    thumbMask: Int,
    client: NativeStreamClient,
    opacity: Float,
    diameter: Dp,
    buttonScale: Float,
    onButtonTone: () -> Unit,
    onChange: (Float, Float) -> Unit,
) {
    Column(
        horizontalAlignment = Alignment.CenterHorizontally,
        verticalArrangement = Arrangement.spacedBy(6.dp),
    ) {
        GamepadPillButton(
            label = thumbLabel,
            mask = thumbMask,
            client = client,
            opacity = opacity,
            width = 56.dp * buttonScale,
            height = 34.dp * buttonScale,
            onPressTone = onButtonTone,
        )
        VirtualStick(
            label = stickLabel,
            client = client,
            opacity = opacity,
            diameter = diameter,
            onChange = onChange,
        )
    }
}

@Composable
private fun VirtualStick(
    label: String,
    client: NativeStreamClient,
    opacity: Float,
    diameter: androidx.compose.ui.unit.Dp,
    onChange: (Float, Float) -> Unit,
) {
    var knobOffset by remember { mutableStateOf(Offset.Zero) }
    val accent = MaterialTheme.colorScheme.primary
    val idleSurface = MaterialTheme.colorScheme.surfaceVariant

    DisposableEffect(client, onChange) {
        onDispose {
            onChange(0f, 0f)
        }
    }

    Box(
        Modifier
            .size(diameter)
            .clip(CircleShape)
            .background(idleSurface.copy(alpha = opacity * 0.72f))
            .border(1.dp, accent.copy(alpha = opacity), CircleShape)
            .pointerInput(client, onChange) {
                awaitPointerEventScope {
                    while (true) {
                        val event = awaitPointerEvent()
                        val change = event.changes.firstOrNull { it.pressed }
                        val maxRadius = min(size.width, size.height) * 0.34f
                        if (change == null) {
                            if (knobOffset != Offset.Zero) {
                                knobOffset = Offset.Zero
                                onChange(0f, 0f)
                            }
                            continue
                        }
                        val center = Offset(size.width / 2f, size.height / 2f)
                        val clamped = clampStickOffset(change.position - center, maxRadius)
                        knobOffset = clamped
                        onChange(
                            (clamped.x / maxRadius).coerceIn(-1f, 1f),
                            (clamped.y / maxRadius).coerceIn(-1f, 1f),
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
                .background(accent.copy(alpha = opacity))
                .border(1.dp, Color.White.copy(alpha = opacity * 0.65f), CircleShape),
            contentAlignment = Alignment.Center,
        ) {
            Text(label, fontWeight = FontWeight.Bold, color = MaterialTheme.colorScheme.onPrimary)
        }
    }
}

@Composable
private fun FaceButtonCluster(client: NativeStreamClient, opacity: Float, scale: Float, onButtonTone: () -> Unit) {
    Column(horizontalAlignment = Alignment.CenterHorizontally, verticalArrangement = Arrangement.spacedBy(6.dp)) {
        GamepadButton("Y", 0x8000, client, opacity, 54.dp * scale, onButtonTone)
        Row(horizontalArrangement = Arrangement.spacedBy(6.dp)) {
            GamepadButton("X", 0x4000, client, opacity, 54.dp * scale, onButtonTone)
            Spacer(Modifier.size(54.dp * scale))
            GamepadButton("B", 0x2000, client, opacity, 54.dp * scale, onButtonTone)
        }
        GamepadButton("A", 0x1000, client, opacity, 54.dp * scale, onButtonTone)
    }
}

@Composable
private fun DpadCluster(client: NativeStreamClient, opacity: Float, scale: Float, onButtonTone: () -> Unit) {
    val buttonSize = 54.dp * scale
    Column(horizontalAlignment = Alignment.CenterHorizontally, verticalArrangement = Arrangement.spacedBy(6.dp)) {
        GamepadButton("↑", 0x0001, client, opacity, buttonSize, onButtonTone)
        Row(horizontalArrangement = Arrangement.spacedBy(6.dp)) {
            GamepadButton("←", 0x0004, client, opacity, buttonSize, onButtonTone)
            Spacer(Modifier.size(buttonSize))
            GamepadButton("→", 0x0008, client, opacity, buttonSize, onButtonTone)
        }
        GamepadButton("↓", 0x0002, client, opacity, buttonSize, onButtonTone)
    }
}

@Composable
private fun GamepadTriggerButton(
    label: String,
    left: Boolean,
    client: NativeStreamClient,
    opacity: Float,
    size: androidx.compose.ui.unit.Dp,
    onPressTone: () -> Unit = {},
) {
    var pressed by remember { mutableStateOf(false) }
    val accent = MaterialTheme.colorScheme.primary
    val idleSurface = MaterialTheme.colorScheme.surfaceVariant
    Box(
        Modifier
            .width(size * 1.24f)
            .height(size * 0.78f)
            .clip(RoundedCornerShape(999.dp))
            .background((if (pressed) accent else idleSurface).copy(alpha = opacity))
            .border(1.dp, accent.copy(alpha = opacity), RoundedCornerShape(999.dp))
            .pointerInput(client, left) {
                awaitPointerEventScope {
                    while (true) {
                        val event = awaitPointerEvent()
                        val down = event.changes.any { it.pressed }
                        if (down != pressed) {
                            if (down) onPressTone()
                            pressed = down
                            client.setVirtualTrigger(left, down)
                        }
                        event.changes.forEach { it.consume() }
                    }
                }
            },
        contentAlignment = Alignment.Center,
    ) {
        Text(label, fontWeight = FontWeight.Bold, color = if (pressed) MaterialTheme.colorScheme.onPrimary else TextPrimary)
    }
    DisposableEffect(client, left) {
        onDispose {
            client.setVirtualTrigger(left, false)
        }
    }
}

@Composable
private fun GamepadButton(
    label: String,
    mask: Int,
    client: NativeStreamClient,
    opacity: Float,
    size: androidx.compose.ui.unit.Dp,
    onPressTone: () -> Unit = {},
) {
    var pressed by remember { mutableStateOf(false) }
    val accent = MaterialTheme.colorScheme.primary
    val idleSurface = MaterialTheme.colorScheme.surfaceVariant
    Box(
        Modifier
            .size(size)
            .clip(CircleShape)
            .background((if (pressed) accent else idleSurface).copy(alpha = opacity))
            .border(1.dp, accent.copy(alpha = opacity), CircleShape)
            .pointerInput(client, mask) {
                awaitPointerEventScope {
                    while (true) {
                        val event = awaitPointerEvent()
                        val down = event.changes.any { it.pressed }
                        if (down != pressed) {
                            if (down) onPressTone()
                            pressed = down
                            client.setVirtualButton(mask, down)
                        }
                        event.changes.forEach { it.consume() }
                    }
                }
            },
        contentAlignment = Alignment.Center,
    ) {
        Text(label, fontWeight = FontWeight.Bold, color = if (pressed) MaterialTheme.colorScheme.onPrimary else TextPrimary)
    }
    DisposableEffect(client, mask) {
        onDispose {
            client.setVirtualButton(mask, false)
        }
    }
}

@Composable
private fun GamepadPillButton(
    label: String,
    mask: Int,
    client: NativeStreamClient,
    opacity: Float,
    width: androidx.compose.ui.unit.Dp,
    height: androidx.compose.ui.unit.Dp,
    onPressTone: () -> Unit = {},
) {
    var pressed by remember { mutableStateOf(false) }
    val accent = MaterialTheme.colorScheme.primary
    val idleSurface = MaterialTheme.colorScheme.surfaceVariant
    Box(
        Modifier
            .width(width)
            .height(height)
            .clip(RoundedCornerShape(999.dp))
            .background((if (pressed) accent else idleSurface).copy(alpha = opacity))
            .border(1.dp, accent.copy(alpha = opacity), RoundedCornerShape(999.dp))
            .pointerInput(client, mask) {
                awaitPointerEventScope {
                    while (true) {
                        val event = awaitPointerEvent()
                        val down = event.changes.any { it.pressed }
                        if (down != pressed) {
                            if (down) onPressTone()
                            pressed = down
                            client.setVirtualButton(mask, down)
                        }
                        event.changes.forEach { it.consume() }
                    }
                }
            },
        contentAlignment = Alignment.Center,
    ) {
        Text(
            label,
            fontWeight = FontWeight.Bold,
            color = if (pressed) MaterialTheme.colorScheme.onPrimary else TextPrimary,
            maxLines = 1,
            overflow = TextOverflow.Ellipsis,
        )
    }
    DisposableEffect(client, mask) {
        onDispose {
            client.setVirtualButton(mask, false)
        }
    }
}

@Composable
private fun SortPicker(options: List<CatalogSortOption>, selected: String, onSelect: (String) -> Unit, modifier: Modifier = Modifier) {
    val labels = options.ifEmpty { listOf(CatalogSortOption("relevance", "Relevance", "")) }
    val selectedLabel = labels.firstOrNull { it.id == selected }?.label ?: labels.first().label
    var expanded by remember { mutableStateOf(false) }
    val controlShape = RoundedCornerShape(999.dp)
    val controlColor = MaterialTheme.colorScheme.surfaceVariant.copy(alpha = 0.46f)
    Box(modifier) {
        OutlinedButton(
            onClick = { expanded = true },
            modifier = Modifier.fillMaxWidth().height(40.dp),
            shape = controlShape,
            colors = ButtonDefaults.outlinedButtonColors(
                containerColor = controlColor,
                contentColor = TextPrimary,
            ),
            contentPadding = PaddingValues(horizontal = 12.dp),
        ) {
            Text("Sort: $selectedLabel", maxLines = 1, overflow = TextOverflow.Ellipsis)
        }
        DropdownMenu(expanded = expanded, onDismissRequest = { expanded = false }) {
            labels.forEach { option ->
                DropdownMenuItem(
                    text = {
                        Row(verticalAlignment = Alignment.CenterVertically) {
                            Text(if (option.id == selected) "✓" else "", modifier = Modifier.width(24.dp))
                            Text(option.label)
                        }
                    },
                    onClick = {
                        expanded = false
                        onSelect(option.id)
                    },
                )
            }
        }
    }
}

@OptIn(ExperimentalLayoutApi::class)
@Composable
private fun SelectedFilterChips(options: List<CatalogFilterOption>, selectedIds: List<String>, onToggle: (String) -> Unit) {
    val selectedOptions = options.filter { it.id in selectedIds }
    if (selectedOptions.isEmpty()) return
    FlowRow(horizontalArrangement = Arrangement.spacedBy(6.dp), verticalArrangement = Arrangement.spacedBy(6.dp)) {
        selectedOptions.take(4).forEach { option ->
            AssistChip(onClick = { onToggle(option.id) }, label = { Text(option.label, maxLines = 1, overflow = TextOverflow.Ellipsis) })
        }
        if (selectedOptions.size > 4) {
            AssistChip(onClick = {}, label = { Text("+${selectedOptions.size - 4}") })
        }
    }
}

private fun catalogVisibleFilterGroups(groups: List<CatalogFilterGroup>): List<CatalogFilterGroup> =
    groups.filter { it.id in setOf("digital_store", "genre", "subscriptions") }

private fun catalogFilterOptions(groups: List<CatalogFilterGroup>): List<CatalogFilterOption> =
    groups.flatMap { group -> group.options.take(if (group.id == "genre") 10 else group.options.size) }

@Composable
private fun FilterMenu(options: List<CatalogFilterOption>, selectedIds: List<String>, onToggle: (String) -> Unit) {
    var expanded by remember { mutableStateOf(false) }
    val filterControlShape = RoundedCornerShape(999.dp)
    val filterControlColor = MaterialTheme.colorScheme.surfaceVariant.copy(alpha = 0.46f)
    Row(
        horizontalArrangement = Arrangement.spacedBy(8.dp, Alignment.End),
        verticalAlignment = Alignment.CenterVertically,
    ) {
        Box {
            OutlinedButton(
                onClick = { expanded = true },
                modifier = Modifier.height(40.dp),
                shape = filterControlShape,
                colors = ButtonDefaults.outlinedButtonColors(
                    containerColor = filterControlColor,
                    contentColor = TextPrimary,
                ),
                contentPadding = PaddingValues(horizontal = 12.dp),
            ) {
                Text(if (selectedIds.isEmpty()) "Filters" else "Filters (${selectedIds.size})", maxLines = 1)
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
        Box(
            Modifier
                .height(40.dp)
                .clip(filterControlShape)
                .background(filterControlColor)
                .padding(horizontal = 12.dp),
            contentAlignment = Alignment.Center,
        ) {
            Text(
                "${options.size} available",
                color = TextMuted,
                style = MaterialTheme.typography.bodySmall,
                maxLines = 1,
            )
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

    BoxWithConstraints(
        Modifier
            .fillMaxSize()
            .background(Color.Black.copy(alpha = 0.72f))
            .clickable(enabled = false) {},
    ) {
        val phoneLandscape = isPhoneLandscape(maxWidth, maxHeight)
        Box(
            Modifier.fillMaxSize(),
            contentAlignment = if (phoneLandscape) Alignment.CenterEnd else Alignment.Center,
        ) {
            Card(
                modifier = modifier
                    .then(
                        if (phoneLandscape) {
                            Modifier
                                .padding(end = 12.dp)
                                .fillMaxWidth(0.9f)
                                .fillMaxHeight(0.9f)
                        } else {
                            Modifier
                                .fillMaxWidth(0.94f)
                                .fillMaxHeight(0.82f)
                        },
                    ),
                colors = CardDefaults.cardColors(containerColor = Panel),
                shape = RoundedCornerShape(22.dp),
            ) {
                if (phoneLandscape) {
                    Row(
                        Modifier.fillMaxSize().padding(14.dp),
                        horizontalArrangement = Arrangement.spacedBy(14.dp),
                        verticalAlignment = Alignment.CenterVertically,
                    ) {
                        PrintedWasteGameSummary(
                            game = game,
                            modifier = Modifier
                                .width(190.dp)
                                .fillMaxHeight(),
                        )
                        PrintedWasteOptionsColumn(
                            state = state,
                            zones = sortedZones,
                            selectedZoneId = selectedZoneId,
                            selectedZone = selectedZone,
                            autoZone = autoZone,
                            showRecommendedCard = true,
                            onSelectZone = { selectedZoneId = it },
                            onRetry = viewModel::refreshPrintedWasteQueues,
                            onDismiss = viewModel::dismissPrintedWasteSelector,
                            onDefault = { viewModel.launchWithPrintedWaste(null) },
                            onLaunch = { viewModel.launchWithPrintedWaste(selectedZone?.routingUrl) },
                            modifier = Modifier
                                .weight(1f)
                                .fillMaxHeight(),
                        )
                    }
                } else {
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
                        PrintedWasteOptionsColumn(
                            state = state,
                            zones = sortedZones,
                            selectedZoneId = selectedZoneId,
                            selectedZone = selectedZone,
                            autoZone = autoZone,
                            showRecommendedCard = true,
                            onSelectZone = { selectedZoneId = it },
                            onRetry = viewModel::refreshPrintedWasteQueues,
                            onDismiss = viewModel::dismissPrintedWasteSelector,
                            onDefault = { viewModel.launchWithPrintedWaste(null) },
                            onLaunch = { viewModel.launchWithPrintedWaste(selectedZone?.routingUrl) },
                            modifier = Modifier.weight(1f),
                        )
                    }
                }
            }
        }
    }
}

@Composable
private fun PrintedWasteGameSummary(
    game: GameInfo,
    modifier: Modifier = Modifier,
) {
    Column(modifier, verticalArrangement = Arrangement.spacedBy(10.dp)) {
        UrlImage(
            game.imageUrl,
            Modifier
                .fillMaxWidth()
                .weight(1f)
                .clip(RoundedCornerShape(16.dp)),
        )
        Column(verticalArrangement = Arrangement.spacedBy(3.dp)) {
            Text(game.title, fontWeight = FontWeight.Bold, maxLines = 2, overflow = TextOverflow.Ellipsis)
            Text("Free tier queue routing", color = TextMuted, style = MaterialTheme.typography.bodySmall, maxLines = 1)
        }
    }
}

@Composable
private fun PrintedWasteOptionsColumn(
    state: OpenNowUiState,
    zones: List<PrintedWasteZoneOption>,
    selectedZoneId: String?,
    selectedZone: PrintedWasteZoneOption?,
    autoZone: PrintedWasteZoneOption?,
    showRecommendedCard: Boolean,
    onSelectZone: (String) -> Unit,
    onRetry: () -> Unit,
    onDismiss: () -> Unit,
    onDefault: () -> Unit,
    onLaunch: () -> Unit,
    modifier: Modifier = Modifier,
) {
    Column(modifier, verticalArrangement = Arrangement.spacedBy(12.dp)) {
        if (state.printedWasteLoading) {
            Box(Modifier.weight(1f).fillMaxWidth(), contentAlignment = Alignment.Center) {
                Column(horizontalAlignment = Alignment.CenterHorizontally, verticalArrangement = Arrangement.spacedBy(12.dp)) {
                    CircularProgressIndicator(color = MaterialTheme.colorScheme.primary)
                    Text("Checking PrintedWaste queues and latency", color = TextMuted)
                }
            }
        } else if (state.printedWasteError != null) {
            Box(Modifier.weight(1f).fillMaxWidth(), contentAlignment = Alignment.Center) {
                Column(horizontalAlignment = Alignment.CenterHorizontally, verticalArrangement = Arrangement.spacedBy(12.dp)) {
                    Text(state.printedWasteError, color = Color(0xffff9f9f))
                    OutlinedButton(onClick = onRetry) { Text("Retry") }
                }
            }
        } else {
            if (showRecommendedCard) {
                autoZone?.let {
                    RecommendedPrintedWasteCard(it)
                }
            }
            LazyColumn(
                modifier = Modifier.weight(1f),
                verticalArrangement = Arrangement.spacedBy(8.dp),
            ) {
                items(zones, key = { it.zoneId }) { zoneOption ->
                    PrintedWasteZoneRow(
                        zoneOption = zoneOption,
                        selected = zoneOption.zoneId == selectedZoneId,
                        onClick = { onSelectZone(zoneOption.zoneId) },
                    )
                }
            }
        }

        Row(Modifier.fillMaxWidth(), horizontalArrangement = Arrangement.spacedBy(10.dp), verticalAlignment = Alignment.CenterVertically) {
            TextButton(onClick = onDismiss) { Text("Cancel") }
            OutlinedButton(onClick = onDefault, modifier = Modifier.weight(1f)) {
                Text("Default", maxLines = 1, overflow = TextOverflow.Ellipsis)
            }
            Button(
                onClick = onLaunch,
                enabled = !state.printedWasteLoading && selectedZone != null,
                modifier = Modifier.weight(1f),
            ) {
                Text("Launch", maxLines = 1, overflow = TextOverflow.Ellipsis)
            }
        }
    }
}

@Composable
private fun RecommendedPrintedWasteCard(zoneOption: PrintedWasteZoneOption) {
    Surface(
        modifier = Modifier.fillMaxWidth(),
        shape = RoundedCornerShape(16.dp),
        color = MaterialTheme.colorScheme.primary.copy(alpha = 0.12f),
    ) {
        Column(Modifier.padding(14.dp), verticalArrangement = Arrangement.spacedBy(4.dp)) {
            Text("Recommended: ${zoneOption.zoneId}", color = MaterialTheme.colorScheme.primary, fontWeight = FontWeight.Bold, maxLines = 1, overflow = TextOverflow.Ellipsis)
            Text(
                listOfNotNull(
                    zoneOption.pingMs?.let { "${it}ms" },
                    "Queue ${zoneOption.zone.QueuePosition}",
                    zoneOption.zone.eta?.let { formatPrintedWasteWait(it) },
                ).joinToString(" · "),
                color = TextMuted,
                maxLines = 1,
                overflow = TextOverflow.Ellipsis,
            )
        }
    }
}

@Composable
private fun PrintedWasteZoneRow(
    zoneOption: PrintedWasteZoneOption,
    selected: Boolean,
    onClick: () -> Unit,
) {
    val zone = zoneOption.zone
    Surface(
        modifier = Modifier
            .fillMaxWidth()
            .clip(RoundedCornerShape(12.dp))
            .clickable { onClick() },
        shape = RoundedCornerShape(12.dp),
        color = if (selected) MaterialTheme.colorScheme.primary.copy(alpha = 0.16f) else PanelAlt,
        tonalElevation = if (selected) 2.dp else 0.dp,
    ) {
        Row(Modifier.padding(12.dp), verticalAlignment = Alignment.CenterVertically) {
            Column(Modifier.weight(1f)) {
                Text(zoneOption.zoneId, fontWeight = FontWeight.Bold, color = if (selected) MaterialTheme.colorScheme.primary else TextPrimary)
                Text(regionLabel(zone.Region), color = TextMuted, style = MaterialTheme.typography.bodySmall)
            }
            Text(zoneOption.pingMs?.let { "${it}ms" } ?: "--", color = zoneOption.pingMs?.let(::pingColor) ?: TextMuted, fontWeight = FontWeight.Bold)
            Spacer(Modifier.width(10.dp))
            Text("Q ${zone.QueuePosition}", color = queueColor(zone.QueuePosition), fontWeight = FontWeight.Bold)
            zone.eta?.let {
                Spacer(Modifier.width(10.dp))
                Text(formatPrintedWasteWait(it), color = TextMuted, maxLines = 1, overflow = TextOverflow.Ellipsis)
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

private sealed interface UrlImageState {
    data object Empty : UrlImageState
    data object Loading : UrlImageState
    data object Failed : UrlImageState
    data class Loaded(val bitmap: ImageBitmap) : UrlImageState
}

@Composable
private fun UrlImage(url: String?, modifier: Modifier = Modifier) {
    val imageState by produceState<UrlImageState>(
        initialValue = if (url.isNullOrBlank()) UrlImageState.Empty else UrlImageState.Loading,
        key1 = url,
    ) {
        value = if (url.isNullOrBlank()) {
            UrlImageState.Empty
        } else {
            withContext(Dispatchers.IO) {
                runCatching {
                    URL(url).openStream().use { stream ->
                        BitmapFactory.decodeStream(stream)?.asImageBitmap()
                    }
                }.getOrNull()?.let(UrlImageState::Loaded) ?: UrlImageState.Failed
            }
        }
    }
    Box(modifier.background(Color(0xff102015)), contentAlignment = Alignment.Center) {
        when (val state = imageState) {
            UrlImageState.Loading -> GameImagePulse(Modifier.fillMaxSize())
            is UrlImageState.Loaded -> Image(
                bitmap = state.bitmap,
                contentDescription = null,
                modifier = Modifier.fillMaxSize(),
                contentScale = ContentScale.Crop,
            )
            UrlImageState.Empty,
            UrlImageState.Failed,
            -> OpenNowMark(42.dp)
        }
    }
}

@Composable
private fun GameImagePulse(modifier: Modifier = Modifier) {
    val transition = rememberInfiniteTransition(label = "game-image-loading")
    val pulse by transition.animateFloat(
        initialValue = 0.86f,
        targetValue = 1f,
        animationSpec = infiniteRepeatable(
            animation = tween(durationMillis = 950),
            repeatMode = RepeatMode.Reverse,
        ),
        label = "game-image-loading-alpha",
    )
    Box(
        modifier
            .background(Color(0xff0d1216))
            .background(Color(0xff1a2329).copy(alpha = pulse)),
    )
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

@Composable
private fun OpenNowAppIcon(size: androidx.compose.ui.unit.Dp) {
    Image(
        painter = painterResource(R.drawable.opennow_icon),
        contentDescription = "OpenNOW",
        modifier = Modifier.size(size),
        contentScale = ContentScale.Fit,
    )
}

internal val ColorQuality.label: String
    get() = when (this) {
        ColorQuality.EightBit420 -> "8-bit 4:2:0"
        ColorQuality.EightBit444 -> "8-bit 4:4:4"
        ColorQuality.TenBit420 -> "10-bit 4:2:0"
        ColorQuality.TenBit444 -> "10-bit 4:4:4"
    }

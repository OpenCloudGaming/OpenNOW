package com.opencloudgaming.opennow

import android.app.Activity
import android.content.Intent
import android.graphics.Color as AndroidColor
import android.graphics.BitmapFactory
import android.net.Uri
import android.os.Build
import android.speech.RecognizerIntent
import android.view.InputDevice
import android.view.KeyEvent
import android.view.MotionEvent
import android.view.PointerIcon
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
import androidx.compose.foundation.gestures.detectVerticalDragGestures
import androidx.compose.foundation.text.BasicTextField
import androidx.compose.foundation.Image
import androidx.compose.foundation.background
import androidx.compose.foundation.border
import androidx.compose.foundation.clickable
import androidx.compose.foundation.combinedClickable
import androidx.compose.foundation.focusable
import androidx.compose.foundation.layout.Arrangement
import androidx.compose.foundation.layout.Box
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
import androidx.compose.material3.Card
import androidx.compose.material3.CardDefaults
import androidx.compose.material3.Checkbox
import androidx.compose.material3.CircularProgressIndicator
import androidx.compose.material3.DropdownMenu
import androidx.compose.material3.DropdownMenuItem
import androidx.compose.material3.ElevatedButton
import androidx.compose.material3.FilterChip
import androidx.compose.material3.Icon
import androidx.compose.material3.IconButton
import androidx.compose.material3.LinearProgressIndicator
import androidx.compose.material3.MaterialTheme
import androidx.compose.material3.NavigationBar
import androidx.compose.material3.NavigationBarItem
import androidx.compose.material3.NavigationRail
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
import androidx.compose.runtime.rememberCoroutineScope
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
import androidx.compose.ui.platform.LocalContext
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
import java.util.Locale
import java.util.WeakHashMap
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
private val SettingsBackground = Color(0xff090b0d)
private val SettingsPanel = Color(0xff11161a)
private val SettingsPanelAlt = Color(0xff171d22)
private val SettingsText = Color(0xffeef3f5)
private val SettingsTextMuted = Color(0xff98a4aa)
private val externalMouseCaptureRetries = WeakHashMap<android.view.View, Runnable>()

private data class SettingsChoiceOption(val value: String, val label: String)
private data class ChoiceMenuOption(
    val value: String,
    val label: String,
    val enabled: Boolean = true,
    val badge: String? = null,
)

private val keyboardLayoutOptions = listOf(
    SettingsChoiceOption("en-US", "English (US)"),
    SettingsChoiceOption("en-GB", "English (UK)"),
    SettingsChoiceOption("tr-TR", "Turkish Q"),
    SettingsChoiceOption("de-DE", "German"),
    SettingsChoiceOption("fr-FR", "French"),
    SettingsChoiceOption("es-ES", "Spanish"),
    SettingsChoiceOption("es-MX", "Spanish (Latin America)"),
    SettingsChoiceOption("it-IT", "Italian"),
    SettingsChoiceOption("pt-PT", "Portuguese (Portugal)"),
    SettingsChoiceOption("pt-BR", "Portuguese (Brazil)"),
    SettingsChoiceOption("pl-PL", "Polish"),
    SettingsChoiceOption("ru-RU", "Russian"),
    SettingsChoiceOption("ja-JP", "Japanese"),
    SettingsChoiceOption("ko-KR", "Korean"),
    SettingsChoiceOption("zh-CN", "Chinese (Simplified)"),
    SettingsChoiceOption("zh-TW", "Chinese (Traditional)"),
)

private val gameLanguageOptions = listOf(
    SettingsChoiceOption("en_US", "English (US)"),
    SettingsChoiceOption("en_GB", "English (UK)"),
    SettingsChoiceOption("de_DE", "Deutsch"),
    SettingsChoiceOption("fr_FR", "Francais"),
    SettingsChoiceOption("es_ES", "Espanol (ES)"),
    SettingsChoiceOption("es_MX", "Espanol (MX)"),
    SettingsChoiceOption("it_IT", "Italiano"),
    SettingsChoiceOption("pt_PT", "Portugues (PT)"),
    SettingsChoiceOption("pt_BR", "Portugues (BR)"),
    SettingsChoiceOption("ru_RU", "Russian"),
    SettingsChoiceOption("pl_PL", "Polish"),
    SettingsChoiceOption("tr_TR", "Turkish"),
    SettingsChoiceOption("ar_SA", "Arabic"),
    SettingsChoiceOption("ja_JP", "Japanese"),
    SettingsChoiceOption("ko_KR", "Korean"),
    SettingsChoiceOption("zh_CN", "Chinese (Simplified)"),
    SettingsChoiceOption("zh_TW", "Chinese (Traditional)"),
    SettingsChoiceOption("th_TH", "Thai"),
    SettingsChoiceOption("vi_VN", "Vietnamese"),
    SettingsChoiceOption("id_ID", "Indonesian"),
    SettingsChoiceOption("cs_CZ", "Czech"),
    SettingsChoiceOption("el_GR", "Greek"),
    SettingsChoiceOption("hu_HU", "Hungarian"),
    SettingsChoiceOption("ro_RO", "Romanian"),
    SettingsChoiceOption("uk_UA", "Ukrainian"),
    SettingsChoiceOption("nl_NL", "Dutch"),
    SettingsChoiceOption("sv_SE", "Swedish"),
    SettingsChoiceOption("da_DK", "Danish"),
    SettingsChoiceOption("fi_FI", "Finnish"),
    SettingsChoiceOption("no_NO", "Norwegian"),
)
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
private fun uiAccentLabel(accent: UiAccent): String = when (accent) {
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
    val preferTvDeviceLogin = tvLogin && deviceCodeLoginAvailable
    val deviceLoginPrompt = state.deviceLoginPrompt.takeIf { deviceCodeLoginAvailable }
    LaunchedEffect(preferTvDeviceLogin, deviceLoginPrompt == null) {
        if (preferTvDeviceLogin && deviceLoginPrompt == null) {
            runCatching { signInFocusRequester.requestFocus() }
        }
    }
    if (preferTvDeviceLogin && deviceLoginPrompt != null) {
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
                        preferTvDeviceLogin -> stringResource(R.string.login_tv_start, state.selectedProvider.displayName)
                        else -> stringResource(R.string.login_with_provider, state.selectedProvider.displayName)
                    },
                )
            }
            if (!tvLogin && deviceCodeLoginAvailable) {
                TextButton(onClick = { viewModel.loginWithCode() }) {
                    Text("Use TV code sign-in")
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

private fun openExternalUrl(context: android.content.Context, url: String): Boolean {
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

@Composable
private fun MainShell(state: OpenNowUiState, viewModel: OpenNowViewModel) {
    val inStream = state.page == AppPage.Stream
    val streamingActive = inStream && state.streamStatus != "idle"
    val modalPickerOpen = state.pendingPrintedWasteGame != null || state.pendingStoreChoiceGame != null
    val tvProfile = state.codecReport?.androidTvProfile == true
    BackHandler(enabled = state.selectedGame != null && !inStream) {
        viewModel.clearSelectedGame()
    }
    BackHandler(enabled = tvProfile && !inStream && state.selectedGame == null && state.page != AppPage.Home) {
        viewModel.setPage(AppPage.Home)
    }
    Scaffold(
        contentWindowInsets = if (streamingActive) WindowInsets(0, 0, 0, 0) else ScaffoldDefaults.contentWindowInsets,
        bottomBar = {
            if (!inStream && !tvProfile) {
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
            Row(Modifier.fillMaxSize()) {
                if (tvProfile && !inStream) {
                    TvNavigationRail(state = state, viewModel = viewModel)
                }
                Column(Modifier.fillMaxSize()) {
                    if (!inStream) {
                        TopStatusBar(
                            state = state,
                            onResumeActiveSession = viewModel::resumeActiveSession,
                        )
                    }
                    Box(Modifier.fillMaxSize()) {
                        when (state.page) {
                            AppPage.Home -> HomeScreen(state, viewModel, tvProfile)
                            AppPage.Library -> LibraryScreen(state, viewModel, tvProfile)
                            AppPage.Settings -> SettingsScreen(state, viewModel, tvProfile)
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

@Composable
private fun TvNavigationRail(state: OpenNowUiState, viewModel: OpenNowViewModel) {
    NavigationRail(
        modifier = Modifier.fillMaxHeight(),
        containerColor = if (state.page == AppPage.Settings) SettingsBackground else MaterialTheme.colorScheme.background,
    ) {
        Spacer(Modifier.height(8.dp))
        TvNavigationRailItem(
            selected = state.page == AppPage.Home,
            onClick = { viewModel.setPage(AppPage.Home) },
            iconRes = R.drawable.ic_tab_store,
            label = stringResource(R.string.nav_store),
        )
        TvNavigationRailItem(
            selected = state.page == AppPage.Library,
            onClick = { viewModel.setPage(AppPage.Library) },
            iconRes = R.drawable.ic_tab_library,
            label = stringResource(R.string.nav_library),
        )
        TvNavigationRailItem(
            selected = state.page == AppPage.Settings,
            onClick = { viewModel.setPage(AppPage.Settings) },
            iconRes = R.drawable.ic_tab_settings,
            label = stringResource(R.string.nav_settings),
        )
    }
}

@Composable
private fun TvNavigationRailItem(selected: Boolean, onClick: () -> Unit, iconRes: Int, label: String) {
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
private fun NativeSearchField(
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

private fun handleVerticalDpadFocusMove(event: androidx.compose.ui.input.key.KeyEvent, focusManager: FocusManager): Boolean {
    if (event.type != KeyEventType.KeyDown) return false
    val direction = when (event.key) {
        Key.DirectionUp -> FocusDirection.Up
        Key.DirectionDown -> FocusDirection.Down
        else -> return false
    }
    focusManager.moveFocus(direction)
    return true
}

private fun isTvActivateKey(event: androidx.compose.ui.input.key.KeyEvent): Boolean =
    event.type == KeyEventType.KeyUp &&
        event.key in setOf(
            Key.DirectionCenter,
            Key.Enter,
            Key.NumPadEnter,
        )

@OptIn(ExperimentalComposeUiApi::class)
@Composable
private fun HomeScreen(state: OpenNowUiState, viewModel: OpenNowViewModel, tvProfile: Boolean) {
    val visibleGames = state.games.ifEmpty { state.catalogResult.games }
    val searchingCatalog = state.loadingGames && state.catalogSearch.isNotBlank()
    val gridState = rememberLazyGridState()
    val searchFocusRequester = remember { FocusRequester() }
    val scope = rememberCoroutineScope()
    val focusManager = LocalFocusManager.current
    val keyboardController = LocalSoftwareKeyboardController.current
    val showScrollActions = gridState.firstVisibleItemIndex > 0 || gridState.firstVisibleItemScrollOffset > 80
    SwipeToRefreshContainer(
        refreshing = state.loadingGames,
        showRefreshIndicator = !searchingCatalog,
        onRefresh = viewModel::refreshGames,
        modifier = Modifier.fillMaxSize(),
    ) {
        Column(
            Modifier
                .fillMaxSize()
                .padding(12.dp),
            verticalArrangement = Arrangement.spacedBy(10.dp),
        ) {
            NativeSearchField(
                modifier = Modifier.fillMaxWidth(),
                query = state.catalogSearch,
                onQueryChange = viewModel::setCatalogSearch,
                placeholder = stringResource(R.string.search_games),
                searching = searchingCatalog,
                focusRequester = searchFocusRequester,
                onOpen = {
                    if (gridState.firstVisibleItemIndex > 0 || gridState.firstVisibleItemScrollOffset > 0) {
                        scope.launch { gridState.animateScrollToItem(0) }
                    }
                },
            )
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

@Composable
private fun StoreScrollableControls(
    state: OpenNowUiState,
    onSortChange: (String) -> Unit,
    onFilterToggle: (String) -> Unit,
) {
    Column(verticalArrangement = Arrangement.spacedBy(10.dp)) {
        SortPicker(state.catalogResult.sortOptions, state.catalogSortId, onSortChange)
        FilterRow(state.catalogResult.filterGroups, state.catalogFilterIds, onFilterToggle)
        if (state.error != null) {
            Text(state.error.orEmpty(), color = Color(0xffff9f9f), modifier = Modifier.padding(horizontal = 4.dp))
        }
    }
}

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
private fun LibraryScreen(state: OpenNowUiState, viewModel: OpenNowViewModel, tvProfile: Boolean) {
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
                tvProfile,
                viewModel::selectGame,
                viewModel::updateFavorites,
                viewModel::play,
                viewModel::chooseStore,
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
    tvProfile: Boolean,
    onSelect: (GameInfo) -> Unit,
    onFavorite: (String) -> Unit,
    onPlay: (GameInfo) -> Unit,
    onChooseStore: (GameInfo) -> Unit,
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
    BoxWithConstraints(modifier.fillMaxSize()) {
        val columns = gameGridColumnCount(maxWidth)
        LazyVerticalGrid(
            modifier = Modifier.fillMaxSize(),
            columns = GridCells.Fixed(columns),
            contentPadding = PaddingValues(4.dp),
            horizontalArrangement = Arrangement.spacedBy(if (compact) 8.dp else 10.dp),
            verticalArrangement = Arrangement.spacedBy(if (compact) 10.dp else 12.dp),
        ) {
            gridItems(games, key = { it.id }) { game ->
                GameCard(game, game.id in favoriteIds, settings, tvProfile, cardHeight * scale, onSelect, onFavorite, onPlay, onChooseStore)
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
    gridState: androidx.compose.foundation.lazy.grid.LazyGridState,
    modifier: Modifier = Modifier,
) {
    if (games.isEmpty()) {
        Column(modifier.fillMaxSize(), verticalArrangement = Arrangement.spacedBy(10.dp)) {
            StoreScrollableControls(state, onSortChange, onFilterToggle)
            Box(Modifier.fillMaxSize(), contentAlignment = Alignment.Center) {
                Text(stringResource(R.string.no_games_loaded), color = TextMuted)
            }
        }
        return
    }
    val scale = settings.posterSizeScale.coerceIn(0.82f, 1.08f)
    val compact = settings.compactGameCards
    val cardHeight = if (compact) 252.dp else 286.dp
    BoxWithConstraints(modifier.fillMaxSize()) {
        val columns = gameGridColumnCount(maxWidth)
        LazyVerticalGrid(
            modifier = Modifier.fillMaxSize(),
            state = gridState,
            columns = GridCells.Fixed(columns),
            contentPadding = PaddingValues(0.dp),
            horizontalArrangement = Arrangement.spacedBy(if (compact) 8.dp else 10.dp),
            verticalArrangement = Arrangement.spacedBy(if (compact) 10.dp else 12.dp),
        ) {
            item(span = { GridItemSpan(maxLineSpan) }) {
                StoreScrollableControls(state, onSortChange, onFilterToggle)
            }
            gridItems(games, key = { it.id }) { game ->
                GameCard(game, game.id in favoriteIds, settings, tvProfile, cardHeight * scale, onSelect, onFavorite, onPlay, onChooseStore)
            }
        }
    }
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
            LongPressPlayButton(
                onClick = { onPlay(game) },
                onLongClick = { onChooseStore(game) },
                modifier = Modifier.fillMaxWidth(),
            )
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
) {
    val description = gameDescriptionForDetails(game)
    val chips = gameDetailChips(game)
    val launchStores = displayStoresForVariants(game.variants).ifEmpty { game.availableStores }.map(::gameStoreDisplayName).distinct()
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
                    .padding(if (shortHeight) 16.dp else 20.dp),
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
        }

        Column(
            Modifier
                .weight(1.08f)
                .fillMaxHeight(),
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
        contentAlignment = Alignment.Center,
    ) {
        val landscape = maxWidth > maxHeight
        Card(
            modifier = modifier
                .fillMaxWidth(if (landscape) 0.78f else 0.92f)
                .fillMaxHeight(if (landscape) 0.86f else 0.64f),
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
                        Text(stringResource(R.string.store_selector_choose_launcher), color = TextMuted, style = MaterialTheme.typography.bodySmall)
                    }
                }

                LazyColumn(
                    modifier = Modifier.weight(1f),
                    verticalArrangement = Arrangement.spacedBy(8.dp),
                ) {
                    items(variants, key = { it.id }) { variant ->
                        val isSelected = variant.id == selectedVariantId
                        val isSavedDefault = variant.id == defaultVariantId
                        Surface(
                            modifier = Modifier
                                .fillMaxWidth()
                                .clickable { selectedVariantId = variant.id },
                            shape = RoundedCornerShape(14.dp),
                            color = if (isSelected) MaterialTheme.colorScheme.primary.copy(alpha = 0.18f) else PanelAlt,
                        ) {
                            Row(
                                Modifier.padding(14.dp),
                                verticalAlignment = Alignment.CenterVertically,
                                horizontalArrangement = Arrangement.spacedBy(12.dp),
                            ) {
                                Column(Modifier.weight(1f), verticalArrangement = Arrangement.spacedBy(2.dp)) {
                                    Text(gameStoreDisplayName(variant.store), fontWeight = FontWeight.Bold)
                                    val details = listOf(
                                        if (isSavedDefault) stringResource(R.string.store_selector_default) else "",
                                        variantDetailsText(variant),
                                    ).filter { it.isNotBlank() }.joinToString(" - ")
                                    if (details.isNotBlank()) {
                                        Text(details, color = TextMuted, style = MaterialTheme.typography.bodySmall, maxLines = 1, overflow = TextOverflow.Ellipsis)
                                    }
                                }
                                if (isSelected) {
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
                }

                Row(
                    modifier = Modifier
                        .fillMaxWidth()
                        .clip(RoundedCornerShape(14.dp))
                        .clickable { rememberDefaultStore = !rememberDefaultStore }
                        .padding(vertical = 2.dp),
                    verticalAlignment = Alignment.CenterVertically,
                    horizontalArrangement = Arrangement.spacedBy(8.dp),
                ) {
                    Checkbox(
                        checked = rememberDefaultStore,
                        onCheckedChange = { rememberDefaultStore = it },
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
                            if (rememberDefaultStore || defaultVariantId != null) {
                                onSetDefaultStore(game.id, if (rememberDefaultStore) variant.id else null)
                            }
                            if (rememberDefaultStore) {
                                Toast.makeText(context, context.getString(R.string.store_selector_long_press_tip), Toast.LENGTH_LONG).show()
                            }
                            onLaunch(game, variant)
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
    }
}

@Composable
private fun SettingsScreen(state: OpenNowUiState, viewModel: OpenNowViewModel, tvProfile: Boolean) {
    var showSessionProxyWarning by remember { mutableStateOf(false) }
    val scrollState = rememberScrollState()
    val focusManager = LocalFocusManager.current
    if (showSessionProxyWarning) {
        SessionProxyWarningDialog(
            onCancel = { showSessionProxyWarning = false },
            onEnable = {
                viewModel.updateStreamSettings { s -> s.copy(sessionProxyEnabled = true) }
                showSessionProxyWarning = false
            },
        )
    }
    if (tvProfile) {
        Column(
            Modifier
                .fillMaxSize()
                .background(SettingsBackground)
                .onPreviewKeyEvent { handleVerticalDpadFocusMove(it, focusManager) }
                .verticalScroll(scrollState)
                .padding(20.dp),
            verticalArrangement = Arrangement.spacedBy(14.dp),
        ) {
            SettingsContent(state = state, viewModel = viewModel, showSessionProxyWarning = { showSessionProxyWarning = true })
        }
    } else {
        LazyColumn(
            Modifier
                .fillMaxSize()
                .background(SettingsBackground),
            contentPadding = PaddingValues(14.dp),
            verticalArrangement = Arrangement.spacedBy(12.dp),
        ) {
            item {
                SettingsContent(state = state, viewModel = viewModel, showSessionProxyWarning = { showSessionProxyWarning = true })
            }
        }
    }
}

@Composable
private fun SettingsContent(
    state: OpenNowUiState,
    viewModel: OpenNowViewModel,
    showSessionProxyWarning: () -> Unit,
) {
    val settings = state.settings
    Column(Modifier.fillMaxWidth(), verticalArrangement = Arrangement.spacedBy(12.dp)) {
    SettingsSection(stringResource(R.string.settings_section_stream)) {
                val fallbackMembershipTier = state.authSession?.user?.membershipTier
                val resolutionChoices = streamResolutionChoicesForAspect(settings.stream.aspectRatio).ifEmpty {
                    streamResolutionChoicesForAspect("16:9")
                }
                val selectedResolution = normalizeStreamResolutionForAspect(settings.stream.resolution, settings.stream.aspectRatio)
                ChoiceMenuRow(
                    label = stringResource(R.string.settings_resolution),
                    options = resolutionChoices.map { choice ->
                        val available = choice.isAvailableFor(state.subscriptionInfo, fallbackMembershipTier)
                        ChoiceMenuOption(
                            value = choice.value,
                            label = choice.label,
                            enabled = available,
                            badge = if (available) null else choice.requiredPlanLabel,
                        )
                    },
                    selectedLabel = resolutionChoices.firstOrNull { it.value == selectedResolution }?.label ?: selectedResolution,
                ) {
                    viewModel.updateStreamSettings { s -> s.copy(resolution = it) }
                }
                ChoiceMenuRow(
                    label = stringResource(R.string.settings_aspect_ratio),
                    options = streamAspectRatioOptions().map { aspectRatio ->
                        val choices = streamResolutionChoicesForAspect(aspectRatio)
                        val available = choices.any { it.isAvailableFor(state.subscriptionInfo, fallbackMembershipTier) }
                        ChoiceMenuOption(
                            value = aspectRatio,
                            label = aspectRatio,
                            enabled = available,
                            badge = if (available) null else choices.firstNotNullOfOrNull { it.requiredPlanLabel },
                        )
                    },
                    selectedLabel = settings.stream.aspectRatio,
                ) {
                    viewModel.updateStreamSettings { s ->
                        s.copy(
                            aspectRatio = it,
                            resolution = normalizeStreamResolutionForAspect(s.resolution, it),
                        )
                    }
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
                SettingSwitch(stringResource(R.string.settings_session_proxy), settings.stream.sessionProxyEnabled) { enabled ->
                    if (enabled) {
                        showSessionProxyWarning()
                    } else {
                        viewModel.updateStreamSettings { s -> s.copy(sessionProxyEnabled = false) }
                    }
                }
                Text(
                    stringResource(R.string.settings_session_proxy_hint),
                    color = SettingsTextMuted,
                    style = MaterialTheme.typography.bodySmall,
                )
                if (settings.stream.sessionProxyEnabled) {
                    OutlinedTextField(
                        value = settings.stream.sessionProxyUrl,
                        onValueChange = { value -> viewModel.updateStreamSettings { s -> s.copy(sessionProxyUrl = value) } },
                        modifier = Modifier.fillMaxWidth(),
                        singleLine = true,
                        label = { Text(stringResource(R.string.settings_session_proxy_url)) },
                        placeholder = { Text("http://127.0.0.1:8080") },
                    )
                }
                SettingSwitch(stringResource(R.string.settings_l4s), settings.stream.enableL4S) { viewModel.updateStreamSettings { s -> s.copy(enableL4S = it) } }
                SettingSwitch(stringResource(R.string.settings_cloud_gsync), settings.stream.enableCloudGsync) { viewModel.updateStreamSettings { s -> s.copy(enableCloudGsync = it) } }
            }
    SettingsSection("Input") {
                NumberSlider("Mouse sensitivity", settings.stream.mouseSensitivity, 0.25f, 3f, 0.05f) {
                    viewModel.updateStreamSettings { s -> s.copy(mouseSensitivity = it) }
                }
                NumberSlider("Mouse acceleration", settings.stream.mouseAcceleration.toFloat(), 1f, 150f, 1f) {
                    viewModel.updateStreamSettings { s -> s.copy(mouseAcceleration = it.roundToInt()) }
                }
                ChoiceOptionRow("Keyboard layout", keyboardLayoutOptions, settings.stream.keyboardLayout) {
                    viewModel.updateStreamSettings { s -> s.copy(keyboardLayout = it) }
                }
                ChoiceOptionRow("Game language", gameLanguageOptions, settings.stream.gameLanguage) {
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
    SettingsSection("Account") {
                AccountSettingsPanel(state = state, viewModel = viewModel)
            }
    SettingsSection("Codec Diagnostics") {
                CodecDiagnosticsPanel(state.codecReport)
            }
    SettingsSection("Debug Logs") {
                DebugLogsPanel(state = state, viewModel = viewModel)
            }
    SettingsSection("About") {
                AppVersionPanel()
            }
    }
}

@Composable
private fun AccountSettingsPanel(state: OpenNowUiState, viewModel: OpenNowViewModel) {
    val currentUserId = state.authSession?.user?.userId
    Column(verticalArrangement = Arrangement.spacedBy(10.dp)) {
        state.savedAccounts.ifEmpty {
            state.authSession?.toSavedAccount()?.let { listOf(it) } ?: emptyList()
        }.forEach { account ->
            val selected = account.userId == currentUserId
            Surface(
                modifier = Modifier.fillMaxWidth(),
                shape = RoundedCornerShape(14.dp),
                color = if (selected) MaterialTheme.colorScheme.primary.copy(alpha = 0.16f) else MaterialTheme.colorScheme.surfaceVariant.copy(alpha = 0.76f),
            ) {
                Row(
                    Modifier.padding(horizontal = 12.dp, vertical = 10.dp),
                    verticalAlignment = Alignment.CenterVertically,
                    horizontalArrangement = Arrangement.spacedBy(10.dp),
                ) {
                    Column(Modifier.weight(1f)) {
                        Text(account.displayName.ifBlank { "NVIDIA Account" }, color = SettingsText, fontWeight = FontWeight.SemiBold, maxLines = 1, overflow = TextOverflow.Ellipsis)
                        Text(
                            listOfNotNull(account.email?.takeIf { it.isNotBlank() }, account.providerCode, account.membershipTier).joinToString(" - "),
                            color = SettingsTextMuted,
                            style = MaterialTheme.typography.bodySmall,
                            maxLines = 1,
                            overflow = TextOverflow.Ellipsis,
                        )
                    }
                    if (selected) {
                        Text("Active", color = MaterialTheme.colorScheme.primary, style = MaterialTheme.typography.labelMedium, fontWeight = FontWeight.Bold)
                    } else {
                        OutlinedButton(onClick = { viewModel.switchAccount(account.userId) }, contentPadding = PaddingValues(horizontal = 10.dp, vertical = 6.dp)) {
                            Text("Switch")
                        }
                    }
                }
            }
        }
        Row(horizontalArrangement = Arrangement.spacedBy(8.dp), modifier = Modifier.fillMaxWidth()) {
            Button(onClick = { viewModel.login() }, modifier = Modifier.weight(1f)) { Text("Add account") }
            OutlinedButton(onClick = viewModel::logout, modifier = Modifier.weight(1f)) { Text("Sign out") }
        }
        OutlinedButton(onClick = viewModel::logoutAll, modifier = Modifier.fillMaxWidth()) { Text("Sign out all accounts") }
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

@Composable
private fun CodecDiagnosticsPanel(report: RuntimeCodecReport?) {
    if (report == null) {
        Text(stringResource(R.string.settings_codec_diagnostics_unavailable), color = SettingsTextMuted)
        return
    }
    val clipboard = LocalClipboardManager.current
    var copied by remember(report) { mutableStateOf(false) }
    val safeDecoders = report.capabilities.count { it.streamingRealtimeSafe() }
    Column(verticalArrangement = Arrangement.spacedBy(10.dp)) {
        Button(
            onClick = {
                clipboard.setText(AnnotatedString(formatCodecDiagnosticReport(report)))
                copied = true
            },
            modifier = Modifier.fillMaxWidth(),
        ) {
            Text(
                if (copied) {
                    stringResource(R.string.settings_codec_diagnostics_copied)
                } else {
                    stringResource(R.string.settings_codec_diagnostics_copy)
                },
                maxLines = 1,
                overflow = TextOverflow.Ellipsis,
            )
        }
        Row(horizontalArrangement = Arrangement.spacedBy(8.dp), modifier = Modifier.fillMaxWidth()) {
            CodecSummaryChip("${safeDecoders}/${report.capabilities.size}", "real-time decoders")
            CodecSummaryChip(if (report.lowPowerGpuProfile) "Low power" else "Standard", "device profile")
            CodecSummaryChip(if (report.androidTvProfile) "TV" else "Mobile", "shell")
        }
        report.capabilities.forEach { capability ->
            CodecCapabilityRow(capability)
        }
        Text(
            report.nativeRuntimeSummary.replace("{", "").replace("}", "").replace("\"", ""),
            color = SettingsTextMuted,
            style = MaterialTheme.typography.bodySmall,
            maxLines = 3,
            overflow = TextOverflow.Ellipsis,
        )
    }
}

private fun formatCodecDiagnosticReport(report: RuntimeCodecReport): String = buildString {
    appendLine("OpenNOW Android codec diagnostics")
    appendLine("nativeRuntimeSummary=${report.nativeRuntimeSummary}")
    appendLine("androidTvProfile=${report.androidTvProfile}")
    appendLine("lowPowerGpuProfile=${report.lowPowerGpuProfile}")
    report.capabilities.forEach { capability ->
        appendLine()
        appendLine("codec=${capability.codec}")
        appendLine("decoderAvailable=${capability.decoderAvailable}")
        appendLine("decoderName=${capability.decoderName ?: "none"}")
        appendLine("hardwareDecoder=${capability.hardwareDecoder}")
        appendLine("realtimeSafe=${capability.realtimeSafe}")
        appendLine("webRtcDecoderAvailable=${capability.webRtcDecoderAvailable ?: "unknown"}")
        appendLine("webRtcDecoderName=${capability.webRtcDecoderName ?: "none"}")
        appendLine("webRtcHardwareDecoderAvailable=${capability.webRtcHardwareDecoderAvailable ?: "unknown"}")
        appendLine("webRtcProfiles=${capability.webRtcCodecProfiles.joinToString(", ").ifBlank { "none" }}")
        appendLine("encoderAvailable=${capability.encoderAvailable}")
        appendLine("encoderName=${capability.encoderName ?: "none"}")
        appendLine("hardwareEncoder=${capability.hardwareEncoder}")
    }
}

@Composable
private fun RowScope.CodecSummaryChip(value: String, label: String) {
    Surface(
        modifier = Modifier.weight(1f),
        shape = RoundedCornerShape(14.dp),
        color = MaterialTheme.colorScheme.surfaceVariant.copy(alpha = 0.76f),
    ) {
        Column(Modifier.padding(horizontal = 10.dp, vertical = 8.dp)) {
            Text(value, color = SettingsText, fontWeight = FontWeight.Bold, maxLines = 1, overflow = TextOverflow.Ellipsis)
            Text(label, color = SettingsTextMuted, style = MaterialTheme.typography.labelSmall, maxLines = 1, overflow = TextOverflow.Ellipsis)
        }
    }
}

@Composable
private fun CodecCapabilityRow(capability: CodecCapability) {
    val streamingReady = capability.streamingDecoderAvailable()
    val healthy = capability.streamingRealtimeSafe()
    val status = when {
        healthy -> "Ready"
        streamingReady -> "WebRTC ready"
        capability.decoderAvailable -> "Platform only"
        else -> "Unavailable"
    }
    Surface(
        modifier = Modifier.fillMaxWidth(),
        shape = RoundedCornerShape(14.dp),
        color = MaterialTheme.colorScheme.surfaceVariant.copy(alpha = 0.76f),
    ) {
        Column(Modifier.padding(horizontal = 12.dp, vertical = 10.dp), verticalArrangement = Arrangement.spacedBy(4.dp)) {
            Row(verticalAlignment = Alignment.CenterVertically, horizontalArrangement = Arrangement.spacedBy(8.dp)) {
                Text(capability.codec.name, color = SettingsText, fontWeight = FontWeight.Bold, modifier = Modifier.weight(1f))
                Text(
                    status,
                    color = if (healthy) MaterialTheme.colorScheme.primary else Color(0xffffc266),
                    style = MaterialTheme.typography.labelMedium,
                    fontWeight = FontWeight.Bold,
                )
            }
            Text(
                "WebRTC: ${capability.streamingDecoderName() ?: "none"}",
                color = SettingsTextMuted,
                style = MaterialTheme.typography.bodySmall,
                maxLines = 1,
                overflow = TextOverflow.Ellipsis,
            )
            Text(
                "Hardware decode ${yesNo(capability.streamingHardwareDecoderAvailable())} - platform ${capability.decoderName ?: "none"}",
                color = SettingsTextMuted,
                style = MaterialTheme.typography.bodySmall,
                maxLines = 1,
                overflow = TextOverflow.Ellipsis,
            )
        }
    }
}

private fun yesNo(value: Boolean): String = if (value) "yes" else "no"

@Composable
private fun AppVersionPanel() {
    Row(
        Modifier
            .fillMaxWidth()
            .clip(RoundedCornerShape(14.dp))
            .background(SettingsPanelAlt)
            .padding(horizontal = 14.dp, vertical = 12.dp),
        horizontalArrangement = Arrangement.spacedBy(12.dp),
        verticalAlignment = Alignment.CenterVertically,
    ) {
        Column(Modifier.weight(1f), verticalArrangement = Arrangement.spacedBy(2.dp)) {
            Text("OpenNOW Android", color = SettingsText, fontWeight = FontWeight.SemiBold)
            Text("Version ${BuildConfig.VERSION_NAME}", color = SettingsTextMuted, style = MaterialTheme.typography.bodySmall)
        }
        Text("Build ${BuildConfig.VERSION_CODE}", color = SettingsTextMuted, style = MaterialTheme.typography.labelMedium)
    }
}

@Composable
private fun DebugLogsPanel(state: OpenNowUiState, viewModel: OpenNowViewModel) {
    val context = LocalContext.current
    val clipboard = LocalClipboardManager.current
    var copied by remember { mutableStateOf(false) }
    var saved by remember { mutableStateOf(false) }
    var saveError by remember { mutableStateOf<String?>(null) }
    var pendingLogText by remember { mutableStateOf("") }
    val saveLauncher = rememberLauncherForActivityResult(ActivityResultContracts.CreateDocument("text/plain")) { uri ->
        if (uri == null) return@rememberLauncherForActivityResult
        runCatching {
            context.contentResolver.openOutputStream(uri)?.use { output ->
                output.write(pendingLogText.toByteArray(Charsets.UTF_8))
            } ?: error("Could not open log file")
        }.onSuccess {
            saved = true
            saveError = null
        }.onFailure { error ->
            saveError = error.message ?: "Could not save logs"
        }
    }
    Text("Includes launch state, queue state, ads, stream settings, input settings, and codec capabilities.", color = SettingsTextMuted)
    Row(horizontalArrangement = Arrangement.spacedBy(8.dp), modifier = Modifier.fillMaxWidth()) {
        Button(
            onClick = {
                clipboard.setText(AnnotatedString(viewModel.debugLogText()))
                copied = true
            },
            modifier = Modifier.weight(1f),
        ) {
            Text(if (copied) "Copied logs" else "Copy logs", maxLines = 1, overflow = TextOverflow.Ellipsis)
        }
        OutlinedButton(
            onClick = {
                pendingLogText = viewModel.debugLogText()
                saved = false
                saveError = null
                saveLauncher.launch("opennow-android-logs.txt")
            },
            modifier = Modifier.weight(1f),
        ) {
            Text(if (saved) "Saved .txt" else "Save .txt", maxLines = 1, overflow = TextOverflow.Ellipsis)
        }
    }
    state.error?.let { error ->
        OutlinedButton(onClick = {
            clipboard.setText(AnnotatedString(error))
            copied = true
        }) {
            Text("Copy error")
        }
    }
    saveError?.let {
        Text(it, color = Color(0xffff9f9f), style = MaterialTheme.typography.bodySmall)
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
    var streamStats by remember { mutableStateOf(StreamRuntimeStats()) }
    val streamReady = session?.status in setOf(2, 3)
    val streamSettings = state.activeStreamSettings ?: state.settings.stream
    BackHandler(enabled = streamReady) {
        when {
            exitConfirmOpen -> exitConfirmOpen = false
            keyboardOpen -> keyboardOpen = false
            controlsOpen -> controlsOpen = false
            else -> exitConfirmOpen = true
        }
    }
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
            NativeStreamInputRouter.setSystemMenuHandler(null)
            NativeStreamInputRouter.setSystemBackHandler(null)
            NativeStreamInputRouter.setStreamUiActive(false)
            NativeStreamInputRouter.detach(client)
            client.release()
        }
    }

    LaunchedEffect(streamReady, controlsOpen, exitConfirmOpen, keyboardOpen) {
        NativeStreamInputRouter.setStreamUiActive(streamReady && (controlsOpen || exitConfirmOpen || keyboardOpen))
        NativeStreamInputRouter.setSystemMenuHandler {
            keyboardOpen = false
            exitConfirmOpen = false
            controlsOpen = true
        }
        NativeStreamInputRouter.setSystemBackHandler {
            when {
                exitConfirmOpen -> exitConfirmOpen = false
                keyboardOpen -> keyboardOpen = false
                controlsOpen -> controlsOpen = false
                else -> exitConfirmOpen = true
            }
        }
    }

    LaunchedEffect(streamReady, state.settings.androidTouch.mousePad) {
        NativeStreamInputRouter.setTouchMouseEnabled(streamReady && state.settings.androidTouch.mousePad)
    }
    LaunchedEffect(streamReady, state.settings.androidTouch.mousePad, controlsOpen, exitConfirmOpen, keyboardOpen, state.settings.androidTouch.enabled) {
        NativeStreamInputRouter.setCaptureAllTouch(
            streamReady &&
                state.settings.androidTouch.mousePad &&
                !controlsOpen &&
                !exitConfirmOpen &&
                !keyboardOpen &&
                !state.settings.androidTouch.enabled,
        )
    }
    DisposableEffect(Unit) {
        onDispose {
            NativeStreamInputRouter.setCaptureAllTouch(false)
        }
    }
    LaunchedEffect(session?.sessionId, session?.status, streamSettings) {
        if (session != null && streamReady) {
            client.start(session, streamSettings)
        }
    }

    Box(Modifier.fillMaxSize().background(Color.Black)) {
        if (session == null && state.streamStatus != "idle") {
            QueueLoadingScreen(state, viewModel)
        } else if (session == null) {
            NoActiveStreamScreen(
                canEndSession = state.authSession != null,
                onBack = { viewModel.setPage(AppPage.Home) },
                onEndSession = viewModel::stopStream,
            )
        } else if (!streamReady) {
            QueueLoadingScreen(state, viewModel)
        } else {
            StreamVideoSurface(
                client = client,
                settings = streamSettings,
                captureExternalMouse = streamReady && !controlsOpen && !exitConfirmOpen && !keyboardOpen,
                touchMouseEnabled = state.settings.androidTouch.mousePad,
                externalMouseCaptureRoot = activity?.window?.decorView,
            )
            if (statsVisible) {
                StreamStatsPill(
                    gameTitle = game?.title ?: "Stream",
                    status = streamState,
                    streamStats = streamStats,
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
private fun StreamVideoSurface(
    client: NativeStreamClient,
    settings: StreamSettings,
    captureExternalMouse: Boolean,
    touchMouseEnabled: Boolean,
    externalMouseCaptureRoot: android.view.View?,
    modifier: Modifier = Modifier,
) {
    val rootView = LocalView.current
    val captureRootView = externalMouseCaptureRoot ?: rootView
    val (streamWidth, streamHeight) = streamResolutionPixels(settings)
    val streamAspect = (streamWidth.toFloat() / streamHeight.toFloat()).takeIf { it.isFinite() && it > 0f } ?: (16f / 9f)
    DisposableEffect(rootView, captureRootView, captureExternalMouse) {
        captureRootView.captureExternalMousePointer(captureExternalMouse)
        if (captureExternalMouse) {
            captureRootView.hideAndroidPointerTree()
        } else {
            captureRootView.showAndroidPointerTree()
        }
        onDispose {
            captureRootView.captureExternalMousePointer(false)
            captureRootView.showAndroidPointerTree()
        }
    }
    BoxWithConstraints(
        modifier
            .fillMaxSize()
            .background(Color.Black),
        contentAlignment = Alignment.Center,
    ) {
        val containerAspect = if (maxHeight.value > 0f) maxWidth.value / maxHeight.value else streamAspect
        val viewportModifier = if (containerAspect > streamAspect) {
            Modifier
                .fillMaxHeight()
                .aspectRatio(streamAspect)
        } else {
            Modifier
                .fillMaxWidth()
                .aspectRatio(streamAspect)
        }
        Box(
            viewportModifier
                .clipToBounds()
                .background(Color.Black),
        ) {
            AndroidView(
                modifier = Modifier.fillMaxSize(),
                factory = { ctx ->
                    client.createRenderer(ctx).apply {
                        isFocusable = false
                        isFocusableInTouchMode = false
                        hideAndroidPointerTree()
                    }
                },
                update = { renderer ->
                    renderer.isFocusable = false
                    renderer.isFocusableInTouchMode = false
                    if (captureExternalMouse) {
                        captureRootView.hideAndroidPointerTree()
                        renderer.hideAndroidPointerTree()
                    } else {
                        captureRootView.showAndroidPointerTree()
                        renderer.showAndroidPointerTree()
                    }
                    renderer.setOnKeyListener(null)
                    renderer.setOnGenericMotionListener { _, event ->
                        if (captureExternalMouse) captureRootView.hideAndroidPointerTree()
                        client.dispatchMotion(event)
                    }
                    renderer.setOnTouchListener { view, event ->
                        NativeStreamInputRouter.dispatchTouch(event, view.width, view.height)
                    }
                },
            )
            FingerMouseInputLayer(
                enabled = touchMouseEnabled,
                modifier = Modifier.matchParentSize(),
            )
        }
    }
}

private fun MotionEvent.isExternalMousePointerEvent(): Boolean =
    (source and InputDevice.SOURCE_MOUSE) == InputDevice.SOURCE_MOUSE ||
        (source and InputDevice.SOURCE_MOUSE_RELATIVE) == InputDevice.SOURCE_MOUSE_RELATIVE

private fun androidNullPointerIcon(view: android.view.View): PointerIcon? =
    if (Build.VERSION.SDK_INT >= 24) {
        PointerIcon.getSystemIcon(view.context, PointerIcon.TYPE_NULL)
    } else {
        null
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

private fun android.view.View.captureExternalMousePointer(enabled: Boolean) {
    if (Build.VERSION.SDK_INT < 26) return
    val view = this
    val requestCapture = {
        if (!view.isFocusableInTouchMode) {
            view.isFocusableInTouchMode = true
        }
        if (!view.hasFocus()) {
            view.requestFocus()
        }
        if (!view.hasPointerCapture() && view.isAttachedToWindow && view.hasWindowFocus()) {
            runCatching { view.requestPointerCapture() }
        }
    }
    externalMouseCaptureRetries.remove(view)?.let(view::removeCallbacks)
    setOnCapturedPointerListener(
        if (enabled) {
            android.view.View.OnCapturedPointerListener { _, event ->
                NativeStreamInputRouter.dispatchCapturedExternalMouse(event)
            }
        } else {
            null
        },
    )
    if (enabled) {
        setOnFocusChangeListener { _, hasFocus ->
            if (hasFocus) post { requestCapture() }
        }
        lateinit var retryCapture: Runnable
        retryCapture = Runnable {
            requestCapture()
            if (externalMouseCaptureRetries[view] === retryCapture) {
                view.postDelayed(retryCapture, 350L)
            }
        }
        externalMouseCaptureRetries[view] = retryCapture
        post(retryCapture)
    } else if (hasPointerCapture()) {
        runCatching { releasePointerCapture() }
        setOnFocusChangeListener(null)
        externalMouseCaptureRetries.remove(view)?.let(view::removeCallbacks)
    } else {
        setOnFocusChangeListener(null)
        externalMouseCaptureRetries.remove(view)?.let(view::removeCallbacks)
    }
}

@Composable
private fun FingerMouseInputLayer(enabled: Boolean, modifier: Modifier = Modifier) {
    if (!enabled) return
    var width by remember { mutableStateOf(0) }
    var height by remember { mutableStateOf(0) }
    Box(
        modifier
            .onSizeChanged {
                width = it.width
                height = it.height
            }
            .pointerInteropFilter { event ->
                if (event.actionMasked == MotionEvent.ACTION_DOWN) {
                    NativeInputDiagnostics.add("compose finger layer down size=${width}x$height")
                }
                NativeStreamInputRouter.dispatchTouch(event, width, height)
            },
    )
}

@Composable
private fun NoActiveStreamScreen(
    canEndSession: Boolean,
    onBack: () -> Unit,
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
    onExit: () -> Unit,
    onTouchControlsToggle: () -> Unit,
    onMousePadToggle: () -> Unit,
    onTouchScaleChange: (Float) -> Unit,
    onButtonScaleChange: (Float) -> Unit,
    onStickScaleChange: (Float) -> Unit,
    onOpacityChange: (Float) -> Unit,
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
                        onClick = onExit,
                        contentPadding = PaddingValues(horizontal = 10.dp, vertical = 6.dp),
                    ) {
                        Text("Exit")
                    }
                    OutlinedButton(
                        onClick = onClose,
                        modifier = Modifier.focusRequester(doneFocusRequester),
                        contentPadding = PaddingValues(horizontal = 10.dp, vertical = 6.dp),
                    ) {
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
    modifier: Modifier = Modifier,
) {
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
private fun TouchOverlay(client: NativeStreamClient, touch: AndroidTouchSettings, modifier: Modifier = Modifier) {
    val opacity = touch.opacity
    val layoutScale = touch.scale
    val buttonScale = touch.buttonScale
    val stickScale = touch.stickScale

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

    Row(
        modifier
            .fillMaxWidth()
            .padding(18.dp)
            .onGloballyPositioned { coordinates ->
                val bounds = coordinates.boundsInRoot()
                NativeStreamInputRouter.setTouchControllerPassthroughBounds(
                    bounds.left.roundToInt(),
                    bounds.top.roundToInt(),
                    bounds.right.roundToInt(),
                    bounds.bottom.roundToInt(),
                )
            },
        horizontalArrangement = Arrangement.SpaceBetween,
        verticalAlignment = Alignment.Bottom,
    ) {
        Column(verticalArrangement = Arrangement.spacedBy(8.dp), horizontalAlignment = Alignment.Start) {
            if (touch.enabled) {
                Row(horizontalArrangement = Arrangement.spacedBy(8.dp)) {
                    GamepadButton("LB", 0x0100, client, opacity, 48.dp * buttonScale * layoutScale)
                    GamepadTriggerButton("LT", left = true, client = client, opacity = opacity, size = 48.dp * buttonScale * layoutScale)
                }
                Spacer(Modifier.height(44.dp * buttonScale * layoutScale))
                Row(
                    horizontalArrangement = Arrangement.spacedBy(12.dp),
                    verticalAlignment = Alignment.Bottom,
                ) {
                    VirtualStick(
                        label = "L",
                        client = client,
                        opacity = opacity,
                        diameter = 116.dp * stickScale * layoutScale,
                        onChange = client::setVirtualLeftStick,
                    )
                    DpadCluster(client, opacity, buttonScale * layoutScale)
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
                Row(
                    horizontalArrangement = Arrangement.spacedBy(12.dp),
                    verticalAlignment = Alignment.Bottom,
                ) {
                    VirtualStick(
                        label = "R",
                        client = client,
                        opacity = opacity,
                        diameter = 104.dp * stickScale * layoutScale,
                        onChange = client::setVirtualRightStick,
                    )
                    FaceButtonCluster(client, opacity, buttonScale * layoutScale)
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
private fun FaceButtonCluster(client: NativeStreamClient, opacity: Float, scale: Float) {
    Column(horizontalAlignment = Alignment.CenterHorizontally, verticalArrangement = Arrangement.spacedBy(6.dp)) {
        GamepadButton("Y", 0x8000, client, opacity, 54.dp * scale)
        Row(horizontalArrangement = Arrangement.spacedBy(6.dp)) {
            GamepadButton("X", 0x4000, client, opacity, 54.dp * scale)
            Spacer(Modifier.size(54.dp * scale))
            GamepadButton("B", 0x2000, client, opacity, 54.dp * scale)
        }
        GamepadButton("A", 0x1000, client, opacity, 54.dp * scale)
    }
}

@Composable
private fun DpadCluster(client: NativeStreamClient, opacity: Float, scale: Float) {
    val buttonSize = 54.dp * scale
    Column(horizontalAlignment = Alignment.CenterHorizontally, verticalArrangement = Arrangement.spacedBy(6.dp)) {
        GamepadButton("↑", 0x0001, client, opacity, buttonSize)
        Row(horizontalArrangement = Arrangement.spacedBy(6.dp)) {
            GamepadButton("←", 0x0004, client, opacity, buttonSize)
            Spacer(Modifier.size(buttonSize))
            GamepadButton("→", 0x0008, client, opacity, buttonSize)
        }
        GamepadButton("↓", 0x0002, client, opacity, buttonSize)
    }
}

@Composable
private fun GamepadTriggerButton(label: String, left: Boolean, client: NativeStreamClient, opacity: Float, size: androidx.compose.ui.unit.Dp) {
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
            .pointerInput(left) {
                awaitPointerEventScope {
                    while (true) {
                        val event = awaitPointerEvent()
                        val down = event.changes.any { it.pressed }
                        if (down != pressed) {
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
}

@Composable
private fun GamepadButton(label: String, mask: Int, client: NativeStreamClient, opacity: Float, size: androidx.compose.ui.unit.Dp) {
    var pressed by remember { mutableStateOf(false) }
    val accent = MaterialTheme.colorScheme.primary
    val idleSurface = MaterialTheme.colorScheme.surfaceVariant
    Box(
        Modifier
            .size(size)
            .clip(CircleShape)
            .background((if (pressed) accent else idleSurface).copy(alpha = opacity))
            .border(1.dp, accent.copy(alpha = opacity), CircleShape)
            .pointerInput(mask) {
                awaitPointerEventScope {
                    while (true) {
                        val event = awaitPointerEvent()
                        val down = event.changes.any { it.pressed }
                        if (down != pressed) {
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
}

@Composable
private fun SettingsSection(title: String, content: @Composable ColumnScope.() -> Unit) {
    val sectionShape = RoundedCornerShape(14.dp)
    Card(
        modifier = Modifier.fillMaxWidth(),
        colors = CardDefaults.cardColors(containerColor = MaterialTheme.colorScheme.surface),
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
    val focusManager = LocalFocusManager.current
    var focused by remember { mutableStateOf(false) }
    val shape = RoundedCornerShape(14.dp)
    val toggle = { onCheckedChange(!checked) }
    Row(
        Modifier
            .fillMaxWidth()
            .onFocusChanged { focused = it.isFocused || it.hasFocus }
            .border(
                width = if (focused) 2.dp else 1.dp,
                color = if (focused) MaterialTheme.colorScheme.primary else Color.Transparent,
                shape = shape,
            )
            .clip(shape)
            .background(MaterialTheme.colorScheme.surfaceVariant.copy(alpha = 0.76f))
            .clickable(onClick = toggle)
            .onPreviewKeyEvent { event ->
                when {
                    isTvActivateKey(event) -> {
                        toggle()
                        true
                    }
                    else -> handleVerticalDpadFocusMove(event, focusManager)
                }
            }
            .focusable()
            .padding(horizontal = 12.dp, vertical = 8.dp),
        verticalAlignment = Alignment.CenterVertically,
    ) {
        Text(label, Modifier.weight(1f), color = MaterialTheme.colorScheme.onSurface, maxLines = 2, overflow = TextOverflow.Ellipsis)
        Switch(checked = checked, onCheckedChange = onCheckedChange)
    }
}

@Composable
private fun SessionProxyWarningDialog(onCancel: () -> Unit, onEnable: () -> Unit) {
    AlertDialog(
        onDismissRequest = onCancel,
        title = { Text(stringResource(R.string.settings_session_proxy_warning_title), fontWeight = FontWeight.Bold) },
        text = {
            Column(verticalArrangement = Arrangement.spacedBy(10.dp)) {
                Text(stringResource(R.string.settings_session_proxy_warning_traffic), style = MaterialTheme.typography.bodySmall)
                Text(stringResource(R.string.settings_session_proxy_warning_breakage), style = MaterialTheme.typography.bodySmall)
                Text(stringResource(R.string.settings_session_proxy_warning_trust), style = MaterialTheme.typography.bodySmall)
            }
        },
        confirmButton = {
            Button(onClick = onEnable) {
                Text(stringResource(R.string.settings_session_proxy_warning_enable))
            }
        },
        dismissButton = {
            TextButton(onClick = onCancel) {
                Text(stringResource(R.string.action_cancel))
            }
        },
        containerColor = SettingsPanel,
        titleContentColor = SettingsText,
        textContentColor = SettingsTextMuted,
    )
}

@Composable
private fun NumberSlider(label: String, value: Float, min: Float, max: Float, step: Float, onChange: (Float) -> Unit) {
    var local by remember(value) { mutableFloatStateOf(value) }
    val focusManager = LocalFocusManager.current
    Column(
        Modifier
            .fillMaxWidth()
            .clip(RoundedCornerShape(14.dp))
            .background(MaterialTheme.colorScheme.surfaceVariant.copy(alpha = 0.76f))
            .padding(horizontal = 12.dp, vertical = 10.dp),
    ) {
        Row(Modifier.fillMaxWidth(), verticalAlignment = Alignment.CenterVertically) {
            Text(label, Modifier.weight(1f), color = MaterialTheme.colorScheme.onSurface, maxLines = 2, overflow = TextOverflow.Ellipsis)
            Text(if (step < 1f) "%.2f".format(local) else local.roundToInt().toString(), color = MaterialTheme.colorScheme.onSurfaceVariant)
        }
        Slider(
            modifier = Modifier.onPreviewKeyEvent { handleVerticalDpadFocusMove(it, focusManager) },
            value = local,
            onValueChange = { local = ((it / step).roundToInt() * step).coerceIn(min, max) },
            onValueChangeFinished = { onChange(local) },
            valueRange = min..max,
        )
    }
}

@Composable
private fun ChoiceRow(label: String, options: List<String>, selected: String, onSelect: (String) -> Unit) {
    ChoiceMenuRow(
        label = label,
        options = options.map { ChoiceMenuOption(value = it, label = it) },
        selectedLabel = selected,
        onSelect = onSelect,
    )
}

@Composable
private fun ChoiceMenuRow(
    label: String,
    options: List<ChoiceMenuOption>,
    selectedLabel: String,
    onSelect: (String) -> Unit,
) {
    var expanded by remember { mutableStateOf(false) }
    var focused by remember { mutableStateOf(false) }
    val focusManager = LocalFocusManager.current
    val autoLabel = stringResource(R.string.option_auto)
    val shape = RoundedCornerShape(14.dp)
    Row(
        Modifier
            .fillMaxWidth()
            .onFocusChanged { focused = it.isFocused || it.hasFocus }
            .border(
                width = if (focused) 2.dp else 1.dp,
                color = if (focused) MaterialTheme.colorScheme.primary else Color.Transparent,
                shape = shape,
            )
            .clip(shape)
            .background(MaterialTheme.colorScheme.surfaceVariant.copy(alpha = 0.76f))
            .clickable { expanded = true }
            .onPreviewKeyEvent { event ->
                when {
                    isTvActivateKey(event) -> {
                        expanded = true
                        true
                    }
                    else -> handleVerticalDpadFocusMove(event, focusManager)
                }
            }
            .focusable()
            .padding(horizontal = 12.dp, vertical = 8.dp),
        verticalAlignment = Alignment.CenterVertically,
    ) {
        Text(label, Modifier.weight(1f), color = MaterialTheme.colorScheme.onSurface, maxLines = 2, overflow = TextOverflow.Ellipsis)
        Box {
            OutlinedButton(onClick = { expanded = true }) { Text(selectedLabel.ifBlank { autoLabel }, maxLines = 1, overflow = TextOverflow.Ellipsis) }
            DropdownMenu(expanded = expanded, onDismissRequest = { expanded = false }) {
                options.forEach { option ->
                    DropdownMenuItem(
                        text = {
                            Row(
                                verticalAlignment = Alignment.CenterVertically,
                                horizontalArrangement = Arrangement.spacedBy(8.dp),
                            ) {
                                Text(
                                    option.label,
                                    color = if (option.enabled) MaterialTheme.colorScheme.onSurface else MaterialTheme.colorScheme.onSurfaceVariant.copy(alpha = 0.48f),
                                )
                                option.badge?.let { badge ->
                                    Text(
                                        badge,
                                        color = MaterialTheme.colorScheme.onSurfaceVariant.copy(alpha = 0.7f),
                                        style = MaterialTheme.typography.labelSmall,
                                        fontWeight = FontWeight.Bold,
                                        modifier = Modifier
                                            .border(1.dp, MaterialTheme.colorScheme.onSurfaceVariant.copy(alpha = 0.3f), RoundedCornerShape(3.dp))
                                            .padding(horizontal = 4.dp, vertical = 1.dp),
                                    )
                                }
                            }
                        },
                        enabled = option.enabled,
                        onClick = {
                            expanded = false
                            onSelect(option.value)
                        },
                    )
                }
            }
        }
    }
}

@Composable
private fun ChoiceOptionRow(label: String, options: List<SettingsChoiceOption>, selectedValue: String, onSelect: (String) -> Unit) {
    val selectedLabel = options.firstOrNull { it.value == selectedValue }?.label ?: selectedValue
    ChoiceRow(label, options.map { it.label }, selectedLabel) { selected ->
        options.firstOrNull { it.label == selected }?.value?.let(onSelect)
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
                            CircularProgressIndicator(color = MaterialTheme.colorScheme.primary)
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
                            color = MaterialTheme.colorScheme.primary.copy(alpha = 0.12f),
                        ) {
                            Column(Modifier.padding(14.dp), verticalArrangement = Arrangement.spacedBy(4.dp)) {
                                Text("Recommended: ${zoneOption.zoneId}", color = MaterialTheme.colorScheme.primary, fontWeight = FontWeight.Bold)
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
                                color = if (selected) MaterialTheme.colorScheme.primary.copy(alpha = 0.16f) else PanelAlt,
                                tonalElevation = if (selected) 2.dp else 0.dp,
                            ) {
                                Row(Modifier.padding(12.dp), verticalAlignment = Alignment.CenterVertically) {
                                    Column(Modifier.weight(1f)) {
                                        Text(zoneId, fontWeight = FontWeight.Bold, color = if (selected) MaterialTheme.colorScheme.primary else TextPrimary)
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

private val ColorQuality.label: String
    get() = when (this) {
        ColorQuality.EightBit420 -> "8-bit 4:2:0"
        ColorQuality.EightBit444 -> "8-bit 4:4:4"
        ColorQuality.TenBit420 -> "10-bit 4:2:0"
        ColorQuality.TenBit444 -> "10-bit 4:4:4"
    }

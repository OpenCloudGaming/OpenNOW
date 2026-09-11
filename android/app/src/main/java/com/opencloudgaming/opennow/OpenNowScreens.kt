package com.opencloudgaming.opennow

import android.Manifest
import android.app.Activity
import android.content.Context
import android.content.pm.PackageManager
import android.content.res.Configuration
import android.content.Intent
import android.hardware.input.InputManager
import android.net.ConnectivityManager
import android.net.NetworkCapabilities
import android.net.Uri
import android.os.Build
import android.os.SystemClock
import android.provider.Settings
import androidx.annotation.StringRes
import android.speech.RecognizerIntent
import android.view.InputDevice
import android.view.KeyEvent
import android.view.MotionEvent
import android.view.PointerIcon
import android.view.View
import android.view.ViewGroup
import android.widget.Toast
import androidx.activity.compose.BackHandler
import androidx.activity.compose.rememberLauncherForActivityResult
import androidx.activity.result.contract.ActivityResultContracts
import androidx.lifecycle.Lifecycle
import androidx.lifecycle.LifecycleEventObserver
import androidx.lifecycle.compose.LocalLifecycleOwner
import androidx.compose.animation.AnimatedContent
import androidx.compose.animation.AnimatedVisibility
import androidx.compose.animation.animateColorAsState
import androidx.compose.animation.fadeIn
import androidx.compose.animation.fadeOut
import androidx.compose.animation.scaleIn
import androidx.compose.animation.scaleOut
import androidx.compose.animation.slideInVertically
import androidx.compose.animation.slideOutVertically
import androidx.compose.animation.togetherWith
import androidx.compose.animation.core.Animatable
import androidx.compose.animation.core.FastOutSlowInEasing
import androidx.compose.animation.core.LinearEasing
import androidx.compose.animation.core.MutableTransitionState
import androidx.compose.animation.core.RepeatMode
import androidx.compose.animation.core.animateFloat
import androidx.compose.animation.core.animateFloatAsState
import androidx.compose.animation.ContentTransform
import androidx.compose.animation.slideInHorizontally
import androidx.compose.animation.slideOutHorizontally
import androidx.compose.animation.togetherWith
import androidx.compose.animation.core.infiniteRepeatable
import androidx.compose.animation.core.rememberInfiniteTransition
import androidx.compose.animation.core.tween
import androidx.compose.foundation.ExperimentalFoundationApi
import androidx.compose.foundation.BorderStroke
import androidx.compose.foundation.Canvas
import androidx.compose.foundation.gestures.awaitEachGesture
import androidx.compose.foundation.gestures.awaitFirstDown
import androidx.compose.foundation.gestures.detectDragGestures
import androidx.compose.foundation.gestures.detectTapGestures
import androidx.compose.foundation.text.KeyboardActions
import androidx.compose.foundation.text.KeyboardOptions
import androidx.compose.foundation.text.BasicTextField
import androidx.compose.foundation.Image
import androidx.compose.foundation.background
import androidx.compose.foundation.border
import androidx.compose.foundation.clickable
import androidx.compose.foundation.combinedClickable
import androidx.compose.foundation.focusGroup
import androidx.compose.foundation.focusable
import androidx.compose.foundation.gestures.Orientation
import androidx.compose.foundation.gestures.draggable
import androidx.compose.foundation.gestures.rememberDraggableState
import androidx.compose.foundation.interaction.MutableInteractionSource
import androidx.compose.foundation.interaction.collectIsHoveredAsState
import androidx.compose.foundation.interaction.collectIsPressedAsState
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
import androidx.compose.foundation.layout.heightIn
import androidx.compose.foundation.layout.ime
import androidx.compose.foundation.layout.imePadding
import androidx.compose.foundation.layout.offset
import androidx.compose.foundation.layout.padding
import androidx.compose.foundation.layout.size
import androidx.compose.foundation.layout.width
import androidx.compose.foundation.layout.widthIn
import androidx.compose.foundation.lazy.LazyColumn
import androidx.compose.foundation.lazy.LazyListScope
import androidx.compose.foundation.lazy.LazyRow
import androidx.compose.foundation.lazy.items
import androidx.compose.foundation.lazy.rememberLazyListState
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
import androidx.compose.material3.ButtonDefaults
import androidx.compose.material3.Card
import androidx.compose.material3.CardDefaults
import androidx.compose.material3.Checkbox
import androidx.compose.material3.CircularProgressIndicator
import androidx.compose.material3.DropdownMenu
import androidx.compose.material3.DropdownMenuItem
import androidx.compose.material3.ElevatedButton
import androidx.compose.material3.ExperimentalMaterial3Api
import androidx.compose.material3.Icon
import androidx.compose.material.icons.Icons
import androidx.compose.material.icons.outlined.Cast
import androidx.compose.material.icons.rounded.Check
import androidx.compose.material.icons.rounded.Keyboard
import androidx.compose.material.icons.rounded.KeyboardArrowDown
import androidx.compose.material.icons.rounded.KeyboardArrowUp
import androidx.compose.material.icons.rounded.Refresh
import androidx.compose.material3.IconButton
import androidx.compose.material3.HorizontalDivider
import androidx.compose.material3.LinearProgressIndicator
import androidx.compose.material3.MaterialTheme
import androidx.compose.material3.minimumInteractiveComponentSize
import androidx.compose.material3.NavigationBar
import androidx.compose.material3.NavigationBarItem
import androidx.compose.material3.NavigationBarItemDefaults
import androidx.compose.material3.OutlinedButton
import androidx.compose.material3.OutlinedTextField
import androidx.compose.material3.OutlinedTextFieldDefaults
import androidx.compose.material3.Scaffold
import androidx.compose.material3.ScaffoldDefaults
import androidx.compose.material3.Slider
import androidx.compose.material3.Surface
import androidx.compose.material3.Switch
import androidx.compose.material3.Text
import androidx.compose.material3.TextButton
import androidx.compose.material3.darkColorScheme
import androidx.compose.material.icons.automirrored.rounded.BatteryUnknown
import androidx.compose.material.icons.rounded.Battery0Bar
import androidx.compose.material.icons.rounded.Battery1Bar
import androidx.compose.material.icons.rounded.Battery2Bar
import androidx.compose.material.icons.rounded.Battery3Bar
import androidx.compose.material.icons.rounded.Battery4Bar
import androidx.compose.material.icons.rounded.Battery5Bar
import androidx.compose.material.icons.rounded.Battery6Bar
import androidx.compose.material.icons.rounded.BatteryFull
import androidx.compose.material.icons.rounded.Bolt
import androidx.compose.material.icons.rounded.SignalCellular0Bar
import androidx.compose.material.icons.rounded.SignalCellular4Bar
import androidx.compose.material.icons.rounded.SignalCellularAlt
import androidx.compose.material.icons.rounded.SignalCellularAlt1Bar
import androidx.compose.material.icons.rounded.SignalCellularAlt2Bar
import androidx.compose.material.icons.rounded.SignalWifi0Bar
import androidx.compose.material.icons.rounded.Wifi
import androidx.compose.material.icons.rounded.Wifi1Bar
import androidx.compose.material.icons.rounded.Wifi2Bar
import androidx.compose.material.icons.rounded.WifiOff
import androidx.compose.material3.dynamicDarkColorScheme
import androidx.compose.material3.pulltorefresh.PullToRefreshBox
import androidx.compose.material3.pulltorefresh.PullToRefreshDefaults
import androidx.compose.material3.pulltorefresh.rememberPullToRefreshState
import androidx.compose.runtime.Composable
import androidx.compose.runtime.LaunchedEffect
import androidx.compose.runtime.State
import androidx.compose.runtime.getValue
import androidx.compose.runtime.mutableFloatStateOf
import androidx.compose.runtime.mutableIntStateOf
import androidx.compose.runtime.mutableStateOf
import androidx.compose.runtime.produceState
import androidx.compose.runtime.remember
import androidx.compose.runtime.rememberUpdatedState
import androidx.compose.runtime.rememberCoroutineScope
import androidx.compose.runtime.saveable.rememberSaveable
import androidx.compose.runtime.key
import androidx.compose.runtime.setValue
import androidx.compose.runtime.CompositionLocalProvider
import androidx.compose.runtime.DisposableEffect
import androidx.compose.runtime.MutableIntState
import androidx.compose.runtime.compositionLocalOf
import androidx.compose.runtime.staticCompositionLocalOf
import androidx.compose.ui.Alignment
import androidx.compose.ui.ExperimentalComposeUiApi
import androidx.compose.ui.Modifier
import androidx.compose.ui.draw.blur
import androidx.compose.ui.draw.clip
import androidx.compose.ui.draw.clipToBounds
import androidx.compose.ui.draw.drawBehind
import androidx.compose.ui.draw.drawWithCache
import androidx.compose.ui.zIndex
import androidx.compose.ui.focus.FocusDirection
import androidx.compose.ui.focus.FocusManager
import androidx.compose.ui.focus.FocusRequester
import androidx.compose.ui.focus.focusRequester
import androidx.compose.ui.focus.focusProperties
import androidx.compose.ui.focus.onFocusChanged
import androidx.compose.ui.geometry.CornerRadius
import androidx.compose.ui.geometry.Offset
import androidx.compose.ui.geometry.Size
import androidx.compose.ui.graphics.Brush
import androidx.compose.ui.graphics.Color
import androidx.compose.ui.graphics.Path
import androidx.compose.ui.graphics.Shadow
import androidx.compose.ui.graphics.SolidColor
import androidx.compose.ui.graphics.graphicsLayer
import androidx.compose.ui.graphics.drawscope.Stroke
import androidx.compose.ui.input.pointer.pointerInput
import androidx.compose.ui.input.pointer.PointerEventPass
import androidx.compose.ui.input.pointer.pointerInteropFilter
import androidx.compose.ui.input.key.Key
import androidx.compose.ui.input.key.KeyEventType
import androidx.compose.ui.input.key.key
import androidx.compose.ui.input.key.onPreviewKeyEvent
import androidx.compose.ui.input.key.type
import androidx.compose.ui.layout.ContentScale
import androidx.compose.ui.unit.IntOffset
import androidx.compose.ui.layout.layout
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
import androidx.compose.ui.semantics.Role
import androidx.compose.ui.semantics.contentDescription
import androidx.compose.ui.semantics.role
import androidx.compose.ui.semantics.semantics
import androidx.compose.ui.text.AnnotatedString
import androidx.compose.ui.text.TextStyle
import androidx.compose.ui.text.font.FontWeight
import androidx.compose.ui.text.input.ImeAction
import androidx.compose.ui.text.input.KeyboardType
import androidx.compose.ui.text.input.PasswordVisualTransformation
import androidx.compose.ui.text.input.TextFieldValue
import androidx.compose.ui.text.style.TextAlign
import androidx.compose.ui.text.style.TextDecoration
import androidx.compose.ui.text.style.TextOverflow
import androidx.compose.ui.unit.dp
import androidx.compose.ui.unit.sp
import androidx.compose.ui.unit.Dp
import androidx.compose.ui.unit.IntSize
import androidx.compose.ui.viewinterop.AndroidView
import androidx.compose.ui.window.Dialog
import androidx.compose.ui.window.DialogProperties
import androidx.lifecycle.compose.collectAsStateWithLifecycle
import androidx.core.content.ContextCompat
import androidx.media3.common.MediaItem
import androidx.media3.common.Player
import androidx.media3.exoplayer.ExoPlayer
import androidx.media3.ui.PlayerView
import coil3.compose.AsyncImage
import kotlinx.coroutines.delay
import kotlinx.coroutines.launch
import java.io.File
import java.text.DateFormat
import java.util.Date
import java.util.Locale
import kotlin.math.min
import kotlin.math.floor
import com.opencloudgaming.opennow.ui.controls.ControlActionRow
import com.opencloudgaming.opennow.ui.controls.ControlNavigationRow
import com.opencloudgaming.opennow.ui.controls.ControlRowStyle
import com.opencloudgaming.opennow.ui.controls.ControlSection
import com.opencloudgaming.opennow.ui.controls.ControlSectionStyle
import com.opencloudgaming.opennow.ui.controls.ControlSliderRow
import com.opencloudgaming.opennow.ui.controls.ControlSwitchRow
import com.opencloudgaming.opennow.ui.controls.LocalControlRowStyle
import com.opencloudgaming.opennow.ui.controls.LocalControlSectionStyle
import com.opencloudgaming.opennow.ui.theme.LocalReduceMotion
import com.opencloudgaming.opennow.ui.theme.OpenNowMotion
import com.opencloudgaming.opennow.ui.theme.OpenNowPalette
import com.opencloudgaming.opennow.ui.theme.OpenNowRadius
import com.opencloudgaming.opennow.ui.theme.OpenNowShapes
import com.opencloudgaming.opennow.ui.theme.OpenNowSpacing
import com.opencloudgaming.opennow.ui.theme.OpenNowTypography
import com.opencloudgaming.opennow.ui.theme.numeric
import com.opencloudgaming.opennow.ui.theme.tint
import kotlin.math.roundToInt
import kotlin.math.sin
import kotlin.math.sqrt

// Aliases onto the shared token layer. The names stay so existing call sites keep working; the
// values now live in exactly one place instead of being duplicated across two files.
internal val Green = OpenNowPalette.AccentDefault
internal val Background = OpenNowPalette.Background
internal val Panel = OpenNowPalette.Panel
internal val ProfileMenuContainerColor = Panel
internal val PanelAlt = OpenNowPalette.PanelAlt
internal val TextPrimary = OpenNowPalette.TextPrimary
internal val TextMuted = OpenNowPalette.TextMuted
private val ChromeScrim = OpenNowPalette.ChromeScrim
internal val TopBarCompactControlHeight = 30.dp
internal const val DEVICE_LOGIN_SIDE_BY_SIDE_MIN_WIDTH_DP = 520
internal const val COMPACT_STREAM_DEVICE_STATUS_REFRESH_MS = 5_000L
internal const val QUEUE_POSITION_VISUAL_SETTLE_MS = 1100L
internal const val ACTIVE_STREAM_MODE_NOTICE_DURATION_MS = 8_000L
internal const val STREAM_NETWORK_NOTICE_DURATION_MS = 12_000L
internal val UiAccent.color: Color
    get() = when (this) {
        UiAccent.OpenNow -> OpenNowPalette.AccentDefault
        UiAccent.Pixel -> OpenNowPalette.AccentPixel
        UiAccent.HotPink -> OpenNowPalette.AccentHotPink
        UiAccent.Lime -> OpenNowPalette.AccentLime
        UiAccent.Coral -> OpenNowPalette.AccentCoral
        UiAccent.Violet -> OpenNowPalette.AccentViolet
        UiAccent.LegacyOrange -> OpenNowPalette.AccentViolet
        UiAccent.AbsoluteCinema -> Color.White
        UiAccent.Switch -> OpenNowPalette.AccentSwitchRed
    }

@Composable
internal fun uiAccentLabel(accent: UiAccent): String = when (accent) {
    UiAccent.OpenNow -> stringResource(R.string.accent_opennow)
    UiAccent.Pixel -> stringResource(R.string.accent_pixel)
    UiAccent.HotPink -> stringResource(R.string.accent_hot_pink)
    UiAccent.Lime -> stringResource(R.string.accent_lime)
    UiAccent.Coral -> stringResource(R.string.accent_coral)
    UiAccent.Violet -> stringResource(R.string.accent_violet)
    UiAccent.LegacyOrange -> stringResource(R.string.accent_violet)
    UiAccent.AbsoluteCinema -> stringResource(R.string.accent_absolute_cinema)
    UiAccent.Switch -> stringResource(R.string.accent_switch)
}

/** Accent selection is independent from the switches that opt into Cinema border effects. */
internal fun selectableUiAccents(): List<UiAccent> = UiAccent.entries.filterNot { it == UiAccent.LegacyOrange }

internal val UiAccent.secondaryColor: Color
    get() = when (this) {
        UiAccent.OpenNow -> OpenNowPalette.AccentDefaultSecondary
        UiAccent.AbsoluteCinema -> Color.White
        UiAccent.Switch -> OpenNowPalette.AccentSwitchBlue
        else -> color
    }

internal data class ActiveSelectionEffectStyle(
    val color: Color,
    val secondaryColor: Color,
    /** Flat selected fills, icon tints, and menu highlights — everything that is not an effect. */
    val tintColor: Color,
    /** Static borders around game artwork; independent from the animated Cinema treatment. */
    val gameCardBordersEnabled: Boolean,
    val enabled: Boolean,
    val absoluteCinemaActive: Boolean,
    val absoluteCinemaEverywhere: Boolean,
)

/**
 * The fixed tint for selected navigation — the tab bar and its rotated form, the rail.
 *
 * Navigation chrome deliberately does not follow the interface accent. It is the one surface that
 * is always on screen, and an accent there reads as the app changing identity rather than as a
 * highlight.
 */
internal val NavigationSelectionColor = Color.White

/**
 * Absolute Cinema is an explicit animation toggle, not a color override. The everywhere suboption
 * broadens the same treatment to additional hovered and focused surfaces. The user's selected
 * accent remains the color owner.
 *
 * [ActiveSelectionEffectStyle.tintColor] splits flat selection from animated energy. Both use the
 * selected color; enabling Cinema never changes the Material palette.
 */
internal fun AppSettings.activeSelectionEffectStyle(): ActiveSelectionEffectStyle {
    val cinemaEverywhere = absoluteCinemaEffects && absoluteCinemaEverywhere
    val cinemaActive = absoluteCinemaEffects
    val baseColor = if (uiAccent == UiAccent.OpenNow) Color.White else uiAccent.color
    val baseSecondaryColor = if (uiAccent == UiAccent.OpenNow) Color.White else uiAccent.secondaryColor
    return ActiveSelectionEffectStyle(
        color = baseColor,
        secondaryColor = baseSecondaryColor,
        tintColor = if (uiAccent.usesDefaultSelectionTint()) Color.White else uiAccent.color,
        gameCardBordersEnabled = liveSelectedOutlines,
        // Selection borders are an Absolute Cinema effect. Other accents still own fills, icon
        // tints, and text emphasis, but no longer draw outlines around ordinary controls.
        enabled = cinemaActive,
        absoluteCinemaActive = cinemaActive,
        absoluteCinemaEverywhere = cinemaEverywhere,
    )
}

/** Accents whose flat selection colour stays the default white regardless of the effect colour. */
internal fun UiAccent.usesDefaultSelectionTint(): Boolean =
    this == UiAccent.OpenNow || this == UiAccent.AbsoluteCinema

/**
 * The Material palette colour for an accent.
 *
 * Absolute Cinema keeps the regular Material palette and white artwork/focus borders. Selecting it
 * never enables those effects by itself; choosing another accent can still tint Cinema effects.
 */
internal val UiAccent.themeColor: Color
    get() = if (this == UiAccent.AbsoluteCinema) OpenNowPalette.AccentDefault else color

internal val UiAccent.themeSecondaryColor: Color
    get() = if (this == UiAccent.AbsoluteCinema) OpenNowPalette.AccentDefaultSecondary else secondaryColor

internal val LocalActiveSelectionColor = staticCompositionLocalOf { Color.White }
internal val LocalActiveSelectionSecondaryColor = staticCompositionLocalOf { Color.White }
/** Flat selected fills and icon tints. See [ActiveSelectionEffectStyle.tintColor]. */
internal val LocalSelectionTintColor = staticCompositionLocalOf { Color.White }
internal val LocalGameCardBordersEnabled = staticCompositionLocalOf { false }
internal val LocalActiveSelectionEnabled = staticCompositionLocalOf { true }
internal val LocalAbsoluteCinemaEffects = staticCompositionLocalOf { false }
internal val LocalAbsoluteCinemaEverywhere = staticCompositionLocalOf { false }
/** Selects the classic blue/orange palette only inside animated Absolute Cinema frames. */
internal val LocalAbsoluteCinemaPalette = staticCompositionLocalOf { false }
internal val LocalVibrationEnabled = staticCompositionLocalOf { true }
// Leave a full control-height runway below the last item so mobile focus/hover frames do not
// collide with the viewport edge in Settings, Library, or Store.
internal val AppScrollEndSpacing = 72.dp

internal fun shouldShowAppWallpaper(
    page: AppPage,
    inStream: Boolean,
    settings: AppSettings,
): Boolean =
    !inStream &&
        (page == AppPage.Home || page == AppPage.Library || page == AppPage.Settings) &&
        shouldShowCatalogWallpaper(settings)

@Composable
fun OpenNowTheme(
    settings: AppSettings,
    physicalControllerConnected: Boolean,
    content: @Composable () -> Unit,
) {
    val context = LocalContext.current
    val accent = settings.uiAccent.themeColor
    val fallbackScheme = darkColorScheme(
        primary = accent,
        onPrimary = OpenNowPalette.OnAccent,
        background = Background,
        surface = Panel,
        surfaceVariant = PanelAlt,
        onBackground = TextPrimary,
        onSurface = TextPrimary,
        onSurfaceVariant = TextMuted,
        secondary = settings.uiAccent.themeSecondaryColor,
        errorContainer = OpenNowPalette.ErrorContainer,
        onErrorContainer = OpenNowPalette.OnErrorContainer,
    )
    val colorScheme = if (settings.dynamicColor && Build.VERSION.SDK_INT >= Build.VERSION_CODES.S) {
        dynamicDarkColorScheme(context).copy(
            primary = accent,
            onPrimary = OpenNowPalette.OnAccent,
            secondary = settings.uiAccent.themeSecondaryColor,
            tertiary = Green,
            errorContainer = OpenNowPalette.ErrorContainer,
            onErrorContainer = OpenNowPalette.OnErrorContainer,
        )
    } else {
        fallbackScheme
    }
    // Honour both the system-wide animation switch and the in-app toggle. Infinite transitions
    // (shimmer, focus energy, carousel auto-advance) read this and stop entirely.
    val reduceMotion = remember(settings.controllerBackgroundAnimations, context) {
        val systemScale = runCatching {
            Settings.Global.getFloat(
                context.contentResolver,
                Settings.Global.ANIMATOR_DURATION_SCALE,
                1f,
            )
        }.getOrDefault(1f)
        systemScale == 0f || !settings.controllerBackgroundAnimations
    }
    val selectionEffectStyle = settings.activeSelectionEffectStyle()
    val gamingHandheld = remember { isGamingHandheldDevice() }
    val openNowHaptics = rememberOpenNowHaptics(
        enabled = settings.vibrationEnabled,
        navigationEnabled = settings.vibrationEnabled && (gamingHandheld || physicalControllerConnected),
        handheldFeedback = gamingHandheld,
    )
    CompositionLocalProvider(
        LocalReduceMotion provides reduceMotion,
        LocalOpenNowHaptics provides openNowHaptics,
        LocalActiveSelectionColor provides selectionEffectStyle.color,
        LocalActiveSelectionSecondaryColor provides selectionEffectStyle.secondaryColor,
        LocalSelectionTintColor provides selectionEffectStyle.tintColor,
        LocalGameCardBordersEnabled provides selectionEffectStyle.gameCardBordersEnabled,
        LocalActiveSelectionEnabled provides selectionEffectStyle.enabled,
        LocalAbsoluteCinemaEffects provides selectionEffectStyle.absoluteCinemaActive,
        LocalAbsoluteCinemaEverywhere provides selectionEffectStyle.absoluteCinemaEverywhere,
        LocalAbsoluteCinemaPalette provides (settings.uiAccent == UiAccent.AbsoluteCinema),
        LocalVibrationEnabled provides settings.vibrationEnabled,
    ) {
        MaterialTheme(
            colorScheme = colorScheme,
            typography = OpenNowTypography,
            shapes = OpenNowShapes,
            content = content,
        )
    }
}

@Composable
fun OpenNowApp(
    viewModel: OpenNowViewModel,
    onMicrophoneCaptureActiveChange: (Boolean) -> Unit = {},
) {
    val state by viewModel.state.collectAsStateWithLifecycle()
    val context = LocalContext.current
    val lifecycleOwner = LocalLifecycleOwner.current
    val physicalControllerConnected = rememberPhysicalControllerConnected(enabled = true)
    val controllerFocusEnabled = shouldShowControllerFocus(
        focused = true,
        tvProfile = state.androidTvProfile,
        physicalControllerConnected = physicalControllerConnected,
    )
    val launchAudioController = remember(context) { AndroidNerdAudioController(context.applicationContext) }
    val playIntroOnAppLaunch = remember { state.settings.streamIntroMusic }
    val introStartsMutedOnLaunch = remember {
        state.settings.streamIntroMusic && state.settings.streamIntroStartMode == IntroMusicStartMode.Muted
    }
    val musicControlsEnabled = state.settings.streamIntroMusic || state.settings.queueReadyMusic
    val streamActive = state.page == AppPage.Stream || state.streamStatus != "idle"
    val systemWallpaperVisible = shouldShowSystemWallpaperBackground(
        settings = state.settings,
        inStream = state.page == AppPage.Stream,
    )
    val gameDetailsTransitionRegistry = remember { GameDetailsTransitionRegistry() }
    var launchIntroStarted by remember { mutableStateOf(false) }
    var launchMusicMuted by remember { mutableStateOf(introStartsMutedOnLaunch) }
    var launchMusicPlaying by remember { mutableStateOf(false) }
    var previousStreamStatus by remember { mutableStateOf(state.streamStatus) }
    var queuedForStartCue by remember { mutableStateOf(false) }
    var lastStartCueSessionId by remember { mutableStateOf<String?>(null) }
    var hiddenUpdatePromptKey by remember { mutableStateOf<String?>(null) }
    var completedSessionBugReportOpen by rememberSaveable { mutableStateOf(false) }
    val updatePromptKey = state.androidUpdate.visibleNoticeKey(state.dismissedAndroidUpdateNoticeKey)
    // After sign-in, not before: the appearance step previews the user's own box art, and there is
    // no catalog to draw from until an account is attached.
    val showSetupFlow = state.authSession != null && shouldShowSetupFlow(state.settings)
    val diagnosticDialogVisible = state.diagnosticShare.awaitingConsent ||
        state.diagnosticShare.uploading ||
        state.diagnosticShare.pasteUrl != null
    val showCompletedSessionBugReport = completedSessionBugReportOpen && !diagnosticDialogVisible
    val showSessionReport = state.sessionReport != null &&
        state.settings.showSessionReportAfterStream &&
        !diagnosticDialogVisible &&
        !showCompletedSessionBugReport
    val showUpdatePrompt = updatePromptKey != null &&
        updatePromptKey != hiddenUpdatePromptKey &&
        !showSetupFlow &&
        !showSessionReport &&
        !showCompletedSessionBugReport &&
        !diagnosticDialogVisible &&
        state.androidUpdate.status in setOf(AndroidUpdateStatus.Available, AndroidUpdateStatus.Downloaded)

    DisposableEffect(launchAudioController) {
        onDispose {
            launchAudioController.release()
        }
    }
    DisposableEffect(lifecycleOwner, launchAudioController) {
        val observer = LifecycleEventObserver { _, event ->
            when (event) {
                Lifecycle.Event.ON_PAUSE -> launchAudioController.pauseAll { launchMusicPlaying = it }
                Lifecycle.Event.ON_RESUME -> launchAudioController.resumeAll { launchMusicPlaying = it }
                else -> Unit
            }
        }
        lifecycleOwner.lifecycle.addObserver(observer)
        onDispose {
            lifecycleOwner.lifecycle.removeObserver(observer)
        }
    }
    LaunchedEffect(
        playIntroOnAppLaunch,
        state.settings.streamIntroMusic,
        state.settings.queueReadyMusic,
        launchMusicMuted,
        state.page,
        state.streamStatus,
        state.launchPhase,
        state.queuePosition,
        state.streamSession?.sessionId,
        state.streamSession?.queuePosition,
        state.streamSession?.seatSetupStep,
    ) {
        val sessionId = state.streamSession?.sessionId
        val queueReadyForStream =
            previousStreamStatus == "queue" &&
                state.streamStatus == "connecting" &&
                sessionId != null &&
                sessionId != lastStartCueSessionId
        previousStreamStatus = state.streamStatus
        if (state.streamStatus == "queue") {
            queuedForStartCue = queuedForStartCue ||
                queueDisplayPosition(state) != null ||
                state.launchPhase.equals("Queue", ignoreCase = true)
        }

        if (!musicControlsEnabled) {
            launchIntroStarted = false
            launchMusicMuted = false
            launchAudioController.stopAll { launchMusicPlaying = it }
            if (state.streamStatus == "idle") {
                queuedForStartCue = false
            }
            return@LaunchedEffect
        }
        if (!state.settings.streamIntroMusic && launchMusicMuted) {
            launchMusicMuted = false
        }
        if (!state.settings.streamIntroMusic) {
            launchAudioController.stopIntro { launchMusicPlaying = it }
        }
        if (!state.settings.queueReadyMusic) {
            launchAudioController.stopQueueReadyReminder { launchMusicPlaying = it }
        }

        if (!state.settings.streamIntroMusic && !state.settings.queueReadyMusic) {
            launchAudioController.stopAll { launchMusicPlaying = it }
        } else if (queueReadyForStream && queuedForStartCue) {
            launchMusicMuted = false
            lastStartCueSessionId = sessionId
            queuedForStartCue = false
            launchAudioController.startQueueReadyReminder(enabled = state.settings.queueReadyMusic) { launchMusicPlaying = it }
        } else if (launchMusicMuted) {
            launchAudioController.stopIntro { launchMusicPlaying = it }
        } else if (playIntroOnAppLaunch && state.settings.streamIntroMusic && !streamActive) {
            if (!launchIntroStarted) {
                launchIntroStarted = true
                launchAudioController.startIntro(enabled = true) { launchMusicPlaying = it }
            }
        } else {
            launchAudioController.stopIntro { launchMusicPlaying = it }
        }
        if (state.streamStatus == "idle") {
            queuedForStartCue = false
        }
    }
    val musicControl = TopBarMusicControl(
        visible = musicControlsEnabled,
        playing = launchMusicPlaying,
        muted = launchMusicMuted,
        onToggle = {
            when {
                launchMusicMuted -> {
                    launchMusicMuted = false
                    if (state.settings.streamIntroMusic && !streamActive) {
                        launchIntroStarted = true
                        launchAudioController.startIntro(enabled = true) { launchMusicPlaying = it }
                    }
                }
                launchMusicPlaying -> {
                    launchMusicMuted = true
                    launchAudioController.stopAll { launchMusicPlaying = it }
                }
                state.settings.streamIntroMusic && !streamActive -> {
                    launchIntroStarted = true
                    launchAudioController.startIntro(enabled = true) { launchMusicPlaying = it }
                }
                else -> {
                    launchMusicMuted = true
                    launchAudioController.stopAll { launchMusicPlaying = it }
                }
            }
        },
    )

    OpenNowTheme(
        settings = state.settings,
        physicalControllerConnected = physicalControllerConnected,
    ) {
        val primaryColor = MaterialTheme.colorScheme.primary
        CompositionLocalProvider(
            LocalTvLoadingProfile provides state.androidTvProfile,
            LocalControllerFocusEnabled provides controllerFocusEnabled,
            LocalGameDetailsTransitionRegistry provides gameDetailsTransitionRegistry,
        ) {
            Box(
                Modifier
                    .fillMaxSize()
                    .background(
                        if (systemWallpaperVisible) Color.Transparent
                        else MaterialTheme.colorScheme.background,
                    )
                    .drawWithCache {
                        if (systemWallpaperVisible) {
                            onDrawBehind { }
                        } else {
                            val brush = Brush.radialGradient(
                                colors = listOf(
                                    primaryColor.copy(alpha = 0.15f),
                                    Color.Transparent,
                                ),
                                center = Offset(size.width, 0f),
                                radius = size.width.coerceAtLeast(size.height) * 0.8f,
                            )
                            onDrawBehind {
                                drawRect(brush)
                            }
                        }
                    }
            ) {
                Surface(Modifier.fillMaxSize(), color = Color.Transparent) {
                    when {
                        showSetupFlow -> SetupFlowScreen(state = state, viewModel = viewModel)
                        state.authSession != null -> MainShell(
                            state = state,
                            viewModel = viewModel,
                            musicControl = musicControl,
                            onMicrophoneCaptureActiveChange = onMicrophoneCaptureActiveChange,
                        )
                        else -> LoginScreen(state, viewModel)
                    }
                }
                state.sessionReport?.takeIf { showSessionReport }?.let { report ->
                    SessionReportDialog(
                        report = report,
                        onDismiss = { dontShowAgain ->
                            if (dontShowAgain) {
                                viewModel.updateSettings(
                                    state.settings.copy(showSessionReportAfterStream = false),
                                )
                            }
                            viewModel.dismissSessionReport()
                        },
                        onReportBug = { dontShowAgain ->
                            if (dontShowAgain) {
                                viewModel.updateSettings(
                                    state.settings.copy(showSessionReportAfterStream = false),
                                )
                            }
                            viewModel.resetBugReportSubmission()
                            completedSessionBugReportOpen = true
                        },
                    )
                }
                if (showCompletedSessionBugReport) {
                    CompletedSessionBugReportDialog(
                        submission = state.bugReportSubmission,
                        versionCheck = state.bugReportVersionCheck,
                        update = state.androidUpdate,
                        onSubmit = { title, description, knownIssueOverrideKey ->
                            viewModel.submitBugReport(title, description, knownIssueOverrideKey)
                        },
                        onReset = viewModel::resetBugReportSubmission,
                        onVersionCheck = viewModel::verifyBugReportVersion,
                        onOpenUpdate = viewModel::performAndroidUpdatePrimaryAction,
                        preflightProvider = {
                            buildBugReportPreflightDeck(
                                BugReportPreflightEvidence(
                                    requestedSettings = state.settings.stream,
                                    recommendedSettings = state.recommendedStreamSettings,
                                    nativeLowLatencyDecoderEnabled = state.settings.nativeLowLatencyDecoder,
                                    runtimeDiagnostics = AndroidRuntimeDiagnostics.snapshot(context),
                                    sessionReport = state.sessionReport,
                                    codecReport = state.codecReport,
                                    androidTvProfile = state.androidTvProfile,
                                    serverZone = state.streamSession?.zone,
                                    manuallySelectedServer = state.manuallySelectedServerForReport,
                                    inputDiagnostics = NativeInputDiagnostics.snapshot(),
                                ),
                            )
                        },
                        onDismiss = {
                            if (!state.bugReportSubmission.uploading) {
                                completedSessionBugReportOpen = false
                                viewModel.dismissSessionReport()
                                viewModel.resetBugReportSubmission()
                            }
                        },
                    )
                }
                updatePromptKey?.takeIf { showUpdatePrompt }?.let { promptKey ->
                    AndroidUpdatePromptDialog(
                        update = state.androidUpdate,
                        onPrimary = {
                            hiddenUpdatePromptKey = promptKey
                            when (state.androidUpdate.status) {
                                AndroidUpdateStatus.Available -> viewModel.performAndroidUpdatePrimaryAction()
                                AndroidUpdateStatus.Downloaded -> viewModel.installAndroidUpdate()
                                else -> Unit
                            }
                        },
                        onDetails = {
                            hiddenUpdatePromptKey = promptKey
                            viewModel.openAndroidUpdateSettings()
                        },
                        onDismiss = viewModel::dismissAndroidUpdateNotice,
                    )
                }
                UselessMascotOverlay(
                    settings = state.settings,
                    allowed = !showSetupFlow && state.authSession != null &&
                        state.streamStatus != "streaming" && state.streamStatus != "connecting",
                )
                DiagnosticShareDialog(
                    state = state,
                    onUpload = viewModel::uploadDiagnosticShare,
                    onDismiss = viewModel::dismissDiagnosticShare,
                )
            }
        }
    }
}

@Composable
private fun MainShell(
    state: OpenNowUiState,
    viewModel: OpenNowViewModel,
    musicControl: TopBarMusicControl,
    onMicrophoneCaptureActiveChange: (Boolean) -> Unit,
) {
    val context = LocalContext.current
    val inStream = state.page == AppPage.Stream
    val streamingActive = inStream && state.streamStatus != "idle"
    val modalPickerOpen = state.pendingPrintedWasteGame != null ||
        state.pendingStoreChoiceGame != null ||
        state.pendingMembershipNotice != null
    val tvProfile = state.androidTvProfile
    val physicalControllerConnected = rememberPhysicalControllerConnected(enabled = !inStream)
    val navAudioController = remember(context) { AndroidNerdAudioController(context.applicationContext) }
    var visibleSearchTarget by remember { mutableStateOf<SearchTarget?>(null) }
    var settingsSearchQuery by remember { mutableStateOf("") }
    var settingsDetailRouteOpen by remember { mutableStateOf(false) }
    var settingsBackRequestToken by remember { mutableStateOf(0) }
    val tvStreamReturnFocusRequester = remember { FocusRequester() }
    val topBarProfileFocusRequester = remember { FocusRequester() }
    val catalogFilterFocusRequester = remember { FocusRequester() }
    var previouslyInStream by remember { mutableStateOf(inStream) }
    val navigationToneEnabled = state.settings.controllerUiSounds && !inStream
    val showMinimizedQueueDock = state.streamLaunchMinimized && shouldShowQueueLaunchStatus(state)
    DisposableEffect(navAudioController) {
        onDispose {
            navAudioController.release()
        }
    }
    LaunchedEffect(state.page) {
        if (state.page != AppPage.Settings) {
            settingsDetailRouteOpen = false
        }
    }
    LaunchedEffect(inStream, tvProfile) {
        val shouldRestoreFocus = shouldRestoreTvNavigationFocus(
            previouslyInStream = previouslyInStream,
            currentlyInStream = inStream,
            tvProfile = tvProfile,
        )
        previouslyInStream = inStream
        if (shouldRestoreFocus) {
            delay(120)
            repeat(3) { attempt ->
                if (runCatching { tvStreamReturnFocusRequester.requestFocus() }.isSuccess) {
                    return@LaunchedEffect
                }
                if (attempt < 2) delay(80)
            }
        }
    }
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
    fun navigateFromAppChrome(page: AppPage) {
        if (page == AppPage.Settings) {
            viewModel.recordSettingsIconTap()
        }
        visibleSearchTarget = null
        viewModel.setPage(page)
    }
    BackHandler(enabled = state.selectedGame != null && !inStream) {
        viewModel.clearSelectedGame()
    }
    BackHandler(
        enabled = (tvProfile || physicalControllerConnected) &&
            !inStream &&
            state.selectedGame == null &&
            !modalPickerOpen &&
            state.page != AppPage.Home,
    ) {
        viewModel.setPage(AppPage.Home)
    }
    BoxWithConstraints(Modifier.fillMaxSize()) {
        var phoneLandscapeScrollChromeHidden by remember { mutableStateOf(false) }
        val horizontalChrome = maxWidth > maxHeight
        val phoneLandscapeChrome = !tvProfile && !inStream && isPhoneLandscape(maxWidth, maxHeight)
        val portraitChrome = !inStream && maxHeight >= maxWidth
        val showNavigationRail = !inStream && (tvProfile || phoneLandscapeChrome)
        val scrollChromePage = state.page == AppPage.Home || state.page == AppPage.Library
        val wallpaperPage = scrollChromePage || state.page == AppPage.Settings
        val tvCatalogChrome = tvProfile && scrollChromePage
        val mobileCatalogControlsInTopBar = !tvProfile && portraitChrome
        val storeControlsInTopBar = (mobileCatalogControlsInTopBar || phoneLandscapeChrome || tvCatalogChrome) && state.page == AppPage.Home
        val libraryControlsInTopBar = (mobileCatalogControlsInTopBar || phoneLandscapeChrome || tvCatalogChrome) && state.page == AppPage.Library
        val screenEdgePadding = appContentEdgePaddingDp(
            settings = state.settings,
            inStream = inStream,
            tvProfile = tvProfile,
        ).dp
        LaunchedEffect(phoneLandscapeChrome, scrollChromePage) {
            if (!phoneLandscapeChrome || !scrollChromePage) {
                phoneLandscapeScrollChromeHidden = false
            }
        }
        val systemWallpaperVisible = shouldShowSystemWallpaperBackground(state.settings, inStream)
        if (!systemWallpaperVisible) {
            Box(Modifier.fillMaxSize().background(MaterialTheme.colorScheme.background))
        }
        // A chosen app wallpaper stays visible through Settings as well as Store and Library.
        // Stream surfaces remain opaque so custom art can never compete with video or overlays.
        val wallpaperVisible = shouldShowAppWallpaper(state.page, inStream, state.settings)
        if (wallpaperVisible) {
            CatalogWallpaperBackdrop(
                settings = state.settings,
                tvProfile = tvProfile,
                width = maxWidth,
                height = maxHeight,
            )
        } else if (!inStream && state.settings.ambientBackgroundEnabled) {
            AmbientBackground()
        }

        Scaffold(
            containerColor = Color.Transparent,
            contentWindowInsets = if (streamingActive || tvProfile) WindowInsets(0, 0, 0, 0) else ScaffoldDefaults.contentWindowInsets,
            bottomBar = {
                if (!inStream && !showNavigationRail) {
                    Column {
                        if (showMinimizedQueueDock) {
                            MinimizedQueueDock(
                                state = state,
                                onRestore = viewModel::restoreStreamLaunch,
                                onCancel = viewModel::stopStream,
                            )
                        }
                        NavigationBar(
                            containerColor = MaterialTheme.colorScheme.background,
                            tonalElevation = 0.dp,
                        ) {
                            BottomNavItem(
                                selected = state.page == AppPage.Home,
                                onClick = {
                                    visibleSearchTarget = null
                                    viewModel.setPage(AppPage.Home)
                                },
                                iconRes = R.drawable.ic_tab_store,
                                label = stringResource(R.string.nav_store),
                            )
                            BottomNavItem(
                                // Search is a mode, not a destination: it never claims selection.
                                selected = false,
                                onClick = { revealSearch() },
                                iconRes = R.drawable.ic_search,
                                label = stringResource(R.string.nav_search),
                            )
                            BottomNavItem(
                                selected = state.page == AppPage.Library,
                                onClick = {
                                    visibleSearchTarget = null
                                    viewModel.setPage(AppPage.Library)
                                },
                                iconRes = R.drawable.ic_tab_library,
                                label = stringResource(R.string.nav_library),
                            )
                            BottomNavItem(
                                selected = state.page == AppPage.Settings,
                                onClick = {
                                    navigateFromAppChrome(AppPage.Settings)
                                },
                                iconRes = R.drawable.ic_tab_settings,
                                label = stringResource(R.string.nav_settings),
                            )
                        }
                    }
                }
            },
        ) { padding ->
            Box(
                Modifier
                    .fillMaxSize()
                    .padding(padding)
                    .onPreviewKeyEvent { event ->
                        if (isNavigationToneKey(event)) {
                            navAudioController.playButtonTone(navigationToneEnabled)
                        }
                        false
                    },
            ) {
                Row(
                    Modifier
                        .fillMaxSize()
                        .padding(screenEdgePadding),
                ) {
                    if (showNavigationRail) {
                        AppNavigationRail(
                            state = state,
                            activeSearchTarget = visibleSearchTarget,
                            largeIcons = phoneLandscapeChrome,
                            darkenForCatalogBackground =
                                (state.settings.nerdCatalogBackground || systemWallpaperVisible) && wallpaperPage,
                            showSettingsBack = shouldShowSettingsBackRail(
                                tvProfile = tvProfile,
                                settingsPageOpen = state.page == AppPage.Settings,
                                horizontalChrome = horizontalChrome,
                                detailRouteOpen = settingsDetailRouteOpen,
                            ),
                            onNavigate = { page ->
                                navigateFromAppChrome(page)
                            },
                            onSearch = { revealSearch(it) },
                            onSettingsBack = { settingsBackRequestToken += 1 },
                            streamReturnFocusRequester = tvStreamReturnFocusRequester,
                        )
                    }
                    Column(
                        Modifier
                            .weight(1f)
                            .fillMaxHeight(),
                    ) {
                        AnimatedVisibility(
                            visible = shouldShowTopStatusBar(
                                inStream = inStream,
                                portraitChrome = portraitChrome,
                                phoneLandscapeChrome = phoneLandscapeChrome,
                                phoneLandscapeScrollChromeHidden = phoneLandscapeScrollChromeHidden,
                                tvProfile = tvProfile,
                            ),
                        ) {
                            if (!inStream) {
                                TopStatusBar(
                                    state = state,
                                    profileFocusRequester = topBarProfileFocusRequester,
                                    catalogControlFocusRequester = catalogFilterFocusRequester.takeIf {
                                        storeControlsInTopBar || libraryControlsInTopBar
                                    },
                                    onResumeActiveSession = viewModel::resumeActiveSession,
                                    onOpenSettings = { navigateFromAppChrome(AppPage.Settings) },
                                    onOpenLocalApps = {
                                        visibleSearchTarget = null
                                        viewModel.openInterfaceSettings()
                                    },
                                    onOpenStreamSettings = viewModel::openStreamSettings,
                                    musicControl = musicControl,
                                    showRefreshAction = shouldShowTvRefreshAction(tvProfile, inStream),
                                    refreshing = when (state.page) {
                                        AppPage.Settings -> state.settingsRefreshing
                                        AppPage.Home, AppPage.Library -> state.loadingGames
                                        AppPage.Stream -> false
                                    },
                                    onRefresh = {
                                        when (state.page) {
                                            AppPage.Settings -> viewModel.refreshSettings()
                                            AppPage.Home, AppPage.Library -> viewModel.refreshGames()
                                            AppPage.Stream -> Unit
                                        }
                                    },
                                    showChromeScrim = portraitChrome,
                                ) {
                                    if (storeControlsInTopBar) {
                                        Spacer(Modifier.weight(1f))
                                        val filterOptions = rememberCatalogFilterOptions(
                                            remember(state.catalogResult.filterGroups) {
                                                catalogVisibleFilterGroups(state.catalogResult.filterGroups)
                                            },
                                        )
                                        CatalogSortFilterMenu(
                                            sortOptions = state.catalogResult.sortOptions,
                                            selectedSortId = state.catalogSortId,
                                            filterOptions = filterOptions,
                                            selectedFilterIds = state.catalogFilterIds,
                                            onSortChange = viewModel::setCatalogSort,
                                            onFilterToggle = viewModel::toggleCatalogFilter,
                                            focusRequester = catalogFilterFocusRequester,
                                            leadingFocusRequester = topBarProfileFocusRequester,
                                        )
                                    } else if (libraryControlsInTopBar) {
                                        val orderedLibraryGames = remember(
                                            state.libraryGames,
                                            state.settings.favoriteGameIds,
                                            state.librarySortId,
                                        ) {
                                            sortLibraryGames(
                                                favoriteOrderedGames(state.libraryGames, state.settings.favoriteGameIds),
                                                state.librarySortId,
                                            )
                                        }
                                        val touchFilterLabel = stringResource(R.string.catalog_filter_touch_controls)
                                        val libraryFilterOptions = remember(orderedLibraryGames, touchFilterLabel) {
                                            libraryStoreFilterOptions(orderedLibraryGames, touchFilterLabel)
                                        }
                                        Spacer(Modifier.weight(1f))
                                        CatalogSortFilterMenu(
                                            sortOptions = librarySortOptions(),
                                            selectedSortId = state.librarySortId,
                                            filterOptions = libraryFilterOptions,
                                            selectedFilterIds = state.libraryFilterIds,
                                            onSortChange = viewModel::setLibrarySort,
                                            onFilterToggle = viewModel::toggleLibraryFilter,
                                            focusRequester = catalogFilterFocusRequester,
                                            leadingFocusRequester = topBarProfileFocusRequester,
                                        )
                                    }
                                }
                            }
                        }
                        Box(
                            Modifier
                                .weight(1f)
                                .fillMaxWidth(),
                        ) {
                            CompositionLocalProvider(LocalSelectedCatalogGameId provides state.selectedGame?.id) {
                                when (state.page) {
                                    AppPage.Home -> HomeScreen(
                                        state = state,
                                        viewModel = viewModel,
                                        tvProfile = tvProfile,
                                        hideChromeWhenScrolled = phoneLandscapeChrome,
                                        controlsInTopBar = storeControlsInTopBar,
                                        topBarFocusRequester = catalogFilterFocusRequester.takeIf {
                                            storeControlsInTopBar
                                        },
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
                                        controlsInTopBar = libraryControlsInTopBar,
                                        topBarFocusRequester = catalogFilterFocusRequester.takeIf {
                                            libraryControlsInTopBar
                                        },
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
                                        backRequestToken = settingsBackRequestToken,
                                        onSearchQueryChange = { next ->
                                            settingsSearchQuery = next
                                            if (next.isBlank() && visibleSearchTarget == SearchTarget.Settings) {
                                                visibleSearchTarget = null
                                            }
                                        },
                                        onDetailRouteChange = { settingsDetailRouteOpen = it },
                                    )
                                    AppPage.Stream -> StreamScreen(
                                        state = state,
                                        viewModel = viewModel,
                                        onMicrophoneCaptureActiveChange = onMicrophoneCaptureActiveChange,
                                    )
                                }
                            }
                        }
                        if (showMinimizedQueueDock && showNavigationRail) {
                            MinimizedQueueDock(
                                state = state,
                                onRestore = viewModel::restoreStreamLaunch,
                                onCancel = viewModel::stopStream,
                            )
                        }
                    }
                }
                state.selectedGame?.takeIf { !inStream && !modalPickerOpen }?.let { game ->
                    ControllerModalOverlay(onDismissRequest = viewModel::clearSelectedGame) {
                        // Keep details in the app's window so the activated artwork and destination
                        // banner share coordinates and the first visible frame follows the haptic.
                        GameDetailsSheet(
                            game = game,
                            favorite = game.id in state.settings.favoriteGameIds,
                            defaultVariantId = state.settings.defaultGameVariantIds[game.id],
                            fullScreen = tvProfile,
                            safeAreaPadding = screenEdgePadding,
                            onPlay = viewModel::play,
                            onChooseStore = viewModel::chooseStore,
                            onFavorite = viewModel::updateFavorites,
                            connectedTvName = state.localTvConnector.connectedTvName,
                            onPlayOnTv = viewModel::playOnLocalTv,
                            onDismiss = viewModel::clearSelectedGame,
                        )
                    }
                }
                state.pendingPrintedWasteGame?.let { game ->
                    ControllerModalDialog(onDismissRequest = viewModel::dismissPrintedWasteSelector) {
                        AnimatedLaunchOverlay(Modifier.fillMaxSize()) {
                            PrintedWasteSelector(state, game, viewModel)
                        }
                    }
                }
                state.pendingMembershipNotice?.let { notice ->
                    ControllerModalDialog(onDismissRequest = viewModel::dismissMembershipNotice) {
                        MembershipRequirementDialog(
                            notice = notice,
                            onCancel = viewModel::dismissMembershipNotice,
                            onContinue = viewModel::continuePastMembershipNotice,
                        )
                    }
                }
                state.pendingStoreChoiceGame?.let { game ->
                    ControllerModalDialog(onDismissRequest = viewModel::dismissStoreChoice) {
                        AnimatedLaunchOverlay(Modifier.fillMaxSize()) {
                            StoreLaunchSelector(
                                game = game,
                                defaultVariantId = state.settings.defaultGameVariantIds[game.id],
                                onLaunch = viewModel::playVariant,
                                onSetDefaultStore = viewModel::setDefaultGameVariant,
                                onDismiss = viewModel::dismissStoreChoice,
                            )
                        }
                    }
                }
            }
        }
    }
}

@Composable
private fun ControllerModalOverlay(
    onDismissRequest: () -> Unit,
    content: @Composable () -> Unit,
) {
    Box(
        Modifier
            .fillMaxSize()
            .zIndex(10f)
            .onPreviewKeyEvent { event ->
                if (event.key == Key.ButtonB || event.key == Key.Back || event.key == Key.Escape) {
                    if (event.type == KeyEventType.KeyUp) onDismissRequest()
                    true
                } else {
                    false
                }
            },
        contentAlignment = Alignment.Center,
    ) {
        content()
    }
}

@Composable
private fun ControllerModalDialog(
    onDismissRequest: () -> Unit,
    content: @Composable () -> Unit,
) {
    Dialog(
        onDismissRequest = onDismissRequest,
        properties = DialogProperties(
            dismissOnBackPress = true,
            dismissOnClickOutside = false,
            usePlatformDefaultWidth = false,
        ),
    ) {
        Box(
            Modifier
                .fillMaxSize()
                .onPreviewKeyEvent { event ->
                    if (event.key == Key.ButtonB || event.key == Key.Back || event.key == Key.Escape) {
                        if (event.type == KeyEventType.KeyUp) onDismissRequest()
                        true
                    } else {
                        false
                    }
                },
            contentAlignment = Alignment.Center,
        ) {
            content()
        }
    }
}

internal fun shouldRestoreTvNavigationFocus(
    previouslyInStream: Boolean,
    currentlyInStream: Boolean,
    tvProfile: Boolean,
): Boolean = tvProfile && previouslyInStream && !currentlyInStream

@Composable
private fun AppNavigationRail(
    state: OpenNowUiState,
    activeSearchTarget: SearchTarget?,
    largeIcons: Boolean,
    darkenForCatalogBackground: Boolean,
    showSettingsBack: Boolean,
    onNavigate: (AppPage) -> Unit,
    onSearch: (SearchTarget) -> Unit,
    onSettingsBack: () -> Unit,
    streamReturnFocusRequester: FocusRequester,
) {
    val bonanzaActive = LocalAbsoluteCinemaEverywhere.current
    Box(
        modifier = Modifier
            .width(APP_NAV_RAIL_WIDTH)
            .fillMaxHeight()
            .padding(start = 6.dp, top = 8.dp, end = 6.dp, bottom = 8.dp),
    ) {
        Surface(
            modifier = Modifier.fillMaxSize(),
            shape = RoundedCornerShape(26.dp),
            color = if (bonanzaActive) Color.Transparent else navigationRailScrim(darkenForCatalogBackground),
            border = if (bonanzaActive) BorderStroke(1.dp, Color.White.copy(alpha = 0.88f)) else null,
            tonalElevation = 0.dp,
            shadowElevation = 0.dp,
        ) {
            Box(Modifier.fillMaxSize()) {
                Column(
                    modifier = Modifier
                        .align(if (showSettingsBack) Alignment.BottomCenter else Alignment.Center)
                        .fillMaxWidth()
                        .padding(bottom = if (showSettingsBack) 8.dp else 0.dp),
                    horizontalAlignment = Alignment.CenterHorizontally,
                ) {
                    AppNavigationRailItem(
                        selected = state.page == AppPage.Home,
                        onClick = { onNavigate(AppPage.Home) },
                        iconRes = R.drawable.ic_tab_store,
                        label = stringResource(R.string.nav_store),
                        iconSize = if (largeIcons) 30.dp else 24.dp,
                        focusRequester = streamReturnFocusRequester,
                    )
                    AppNavigationRailItem(
                        // See the bottom bar: search is a mode, not a destination.
                        selected = false,
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
                        iconSize = if (largeIcons) 30.dp else 24.dp,
                    )
                    AppNavigationRailItem(
                        selected = state.page == AppPage.Library,
                        onClick = { onNavigate(AppPage.Library) },
                        iconRes = R.drawable.ic_tab_library,
                        label = stringResource(R.string.nav_library),
                        iconSize = if (largeIcons) 30.dp else 24.dp,
                    )
                    AppNavigationRailItem(
                        selected = state.page == AppPage.Settings,
                        onClick = { onNavigate(AppPage.Settings) },
                        iconRes = R.drawable.ic_tab_settings,
                        label = stringResource(R.string.nav_settings),
                        iconSize = if (largeIcons) 30.dp else 24.dp,
                        showConnectionDot = shouldShowLocalTvConnectionDot(
                            tvProfile = state.androidTvProfile,
                            pairedDeviceName = state.localTvConnector.pairedDeviceName,
                        ),
                    )
                    AnimatedVisibility(visible = showSettingsBack) {
                        Column(horizontalAlignment = Alignment.CenterHorizontally) {
                            Spacer(Modifier.height(6.dp))
                            AppNavigationRailItem(
                                selected = false,
                                onClick = onSettingsBack,
                                iconRes = R.drawable.ic_arrow_back,
                                label = stringResource(R.string.action_back),
                                iconSize = if (largeIcons) 30.dp else 24.dp,
                            )
                        }
                    }
                }
            }
        }
    }
}

internal fun navigationRailScrim(darkenForCatalogBackground: Boolean): Color =
    if (darkenForCatalogBackground) Color.Black.copy(alpha = 0.76f) else ChromeScrim

internal fun shouldShowLocalTvConnectionDot(tvProfile: Boolean, pairedDeviceName: String?): Boolean =
    tvProfile && !pairedDeviceName.isNullOrBlank()

internal fun shouldShowLocalAppsProfileAction(
    localAppLauncherSupported: Boolean,
    @Suppress("UNUSED_PARAMETER")
    localAppsEnabled: Boolean,
): Boolean = localAppLauncherSupported

internal fun shouldAnimateOpenNowAppIcon(
    codecReport: RuntimeCodecReport?,
    reduceMotion: Boolean,
    absoluteCinemaEnabled: Boolean,
    androidTvProfile: Boolean = false,
): Boolean {
    if (reduceMotion) return false
    if (androidTvProfile) return true
    return absoluteCinemaEnabled &&
        codecReport?.let { !it.lowPowerGpuProfile && !it.constrainedRuntimeProfile } == true
}

internal fun shouldShowTopStatusBar(
    inStream: Boolean,
    portraitChrome: Boolean,
    phoneLandscapeChrome: Boolean,
    phoneLandscapeScrollChromeHidden: Boolean,
    tvProfile: Boolean,
): Boolean =
    !inStream && (
        tvProfile ||
            portraitChrome ||
            (phoneLandscapeChrome && !phoneLandscapeScrollChromeHidden)
        )

internal fun shouldShowTvRefreshAction(tvProfile: Boolean, inStream: Boolean): Boolean =
    tvProfile && !inStream

internal fun shouldShowSettingsBackRail(
    tvProfile: Boolean,
    settingsPageOpen: Boolean,
    horizontalChrome: Boolean,
    detailRouteOpen: Boolean,
): Boolean = !tvProfile && settingsPageOpen && horizontalChrome && detailRouteOpen

@Composable
private fun AppNavigationRailItem(
    selected: Boolean,
    onClick: () -> Unit,
    iconRes: Int,
    label: String,
    modifier: Modifier = Modifier,
    iconSize: Dp = 24.dp,
    focusRequester: FocusRequester? = null,
    showConnectionDot: Boolean = false,
) {
    var focused by remember { mutableStateOf(false) }
    val haptics = LocalOpenNowHaptics.current
    // Flat rail chrome is fixed; only the animated frame around it follows the accent.
    val navTint = NavigationSelectionColor
    val showActiveFrame = selected || focused
    val contentColor = when {
        focused -> Color.White
        selected -> navTint
        else -> TextMuted
    }
    Box(
        modifier = modifier
            .fillMaxWidth()
            .padding(horizontal = 5.dp, vertical = 2.dp),
    ) {
        Surface(
            onClick = {
                haptics?.play(HapticCue.Activate)
                onClick()
            },
            modifier = Modifier
                .fillMaxWidth()
                .onFocusChanged { focused = it.isFocused }
                .focusMoveHaptics()
                .then(focusRequester?.let { Modifier.focusRequester(it) } ?: Modifier),
            shape = RoundedCornerShape(18.dp),
            color = if (selected && !focused) navTint.copy(alpha = 0.12f) else Color.Transparent,
            border = null,
            tonalElevation = 0.dp,
            shadowElevation = 0.dp,
        ) {
            Column(
                modifier = Modifier
                    .fillMaxWidth()
                    .heightIn(min = 58.dp)
                    .padding(horizontal = 4.dp, vertical = 6.dp),
                horizontalAlignment = Alignment.CenterHorizontally,
                verticalArrangement = Arrangement.Center,
            ) {
                Icon(
                    painter = painterResource(iconRes),
                    contentDescription = label,
                    tint = contentColor,
                    modifier = Modifier.size(if (selected) iconSize + 2.dp else iconSize),
                )
                if (showConnectionDot) {
                    Spacer(Modifier.height(2.dp))
                    Box(
                        Modifier
                            .size(6.dp)
                            .clip(CircleShape)
                            .background(Color(0xffb56cff)),
                    )
                }
                Spacer(Modifier.height(2.dp))
                Text(
                    label,
                    color = contentColor,
                    style = MaterialTheme.typography.labelSmall,
                    fontWeight = if (selected) FontWeight.ExtraBold else FontWeight.Medium,
                    maxLines = 1,
                    overflow = TextOverflow.Ellipsis,
                )
            }
        }
        InteractionFocusFrame(
            visible = showActiveFrame,
            cornerRadius = 18.dp,
            cinemaEffectEnabled = LocalAbsoluteCinemaEffects.current,
        )
    }
}

private data class TopBarMusicControl(
    val visible: Boolean,
    val playing: Boolean,
    val muted: Boolean,
    val onToggle: () -> Unit,
)

@Composable
private fun RowScope.BottomNavItem(
    selected: Boolean,
    onClick: () -> Unit,
    iconRes: Int,
    label: String,
) {
    val haptics = LocalOpenNowHaptics.current
    // See AppNavigationRailItem: the tab bar is the rail rotated, and keeps the same fixed tint.
    val navTint = NavigationSelectionColor
    NavigationBarItem(
        selected = selected,
        onClick = {
            haptics?.play(HapticCue.Activate)
            onClick()
        },
        colors = NavigationBarItemDefaults.colors(
            selectedIconColor = navTint,
            selectedTextColor = navTint,
            indicatorColor = navTint.copy(alpha = 0.20f),
            unselectedIconColor = TextMuted,
            unselectedTextColor = TextMuted,
        ),
        icon = {
            Box(
                modifier = Modifier.size(width = 46.dp, height = 34.dp),
                contentAlignment = Alignment.Center,
            ) {
                Icon(
                    painter = painterResource(iconRes),
                    contentDescription = null,
                    modifier = Modifier.size(if (selected) 27.dp else 24.dp),
                )
            }
        },
        label = {
            Text(
                label,
                fontWeight = if (selected) FontWeight.ExtraBold else FontWeight.Medium,
                maxLines = 1,
                overflow = TextOverflow.Ellipsis,
            )
        },
    )
}

@Composable
private fun TopStatusBar(
    state: OpenNowUiState,
    profileFocusRequester: FocusRequester,
    catalogControlFocusRequester: FocusRequester?,
    onResumeActiveSession: () -> Unit,
    onOpenSettings: () -> Unit,
    onOpenLocalApps: () -> Unit,
    onOpenStreamSettings: () -> Unit,
    musicControl: TopBarMusicControl,
    showRefreshAction: Boolean = false,
    refreshing: Boolean = false,
    onRefresh: () -> Unit = {},
    showChromeScrim: Boolean = true,
    content: @Composable RowScope.() -> Unit = {},
) {
    val barScrim = if (showChromeScrim) ChromeScrim else Color.Transparent
    Surface(
        modifier = Modifier
            .fillMaxWidth()
            .padding(start = 8.dp, top = 8.dp, end = 8.dp, bottom = 5.dp),
        shape = RoundedCornerShape(24.dp),
        color = barScrim,
        border = if (LocalGameCardBordersEnabled.current) {
            BorderStroke(0.5.dp, Color.White)
        } else {
            null
        },
        tonalElevation = 0.dp,
        shadowElevation = 0.dp,
    ) {
        Row(
            Modifier.padding(horizontal = 12.dp, vertical = 7.dp),
            verticalAlignment = Alignment.CenterVertically,
        ) {
            TopBarProfileMenu(
                state = state,
                focusRequester = profileFocusRequester,
                nextFocusRequester = catalogControlFocusRequester,
                onOpenSettings = onOpenSettings,
                onOpenLocalApps = onOpenLocalApps,
            )
            Spacer(Modifier.width(8.dp))
            Row(
                Modifier.weight(1f),
                horizontalArrangement = Arrangement.spacedBy(6.dp),
                verticalAlignment = Alignment.CenterVertically,
            ) {
                if (state.settings.nerdMode) {
                    TopStatusDetails(state, onOpenStreamSettings)
                }
                // Mobile catalogue controls include their own weighted spacer. Keeping them last
                // makes the merged filter button occupy the slot immediately before Music, or the
                // Music slot itself when music controls are disabled.
                content()
            }
            if (musicControl.visible) {
                Spacer(Modifier.width(6.dp))
                TopBarMusicButton(musicControl)
            }
            if (showRefreshAction) {
                Spacer(Modifier.width(6.dp))
                TopBarRefreshButton(refreshing = refreshing, onRefresh = onRefresh)
            }
            if (state.activeSession != null) {
                Spacer(Modifier.width(6.dp))
                ElevatedButton(
                    onClick = onResumeActiveSession,
                    contentPadding = PaddingValues(horizontal = 12.dp, vertical = 7.dp),
                ) {
                    Text(stringResource(R.string.action_resume), style = MaterialTheme.typography.labelMedium)
                }
            }
        }
    }
}

@Composable
private fun TopBarRefreshButton(
    refreshing: Boolean,
    onRefresh: () -> Unit,
) {
    var focused by remember { mutableStateOf(false) }
    val shape = RoundedCornerShape(14.dp)
    Box {
        Surface(
            shape = shape,
            color = Color.White.copy(alpha = 0.1f),
            tonalElevation = 0.dp,
        ) {
            IconButton(
                onClick = onRefresh,
                enabled = !refreshing,
                modifier = Modifier
                    .size(40.dp)
                    .onFocusChanged { focused = it.isFocused },
            ) {
                if (refreshing) {
                    CircularProgressIndicator(
                        modifier = Modifier.size(18.dp),
                        strokeWidth = 2.dp,
                        color = TextPrimary,
                    )
                } else {
                    Icon(
                        imageVector = Icons.Rounded.Refresh,
                        contentDescription = stringResource(R.string.action_refresh),
                        tint = TextPrimary,
                        modifier = Modifier.size(22.dp),
                    )
                }
            }
        }
        InteractionFocusFrame(
            visible = focused,
            cornerRadius = 14.dp,
            cinemaEffectEnabled = LocalAbsoluteCinemaEverywhere.current,
        )
    }
}

@Composable
private fun TopBarProfileMenu(
    state: OpenNowUiState,
    focusRequester: FocusRequester,
    nextFocusRequester: FocusRequester?,
    onOpenSettings: () -> Unit,
    onOpenLocalApps: () -> Unit,
) {
    var expanded by remember { mutableStateOf(false) }
    BackHandler(enabled = expanded) { expanded = false }
    var focused by remember { mutableStateOf(false) }
    val currentUser = state.authSession?.user
    val savedAccount = state.savedAccounts.firstOrNull { it.userId == currentUser?.userId }
        ?: state.savedAccounts.firstOrNull()
    val displayName = currentUser?.displayName?.takeIf { it.isNotBlank() }
        ?: savedAccount?.displayName?.takeIf { it.isNotBlank() }
        ?: stringResource(R.string.profile_not_available)
    val tier = state.subscriptionInfo?.membershipTier?.takeIf { it.isNotBlank() }
        ?: currentUser?.membershipTier?.takeIf { it.isNotBlank() }
        ?: savedAccount?.membershipTier?.takeIf { it.isNotBlank() }
        ?: stringResource(R.string.profile_not_available)
    val email = currentUser?.email?.takeIf { it.isNotBlank() }
        ?: savedAccount?.email?.takeIf { it.isNotBlank() }
        ?: stringResource(R.string.profile_not_available)
    val profileDescription = stringResource(R.string.profile_menu_description)
    val haptics = LocalOpenNowHaptics.current
    val launcherControl = rememberDefaultLauncherControl()

    Box {
        Box(
            modifier = Modifier
                .size(40.dp)
                .focusRequester(focusRequester)
                .then(
                    nextFocusRequester?.let { next ->
                        Modifier.focusProperties { right = next }
                    } ?: Modifier,
                )
                .onFocusChanged { focused = it.isFocused }
                .focusMoveHaptics()
                .semantics {
                    contentDescription = profileDescription
                    role = Role.Button
                }
                .clickable {
                    haptics?.play(HapticCue.Activate)
                    expanded = true
                },
            contentAlignment = Alignment.Center,
        ) {
            OpenNowAppIcon(
                size = 32.dp,
                animate = shouldAnimateOpenNowAppIcon(
                    codecReport = state.codecReport,
                    reduceMotion = LocalReduceMotion.current,
                    absoluteCinemaEnabled = LocalAbsoluteCinemaEffects.current,
                    androidTvProfile = state.androidTvProfile,
                ),
            )
            ControllerFocusFrame(
                visible = focused,
                cornerRadius = 14.dp,
                tint = if (LocalAbsoluteCinemaEffects.current) LocalActiveSelectionColor.current else Color.White,
                secondaryTint = if (LocalAbsoluteCinemaEffects.current) LocalActiveSelectionSecondaryColor.current else Color.White,
            )
        }
        DropdownMenu(
            expanded = expanded,
            onDismissRequest = { expanded = false },
            modifier = Modifier
                .widthIn(min = 250.dp, max = 300.dp),
            // Popups must stay readable over custom wallpaper. Effects Bonanza belongs on each
            // focused item; only the landscape navigation rail is allowed to become transparent.
            containerColor = ProfileMenuContainerColor,
        ) {
            Column(
                Modifier.padding(horizontal = 16.dp, vertical = 10.dp),
                verticalArrangement = Arrangement.spacedBy(3.dp),
            ) {
                Text(
                    stringResource(R.string.profile_identity, displayName, tier),
                    color = TextPrimary,
                    style = MaterialTheme.typography.titleSmall,
                    fontWeight = FontWeight.ExtraBold,
                    maxLines = 1,
                    overflow = TextOverflow.Ellipsis,
                )
                Text(
                    email,
                    color = TextMuted,
                    style = MaterialTheme.typography.bodySmall,
                    maxLines = 1,
                    overflow = TextOverflow.Ellipsis,
                )
            }
            if (BuildConfig.LOCAL_APP_LAUNCHER_SUPPORTED) {
                ProfileDropdownMenuItem(
                    text = {
                        Text(
                            stringResource(
                                if (launcherControl.isDefault) R.string.settings_default_launcher_selected
                                else R.string.settings_default_launcher,
                            ),
                            fontWeight = FontWeight.SemiBold,
                        )
                    },
                    onClick = {
                        expanded = false
                        haptics?.play(HapticCue.Activate)
                        launcherControl.request()
                    },
                )
                if (shouldShowLocalAppsProfileAction(BuildConfig.LOCAL_APP_LAUNCHER_SUPPORTED, state.settings.localAppsEnabled)) {
                    ProfileDropdownMenuItem(
                        text = {
                            Text(
                                stringResource(R.string.settings_local_apps),
                                fontWeight = FontWeight.SemiBold,
                            )
                        },
                        onClick = {
                            expanded = false
                            haptics?.play(HapticCue.Activate)
                            onOpenLocalApps()
                        },
                    )
                }
            }
            HorizontalDivider(color = Color.White.copy(alpha = 0.10f))
            ProfileDropdownMenuItem(
                text = {
                    Text(
                        stringResource(R.string.nav_settings),
                        color = LocalSelectionTintColor.current,
                        fontWeight = FontWeight.Bold,
                    )
                },
                onClick = {
                    expanded = false
                    haptics?.play(HapticCue.Activate)
                    onOpenSettings()
                },
            )
        }
    }
}

@Composable
private fun ProfileDropdownMenuItem(
    text: @Composable () -> Unit,
    onClick: () -> Unit,
) {
    var focused by remember { mutableStateOf(false) }
    Box(Modifier.fillMaxWidth()) {
        DropdownMenuItem(
            text = text,
            onClick = onClick,
            modifier = Modifier.onFocusChanged { focused = it.isFocused || it.hasFocus },
        )
        InteractionFocusFrame(
            visible = focused,
            cornerRadius = 4.dp,
            cinemaEffectEnabled = LocalAbsoluteCinemaEverywhere.current,
        )
    }
}

@Composable
private fun TopStatusDetails(
    state: OpenNowUiState,
    onOpenStreamSettings: () -> Unit,
) {
    val stream = state.activeStreamSettings ?: state.settings.stream
    val summary = streamStatusSummary(stream)
    var focused by remember { mutableStateOf(false) }
    val shape = RoundedCornerShape(999.dp)
    Row(
        horizontalArrangement = Arrangement.spacedBy(5.dp),
        verticalAlignment = Alignment.CenterVertically,
    ) {
        Surface(
            modifier = Modifier
                .height(TopBarCompactControlHeight)
                .onFocusChanged { focused = it.isFocused }
                .focusMoveHaptics()
                .semantics { contentDescription = "Open Stream settings: $summary" }
                .clickable(onClick = onOpenStreamSettings)
                .then(
                    if (focused) Modifier.border(2.dp, MaterialTheme.colorScheme.primary, shape) else Modifier,
                ),
            shape = shape,
            color = if (focused) MaterialTheme.colorScheme.primary.copy(alpha = 0.22f) else PanelAlt.copy(alpha = 0.9f),
            tonalElevation = 0.dp,
        ) {
            Box(Modifier.fillMaxHeight().padding(horizontal = 8.dp), contentAlignment = Alignment.Center) {
                Text(
                    summary,
                    color = TextMuted,
                    style = MaterialTheme.typography.labelSmall,
                    maxLines = 1,
                    overflow = TextOverflow.Ellipsis,
                )
            }
        }
    }
}

@Composable
private fun TopBarMusicButton(control: TopBarMusicControl) {
    val description = when {
        control.muted -> "Music muted"
        control.playing -> "Music playing"
        else -> "Music ready"
    }
    Surface(
        modifier = Modifier
            .width(38.dp)
            .height(TopBarCompactControlHeight)
            .semantics { contentDescription = description }
            .clickable(onClick = control.onToggle),
        shape = RoundedCornerShape(999.dp),
        color = if (control.muted) OpenNowPalette.ErrorContainer.copy(alpha = 0.92f) else PanelAlt.copy(alpha = 0.78f),
        tonalElevation = 0.dp,
    ) {
        Box(Modifier.fillMaxSize(), contentAlignment = Alignment.Center) {
            if (control.muted) {
                Icon(
                    painter = painterResource(R.drawable.ic_volume_off),
                    contentDescription = null,
                    tint = OpenNowPalette.OnErrorContainer,
                    modifier = Modifier.size(17.dp),
                )
            } else {
                MusicBars(playing = control.playing)
            }
        }
    }
}

@Composable
private fun MusicBars(playing: Boolean, modifier: Modifier = Modifier) {
    val phase = if (playing && !LocalReduceMotion.current) {
        rememberInfiniteTransition(label = "top-bar-music-bars").animateFloat(
            initialValue = 0f,
            targetValue = 1f,
            animationSpec = infiniteRepeatable(
                animation = tween(durationMillis = 820, easing = LinearEasing),
                repeatMode = RepeatMode.Restart,
            ),
            label = "top-bar-music-bars-phase",
        )
    } else {
        null
    }
    val color = MaterialTheme.colorScheme.primary
    Canvas(modifier.size(width = 18.dp, height = 16.dp)) {
        val barWidth = size.width / 5.8f
        val gap = (size.width - barWidth * 3f) / 2f
        repeat(3) { index ->
            val wave = if (phase != null) {
                ((sin((phase.value.toDouble() * 6.283185307179586) + index * 1.35) + 1.0) / 2.0).toFloat()
            } else {
                0.36f + index * 0.12f
            }
            val barHeight = size.height * (0.32f + wave * 0.58f)
            val left = index * (barWidth + gap)
            drawRoundRect(
                color = color,
                topLeft = Offset(left, size.height - barHeight),
                size = Size(barWidth, barHeight),
                cornerRadius = CornerRadius(barWidth, barWidth),
            )
        }
    }
}

private fun streamStatusSummary(stream: StreamSettings): String =
    listOf(
        formatTopBarResolution(stream.resolution),
        stream.aspectRatio,
        stream.codec.name,
        "${stream.fps} FPS",
    ).filter { it.isNotBlank() }.joinToString(" • ")

private fun formatTopBarResolution(resolution: String): String {
    val parts = resolution.lowercase(Locale.US).split("x", limit = 2)
    return if (parts.size == 2 && parts.all { it.trim().isNotBlank() }) {
        "${parts[0].trim()} × ${parts[1].trim()}"
    } else {
        resolution
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

internal fun handleDpadFocusMove(event: androidx.compose.ui.input.key.KeyEvent, focusManager: FocusManager): Boolean {
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

internal fun Modifier.lockedFocusGroup(): Modifier =
    focusProperties { onExit = { cancelFocusChange() } }
        .focusGroup()

private fun isNavigationToneKey(event: androidx.compose.ui.input.key.KeyEvent): Boolean =
    event.type == KeyEventType.KeyDown &&
        event.key in setOf(
            Key.DirectionUp,
            Key.DirectionDown,
            Key.DirectionLeft,
            Key.DirectionRight,
        )

internal fun handleVerticalDpadFocusMove(event: androidx.compose.ui.input.key.KeyEvent, focusManager: FocusManager): Boolean {
    if (event.type != KeyEventType.KeyDown) return false
    val direction = when (event.key) {
        Key.DirectionUp -> FocusDirection.Up
        Key.DirectionDown -> FocusDirection.Down
        else -> return false
    }
    return focusManager.moveFocus(direction)
}

/**
 * Key event handler for Compose Sliders when navigated by TV remote or D-pad controller.
 * - D-pad Up/Down  → moves focus to the next/previous focusable element.
 * - D-pad Left     → decrements the slider value by [step], clamped to [min].
 * - D-pad Right    → increments the slider value by [step], clamped to [max].
 * Returns true when the event is consumed (Left/Right) so that Compose does not
 * move focus sideways instead of changing the value.
 */
internal fun handleSliderDpadInput(
    event: androidx.compose.ui.input.key.KeyEvent,
    value: Float,
    min: Float,
    max: Float,
    step: Float,
    focusManager: FocusManager,
    onValueAdjusted: (Float) -> Unit,
): Boolean {
    if (event.type != KeyEventType.KeyDown) return false
    return when (event.key) {
        Key.DirectionUp -> focusManager.moveFocus(FocusDirection.Up)
        Key.DirectionDown -> focusManager.moveFocus(FocusDirection.Down)
        Key.DirectionLeft -> {
            val newValue = (value - step).coerceIn(min, max)
            onValueAdjusted(newValue)
            true
        }
        Key.DirectionRight -> {
            val newValue = (value + step).coerceIn(min, max)
            onValueAdjusted(newValue)
            true
        }
        else -> false
    }
}

internal fun isTvActivationKey(key: Key): Boolean =
    key in setOf(
        Key.DirectionCenter,
        Key.Enter,
        Key.NumPadEnter,
    )

internal fun isTvActivateKey(event: androidx.compose.ui.input.key.KeyEvent): Boolean =
    event.type == KeyEventType.KeyUp && isTvActivationKey(event.key)

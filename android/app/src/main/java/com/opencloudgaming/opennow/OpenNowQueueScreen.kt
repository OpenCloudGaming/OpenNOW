package com.opencloudgaming.opennow

import android.os.SystemClock
import android.view.View
import androidx.activity.compose.BackHandler
import androidx.compose.animation.AnimatedVisibility
import androidx.compose.animation.animateColorAsState
import androidx.compose.animation.fadeIn
import androidx.compose.animation.fadeOut
import androidx.compose.animation.scaleIn
import androidx.compose.animation.slideInVertically
import androidx.compose.animation.core.Animatable
import androidx.compose.animation.core.LinearEasing
import androidx.compose.animation.core.MutableTransitionState
import androidx.compose.animation.core.RepeatMode
import androidx.compose.animation.core.animateFloat
import androidx.compose.animation.core.infiniteRepeatable
import androidx.compose.animation.core.rememberInfiniteTransition
import androidx.compose.animation.core.tween
import androidx.compose.foundation.Canvas
import androidx.compose.foundation.background
import androidx.compose.foundation.border
import androidx.compose.foundation.clickable
import androidx.compose.foundation.layout.Arrangement
import androidx.compose.foundation.layout.Box
import androidx.compose.foundation.layout.BoxWithConstraints
import androidx.compose.foundation.layout.Column
import androidx.compose.foundation.layout.PaddingValues
import androidx.compose.foundation.layout.Row
import androidx.compose.foundation.layout.Spacer
import androidx.compose.foundation.layout.aspectRatio
import androidx.compose.foundation.layout.fillMaxSize
import androidx.compose.foundation.layout.fillMaxWidth
import androidx.compose.foundation.layout.height
import androidx.compose.foundation.layout.offset
import androidx.compose.foundation.layout.padding
import androidx.compose.foundation.layout.size
import androidx.compose.foundation.layout.width
import androidx.compose.foundation.layout.widthIn
import androidx.compose.foundation.shape.CircleShape
import androidx.compose.foundation.shape.RoundedCornerShape
import androidx.compose.material3.CircularProgressIndicator
import androidx.compose.material3.IconButton
import androidx.compose.material3.LinearProgressIndicator
import androidx.compose.material3.MaterialTheme
import androidx.compose.material3.OutlinedButton
import androidx.compose.material3.Surface
import androidx.compose.material3.Text
import androidx.compose.material3.TextButton
import androidx.compose.runtime.Composable
import androidx.compose.runtime.LaunchedEffect
import androidx.compose.runtime.State
import androidx.compose.runtime.getValue
import androidx.compose.runtime.mutableStateOf
import androidx.compose.runtime.mutableFloatStateOf
import androidx.compose.runtime.remember
import androidx.compose.runtime.setValue
import androidx.compose.runtime.DisposableEffect
import androidx.compose.ui.Alignment
import androidx.compose.ui.Modifier
import androidx.compose.ui.draw.blur
import androidx.compose.ui.draw.clip
import androidx.compose.ui.draw.clipToBounds
import androidx.compose.ui.geometry.CornerRadius
import androidx.compose.ui.geometry.Offset
import androidx.compose.ui.geometry.Size
import androidx.compose.ui.graphics.Brush
import androidx.compose.ui.graphics.Color
import androidx.compose.ui.graphics.Path
import androidx.compose.ui.graphics.Shadow
import androidx.compose.ui.graphics.graphicsLayer
import androidx.compose.ui.platform.LocalContext
import androidx.compose.ui.platform.LocalDensity
import androidx.compose.ui.res.stringResource
import androidx.compose.ui.semantics.contentDescription
import androidx.compose.ui.semantics.semantics
import androidx.compose.ui.text.TextStyle
import androidx.compose.ui.text.font.FontWeight
import androidx.compose.ui.text.style.TextAlign
import androidx.compose.ui.text.style.TextOverflow
import androidx.compose.ui.unit.dp
import androidx.compose.ui.unit.Dp
import androidx.compose.ui.viewinterop.AndroidView
import androidx.media3.common.MediaItem
import androidx.media3.common.Player
import androidx.media3.exoplayer.ExoPlayer
import androidx.media3.ui.PlayerView
import kotlinx.coroutines.delay
import com.opencloudgaming.opennow.ui.theme.LocalReduceMotion
import com.opencloudgaming.opennow.ui.theme.OpenNowMotion
import kotlin.math.roundToInt
import kotlin.math.sin

@Composable
internal fun QueueLoadingScreen(state: OpenNowUiState, viewModel: OpenNowViewModel) {
    BackHandler(
        enabled = canMinimizeStreamLaunch(
            streamStatus = state.streamStatus,
            sessionReady = state.streamSession?.isReadyForStream() == true,
        ),
        onBack = viewModel::minimizeStreamLaunch,
    )
    val session = state.streamSession
    val game = state.streamGame
    val ads = sessionAdItems(session?.adState)
    val ad = ads.firstOrNull { it.adId == state.queueAdActiveId } ?: ads.firstOrNull()
    val mediaUrl = ad?.adMediaFiles?.firstOrNull { !it.mediaFileUrl.isNullOrBlank() }?.mediaFileUrl
        ?: ad?.adUrl
        ?: ad?.mediaUrl
    val queuePosition = activeQueuePosition(state)
    val visibleQueuePosition = rememberStableQueuePosition(queuePosition)
    val launchStatus = queueLaunchStatus(state, visibleQueuePosition)
    val queueCopy = queueLaunchStatusText(state, visibleQueuePosition)
    val isConnecting = launchStatus.kind == QueueLaunchStatusKind.ConnectingStream
    val connectingProgress = remember(session?.sessionId) { Animatable(0f) }
    val connectingProgressProvider = remember(connectingProgress) { { connectingProgress.value } }
    LaunchedEffect(isConnecting, session?.sessionId) {
        if (isConnecting) {
            connectingProgress.snapTo(0f)
            connectingProgress.animateTo(
                targetValue = QUEUE_CONNECTING_PROGRESS_LIMIT,
                animationSpec = tween(
                    durationMillis = QUEUE_CONNECTING_PROGRESS_DURATION_MS,
                    easing = LinearEasing,
                ),
            )
        } else {
            connectingProgress.snapTo(0f)
        }
    }
    val hasPlayableAd = ad != null && mediaUrl != null
    val reduceMotion = LocalReduceMotion.current
    val entranceState = remember(game?.id) {
        MutableTransitionState(false).apply { targetState = true }
    }

    BoxWithConstraints(
        Modifier
            .fillMaxSize()
            .clipToBounds(),
        contentAlignment = Alignment.Center,
    ) {
        QueueAmbientBackdrop(
            accent = state.settings.uiAccent.color,
            queuePosition = visibleQueuePosition,
        )
        val useLandscapeAdLayout = hasPlayableAd && maxWidth > maxHeight

        Box(
            Modifier
                .fillMaxSize()
                .padding(18.dp),
            contentAlignment = Alignment.Center,
        ) {
            AnimatedVisibility(
                visibleState = entranceState,
                enter = fadeIn(
                    tween(if (reduceMotion) 0 else OpenNowMotion.DurationFast),
                ) + scaleIn(
                    initialScale = if (reduceMotion) 1f else 0.965f,
                    animationSpec = tween(
                        if (reduceMotion) 0 else OpenNowMotion.DurationStandard,
                        easing = OpenNowMotion.EasingStandard,
                    ),
                ) + slideInVertically(
                    initialOffsetY = { if (reduceMotion) 0 else it / 18 },
                    animationSpec = tween(
                        if (reduceMotion) 0 else OpenNowMotion.DurationStandard,
                        easing = OpenNowMotion.EasingStandard,
                    ),
                ),
            ) {
                if (ad != null && mediaUrl != null) {
                    QueueAdPanel(
                        ad = ad,
                        mediaUrl = mediaUrl,
                        viewModel = viewModel,
                        game = game,
                        queueCopy = queueCopy,
                        queuePosition = visibleQueuePosition,
                        connectingProgress = connectingProgressProvider.takeIf { isConnecting },
                        error = state.error,
                        playbackKey = session?.sessionId.orEmpty(),
                        compact = useLandscapeAdLayout,
                        onMinimize = viewModel::minimizeStreamLaunch,
                        onCancel = viewModel::stopStream,
                        modifier = Modifier
                            .fillMaxWidth(if (useLandscapeAdLayout) 0.72f else 1f)
                            .widthIn(max = if (useLandscapeAdLayout) 900.dp else 620.dp),
                    )
                } else {
                    QueueStatusPanel(
                        game = game,
                        queueCopy = queueCopy,
                        queuePosition = visibleQueuePosition,
                        connectingProgress = connectingProgressProvider.takeIf { isConnecting },
                        error = state.error,
                        compact = false,
                        onMinimize = viewModel::minimizeStreamLaunch,
                        onCancel = viewModel::stopStream,
                        modifier = Modifier.fillMaxWidth(),
                    )
                }
            }
        }
    }
}

@Composable
private fun QueueAmbientBackdrop(
    accent: Color,
    queuePosition: Int?,
    modifier: Modifier = Modifier,
) {
    val reduceMotion = LocalReduceMotion.current
    val driftA: State<Float>
    val driftB: State<Float>
    val phase: State<Float>
    val shimmer: State<Float>
    val orbADim: State<Float>
    val orbBDim: State<Float>
    if (reduceMotion) {
        driftA = remember { mutableFloatStateOf(0f) }
        driftB = remember { mutableFloatStateOf(0f) }
        phase = remember { mutableFloatStateOf(0f) }
        shimmer = remember { mutableFloatStateOf(0f) }
        orbADim = remember { mutableFloatStateOf(0.52f) }
        orbBDim = remember { mutableFloatStateOf(0.4f) }
    } else {
        val transition = rememberInfiniteTransition(label = "queue-ambient")
        driftA = transition.animateFloat(
            initialValue = -1f,
            targetValue = 1f,
            animationSpec = infiniteRepeatable(
                animation = tween(durationMillis = 11000, easing = LinearEasing),
                repeatMode = RepeatMode.Reverse,
            ),
            label = "queue-ambient-drift-a",
        )
        driftB = transition.animateFloat(
            initialValue = 1f,
            targetValue = -1f,
            animationSpec = infiniteRepeatable(
                animation = tween(durationMillis = 14000, easing = LinearEasing),
                repeatMode = RepeatMode.Reverse,
            ),
            label = "queue-ambient-drift-b",
        )
        phase = transition.animateFloat(
            initialValue = 0f,
            targetValue = 1f,
            animationSpec = infiniteRepeatable(
                animation = tween(durationMillis = 16000, easing = LinearEasing),
            ),
            label = "queue-ambient-phase",
        )
        shimmer = transition.animateFloat(
            initialValue = 0f,
            targetValue = 1f,
            animationSpec = infiniteRepeatable(
                animation = tween(durationMillis = 5200, easing = LinearEasing),
            ),
            label = "queue-ambient-shimmer",
        )
        orbADim = transition.animateFloat(
            initialValue = 0.35f,
            targetValue = 0.72f,
            animationSpec = infiniteRepeatable(
                animation = tween(durationMillis = 8200, easing = LinearEasing),
                repeatMode = RepeatMode.Reverse,
            ),
            label = "queue-ambient-orb-a-dim",
        )
        orbBDim = transition.animateFloat(
            initialValue = 0.26f,
            targetValue = 0.56f,
            animationSpec = infiniteRepeatable(
                animation = tween(durationMillis = 9800, easing = LinearEasing),
                repeatMode = RepeatMode.Reverse,
            ),
            label = "queue-ambient-orb-b-dim",
        )
    }

    BoxWithConstraints(
        modifier
            .fillMaxSize()
            .background(
                Brush.verticalGradient(
                    listOf(
                        Color(0xff010203),
                        Color(0xff05080a),
                        Color(0xff020304),
                    ),
                ),
            ),
    ) {
        val baseSize = minOf(maxWidth, maxHeight)
        val density = LocalDensity.current
        val widthPx = with(density) { maxWidth.toPx() }
        val heightPx = with(density) { maxHeight.toPx() }
        QueueAmbientOrb(
            color = accent,
            size = baseSize * 0.92f,
            modifier = Modifier
                .align(Alignment.TopStart)
                .graphicsLayer {
                    translationX = widthPx * (-0.22f + 0.10f * driftA.value)
                    translationY = heightPx * (0.02f + 0.08f * driftB.value)
                    alpha = orbADim.value.coerceIn(0f, 1f)
                },
        )
        QueueAmbientOrb(
            color = Color(0xff2bdcff),
            size = baseSize * 0.7f,
            modifier = Modifier
                .align(Alignment.BottomEnd)
                .graphicsLayer {
                    translationX = widthPx * (0.15f + 0.08f * driftB.value)
                    translationY = heightPx * (0.10f + 0.07f * driftA.value)
                    alpha = orbBDim.value.coerceIn(0f, 1f)
                },
        )
        QueueSignalField(
            accent = accent,
            queuePosition = queuePosition,
            phase = phase,
            shimmer = shimmer,
            modifier = Modifier.matchParentSize(),
        )
        Box(
            Modifier
                .matchParentSize()
                .background(Color.Black.copy(alpha = 0.34f)),
        )
    }
}

@Composable
private fun QueueAmbientOrb(
    color: Color,
    size: Dp,
    modifier: Modifier = Modifier,
) {
    Box(
        modifier
            .size(size)
            .blur(64.dp)
            .background(
                Brush.radialGradient(
                    listOf(
                        color.copy(alpha = 0.58f),
                        color.copy(alpha = 0.16f),
                        Color.Transparent,
                    ),
                ),
                CircleShape,
            ),
    )
}

@Composable
private fun QueueSignalField(
    accent: Color,
    queuePosition: Int?,
    phase: State<Float>,
    shimmer: State<Float>,
    modifier: Modifier = Modifier,
) {
    val heat = queueUrgency(queuePosition)
    Canvas(modifier) {
        val phaseValue = phase.value
        val shimmerValue = shimmer.value
        val lineCount = 9
        val spacing = size.height / lineCount
        val offset = shimmerValue * spacing
        for (index in -1..lineCount) {
            val y = index * spacing + offset
            drawLine(
                color = accent.copy(alpha = 0.035f + heat * 0.035f),
                start = Offset(-size.width * 0.12f, y),
                end = Offset(size.width * 1.08f, y - size.height * 0.10f),
                strokeWidth = 1.dp.toPx(),
            )
        }
        repeat(12) { index ->
            val lane = index + 1
            val x = ((lane * 0.173f + phaseValue * (0.08f + lane * 0.006f)) % 1f) * size.width
            val y = ((lane * 0.291f + shimmerValue * (0.12f + lane * 0.004f)) % 1f) * size.height
            drawCircle(
                color = accent.copy(alpha = 0.05f + heat * 0.04f),
                radius = (1.5f + (index % 4)) * density,
                center = Offset(x, y),
            )
        }
    }
}

@Composable
private fun AnimatedQueueStatusText(
    queueCopy: String,
    queuePosition: Int?,
    compact: Boolean,
    modifier: Modifier = Modifier,
) {
    if (queuePosition == null) {
        Text(
            queueCopy,
            modifier = modifier,
            color = queueIdleStatusColor(queueCopy),
            style = (if (compact) MaterialTheme.typography.bodyLarge else MaterialTheme.typography.titleMedium)
                .copy(fontWeight = FontWeight.Normal),
            textAlign = TextAlign.Center,
        )
        return
    }

    var previousQueuePosition by remember { mutableStateOf<Int?>(null) }
    val numberProgress = remember { Animatable(1f) }
    var numberTrigger by remember { mutableStateOf(0) }
    var numberFrom by remember { mutableStateOf(queuePosition.toString()) }
    var numberTo by remember { mutableStateOf(queuePosition.toString()) }
    val heat = queueUrgency(queuePosition)
    val hotQueue = queuePosition < 10
    val reduceMotion = LocalReduceMotion.current
    val glow: State<Float>
    val moleculePhase: State<Float>
    if (reduceMotion) {
        glow = remember { mutableFloatStateOf(0.72f) }
        moleculePhase = remember { mutableFloatStateOf(0f) }
    } else {
        val transition = rememberInfiniteTransition(label = "queue-status-glow")
        glow = transition.animateFloat(
            initialValue = 0.55f,
            targetValue = 1f,
            animationSpec = infiniteRepeatable(
                animation = tween(durationMillis = if (hotQueue) 520 else 1100, easing = LinearEasing),
                repeatMode = RepeatMode.Reverse,
            ),
            label = "queue-status-glow-alpha",
        )
        moleculePhase = transition.animateFloat(
            initialValue = 0f,
            targetValue = 1f,
            animationSpec = infiniteRepeatable(
                animation = tween(
                    durationMillis = (190 - heat * 95).roundToInt().coerceIn(92, 190),
                    easing = LinearEasing,
                ),
            ),
            label = "queue-status-molecule-phase",
        )
    }
    val statusColor by animateColorAsState(
        targetValue = queueUrgencyColor(queuePosition),
        animationSpec = tween(durationMillis = 240),
        label = "queue-status-color",
    )

    LaunchedEffect(queuePosition) {
        val current = queuePosition
        val previous = previousQueuePosition
        if (previous != null && current < previous) {
            numberFrom = previous.toString()
            numberTo = current.toString()
            numberTrigger += 1
        } else {
            numberFrom = current.toString()
            numberTo = current.toString()
        }
        previousQueuePosition = current
    }

    LaunchedEffect(numberTrigger) {
        if (numberTrigger == 0) return@LaunchedEffect
        numberProgress.snapTo(0f)
        numberProgress.animateTo(
            targetValue = 1f,
            animationSpec = tween(durationMillis = if (hotQueue) 320 else 420),
        )
    }

    val moleculeCagePx = with(LocalDensity.current) {
        (if (hotQueue) (0.45f + heat * 1.45f).dp else 0.dp).toPx()
    }
    val parts = queueStatusParts(queueCopy, queuePosition)
    val textStyle = (if (compact) MaterialTheme.typography.bodyLarge else MaterialTheme.typography.titleMedium)
        .copy(fontWeight = FontWeight.Normal)
    val numberAnimating = numberTrigger > 0 && numberFrom != numberTo
    val numberTravelPx = with(LocalDensity.current) { (if (compact) 18.dp else 22.dp).toPx() }

    Row(
        modifier = modifier,
        horizontalArrangement = Arrangement.Center,
        verticalAlignment = Alignment.CenterVertically,
    ) {
        Text(
            parts.prefix,
            color = TextMuted,
            style = textStyle,
            textAlign = TextAlign.Center,
        )
        AnimatedQueueNumber(
            currentNumber = parts.number,
            previousNumber = numberFrom,
            targetNumber = numberTo,
            animating = numberAnimating,
            phaseProvider = { numberProgress.value },
            travelPx = numberTravelPx,
            color = statusColor,
            style = textStyle.copy(
                shadow = Shadow(
                    color = statusColor.copy(alpha = heat * 0.68f),
                    offset = Offset(0f, 0f),
                    blurRadius = 18f + heat * 14f,
                ),
            ),
            glow = glow,
            moleculePhase = moleculePhase,
            moleculeCagePx = moleculeCagePx,
            heat = heat,
        )
        Text(
            parts.suffix,
            color = TextMuted,
            style = textStyle,
            textAlign = TextAlign.Center,
        )
    }
}

@Composable
private fun AnimatedQueueNumber(
    currentNumber: String,
    previousNumber: String,
    targetNumber: String,
    animating: Boolean,
    phaseProvider: () -> Float,
    travelPx: Float,
    color: Color,
    style: TextStyle,
    glow: State<Float>,
    moleculePhase: State<Float>,
    moleculeCagePx: Float,
    heat: Float,
) {
    val fromNumber = if (animating) previousNumber else currentNumber
    val toNumber = if (animating) targetNumber else currentNumber
    val slotCount = toNumber.length

    Row(
        modifier = Modifier
            .clipToBounds()
            .graphicsLayer {
                val phase = moleculePhase.value
                translationX = if (moleculeCagePx > 0f) {
                    (sin(phase * 31.415928f) * 0.64f + sin(phase * 106.81416f) * 0.36f) * moleculeCagePx
                } else {
                    0f
                }
                translationY = if (moleculeCagePx > 0f) {
                    (sin(phase * 43.982296f) * 0.55f + sin(phase * 81.68141f) * 0.45f) * moleculeCagePx * 0.55f
                } else {
                    0f
                }
                val pulse = 1f + heat * 0.012f * glow.value
                scaleX = pulse
                scaleY = pulse
            },
        verticalAlignment = Alignment.CenterVertically,
    ) {
        repeat(slotCount) { slotIndex ->
            val fromDigit = fromNumber.rightAlignedCharAt(slotIndex, slotCount)
            val toDigit = toNumber.rightAlignedCharAt(slotIndex, slotCount)
            QueueNumberDigitSlot(
                fromDigit = fromDigit,
                toDigit = toDigit,
                digitChanged = animating && fromDigit != toDigit,
                phaseProvider = phaseProvider,
                travelPx = travelPx,
                color = color,
                style = style,
            )
        }
    }
}

@Composable
private fun QueueNumberDigitSlot(
    fromDigit: Char?,
    toDigit: Char?,
    digitChanged: Boolean,
    phaseProvider: () -> Float,
    travelPx: Float,
    color: Color,
    style: TextStyle,
) {
    val from = fromDigit?.toString().orEmpty()
    val to = toDigit?.toString().orEmpty()
    Box(
        modifier = Modifier.clipToBounds(),
        contentAlignment = Alignment.Center,
    ) {
        if (from.isNotEmpty()) {
            Text(
                from,
                modifier = Modifier.graphicsLayer(alpha = 0f),
                color = color,
                style = style,
                textAlign = TextAlign.Center,
            )
        }
        if (to.isNotEmpty() && to != from) {
            Text(
                to,
                modifier = Modifier.graphicsLayer(alpha = 0f),
                color = color,
                style = style,
                textAlign = TextAlign.Center,
            )
        }
        if (digitChanged) {
            if (from.isNotEmpty()) {
                Text(
                    from,
                    modifier = Modifier.graphicsLayer {
                        val phase = phaseProvider()
                        translationY = -travelPx * phase
                        scaleX = 1f - phase * 0.03f
                        scaleY = 1f - phase * 0.03f
                        alpha = 1f - phase
                    },
                    color = color,
                    style = style,
                    textAlign = TextAlign.Center,
                )
            }
            if (to.isNotEmpty()) {
                Text(
                    to,
                    modifier = Modifier.graphicsLayer {
                        val phase = phaseProvider()
                        translationY = travelPx * (1f - phase)
                        scaleX = 0.97f + phase * 0.03f
                        scaleY = 0.97f + phase * 0.03f
                        alpha = phase
                    },
                    color = color,
                    style = style,
                    textAlign = TextAlign.Center,
                )
            }
        } else if (to.isNotEmpty()) {
            Text(
                to,
                color = color,
                style = style,
                textAlign = TextAlign.Center,
            )
        }
    }
}

private fun String.rightAlignedCharAt(slotIndex: Int, slotCount: Int): Char? =
    getOrNull(length - slotCount + slotIndex)

private data class QueueStatusParts(
    val prefix: String,
    val number: String,
    val suffix: String,
)

private fun queueStatusParts(queueCopy: String, queuePosition: Int): QueueStatusParts {
    val number = queuePosition.toString()
    val index = queueCopy.indexOf(number)
    if (index < 0) {
        return QueueStatusParts(prefix = "$queueCopy ", number = number, suffix = "")
    }
    return QueueStatusParts(
        prefix = queueCopy.substring(0, index),
        number = number,
        suffix = queueCopy.substring(index + number.length),
    )
}

private fun queueUrgency(queuePosition: Int?): Float {
    val position = queuePosition ?: return 0f
    if (position >= 10) return 0f
    return ((10 - position).toFloat() / 9f).coerceIn(0f, 1f)
}

private fun activeQueuePosition(state: OpenNowUiState): Int? =
    queueDisplayPosition(state)

@Composable
private fun rememberStableQueuePosition(queuePosition: Int?): Int? {
    var stableQueuePosition by remember { mutableStateOf(queuePosition) }
    LaunchedEffect(queuePosition) {
        if (queuePosition == stableQueuePosition) return@LaunchedEffect
        if (queuePosition == null || stableQueuePosition == null) {
            stableQueuePosition = queuePosition
            return@LaunchedEffect
        }
        delay(QUEUE_POSITION_VISUAL_SETTLE_MS)
        stableQueuePosition = queuePosition
    }
    return stableQueuePosition
}

@Composable
private fun queueLaunchStatusText(state: OpenNowUiState, queuePosition: Int?): String {
    val status = queueLaunchStatus(state, queuePosition)
    return when (status.kind) {
        QueueLaunchStatusKind.QueuePosition -> stringResource(R.string.queue_position, requireNotNull(status.queuePosition))
        QueueLaunchStatusKind.WaitingForRig -> stringResource(R.string.queue_waiting_for_rig)
        QueueLaunchStatusKind.ConnectingStream -> stringResource(R.string.queue_connecting_stream)
        QueueLaunchStatusKind.ResumingSession -> stringResource(R.string.queue_resuming_session)
        QueueLaunchStatusKind.SettingUpRig -> stringResource(R.string.queue_setting_up_rig)
        QueueLaunchStatusKind.StartingSession -> stringResource(R.string.queue_starting_session)
    }
}

@Composable
private fun QueueProgressIndicator(
    connectingProgress: (() -> Float)?,
    modifier: Modifier = Modifier,
) {
    if (connectingProgress == null) {
        LinearProgressIndicator(modifier)
    } else {
        LinearProgressIndicator(
            progress = connectingProgress,
            modifier = modifier,
        )
    }
}

@Composable
private fun queueIdleStatusColor(queueCopy: String): Color =
    if (queueCopy == stringResource(R.string.queue_starting_session)) Green else TextMuted

private fun queueUrgencyColor(queuePosition: Int?): Color {
    val heat = queueUrgency(queuePosition)
    if (heat <= 0f) return TextMuted
    val green = (0.57f - 0.49f * heat).coerceIn(0.06f, 0.57f)
    val blue = (0.25f - 0.17f * heat).coerceIn(0.08f, 0.25f)
    return Color(red = 1f, green = green, blue = blue, alpha = 1f)
}

@Composable
private fun QueueStatusPanel(
    game: GameInfo?,
    queueCopy: String,
    queuePosition: Int?,
    connectingProgress: (() -> Float)?,
    error: String?,
    compact: Boolean,
    onMinimize: () -> Unit,
    onCancel: () -> Unit,
    modifier: Modifier = Modifier,
) {
    val context = LocalContext.current
    Column(
        modifier,
        horizontalAlignment = Alignment.CenterHorizontally,
        verticalArrangement = Arrangement.Center,
    ) {
        val imageWidth = if (compact) 154.dp else 220.dp
        UrlImage(
            gameTvBannerImageUrl(context, game),
            Modifier
                .width(imageWidth)
                .aspectRatio(16f / 9f)
                .clip(RoundedCornerShape(14.dp)),
        )
        Spacer(Modifier.height(if (compact) 12.dp else 16.dp))
        Text(
            game?.title ?: stringResource(R.string.queue_starting_stream),
            color = TextPrimary,
            style = if (compact) MaterialTheme.typography.titleLarge else MaterialTheme.typography.headlineSmall,
            fontWeight = FontWeight.Bold,
            maxLines = 2,
            overflow = TextOverflow.Ellipsis,
            textAlign = TextAlign.Center,
        )
        AnimatedQueueStatusText(
            queueCopy = queueCopy,
            queuePosition = queuePosition,
            compact = compact,
        )
        Spacer(Modifier.height(if (compact) 14.dp else 18.dp))
        QueueProgressIndicator(
            connectingProgress = connectingProgress,
            modifier = Modifier.fillMaxWidth(if (compact) 0.9f else 0.7f),
        )
        Spacer(Modifier.height(12.dp))
        Row(
            Modifier.fillMaxWidth(if (compact) 0.92f else 0.7f),
            horizontalArrangement = Arrangement.spacedBy(10.dp),
        ) {
            OutlinedButton(onClick = onMinimize, modifier = Modifier.weight(1f)) {
                Text(stringResource(R.string.action_minimize), maxLines = 1, overflow = TextOverflow.Ellipsis)
            }
            OutlinedButton(onClick = onCancel, modifier = Modifier.weight(1f)) {
                Text(stringResource(R.string.action_cancel), maxLines = 1, overflow = TextOverflow.Ellipsis)
            }
        }
        if (compact && queuePosition != null) {
            Spacer(Modifier.height(14.dp))
            LandscapeQueuePositionDock(queuePosition = queuePosition)
        }
        error?.let {
            Spacer(Modifier.height(12.dp))
            Text(it, color = Color(0xffff9f9f), textAlign = TextAlign.Center)
        }
    }
}

@Composable
private fun LandscapeQueuePositionDock(queuePosition: Int, modifier: Modifier = Modifier) {
    val accent = queueUrgencyColor(queuePosition)
    val heat = queueUrgency(queuePosition)
    val shape = RoundedCornerShape(16.dp)
    Box(
        modifier
            .fillMaxWidth(0.92f)
            .clip(shape)
            .background(
                Brush.horizontalGradient(
                    listOf(
                        accent.copy(alpha = 0.18f + heat * 0.16f),
                        PanelAlt.copy(alpha = 0.94f),
                        Color.Black.copy(alpha = 0.36f),
                    ),
                ),
            )
            .border(1.dp, accent.copy(alpha = 0.32f + heat * 0.36f), shape)
            .padding(horizontal = 16.dp, vertical = 12.dp),
    ) {
        Row(
            Modifier.fillMaxWidth(),
            verticalAlignment = Alignment.Bottom,
            horizontalArrangement = Arrangement.spacedBy(12.dp),
        ) {
            Column(Modifier.weight(1f), verticalArrangement = Arrangement.spacedBy(2.dp)) {
                Text(
                    stringResource(R.string.queue_title),
                    color = TextMuted,
                    style = MaterialTheme.typography.labelLarge,
                    fontWeight = FontWeight.Bold,
                    maxLines = 1,
                    overflow = TextOverflow.Ellipsis,
                )
                Text(
                    stringResource(R.string.queue_live_position),
                    color = TextMuted.copy(alpha = 0.78f),
                    style = MaterialTheme.typography.labelSmall,
                    maxLines = 1,
                    overflow = TextOverflow.Ellipsis,
                )
            }
            Text(
                queuePosition.toString(),
                color = accent,
                style = MaterialTheme.typography.displaySmall.copy(
                    fontWeight = FontWeight.Black,
                    shadow = Shadow(
                        color = accent.copy(alpha = 0.24f + heat * 0.42f),
                        offset = Offset(0f, 0f),
                        blurRadius = 18f + heat * 14f,
                    ),
                ),
                maxLines = 1,
                textAlign = TextAlign.End,
            )
        }
    }
}

@Composable
private fun QueueAdPanel(
    ad: SessionAdInfo,
    mediaUrl: String,
    viewModel: OpenNowViewModel,
    game: GameInfo?,
    queueCopy: String,
    queuePosition: Int?,
    connectingProgress: (() -> Float)?,
    error: String?,
    playbackKey: String,
    compact: Boolean,
    onMinimize: () -> Unit,
    onCancel: () -> Unit,
    modifier: Modifier = Modifier,
) {
    Surface(
        modifier = modifier,
        shape = RoundedCornerShape(18.dp),
        color = Panel.copy(alpha = 0.95f),
        tonalElevation = 8.dp,
    ) {
        if (compact) {
            Row(
                Modifier.padding(14.dp),
                horizontalArrangement = Arrangement.spacedBy(14.dp),
                verticalAlignment = Alignment.CenterVertically,
            ) {
                QueueAdPlayback(
                    ad = ad,
                    mediaUrl = mediaUrl,
                    playbackKey = playbackKey,
                    viewModel = viewModel,
                    modifier = Modifier
                        .weight(1.55f)
                        .aspectRatio(16f / 9f),
                )
                Column(
                    Modifier.weight(1f),
                    verticalArrangement = Arrangement.spacedBy(10.dp),
                ) {
                    QueueAdHeading(game = game, compact = true)
                    QueueStatusAndActions(
                        queueCopy = queueCopy,
                        queuePosition = queuePosition,
                        connectingProgress = connectingProgress,
                        compact = true,
                        stackActions = true,
                        onMinimize = onMinimize,
                        onCancel = onCancel,
                    )
                    error?.let {
                        Text(it, color = Color(0xffff9f9f), textAlign = TextAlign.Center)
                    }
                }
            }
        } else {
            Column(
                Modifier.padding(16.dp),
                horizontalAlignment = Alignment.CenterHorizontally,
                verticalArrangement = Arrangement.spacedBy(12.dp),
            ) {
                QueueAdHeading(game = game, compact = false)
                QueueAdPlayback(
                    ad = ad,
                    mediaUrl = mediaUrl,
                    playbackKey = playbackKey,
                    viewModel = viewModel,
                    modifier = Modifier
                        .fillMaxWidth()
                        .height(220.dp),
                )
                QueueStatusAndActions(
                    queueCopy = queueCopy,
                    queuePosition = queuePosition,
                    connectingProgress = connectingProgress,
                    compact = false,
                    stackActions = false,
                    onMinimize = onMinimize,
                    onCancel = onCancel,
                )
                error?.let {
                    Text(it, color = Color(0xffff9f9f), textAlign = TextAlign.Center)
                }
            }
        }
    }
}

@Composable
private fun QueueAdPlayback(
    ad: SessionAdInfo,
    mediaUrl: String,
    playbackKey: String,
    viewModel: OpenNowViewModel,
    modifier: Modifier = Modifier,
) {
    QueueAdPlayer(
        adId = ad.adId,
        url = mediaUrl,
        playbackKey = playbackKey,
        modifier = modifier,
        onStarted = { viewModel.reportQueueAd(ad.adId, "start") },
        onPaused = { viewModel.reportQueueAd(ad.adId, "pause") },
        onResumed = { viewModel.reportQueueAd(ad.adId, "resume") },
        onFinished = { watchedTimeInMs ->
            viewModel.reportQueueAd(ad.adId, "finish", watchedTimeInMs = watchedTimeInMs)
        },
        onError = { watchedTimeInMs, errorInfo ->
            viewModel.reportQueueAd(
                ad.adId,
                "cancel",
                watchedTimeInMs = watchedTimeInMs,
                cancelReason = "error",
                errorInfo = errorInfo,
            )
        },
    )
}

@Composable
private fun QueueAdHeading(game: GameInfo?, compact: Boolean) {
    Column(Modifier.fillMaxWidth()) {
        Text(
            stringResource(R.string.queue_advertisement),
            color = TextMuted,
            style = MaterialTheme.typography.labelMedium,
            fontWeight = FontWeight.Bold,
            maxLines = 1,
        )
        Text(
            game?.title ?: stringResource(R.string.queue_starting_stream),
            color = TextPrimary,
            style = if (compact) MaterialTheme.typography.titleMedium else MaterialTheme.typography.titleLarge,
            fontWeight = FontWeight.Bold,
            maxLines = 1,
            overflow = TextOverflow.Ellipsis,
        )
    }
}

@Composable
private fun QueueStatusAndActions(
    queueCopy: String,
    queuePosition: Int?,
    connectingProgress: (() -> Float)?,
    compact: Boolean,
    stackActions: Boolean,
    onMinimize: () -> Unit,
    onCancel: () -> Unit,
) {
    Column(
        Modifier.fillMaxWidth(if (compact) 1f else 0.7f),
        horizontalAlignment = Alignment.CenterHorizontally,
        verticalArrangement = Arrangement.spacedBy(if (compact) 8.dp else 10.dp),
    ) {
        AnimatedQueueStatusText(
            queueCopy = queueCopy,
            queuePosition = queuePosition,
            compact = compact,
        )
        QueueProgressIndicator(
            connectingProgress = connectingProgress,
            modifier = Modifier.fillMaxWidth(),
        )
        if (stackActions) {
            OutlinedButton(onClick = onMinimize, modifier = Modifier.fillMaxWidth()) {
                Text(stringResource(R.string.action_minimize), maxLines = 1, overflow = TextOverflow.Ellipsis)
            }
            OutlinedButton(onClick = onCancel, modifier = Modifier.fillMaxWidth()) {
                Text(stringResource(R.string.action_cancel), maxLines = 1, overflow = TextOverflow.Ellipsis)
            }
        } else {
            Row(
                Modifier.fillMaxWidth(),
                horizontalArrangement = Arrangement.spacedBy(10.dp),
            ) {
                OutlinedButton(onClick = onMinimize, modifier = Modifier.weight(1f)) {
                    Text(stringResource(R.string.action_minimize), maxLines = 1, overflow = TextOverflow.Ellipsis)
                }
                OutlinedButton(onClick = onCancel, modifier = Modifier.weight(1f)) {
                    Text(stringResource(R.string.action_cancel), maxLines = 1, overflow = TextOverflow.Ellipsis)
                }
            }
        }
    }
}

@Composable
internal fun MinimizedQueueDock(
    state: OpenNowUiState,
    onRestore: () -> Unit,
    onCancel: () -> Unit,
    modifier: Modifier = Modifier,
) {
    val queuePosition = activeQueuePosition(state)
    val visibleQueuePosition = rememberStableQueuePosition(queuePosition)
    val queueCopy = queueLaunchStatusText(state, visibleQueuePosition)
    Surface(
        modifier = modifier
            .fillMaxWidth(),
        shape = RoundedCornerShape(topStart = 18.dp, topEnd = 18.dp),
        color = Panel.copy(alpha = 0.98f),
        tonalElevation = 0.dp,
        shadowElevation = 0.dp,
    ) {
        Row(
            Modifier.padding(horizontal = 14.dp, vertical = 10.dp),
            verticalAlignment = Alignment.CenterVertically,
            horizontalArrangement = Arrangement.spacedBy(12.dp),
        ) {
            CircularProgressIndicator(Modifier.size(24.dp), strokeWidth = 2.dp, color = MaterialTheme.colorScheme.primary)
            Column(Modifier.weight(1f)) {
                Text(
                    state.streamGame?.title ?: stringResource(R.string.queue_starting_stream),
                    color = TextPrimary,
                    fontWeight = FontWeight.Bold,
                    maxLines = 1,
                    overflow = TextOverflow.Ellipsis,
                )
                MinimizedQueueStatusText(
                    queueCopy = queueCopy,
                    queuePosition = visibleQueuePosition,
                )
            }
            TextButton(onClick = onRestore) { Text(stringResource(R.string.action_view)) }
            OutlinedButton(onClick = onCancel, contentPadding = PaddingValues(horizontal = 10.dp, vertical = 6.dp)) {
                Text(stringResource(R.string.action_cancel))
            }
        }
    }
}

@Composable
private fun MinimizedQueueStatusText(
    queueCopy: String,
    queuePosition: Int?,
) {
    if (queuePosition == null) {
        Text(queueCopy, color = queueIdleStatusColor(queueCopy), style = MaterialTheme.typography.bodySmall)
        return
    }
    val parts = queueStatusParts(queueCopy, queuePosition)
    Row(verticalAlignment = Alignment.CenterVertically) {
        Text(parts.prefix, color = TextMuted, style = MaterialTheme.typography.bodySmall)
        Text(
            parts.number,
            color = queueUrgencyColor(queuePosition),
            style = MaterialTheme.typography.bodySmall,
        )
        Text(parts.suffix, color = TextMuted, style = MaterialTheme.typography.bodySmall)
    }
}

private const val QUEUE_AD_FORCE_PLAY_TIMEOUT_MS = 10_000L
private const val QUEUE_CONNECTING_PROGRESS_DURATION_MS = 15_000
private const val QUEUE_CONNECTING_PROGRESS_LIMIT = 0.9f
private const val QUEUE_AD_START_TIMEOUT_MS = 30_000L
private const val QUEUE_AD_STUCK_TIMEOUT_MS = 30_000L
private const val QUEUE_AD_PROGRESS_CHECK_INTERVAL_MS = 1_000L

@Composable
private fun QueueAdPlayer(
    adId: String,
    url: String,
    playbackKey: String,
    modifier: Modifier = Modifier,
    onStarted: () -> Unit,
    onPaused: () -> Unit,
    onResumed: () -> Unit,
    onFinished: (watchedTimeInMs: Long) -> Unit,
    onError: (watchedTimeInMs: Long, errorInfo: String) -> Unit,
) {
    val context = LocalContext.current
    var muted by remember { mutableStateOf(false) }
    val player = remember(adId, url, playbackKey) {
        ExoPlayer.Builder(context).build().apply {
            setMediaItem(MediaItem.fromUri(url))
            volume = if (muted) 0f else 1f
            prepare()
            playWhenReady = true
        }
    }
    var reportedStart by remember(adId, url, playbackKey) { mutableStateOf(false) }
    var reportedFinish by remember(adId, url, playbackKey) { mutableStateOf(false) }
    var reportedPause by remember(adId, url, playbackKey) { mutableStateOf(false) }
    var playing by remember(adId, url, playbackKey) { mutableStateOf(player.playWhenReady) }
    var controlsVisible by remember(adId, url, playbackKey) { mutableStateOf(false) }
    LaunchedEffect(player) {
        val loadStartedAtMs = SystemClock.elapsedRealtime()
        var forcePlayAttempted = false
        var playbackObserved = false
        var lastPositionMs = player.currentPosition.coerceAtLeast(0L)
        var lastProgressAtMs = loadStartedAtMs
        while (!reportedFinish) {
            delay(QUEUE_AD_PROGRESS_CHECK_INTERVAL_MS)
            val nowMs = SystemClock.elapsedRealtime()
            if (!reportedStart) {
                if (!forcePlayAttempted && nowMs - loadStartedAtMs >= QUEUE_AD_FORCE_PLAY_TIMEOUT_MS) {
                    forcePlayAttempted = true
                    player.play()
                }
                if (nowMs - loadStartedAtMs >= QUEUE_AD_START_TIMEOUT_MS) {
                    reportedFinish = true
                    onError(player.currentPosition.coerceAtLeast(0L), "Ad play timeout")
                    break
                }
                continue
            }

            if (!playbackObserved) {
                playbackObserved = true
                lastPositionMs = player.currentPosition.coerceAtLeast(0L)
                lastProgressAtMs = nowMs
            }
            if (!player.playWhenReady || player.playbackState == Player.STATE_ENDED) {
                lastPositionMs = player.currentPosition.coerceAtLeast(0L)
                lastProgressAtMs = nowMs
                continue
            }

            val positionMs = player.currentPosition.coerceAtLeast(0L)
            if (positionMs > lastPositionMs) {
                lastPositionMs = positionMs
                lastProgressAtMs = nowMs
            } else if (nowMs - lastProgressAtMs >= QUEUE_AD_STUCK_TIMEOUT_MS) {
                reportedFinish = true
                onError(positionMs, "Ad video is stuck")
                break
            }
        }
    }
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
                if (isPlaying && !reportedStart && !reportedFinish) {
                    reportedStart = true
                    onStarted()
                }
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
                if (playbackState == Player.STATE_ENDED && !reportedFinish) {
                    reportedFinish = true
                    onFinished(player.currentPosition.coerceAtLeast(0L))
                }
            }

            override fun onPlayerError(error: androidx.media3.common.PlaybackException) {
                if (!reportedFinish) {
                    reportedFinish = true
                    onError(player.currentPosition.coerceAtLeast(0L), "Error loading url")
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

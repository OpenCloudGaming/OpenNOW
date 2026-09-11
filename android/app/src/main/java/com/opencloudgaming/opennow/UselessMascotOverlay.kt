package com.opencloudgaming.opennow

import android.os.SystemClock
import androidx.compose.animation.core.Animatable
import androidx.compose.animation.core.FastOutSlowInEasing
import androidx.compose.animation.core.tween
import androidx.compose.foundation.Image
import androidx.compose.foundation.background
import androidx.compose.foundation.border
import androidx.compose.foundation.layout.Box
import androidx.compose.foundation.layout.BoxWithConstraints
import androidx.compose.foundation.layout.WindowInsets
import androidx.compose.foundation.layout.fillMaxSize
import androidx.compose.foundation.layout.padding
import androidx.compose.foundation.layout.safeDrawing
import androidx.compose.foundation.layout.size
import androidx.compose.foundation.layout.widthIn
import androidx.compose.foundation.layout.windowInsetsPadding
import androidx.compose.foundation.shape.RoundedCornerShape
import androidx.compose.material3.Text
import androidx.compose.runtime.Composable
import androidx.compose.runtime.DisposableEffect
import androidx.compose.runtime.LaunchedEffect
import androidx.compose.runtime.getValue
import androidx.compose.runtime.mutableFloatStateOf
import androidx.compose.runtime.mutableIntStateOf
import androidx.compose.runtime.mutableStateOf
import androidx.compose.runtime.remember
import androidx.compose.runtime.setValue
import androidx.compose.runtime.snapshotFlow
import androidx.compose.runtime.withFrameNanos
import androidx.compose.ui.Modifier
import androidx.compose.ui.draw.clipToBounds
import androidx.compose.ui.draw.drawBehind
import androidx.compose.ui.draw.shadow
import androidx.compose.ui.geometry.Offset
import androidx.compose.ui.graphics.Color
import androidx.compose.ui.graphics.Path
import androidx.compose.ui.graphics.graphicsLayer
import androidx.compose.ui.layout.ContentScale
import androidx.compose.ui.layout.onSizeChanged
import androidx.compose.ui.platform.LocalDensity
import androidx.compose.ui.res.painterResource
import androidx.compose.ui.res.stringResource
import androidx.compose.ui.semantics.clearAndSetSemantics
import androidx.compose.ui.text.font.FontWeight
import androidx.compose.ui.text.style.TextAlign
import androidx.compose.ui.unit.IntSize
import androidx.compose.ui.unit.dp
import androidx.compose.ui.unit.sp
import kotlinx.coroutines.delay
import kotlinx.coroutines.flow.first
import kotlinx.coroutines.isActive
import kotlin.math.PI
import kotlin.math.cos
import kotlin.random.Random

@Composable
internal fun UselessMascotOverlay(settings: AppSettings, allowed: Boolean) {
    val activity = LocalMascotActivity.current ?: return
    val overlayInstance = remember { Any() }
    DisposableEffect(activity, overlayInstance) {
        activity.attachOverlay(overlayInstance)
        onDispose { activity.detachOverlay(overlayInstance) }
    }
    if (activity.overlayOwner !== overlayInstance) return
    val enabled = settings.uselessMascotEnabled && allowed
    val foreground = activity.resumed && activity.focused
    val delayMillis = normalizeMascotDelaySeconds(settings.uselessMascotDelaySeconds) * 1_000L
    DisposableEffect(activity, enabled) {
        activity.enabled = enabled
        activity.reset()
        onDispose {
            activity.enabled = false
            activity.reset()
        }
    }
    LaunchedEffect(activity, enabled, foreground, delayMillis) {
        activity.recordInput()
        if (!enabled || !foreground) return@LaunchedEffect
        while (isActive) {
            val remaining = delayMillis - (SystemClock.uptimeMillis() - activity.lastInputMillis)
            if (remaining > 0L || activity.inputHeld) {
                delay(remaining.coerceAtLeast(100L))
            } else {
                activity.visible = true
                // Sleep until an actual input dismisses it; no polling or idle animation loop.
                snapshotFlow { activity.visible }.first { !it }
            }
        }
    }
    if (enabled && foreground && activity.visible) BouncingMascot()
}

@Composable
private fun BouncingMascot() {
    val messages = listOf(
        stringResource(R.string.mascot_bugs),
        stringResource(R.string.mascot_logs),
        stringResource(R.string.mascot_discord),
        stringResource(R.string.mascot_zortos),
        stringResource(R.string.mascot_hobbits),
    )
    val density = LocalDensity.current
    BoxWithConstraints(
        Modifier.fillMaxSize().windowInsetsPadding(WindowInsets.safeDrawing)
            .clipToBounds().clearAndSetSemantics {},
    ) {
        val width = maxWidth.value
        val height = maxHeight.value
        val motion = remember(width, height) {
            MascotMotion(
                x = Random.nextFloat() * (width - MASCOT_SIZE_DP).coerceAtLeast(0f),
                y = Random.nextFloat() * (height - MASCOT_SIZE_DP).coerceAtLeast(0f),
                vx = 0f,
                vy = 0f,
            ).apply { randomizeVelocity() }
        }
        var x by remember(motion) { mutableFloatStateOf(motion.x) }
        var y by remember(motion) { mutableFloatStateOf(motion.y) }
        var rotationY by remember(motion) { mutableFloatStateOf(0f) }
        var rotationZ by remember(motion) { mutableFloatStateOf(0f) }
        var messageIndex by remember { mutableIntStateOf(Random.nextInt(messages.size)) }
        var bubbleSize by remember { mutableStateOf(IntSize.Zero) }
        val bubbleScale = remember { Animatable(1f) }
        LaunchedEffect(messageIndex) {
            bubbleScale.snapTo(0.94f)
            bubbleScale.animateTo(1f, tween(durationMillis = 420, easing = FastOutSlowInEasing))
        }
        LaunchedEffect(motion) {
            var previousFrame = withFrameNanos { it }
            var nextSpin = previousFrame + 800_000_000L
            var spinStart = 0L
            var spinDirection = 1f
            var tumble = false
            while (isActive) {
                withFrameNanos { now ->
                    if (motion.advance(width, height, (now - previousFrame) / 1_000_000_000f)) {
                        messageIndex = nextMascotMessage(messageIndex, messages.size)
                    }
                    previousFrame = now
                    x = motion.x
                    y = motion.y
                    if (now >= nextSpin) {
                        spinStart = now
                        spinDirection = if (Random.nextBoolean()) 1f else -1f
                        tumble = Random.nextBoolean()
                        nextSpin = now + Random.nextLong(1_400_000_000L, 3_600_000_000L)
                    }
                    val progress = if (spinStart == 0L) 1f else ((now - spinStart) / 650_000_000f).coerceIn(0f, 1f)
                    val angle = if (progress >= 1f) 0f else (1f - cos(progress * PI.toFloat())) * 180f * spinDirection
                    rotationY = angle
                    rotationZ = if (tumble) angle else 0f
                }
            }
        }
        Image(
            painter = painterResource(R.drawable.opennow_icon),
            contentDescription = null,
            contentScale = ContentScale.Fit,
            modifier = Modifier.size(MASCOT_SIZE_DP.dp).graphicsLayer {
                translationX = x.dp.toPx()
                translationY = y.dp.toPx()
                this.rotationY = rotationY
                this.rotationZ = rotationZ
                cameraDistance = 12f * this.density
            },
        )
        val bubbleColor = Color(0xFFF4FFE9)
        val ink = Color(0xFF17251C)
        val bubbleShape = RoundedCornerShape(13.dp)
        Box(
            Modifier.widthIn(max = minOf(220.dp, maxWidth))
                .onSizeChanged { bubbleSize = it }
                .graphicsLayer {
                    val bubbleWidthDp = with(density) { bubbleSize.width.toDp().value }
                    val bubbleHeightDp = with(density) { bubbleSize.height.toDp().value }
                    val bubbleX = (x + MASCOT_SIZE_DP / 2f - bubbleWidthDp / 2f)
                        .coerceIn(0f, (width - bubbleWidthDp).coerceAtLeast(0f))
                    val above = y >= bubbleHeightDp + 12f
                    val bubbleY = if (above) y - bubbleHeightDp - 10f else y + MASCOT_SIZE_DP + 10f
                    translationX = bubbleX.dp.toPx()
                    translationY = bubbleY.coerceIn(0f, (height - bubbleHeightDp).coerceAtLeast(0f)).dp.toPx()
                    scaleX = bubbleScale.value
                    scaleY = bubbleScale.value
                }
                .shadow(
                    elevation = 8.dp,
                    shape = bubbleShape,
                    clip = false,
                    ambientColor = Color(0x3328D86E),
                    spotColor = Color(0x5528D86E),
                )
                .drawBehind {
                    val above = y.dp.toPx() >= size.height + 12.dp.toPx()
                    val edgeY = if (above) size.height else 0f
                    val pointY = edgeY + (if (above) 7.dp.toPx() else -7.dp.toPx())
                    val half = 6.dp.toPx()
                    val path = Path().apply {
                        moveTo(size.width / 2f - half, edgeY)
                        lineTo(size.width / 2f, pointY)
                        lineTo(size.width / 2f + half, edgeY)
                        close()
                    }
                    drawPath(path, ink)
                    drawLine(bubbleColor, Offset(size.width / 2f, edgeY), Offset(size.width / 2f, pointY), 3.dp.toPx())
                }
                .background(bubbleColor, bubbleShape).border(1.5.dp, ink, bubbleShape)
                .padding(horizontal = 11.dp, vertical = 8.dp),
        ) {
            Text(
                text = messages[messageIndex],
                color = ink,
                fontSize = 12.sp,
                lineHeight = 16.sp,
                fontWeight = FontWeight.Bold,
                textAlign = TextAlign.Center,
            )
        }
    }
}

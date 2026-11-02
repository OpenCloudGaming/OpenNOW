package com.opencloudgaming.opennow

import androidx.compose.animation.core.Animatable
import androidx.compose.animation.core.FastOutSlowInEasing
import androidx.compose.animation.core.animateFloatAsState
import androidx.compose.animation.core.tween
import androidx.compose.foundation.interaction.MutableInteractionSource
import androidx.compose.foundation.interaction.collectIsHoveredAsState
import androidx.compose.foundation.interaction.collectIsPressedAsState
import androidx.compose.runtime.*
import androidx.compose.ui.Modifier
import androidx.compose.ui.draw.drawWithContent
import androidx.compose.ui.input.InputMode
import androidx.compose.ui.platform.LocalInputModeManager
import androidx.compose.animation.core.animateFloat
import androidx.compose.ui.graphics.graphicsLayer
import androidx.compose.ui.platform.LocalDensity
import androidx.compose.ui.unit.Dp
import androidx.compose.ui.unit.dp
import com.opencloudgaming.opennow.ui.theme.LocalReduceMotion

internal val LocalSpaceAnimatedFocusOutline = compositionLocalOf { true }
internal val LocalSpaceControllerConnected = compositionLocalOf { false }

/** Shared console feedback; focus chrome draws without changing control layout. */
internal class SpaceInteraction(
    val source: MutableInteractionSource,
    val hovered: Boolean,
    val emphasis: Float,
    val modifier: Modifier,
)

@Composable
internal fun rememberSpaceInteraction(focused: Boolean, tile: Boolean = false, enabled: Boolean = true,
    reduceMotion: Boolean = LocalReduceMotion.current, cornerRadius: Dp = 4.dp, showFocusOutline: Boolean = true): SpaceInteraction {
    val fast = LocalSpaceFastNavigation.current
    val source = remember { MutableInteractionSource() }
    val hovered by source.collectIsHoveredAsState()
    val pressed by source.collectIsPressedAsState()
    val engaged = enabled && (focused || hovered)
    val emphasis by animateFloatAsState(if (engaged) 1f else 0f,
        tween(if (reduceMotion) 0 else if (fast) 55 else 180), label = "space-emphasis")
    val scale = animateFloatAsState(when {
        reduceMotion -> 1f
        enabled && pressed -> .97f
        engaged && tile -> 1.045f
        engaged -> 1.015f
        else -> 1f
    }, tween(if (reduceMotion) 0 else if (fast) 45 else if (pressed) 80 else 180, easing = FastOutSlowInEasing), label = "space-scale")
    val lift = with(LocalDensity.current) { (if (tile) 3.dp else 0.dp).toPx() }
    return SpaceInteraction(source, enabled && hovered, emphasis, Modifier.graphicsLayer {
        scaleX = scale.value; scaleY = scale.value
        translationY = if (reduceMotion || pressed) 0f else -lift * emphasis
    }.then(if (tile || !showFocusOutline || !LocalSpaceAnimatedFocusOutline.current) Modifier else Modifier.spaceFocusFrame(engaged, cornerRadius, hovered = enabled && hovered)))
}

/** Finite entrance on navigation/selection, cancelled immediately by the next navigation. */
@Composable
internal fun Modifier.spaceArrival(key: Any?, delayMillis: Int = 0): Modifier {
    val reduceMotion = LocalReduceMotion.current
    val fast = LocalSpaceFastNavigation.current
    val progress = remember(key) { Animatable(if (reduceMotion) 1f else if (fast) .8f else 0f) }
    val distance = with(LocalDensity.current) { 24.dp.toPx() }
    LaunchedEffect(progress, reduceMotion, fast) {
        if (reduceMotion) progress.snapTo(1f)
        else progress.animateTo(1f, tween(if (fast) 65 else 400,
            delayMillis = if (fast || progress.value > 0f) 0 else delayMillis, easing = FastOutSlowInEasing))
    }
    return graphicsLayer {
        alpha = progress.value
        translationY = distance * (1f - progress.value)
    }
}

/** Spring-in frame with a slow moving light while a tile owns focus or hover. */
@Composable
internal fun Modifier.spaceFocusFrame(active: Boolean, radius: Dp, hovered: Boolean = false): Modifier {
    // A mouse owns its hover frame even when no physical controller is connected.
    val directionalInput = LocalSpaceControllerConnected.current || LocalInputModeManager.current.inputMode == InputMode.Keyboard
    val engaged = active && LocalSpaceAnimatedFocusOutline.current && (hovered || directionalInput) &&
        androidx.compose.ui.platform.LocalWindowInfo.current.isWindowFocused
    val reduced = LocalReduceMotion.current
    val fast = LocalSpaceFastNavigation.current
    val reveal = animateFloatAsState(if (engaged) 1f else 0f,
        if (reduced) tween(0) else if (fast) tween(55) else androidx.compose.animation.core.spring(dampingRatio = .68f, stiffness = 420f),
        label = "space-frame-arrival")
    val light = if (engaged && !reduced && !fast) {
        val transition = androidx.compose.animation.core.rememberInfiniteTransition(label = "space-focus-light")
        transition.animateFloat(0f, 6.2831855f,
            androidx.compose.animation.core.infiniteRepeatable(tween(2800, easing = androidx.compose.animation.core.LinearEasing)),
            label = "space-focus-light-angle")
    } else rememberUpdatedState(0f)
    return drawWithContent {
        drawContent()
        val progress = reveal.value
        val angle = light.value
        val opacity = progress.coerceIn(0f, 1f)
        if (opacity > 0f) {
            val gap = (8f - 4f * progress).dp.toPx()
            val corner = androidx.compose.ui.geometry.CornerRadius(radius.toPx() + gap)
            val origin = androidx.compose.ui.geometry.Offset(-gap, -gap)
            val frameSize = androidx.compose.ui.geometry.Size(size.width + gap * 2, size.height + gap * 2)
            drawRoundRect(SpaceWhite.copy(alpha = .15f * opacity), origin, frameSize, corner,
                style = androidx.compose.ui.graphics.drawscope.Stroke(7.dp.toPx()))
            val vector = androidx.compose.ui.geometry.Offset(kotlin.math.cos(angle) * size.width, kotlin.math.sin(angle) * size.height)
            val center = androidx.compose.ui.geometry.Offset(size.width / 2, size.height / 2)
            drawRoundRect(androidx.compose.ui.graphics.Brush.linearGradient(
                listOf(SpaceWhite.copy(alpha = .45f * opacity), SpaceWhite.copy(alpha = opacity), SpaceIce.copy(alpha = .65f * opacity)),
                center - vector, center + vector), origin, frameSize, corner,
                style = androidx.compose.ui.graphics.drawscope.Stroke(2.dp.toPx()))
        }
    }
}

/** A persistent selection cue, separate from the brighter transient input-focus frame. Fits the tile gutter. */
@Composable
internal fun Modifier.spaceSelectionMark(selected: Boolean): Modifier {
    val reveal = animateFloatAsState(if (selected) 1f else 0f,
        tween(if (LocalReduceMotion.current) 0 else if (LocalSpaceFastNavigation.current) 55 else 180), label = "space-selection")
    return drawWithContent {
        drawContent()
        val progress = reveal.value
        if (progress > 0f) {
            val halfWidth = 12.dp.toPx().coerceAtMost(size.width / 4) * progress
            val y = size.height + 6.dp.toPx()
            val start = androidx.compose.ui.geometry.Offset(size.width / 2 - halfWidth, y)
            val end = androidx.compose.ui.geometry.Offset(size.width / 2 + halfWidth, y)
            drawLine(SpaceIce.copy(alpha = .14f * progress), start, end,
                strokeWidth = 6.dp.toPx(), cap = androidx.compose.ui.graphics.StrokeCap.Round)
            drawLine(SpaceIce.copy(alpha = progress), start, end,
                strokeWidth = 2.dp.toPx(), cap = androidx.compose.ui.graphics.StrokeCap.Round)
        }
    }
}

@Composable
internal fun Modifier.spaceArtworkArrival(key: Any?): Modifier {
    val reduced = LocalReduceMotion.current
    val fast = LocalSpaceFastNavigation.current
    val zoom = remember(key) { Animatable(if (reduced) 1f else if (fast) 1.01f else 1.045f) }
    LaunchedEffect(zoom, reduced, fast) {
        if (reduced) zoom.snapTo(1f) else zoom.animateTo(1f, tween(if (fast) 85 else 700, easing = FastOutSlowInEasing))
    }
    return graphicsLayer { scaleX = zoom.value; scaleY = zoom.value }
}

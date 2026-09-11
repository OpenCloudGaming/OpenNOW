package com.opencloudgaming.opennow

import android.graphics.DiscretePathEffect
import androidx.compose.animation.core.LinearEasing
import androidx.compose.animation.core.RepeatMode
import androidx.compose.animation.core.animateFloat
import androidx.compose.animation.core.infiniteRepeatable
import androidx.compose.animation.core.rememberInfiniteTransition
import androidx.compose.animation.core.tween
import androidx.compose.foundation.Canvas
import androidx.compose.foundation.layout.BoxScope
import androidx.compose.runtime.Composable
import androidx.compose.ui.Modifier
import androidx.compose.ui.geometry.CornerRadius
import androidx.compose.ui.geometry.Offset
import androidx.compose.ui.geometry.Size
import androidx.compose.ui.graphics.Color
import androidx.compose.ui.graphics.PathEffect
import androidx.compose.ui.graphics.StrokeCap
import androidx.compose.ui.graphics.drawscope.Stroke
import androidx.compose.ui.graphics.toComposePathEffect
import androidx.compose.ui.unit.Dp
import androidx.compose.ui.unit.dp
import com.opencloudgaming.opennow.ui.theme.LocalReduceMotion
import com.opencloudgaming.opennow.ui.theme.OpenNowPalette
import kotlin.math.PI
import kotlin.math.floor
import kotlin.math.sin

private const val ENERGY_ORBIT_DURATION_MS = 2_600

internal fun shouldShowEnhancedControllerFocus(
    focused: Boolean,
    tvProfile: Boolean,
    controllerActionMode: Boolean,
): Boolean = focused && (tvProfile || controllerActionMode)

internal fun shouldShowActiveSelectionOutline(
    selected: Boolean,
    enabled: Boolean,
): Boolean = selected && enabled

internal fun shouldAnimateControllerFocusFrame(
    absoluteCinemaEnabled: Boolean,
    reduceMotion: Boolean,
): Boolean = absoluteCinemaEnabled && !reduceMotion

internal fun shouldDrawStaticInteractionFocus(
    visible: Boolean,
    cinemaEffectEnabled: Boolean,
): Boolean = visible && !cinemaEffectEnabled

/**
 * Static game-card strokes are independent from the optional animated sibling frame. Controller
 * focus always keeps a bold white navigation cue when those effects are disabled, even if the user
 * also turned off resting game borders.
 */
internal fun catalogCardBorderColor(
    selectionColor: Color,
    gameBorderEnabled: Boolean,
    controllerFocused: Boolean = false,
    borderEffectsEnabled: Boolean = false,
): Color = when {
    controllerFocused && !borderEffectsEnabled -> Color.White
    gameBorderEnabled -> selectionColor
    else -> Color.Transparent
}

/** Resting artwork follows the theme's thin stroke; only the fallback controller cue is bold. */
internal fun catalogCardBorderWidthDp(
    controllerFocused: Boolean = false,
    borderEffectsEnabled: Boolean = false,
): Float = if (controllerFocused && !borderEffectsEnabled) 3f else 1f

internal fun cinemaBorderColor(
    absoluteCinemaEnabled: Boolean,
    cinemaColor: Color,
): Color = if (absoluteCinemaEnabled) cinemaColor else Color.Transparent

private fun controllerFocusLoopProgress(progress: Float): Float {
    val clamped = progress.coerceIn(0f, 1f)
    return if (clamped >= 1f) 0f else clamped
}

internal fun controllerFocusOrbitPhasePx(progress: Float, perimeterPx: Float): Float =
    controllerFocusLoopProgress(progress) * perimeterPx.coerceAtLeast(0f)

internal fun controllerFocusStaticStep(progress: Float): Int =
    floor(controllerFocusLoopProgress(progress) * 48f).toInt()

internal fun controllerFocusFlickerAlpha(progress: Float): Float {
    val loop = controllerFocusLoopProgress(progress)
    return (
        0.88f +
            0.07f * sin(loop * 43.982296f) +
            0.05f * sin(loop * 81.68141f)
        ).coerceIn(0.72f, 1f)
}

/** Keeps the classic Cinema palette scoped to animated energy instead of static theme borders. */
internal fun controllerFocusEnergyColors(
    absoluteCinemaPalette: Boolean,
    tint: Color?,
    secondaryTint: Color?,
): Pair<Color, Color> = if (absoluteCinemaPalette) {
    OpenNowPalette.AccentCinemaOrange to OpenNowPalette.AccentCinemaBlue
} else {
    (tint ?: Color.White) to (secondaryTint ?: tint?.focusShade() ?: Color.White)
}

/**
 * Draws on the exact bounds of an unclipped parent [BoxScope]. Keep this as a sibling of the
 * clipped card or artwork so the core follows its edge and the glow remains visible outside it.
 */
@Composable
internal fun BoxScope.ControllerFocusFrame(
    visible: Boolean,
    cornerRadius: Dp,
    tint: Color? = null,
    secondaryTint: Color? = null,
    verticalInset: Dp = 0.dp,
) {
    // This component used to fall back to a solid white outline. Borders now belong exclusively
    // to the opt-in Absolute Cinema mode.
    if (!visible || !LocalAbsoluteCinemaEffects.current) return
    val absoluteCinemaPalette = LocalAbsoluteCinemaPalette.current
    val animateEnergy = shouldAnimateControllerFocusFrame(
        absoluteCinemaEnabled = LocalAbsoluteCinemaEffects.current,
        reduceMotion = LocalReduceMotion.current,
    )
    if (!animateEnergy) {
        Canvas(Modifier.matchParentSize()) {
            val insetPx = verticalInset.toPx().coerceIn(0f, size.height / 2f)
            drawRoundRect(
                color = (tint ?: Color.White).copy(alpha = 0.96f),
                topLeft = Offset(0f, insetPx),
                size = Size(size.width, (size.height - insetPx * 2f).coerceAtLeast(0f)),
                cornerRadius = CornerRadius(cornerRadius.toPx(), cornerRadius.toPx()),
                style = Stroke(width = 3.dp.toPx()),
            )
        }
        return
    }
    val orbitProgress = rememberInfiniteTransition(label = "controller-focus-energy").animateFloat(
        initialValue = 0f,
        targetValue = 1f,
        animationSpec = infiniteRepeatable(
            animation = tween(ENERGY_ORBIT_DURATION_MS, easing = LinearEasing),
            repeatMode = RepeatMode.Restart,
        ),
        label = "controller-focus-energy-orbit",
    )

    Canvas(Modifier.matchParentSize()) {
        // The path is centered on the parent's exact bounds. Callers place this Canvas beside the
        // clipped artwork, so the glow can spill outward instead of consuming the image edge.
        val insetPx = verticalInset.toPx().coerceIn(0f, size.height / 2f)
        val borderSize = Size(size.width, (size.height - insetPx * 2f).coerceAtLeast(0f))
        if (borderSize.width == 0f || borderSize.height == 0f) return@Canvas
        val radius = cornerRadius.toPx().coerceAtLeast(0f)
        val perimeter = (
            2f * (borderSize.width + borderSize.height - 4f * radius) +
                2f * PI.toFloat() * radius
            ).coerceAtLeast(1f)
        // Observe the clock only while drawing; animation frames do not need recomposition.
        val progress = orbitProgress.value
        val orbitPhase = controllerFocusOrbitPhasePx(progress, perimeter)
        val arcIntervals = floatArrayOf(perimeter * 0.44f, perimeter * 0.56f)
        val blueArc = PathEffect.dashPathEffect(arcIntervals, orbitPhase)
        val fireArc = PathEffect.dashPathEffect(arcIntervals, orbitPhase + perimeter * 0.5f)
        val staticStep = controllerFocusStaticStep(progress)
        val blueStatic = PathEffect.chainPathEffect(
            DiscretePathEffect(
                (3.7f + (staticStep % 4) * 0.15f).dp.toPx(),
                (1.2f + (staticStep % 5) * 0.08f).dp.toPx(),
            ).toComposePathEffect(),
            blueArc,
        )
        val fireStatic = PathEffect.chainPathEffect(
            DiscretePathEffect(
                (3.8f + ((staticStep + 1) % 4) * 0.14f).dp.toPx(),
                (1.16f + ((staticStep + 2) % 5) * 0.09f).dp.toPx(),
            ).toComposePathEffect(),
            fireArc,
        )
        val flicker = controllerFocusFlickerAlpha(progress)
        val topLeft = Offset(0f, insetPx)
        val roundedCorner = CornerRadius(radius, radius)

        fun drawEnergyArc(color: Color, hotColor: Color, smooth: PathEffect, electric: PathEffect) {
            drawRoundRect(
                color = color.copy(alpha = 0.18f * flicker),
                topLeft = topLeft,
                size = borderSize,
                cornerRadius = roundedCorner,
                style = Stroke(width = 9.dp.toPx(), cap = StrokeCap.Round, pathEffect = smooth),
            )
            drawRoundRect(
                color = color.copy(alpha = 0.98f),
                topLeft = topLeft,
                size = borderSize,
                cornerRadius = roundedCorner,
                style = Stroke(width = 2.8.dp.toPx(), cap = StrokeCap.Round, pathEffect = smooth),
            )
            drawRoundRect(
                color = hotColor.copy(alpha = flicker),
                topLeft = topLeft,
                size = borderSize,
                cornerRadius = roundedCorner,
                style = Stroke(width = 1.15.dp.toPx(), cap = StrokeCap.Round, pathEffect = electric),
            )
        }

        val (firstColor, secondColor) = controllerFocusEnergyColors(
            absoluteCinemaPalette = absoluteCinemaPalette,
            tint = tint,
            secondaryTint = secondaryTint,
        )
        val firstHotColor = if (absoluteCinemaPalette) {
            Color(0xffffd166)
        } else {
            tint?.focusHighlight() ?: Color.White
        }
        val secondHotColor = if (absoluteCinemaPalette) {
            Color(0xffd9f8ff)
        } else {
            secondaryTint?.focusHighlight() ?: tint ?: Color.White
        }
        drawEnergyArc(firstColor, firstHotColor, fireArc, fireStatic)
        drawEnergyArc(secondColor, secondHotColor, blueArc, blueStatic)

        val sparkOn = 1.4.dp.toPx()
        val sparkOff = 8.6.dp.toPx()
        val sparkPhase = controllerFocusLoopProgress(progress) * (sparkOn + sparkOff) * 8f
        val sparks = PathEffect.chainPathEffect(
            DiscretePathEffect(
                3.dp.toPx(),
                (1.5f + (staticStep % 4) * 0.12f).dp.toPx(),
            ).toComposePathEffect(),
            PathEffect.dashPathEffect(floatArrayOf(sparkOn, sparkOff), sparkPhase),
        )
        drawRoundRect(
            color = Color.White.copy(alpha = 0.48f * flicker),
            topLeft = topLeft,
            size = borderSize,
            cornerRadius = roundedCorner,
            style = Stroke(width = 1.dp.toPx(), cap = StrokeCap.Round, pathEffect = sparks),
        )
    }
}

/**
 * One focus owner for controls that need a bold white fallback and may opt into Cinema motion.
 * Callers must not draw another focused border underneath this frame: doing so produces the
 * doubled rings that made Settings look heavier than the first account row.
 */
@Composable
internal fun BoxScope.InteractionFocusFrame(
    visible: Boolean,
    cornerRadius: Dp,
    cinemaEffectEnabled: Boolean,
    verticalInset: Dp = 0.dp,
) {
    if (!visible) return
    val cinemaEffectActive = cinemaEffectEnabled && LocalAbsoluteCinemaEffects.current
    if (cinemaEffectActive) {
        ControllerFocusFrame(
            visible = true,
            cornerRadius = cornerRadius,
            tint = LocalActiveSelectionColor.current,
            secondaryTint = LocalActiveSelectionSecondaryColor.current,
            verticalInset = verticalInset,
        )
        return
    }
    if (!shouldDrawStaticInteractionFocus(visible, cinemaEffectActive)) return
    Canvas(Modifier.matchParentSize()) {
        val insetPx = verticalInset.toPx().coerceIn(0f, size.height / 2f)
        drawRoundRect(
            color = Color.White.copy(alpha = 0.96f),
            topLeft = Offset(0f, insetPx),
            size = Size(size.width, (size.height - insetPx * 2f).coerceAtLeast(0f)),
            cornerRadius = CornerRadius(cornerRadius.toPx(), cornerRadius.toPx()),
            style = Stroke(width = 3.dp.toPx()),
        )
    }
}

/**
 * The opt-in, everywhere variant deliberately stays interaction-driven: only the surface currently
 * under a pointer or focus receives another animated canvas. This keeps the exuberant look without
 * running an infinite transition for every visible control at once.
 */
@Composable
internal fun BoxScope.AbsoluteCinemaEverywhereFrame(
    visible: Boolean,
    cornerRadius: Dp,
    verticalInset: Dp = 0.dp,
) {
    ControllerFocusFrame(
        visible = visible && LocalAbsoluteCinemaEverywhere.current,
        cornerRadius = cornerRadius,
        tint = LocalActiveSelectionColor.current,
        secondaryTint = LocalActiveSelectionSecondaryColor.current,
        verticalInset = verticalInset,
    )
}

private fun Color.focusShade(): Color = Color(
    red = red * 0.62f,
    green = green * 0.62f,
    blue = blue * 0.62f,
    alpha = alpha,
)

private fun Color.focusHighlight(): Color = Color(
    red = red + (1f - red) * 0.42f,
    green = green + (1f - green) * 0.42f,
    blue = blue + (1f - blue) * 0.42f,
    alpha = alpha,
)

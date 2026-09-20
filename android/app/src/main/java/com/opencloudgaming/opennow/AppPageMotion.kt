package com.opencloudgaming.opennow

import androidx.compose.animation.core.Animatable
import androidx.compose.animation.core.tween
import androidx.compose.foundation.layout.Box
import androidx.compose.runtime.Composable
import androidx.compose.runtime.LaunchedEffect
import androidx.compose.runtime.getValue
import androidx.compose.runtime.mutableStateOf
import androidx.compose.runtime.remember
import androidx.compose.runtime.setValue
import androidx.compose.ui.Modifier
import androidx.compose.ui.graphics.graphicsLayer
import androidx.compose.ui.platform.LocalDensity
import androidx.compose.ui.unit.dp
import com.opencloudgaming.opennow.ui.theme.LocalReduceMotion
import com.opencloudgaming.opennow.ui.theme.OpenNowMotion

/**
 * Direction for the small spatial cue used when moving between the app's three browse surfaces.
 * Stream is deliberately outside this ordering: video surfaces should never be transformed.
 */
internal fun appPageMotionDirection(from: AppPage, to: AppPage): Float {
    fun AppPage.position(): Int = when (this) {
        AppPage.Home -> 0
        AppPage.Library -> 1
        AppPage.Settings -> 2
        AppPage.Stream -> 3
    }
    return if (to.position() >= from.position()) 1f else -1f
}

internal fun shouldAnimateAppPageEntrance(
    from: AppPage,
    to: AppPage,
    reduceMotion: Boolean,
): Boolean = !reduceMotion && from != to && from != AppPage.Stream && to != AppPage.Stream

/**
 * Animates only the incoming page. This intentionally avoids [androidx.compose.animation.AnimatedContent]:
 * Store, Library, and Settings are large trees, and keeping both pages alive during a transition
 * causes unnecessary image work and missed frames on high-refresh phones and Android TV.
 */
@Composable
internal fun AppPageMotionContent(
    targetState: AppPage,
    modifier: Modifier = Modifier,
    content: @Composable (AppPage) -> Unit,
) {
    val reduceMotion = LocalReduceMotion.current
    var initialized by remember { mutableStateOf(false) }
    var previousPage by remember { mutableStateOf(targetState) }
    val animateEntrance = remember(targetState, reduceMotion) {
        initialized && shouldAnimateAppPageEntrance(
            from = previousPage,
            to = targetState,
            reduceMotion = reduceMotion,
        )
    }
    val direction = remember(targetState) {
        appPageMotionDirection(from = previousPage, to = targetState)
    }
    val progress = remember(targetState, reduceMotion) {
        Animatable(if (animateEntrance) 0f else 1f)
    }
    var motionLayerActive by remember(targetState, reduceMotion) {
        mutableStateOf(animateEntrance)
    }
    val travelPx = with(LocalDensity.current) { 32.dp.toPx() }

    LaunchedEffect(targetState, reduceMotion) {
        previousPage = targetState
        if (!initialized) {
            initialized = true
        } else if (animateEntrance) {
            progress.animateTo(
                targetValue = 1f,
                animationSpec = tween(
                    durationMillis = OpenNowMotion.DurationEmphasized,
                    easing = OpenNowMotion.EasingEmphasizedDecel,
                ),
            )
            motionLayerActive = false
        } else {
            progress.snapTo(1f)
            motionLayerActive = false
        }
    }

    val motionModifier = if (motionLayerActive) {
        Modifier.graphicsLayer {
            val remaining = 1f - progress.value
            translationX = direction * travelPx * remaining
            alpha = 1f - (0.14f * remaining)
            val scale = 1f - (0.012f * remaining)
            scaleX = scale
            scaleY = scale
        }
    } else {
        Modifier
    }
    Box(modifier.then(motionModifier)) {
        content(targetState)
    }
}

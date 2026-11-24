package com.opencloudgaming.opennow

import androidx.compose.runtime.Stable
import androidx.compose.runtime.Composable
import androidx.compose.runtime.LaunchedEffect
import androidx.compose.runtime.getValue
import androidx.compose.runtime.mutableStateOf
import androidx.compose.runtime.remember
import androidx.compose.runtime.setValue
import androidx.compose.runtime.staticCompositionLocalOf
import androidx.compose.animation.core.Animatable
import androidx.compose.animation.core.tween
import androidx.compose.ui.Modifier
import androidx.compose.ui.geometry.Rect
import androidx.compose.ui.graphics.TransformOrigin
import androidx.compose.ui.graphics.graphicsLayer
import androidx.compose.ui.layout.boundsInWindow
import androidx.compose.ui.layout.onGloballyPositioned
import androidx.compose.ui.node.ModifierNodeElement
import androidx.compose.ui.node.requireLayoutCoordinates
import androidx.compose.ui.platform.InspectorInfo
import com.opencloudgaming.opennow.ui.theme.LocalReduceMotion
import com.opencloudgaming.opennow.ui.theme.OpenNowMotion

/** The last catalog artwork rectangle activated by the user, in window coordinates. */
internal data class GameDetailsTransitionOrigin(
    val gameId: String,
    val bounds: Rect,
    val kind: GameDetailsTransitionKind,
)

internal enum class GameDetailsTransitionKind {
    Card,
    Hero,
}

/**
 * Bridges catalog artwork and the details overlay without coupling navigation state to view
 * geometry. The source kind decides whether the surface or only its artwork should transform.
 */
@Stable
internal class GameDetailsTransitionRegistry {
    private var latestOrigin by mutableStateOf<GameDetailsTransitionOrigin?>(null)

    fun record(gameId: String, bounds: Rect, kind: GameDetailsTransitionKind) {
        if (bounds.width <= 0f || bounds.height <= 0f) return
        latestOrigin = GameDetailsTransitionOrigin(gameId, bounds, kind)
    }

    fun originFor(gameId: String): GameDetailsTransitionOrigin? = latestOrigin
        ?.takeIf { it.gameId == gameId }

    fun clear(gameId: String) {
        if (latestOrigin?.gameId == gameId) latestOrigin = null
    }
}

internal val LocalGameDetailsTransitionRegistry =
    staticCompositionLocalOf<GameDetailsTransitionRegistry?> { null }

/** Reads the current artwork bounds on activation without observing every scroll position. */
internal class GameDetailsTransitionAnchor {
    private var node: AnchorNode? = null
    val modifier: Modifier = AnchorElement(this)

    fun boundsInWindow(): Rect? = node?.takeIf { it.isAttached }?.requireLayoutCoordinates()?.boundsInWindow()

    private data class AnchorElement(val anchor: GameDetailsTransitionAnchor) : ModifierNodeElement<AnchorNode>() {
        override fun create() = AnchorNode(anchor)

        override fun update(node: AnchorNode) {
            node.anchor = anchor
        }

        override fun InspectorInfo.inspectableProperties() {
            name = "gameDetailsTransitionAnchor"
        }
    }

    private class AnchorNode(anchor: GameDetailsTransitionAnchor) : Modifier.Node() {
        var anchor = anchor
            set(value) {
                if (field.node === this) field.node = null
                field = value
                if (isAttached) field.node = this
            }

        override fun onAttach() {
            anchor.node = this
        }

        override fun onDetach() {
            if (anchor.node === this) anchor.node = null
        }
    }
}

internal data class GameDetailsContainerTransform(
    val scaleX: Float,
    val scaleY: Float,
    val translationX: Float,
    val translationY: Float,
)

/** Maps a full details surface onto the source card at 0 and onto itself at 1. */
internal fun gameDetailsContainerTransform(
    source: Rect,
    target: Rect,
    progress: Float,
): GameDetailsContainerTransform {
    if (target.width <= 0f || target.height <= 0f) {
        return GameDetailsContainerTransform(1f, 1f, 0f, 0f)
    }
    val fraction = progress.coerceIn(0f, 1f)
    fun interpolate(start: Float, end: Float): Float = start + ((end - start) * fraction)
    return GameDetailsContainerTransform(
        scaleX = interpolate(source.width / target.width, 1f),
        scaleY = interpolate(source.height / target.height, 1f),
        translationX = interpolate(source.left - target.left, 0f),
        translationY = interpolate(source.top - target.top, 0f),
    )
}

/**
 * Moves the details artwork from the activated catalog artwork into its final banner bounds.
 * The target is captured before the layer transform and held stable for the short entrance.
 */
@Composable
internal fun Modifier.gameDetailsArtworkEntrance(gameId: String): Modifier {
    val transitionOrigin = LocalGameDetailsTransitionRegistry.current
        ?.originFor(gameId)
        ?.takeIf { it.kind == GameDetailsTransitionKind.Hero }
        ?.bounds
    val reduceMotion = LocalReduceMotion.current
    var targetBounds by remember(gameId) { mutableStateOf<Rect?>(null) }
    val progress = remember(gameId) {
        Animatable(if (transitionOrigin == null || reduceMotion) 1f else 0f)
    }
    LaunchedEffect(gameId, transitionOrigin, targetBounds, reduceMotion) {
        val target = targetBounds
        if (transitionOrigin == null || target == null || reduceMotion) {
            if (transitionOrigin == null || reduceMotion) progress.snapTo(1f)
            return@LaunchedEffect
        }
        progress.snapTo(0f)
        progress.animateTo(
            targetValue = 1f,
            animationSpec = tween(
                durationMillis = OpenNowMotion.DurationStandard,
                easing = OpenNowMotion.EasingStandard,
            ),
        )
    }
    return onGloballyPositioned { coordinates ->
        if (targetBounds == null) targetBounds = coordinates.boundsInWindow()
    }.graphicsLayer {
        val target = targetBounds
        if (transitionOrigin != null && target != null && !reduceMotion) {
            val transform = gameDetailsContainerTransform(
                source = transitionOrigin,
                target = target,
                progress = progress.value,
            )
            transformOrigin = TransformOrigin(0f, 0f)
            scaleX = transform.scaleX
            scaleY = transform.scaleY
            translationX = transform.translationX
            translationY = transform.translationY
        }
        alpha = if (transitionOrigin != null && target == null && !reduceMotion) 0f else 1f
    }
}

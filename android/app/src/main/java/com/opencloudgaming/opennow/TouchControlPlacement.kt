package com.opencloudgaming.opennow

import androidx.compose.foundation.gestures.detectDragGestures
import androidx.compose.foundation.layout.absoluteOffset
import androidx.compose.runtime.Composable
import androidx.compose.runtime.getValue
import androidx.compose.runtime.mutableStateOf
import androidx.compose.runtime.remember
import androidx.compose.runtime.rememberUpdatedState
import androidx.compose.runtime.setValue
import androidx.compose.ui.Modifier
import androidx.compose.ui.geometry.Offset
import androidx.compose.ui.geometry.Rect
import androidx.compose.ui.geometry.Size
import androidx.compose.ui.input.pointer.pointerInput
import androidx.compose.ui.layout.findRootCoordinates
import androidx.compose.ui.layout.onGloballyPositioned
import androidx.compose.ui.layout.positionInRoot
import androidx.compose.ui.platform.LocalDensity
import androidx.compose.ui.unit.Dp
import androidx.compose.ui.unit.IntSize

/** Shared drag behavior for touch controls and the editor panel; taps remain available to children. */
@Composable
internal fun Modifier.draggableTouchOverlay(
    enabled: Boolean,
    offsetX: Dp,
    offsetY: Dp,
    onOffsetChange: (Float, Float) -> Unit,
): Modifier {
    val density = LocalDensity.current
    val currentOffsetX by rememberUpdatedState(offsetX)
    val currentOffsetY by rememberUpdatedState(offsetY)
    val currentOnOffsetChange by rememberUpdatedState(onOffsetChange)
    var controlBounds by remember { mutableStateOf(Rect.Zero) }
    var viewportSize by remember { mutableStateOf(IntSize.Zero) }
    return absoluteOffset(x = offsetX, y = offsetY)
        .pointerInput(enabled, density) {
            if (!enabled) return@pointerInput
            var start = TouchOffset()
            var bounds = Rect.Zero
            var viewport = IntSize.Zero
            var travel = Offset.Zero
            detectDragGestures(onDragStart = {
                start = TouchOffset(currentOffsetX.value, currentOffsetY.value)
                bounds = controlBounds
                viewport = viewportSize
                travel = Offset.Zero
            }) { change, dragAmount ->
                change.consume()
                travel += dragAmount
                with(density) {
                    currentOnOffsetChange(
                        draggedTouchOffset(start.x, travel.x.toDp().value,
                            bounds.left.toDp().value, bounds.right.toDp().value, viewport.width.toDp().value),
                        draggedTouchOffset(start.y, travel.y.toDp().value,
                            bounds.top.toDp().value, bounds.bottom.toDp().value, viewport.height.toDp().value),
                    )
                }
            }
        }
        .onGloballyPositioned { coordinates ->
            controlBounds = Rect(coordinates.positionInRoot(), Size(
                coordinates.size.width.toFloat(), coordinates.size.height.toFloat(),
            ))
            viewportSize = coordinates.findRootCoordinates().size
        }
}

/** Clamp against the actual viewport, independent of the control's original side. */
internal fun draggedTouchOffset(start: Float, delta: Float, leading: Float, trailing: Float, viewport: Float): Float {
    val minimum = -leading
    val maximum = viewport - trailing
    return start + if (minimum <= maximum) delta.coerceIn(minimum, maximum) else minimum
}

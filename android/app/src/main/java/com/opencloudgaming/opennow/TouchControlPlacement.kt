package com.opencloudgaming.opennow

/** Clamp against the actual viewport, independent of the control's original side. */
internal fun draggedTouchOffset(start: Float, delta: Float, leading: Float, trailing: Float, viewport: Float): Float {
    val minimum = -leading
    val maximum = viewport - trailing
    return start + if (minimum <= maximum) delta.coerceIn(minimum, maximum) else minimum
}

package com.opencloudgaming.opennow.ui.theme

import androidx.compose.foundation.shape.RoundedCornerShape
import androidx.compose.material3.Shapes
import androidx.compose.ui.unit.dp

/**
 * Six radii, down from the thirteen distinct values that were previously spread across ~109
 * inline `RoundedCornerShape(...)` call sites.
 */
object OpenNowRadius {
    val xs = 4.dp
    val sm = 8.dp
    val md = 14.dp
    val lg = 20.dp
    val xl = 28.dp
    val full = 999.dp
}

/** Wired into `MaterialTheme(shapes = ...)`, which previously received no shapes at all. */
val OpenNowShapes = Shapes(
    extraSmall = RoundedCornerShape(OpenNowRadius.xs),
    small = RoundedCornerShape(OpenNowRadius.sm),
    medium = RoundedCornerShape(OpenNowRadius.md),
    large = RoundedCornerShape(OpenNowRadius.lg),
    extraLarge = RoundedCornerShape(OpenNowRadius.xl),
)

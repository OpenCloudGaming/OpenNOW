package com.opencloudgaming.opennow

import kotlinx.serialization.Serializable

@Serializable
enum class TouchButtonShape { Theme, Circle, Square, Rounded, Hexagon, Diamond, Octagon, Trigger }

@Serializable
data class TouchButtonAppearance(
    val label: String = "",
    val icon: String? = null,
    val shape: TouchButtonShape = TouchButtonShape.Theme,
    val toggle: Boolean = false,
    val sizeScale: Float = 1f,
)

internal val touchButtonKeys = listOf("A", "B", "X", "Y", "LT", "RT", "LB", "RB", "LS", "RS", "◀", "▶") +
    (1..TOUCH_EXTRA_BUTTON_COUNT).map { "extra$it" }
internal const val TOUCH_BUTTON_LABEL_LIMIT = 12

internal const val TOUCH_BUTTON_MIN_SIZE_SCALE = 0.5f
internal const val TOUCH_BUTTON_MAX_SIZE_SCALE = 2f

internal fun TouchButtonAppearance.effectiveSizeScale(): Float =
    if (sizeScale.isFinite()) sizeScale.coerceIn(TOUCH_BUTTON_MIN_SIZE_SCALE, TOUCH_BUTTON_MAX_SIZE_SCALE) else 1f

internal fun TouchButtonAppearance.normalized(): TouchButtonAppearance = copy(
    sizeScale = effectiveSizeScale(),
    label = label.filterNot { it.isISOControl() }.trim().take(TOUCH_BUTTON_LABEL_LIMIT),
    icon = icon?.takeIf { id -> TouchButtonIcon.entries.any { it.name == id } },
)

internal fun AndroidTouchSettings.withButtonAppearance(key: String, appearance: TouchButtonAppearance): AndroidTouchSettings {
    if (key !in touchButtonKeys) return this
    val value = appearance.normalized()
    return copy(buttonAppearances = if (value == TouchButtonAppearance()) buttonAppearances - key
        else buttonAppearances + (key to value))
}

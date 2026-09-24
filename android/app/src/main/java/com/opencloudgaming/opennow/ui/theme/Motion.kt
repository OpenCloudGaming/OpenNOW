package com.opencloudgaming.opennow.ui.theme

import androidx.compose.animation.core.CubicBezierEasing
import androidx.compose.runtime.staticCompositionLocalOf

/**
 * Duration and easing tokens, replacing the assorted `tween(1_100)` / `tween(900)` / `tween(820)`
 * values that were picked independently across the UI.
 */
object OpenNowMotion {
    /** Press, toggle, ripple — anything that must feel instantaneous. */
    const val DurationFast = 120

    /** Focus, hover, chip and tab changes. */
    const val DurationStandard = 260

    /** Sheets and page transitions, where the movement itself carries meaning. */
    const val DurationEmphasized = 420

    val EasingStandard = CubicBezierEasing(0.2f, 0f, 0f, 1f)
    val EasingEmphasizedDecel = CubicBezierEasing(0.05f, 0.7f, 0.1f, 1f)
    val EasingEmphasizedAccel = CubicBezierEasing(0.3f, 0f, 0.8f, 0.15f)
}

/**
 * True when the user has turned animations off system-wide, or disabled background animations in
 * app settings. Infinite transitions (shimmer, focus pulse, carousel auto-advance) must check this
 * — an animation that never ends is the one that actually hurts.
 */
val LocalReduceMotion = staticCompositionLocalOf { false }

/**
 * User/system motion preference for the short-lived controller focus effect. Capability-based
 * background throttling must not silently turn an explicitly enabled focus effect into a static
 * border on TV.
 */
val LocalReduceControllerFocusMotion = staticCompositionLocalOf { false }

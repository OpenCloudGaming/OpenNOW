package com.opencloudgaming.opennow

import android.view.KeyEvent
import kotlinx.serialization.Serializable
import kotlin.math.sqrt

/** Settings for hardware input sent to the cloud host. Touch controls have their own settings. */
@Serializable
data class PhysicalInputSettings(
    val stickDeadZone: Float = 0.15f,
    val triggerDeadZone: Float = 0f,
    val stickSensitivity: Float = 1f,
    val invertStickX: Boolean = false,
    val invertStickY: Boolean = false,
    val invertMouseX: Boolean = false,
    val invertMouseY: Boolean = false,
    val swapAB: Boolean = false,
    val swapXY: Boolean = false,
    val swapWASDAndArrows: Boolean = false,
) {
    fun normalized(): PhysicalInputSettings = copy(
        stickDeadZone = if (stickDeadZone.isFinite()) stickDeadZone.coerceIn(0f, 0.4f) else 0.15f,
        triggerDeadZone = if (triggerDeadZone.isFinite()) triggerDeadZone.coerceIn(0f, 0.4f) else 0f,
        stickSensitivity = if (stickSensitivity.isFinite()) stickSensitivity.coerceIn(0.5f, 2f) else 1f,
    )

    internal fun stickScale(x: Float, y: Float): Float {
        val magnitude = sqrt((x * x + y * y).toDouble()).toFloat()
        if (!magnitude.isFinite() || magnitude <= stickDeadZone) return 0f
        return ((magnitude - stickDeadZone) / (1f - stickDeadZone)).coerceIn(0f, 1f) / magnitude
    }

    internal fun stickAxis(value: Float, scale: Float, inverted: Boolean): Float =
        (value * scale * stickSensitivity * if (inverted) -1f else 1f).coerceIn(-1f, 1f)

    internal fun triggerValue(value: Float): Float {
        if (!value.isFinite()) return 0f
        return ((value.coerceIn(0f, 1f) - triggerDeadZone) / (1f - triggerDeadZone)).coerceIn(0f, 1f)
    }

    internal fun mouseX(delta: Int): Int = if (invertMouseX) invertMouseDelta(delta) else delta

    internal fun mouseY(delta: Int): Int = if (invertMouseY) invertMouseDelta(delta) else delta

    private fun invertMouseDelta(delta: Int): Int = if (delta == Int.MIN_VALUE) Int.MAX_VALUE else -delta

    internal fun buttonMask(mask: Int): Int = when (mask) {
        GamepadButtonMapping.A -> if (swapAB) GamepadButtonMapping.B else mask
        GamepadButtonMapping.B -> if (swapAB) GamepadButtonMapping.A else mask
        GamepadButtonMapping.X -> if (swapXY) GamepadButtonMapping.Y else mask
        GamepadButtonMapping.Y -> if (swapXY) GamepadButtonMapping.X else mask
        else -> mask
    }

    internal fun keyboardKeyCode(keyCode: Int): Int {
        if (!swapWASDAndArrows) return keyCode
        return when (keyCode) {
            KeyEvent.KEYCODE_W -> KeyEvent.KEYCODE_DPAD_UP
            KeyEvent.KEYCODE_A -> KeyEvent.KEYCODE_DPAD_LEFT
            KeyEvent.KEYCODE_S -> KeyEvent.KEYCODE_DPAD_DOWN
            KeyEvent.KEYCODE_D -> KeyEvent.KEYCODE_DPAD_RIGHT
            KeyEvent.KEYCODE_DPAD_UP -> KeyEvent.KEYCODE_W
            KeyEvent.KEYCODE_DPAD_LEFT -> KeyEvent.KEYCODE_A
            KeyEvent.KEYCODE_DPAD_DOWN -> KeyEvent.KEYCODE_S
            KeyEvent.KEYCODE_DPAD_RIGHT -> KeyEvent.KEYCODE_D
            else -> keyCode
        }
    }
}

package com.opencloudgaming.opennow

import android.os.SystemClock
import android.view.InputDevice
import android.view.KeyEvent
import android.view.MotionEvent
import androidx.compose.runtime.getValue
import androidx.compose.runtime.mutableStateOf
import androidx.compose.runtime.setValue
import androidx.compose.runtime.staticCompositionLocalOf
import kotlin.math.abs

internal val LocalMascotActivity = staticCompositionLocalOf<MascotActivityTracker?> { null }

/** Observes Activity input without consuming it or publishing every move into Compose state. */
internal class MascotActivityTracker {
    private val overlayInstances = LinkedHashSet<Any>()
    var overlayOwner: Any? by mutableStateOf(null)
        private set
    var enabled = false
    var visible by mutableStateOf(false)
    var resumed by mutableStateOf(false)
        private set
    var focused by mutableStateOf(false)
        private set
    var lastInputMillis = 0L
        private set
    private val heldKeys = mutableSetOf<Int>()
    private var touching = false
    private var joystickActive = false
    val inputHeld: Boolean get() = touching || joystickActive || heldKeys.isNotEmpty()

    /**
     * Keeps transient duplicate compositions (for example, during a screen transition) from
     * rendering independent mascots. Ownership passes to the next live composition when needed.
     */
    fun attachOverlay(instance: Any) {
        if (!overlayInstances.add(instance)) return
        if (overlayOwner == null) overlayOwner = instance
    }

    fun detachOverlay(instance: Any) {
        if (!overlayInstances.remove(instance)) return
        if (overlayOwner === instance) overlayOwner = overlayInstances.firstOrNull()
    }

    fun recordInput() {
        lastInputMillis = SystemClock.uptimeMillis()
        visible = false
    }

    fun reset() {
        heldKeys.clear()
        touching = false
        joystickActive = false
        recordInput()
    }

    fun updateResumed(value: Boolean) {
        resumed = value
        reset()
    }

    fun updateFocused(value: Boolean) {
        focused = value
        reset()
    }

    fun onKey(event: KeyEvent) {
        if (!enabled) return
        if (event.action == KeyEvent.ACTION_DOWN) heldKeys.add(event.keyCode)
        if (event.action == KeyEvent.ACTION_UP) heldKeys.remove(event.keyCode)
        recordInput()
    }

    fun onTouch(event: MotionEvent) {
        if (!enabled) return
        touching = event.actionMasked != MotionEvent.ACTION_UP && event.actionMasked != MotionEvent.ACTION_CANCEL
        recordInput()
    }

    fun onMotion(event: MotionEvent) {
        if (!enabled) return
        if (event.isFromSource(InputDevice.SOURCE_JOYSTICK)) {
            // Resting sticks may keep producing noise. Match meaningful controller motion only.
            val wasActive = joystickActive
            joystickActive = JOYSTICK_AXES.any { abs(event.getAxisValue(it)) > 0.25f }
            if (!joystickActive && !wasActive) return
        }
        recordInput()
    }

    private companion object {
        val JOYSTICK_AXES = intArrayOf(
            MotionEvent.AXIS_X, MotionEvent.AXIS_Y, MotionEvent.AXIS_Z, MotionEvent.AXIS_RZ,
            MotionEvent.AXIS_HAT_X, MotionEvent.AXIS_HAT_Y,
            MotionEvent.AXIS_LTRIGGER, MotionEvent.AXIS_RTRIGGER,
        )
    }
}

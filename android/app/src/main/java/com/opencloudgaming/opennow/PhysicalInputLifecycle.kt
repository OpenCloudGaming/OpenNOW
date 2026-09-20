package com.opencloudgaming.opennow

/**
 * Tracks physical key/button presses that the cloud host accepted locally. Android desktop mode
 * can move focus before delivering the matching UP event (notably for Alt+Tab), so callers must
 * take and release this state when the stream window loses focus.
 */
internal class ForwardedPhysicalInputState {
    private data class KeyIdentity(
        val deviceId: Int,
        val keyCode: Int,
        val scanCode: Int,
    )

    data class ReleaseSnapshot(
        val keys: List<InputEncoder.KeyboardPayload>,
        val mouseButtons: List<Int>,
    ) {
        val isEmpty: Boolean
            get() = keys.isEmpty() && mouseButtons.isEmpty()
    }

    private val lock = Any()
    private val pressedKeys = linkedMapOf<KeyIdentity, InputEncoder.KeyboardPayload>()
    private val pressedMouseButtons = linkedSetOf<Int>()

    fun recordKey(
        deviceId: Int,
        keyCode: Int,
        scanCode: Int,
        payload: InputEncoder.KeyboardPayload,
        pressed: Boolean,
        sent: Boolean,
    ) {
        val identity = KeyIdentity(deviceId, keyCode, scanCode)
        synchronized(lock) {
            if (pressed) {
                if (sent) pressedKeys[identity] = payload
            } else {
                // Forget the local press even if the release raced a closed channel. Carrying it
                // into a replacement transport would release input in the wrong cloud session.
                pressedKeys.remove(identity)
            }
            Unit
        }
    }

    fun recordMouseButton(button: Int, pressed: Boolean, sent: Boolean) {
        synchronized(lock) {
            if (pressed) {
                if (sent) pressedMouseButtons += button
            } else {
                pressedMouseButtons -= button
            }
            Unit
        }
    }

    fun isMouseButtonPressed(button: Int): Boolean = synchronized(lock) {
        button in pressedMouseButtons
    }

    fun takeReleaseSnapshot(): ReleaseSnapshot = synchronized(lock) {
        ReleaseSnapshot(
            keys = pressedKeys.values.toList().asReversed(),
            mouseButtons = pressedMouseButtons.toList().asReversed(),
        ).also {
            pressedKeys.clear()
            pressedMouseButtons.clear()
        }
    }

    fun reset() {
        synchronized(lock) {
            pressedKeys.clear()
            pressedMouseButtons.clear()
        }
    }
}

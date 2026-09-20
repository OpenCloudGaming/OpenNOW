package com.opencloudgaming.opennow

import org.junit.Assert.assertEquals
import org.junit.Assert.assertFalse
import org.junit.Assert.assertTrue
import org.junit.Test

class PhysicalInputLifecycleTest {
    @Test
    fun exposesForwardedMouseButtonStateForDuplicateEdgeSuppression() {
        val state = ForwardedPhysicalInputState()

        assertFalse(state.isMouseButtonPressed(3))
        state.recordMouseButton(button = 3, pressed = true, sent = true)
        assertTrue(state.isMouseButtonPressed(3))
        state.recordMouseButton(button = 3, pressed = false, sent = true)
        assertFalse(state.isMouseButtonPressed(3))
    }

    @Test
    fun snapshotsOnlySuccessfullyForwardedPressedInput() {
        val state = ForwardedPhysicalInputState()
        val alt = InputEncoder.KeyboardPayload(keycode = 0x12, scancode = 0x38, modifiers = 0x04, timestampUs = 1)
        val w = InputEncoder.KeyboardPayload(keycode = 0x57, scancode = 0x11, modifiers = 0, timestampUs = 2)

        state.recordKey(deviceId = 12, keyCode = 57, scanCode = 56, payload = alt, pressed = true, sent = true)
        state.recordKey(deviceId = 12, keyCode = 51, scanCode = 17, payload = w, pressed = true, sent = false)
        state.recordMouseButton(button = 1, pressed = true, sent = true)
        state.recordMouseButton(button = 3, pressed = true, sent = false)

        val snapshot = state.takeReleaseSnapshot()

        assertEquals(listOf(alt), snapshot.keys)
        assertEquals(listOf(1), snapshot.mouseButtons)
        assertTrue(state.takeReleaseSnapshot().isEmpty)
    }

    @Test
    fun matchingUpRemovesInputBeforeFocusLoss() {
        val state = ForwardedPhysicalInputState()
        val alt = InputEncoder.KeyboardPayload(keycode = 0x12, scancode = 0x38, modifiers = 0x04, timestampUs = 1)

        state.recordKey(deviceId = 12, keyCode = 57, scanCode = 56, payload = alt, pressed = true, sent = true)
        state.recordKey(deviceId = 12, keyCode = 57, scanCode = 56, payload = alt, pressed = false, sent = true)
        state.recordMouseButton(button = 1, pressed = true, sent = true)
        state.recordMouseButton(button = 1, pressed = false, sent = false)

        assertTrue(state.takeReleaseSnapshot().isEmpty)
    }
}

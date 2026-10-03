package com.opencloudgaming.opennow

import android.view.KeyEvent
import kotlinx.serialization.encodeToString
import org.junit.Assert.assertEquals
import org.junit.Test

class PhysicalInputSettingsTest {
    @Test
    fun `default profile preserves the existing physical controller behavior`() {
        val profile = PhysicalInputSettings()
        assertEquals(0.15f, profile.stickDeadZone, 0f)
        assertEquals(0f, profile.stickScale(0.1f, 0f), 0f)
        assertEquals(1f, profile.stickScale(1f, 0f), 0f)
        assertEquals(GamepadButtonMapping.A, profile.buttonMask(GamepadButtonMapping.A))
        assertEquals(KeyEvent.KEYCODE_W, profile.keyboardKeyCode(KeyEvent.KEYCODE_W))
    }

    @Test
    fun `dead zone rescales both sticks smoothly`() {
        val profile = PhysicalInputSettings(stickDeadZone = 0.2f)
        assertEquals(0f, profile.stickScale(0.2f, 0f), 0f)
        assertEquals(0.5f, profile.stickScale(0.6f, 0f) * 0.6f, 0.0001f)
        assertEquals(0f, profile.stickScale(Float.NaN, 0f), 0f)
    }

    @Test
    fun `trigger dead zone and stick response preserve endpoints`() {
        val profile = PhysicalInputSettings(
            triggerDeadZone = 0.2f,
            stickSensitivity = 1.5f,
            invertStickX = true,
            invertStickY = false,
        )
        assertEquals(0f, profile.triggerValue(0.1f), 0f)
        assertEquals(0.5f, profile.triggerValue(0.6f), 0.0001f)
        assertEquals(1f, profile.triggerValue(1f), 0f)
        assertEquals(0f, profile.triggerValue(Float.NaN), 0f)
        assertEquals(-0.75f, profile.stickAxis(0.5f, 1f, profile.invertStickX), 0f)
        assertEquals(0.75f, profile.stickAxis(0.5f, 1f, profile.invertStickY), 0f)
        assertEquals(-1f, profile.stickAxis(1f, 1f, profile.invertStickX), 0f)
    }

    @Test
    fun `button and keyboard swaps are symmetric and leave unrelated input alone`() {
        val profile = PhysicalInputSettings(swapAB = true, swapXY = true, swapWASDAndArrows = true)
        assertEquals(GamepadButtonMapping.B, profile.buttonMask(GamepadButtonMapping.A))
        assertEquals(GamepadButtonMapping.A, profile.buttonMask(GamepadButtonMapping.B))
        assertEquals(GamepadButtonMapping.Y, profile.buttonMask(GamepadButtonMapping.X))
        assertEquals(GamepadButtonMapping.X, profile.buttonMask(GamepadButtonMapping.Y))
        assertEquals(GamepadButtonMapping.START, profile.buttonMask(GamepadButtonMapping.START))
        assertEquals(KeyEvent.KEYCODE_DPAD_UP, profile.keyboardKeyCode(KeyEvent.KEYCODE_W))
        assertEquals(KeyEvent.KEYCODE_W, profile.keyboardKeyCode(KeyEvent.KEYCODE_DPAD_UP))
        assertEquals(KeyEvent.KEYCODE_SPACE, profile.keyboardKeyCode(KeyEvent.KEYCODE_SPACE))
    }

    @Test
    fun `mouse inversion is independent per axis`() {
        val profile = PhysicalInputSettings(invertMouseX = true)
        assertEquals(-12, profile.mouseX(12))
        assertEquals(7, profile.mouseY(7))
        assertEquals(-7, profile.copy(invertMouseY = true).mouseY(7))
        assertEquals(Int.MAX_VALUE, profile.mouseX(Int.MIN_VALUE))
    }

    @Test
    fun `persisted settings normalize invalid dead zones`() {
        assertEquals(0.15f, PhysicalInputSettings(stickDeadZone = Float.NaN).normalized().stickDeadZone, 0f)
        assertEquals(0.4f, PhysicalInputSettings(stickDeadZone = 5f).normalized().stickDeadZone, 0f)
        assertEquals(0f, PhysicalInputSettings(triggerDeadZone = Float.POSITIVE_INFINITY).normalized().triggerDeadZone, 0f)
        assertEquals(2f, PhysicalInputSettings(stickSensitivity = 5f).normalized().stickSensitivity, 0f)
        val saved = AppSettings(physicalInput = PhysicalInputSettings(stickDeadZone = 0.25f, swapAB = true, swapWASDAndArrows = true))
        assertEquals(saved, OpenNowJson.decodeFromString<AppSettings>(OpenNowJson.encodeToString(saved)))
        assertEquals(PhysicalInputSettings(), OpenNowJson.decodeFromString<AppSettings>("{}").physicalInput)
    }
}

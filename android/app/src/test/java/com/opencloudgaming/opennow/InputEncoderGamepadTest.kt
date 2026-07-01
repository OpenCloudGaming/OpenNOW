package com.opencloudgaming.opennow

import android.view.InputDevice
import android.view.KeyEvent
import java.nio.ByteBuffer
import java.nio.ByteOrder
import org.junit.Assert.assertEquals
import org.junit.Assert.assertFalse
import org.junit.Assert.assertNull
import org.junit.Assert.assertTrue
import org.junit.Test

class InputEncoderGamepadTest {
    @Test
    fun encodesGuideButtonMaskForSteamOverlay() {
        val encoder = InputEncoder().apply { setProtocolVersion(2) }
        val payload = encoder.encodeGamepadState(
            controllerId = 0,
            buttons = 0x0400,
            leftTrigger = 0,
            rightTrigger = 0,
            leftStickX = 0,
            leftStickY = 0,
            rightStickX = 0,
            rightStickY = 0,
            bitmap = 0x0101,
            partiallyReliable = false,
            timestampUs = 0L,
        )
        val bytes = ByteBuffer.wrap(payload).order(ByteOrder.LITTLE_ENDIAN)

        assertEquals(InputEncoder.INPUT_GAMEPAD, bytes.getInt(0))
        assertEquals(0x0400, bytes.getShort(12).toInt() and 0xffff)
    }

    @Test
    fun mapsAndroidGamepadButtonsToXinputMasks() {
        assertEquals(GamepadButtonMapping.GUIDE, GamepadButtonMapping.maskForKeyCode(KeyEvent.KEYCODE_BUTTON_MODE))
        assertEquals(GamepadButtonMapping.START, GamepadButtonMapping.maskForKeyCode(KeyEvent.KEYCODE_BUTTON_START))
        assertEquals(GamepadButtonMapping.BACK, GamepadButtonMapping.maskForKeyCode(KeyEvent.KEYCODE_BUTTON_SELECT))
        assertEquals(GamepadButtonMapping.LEFT_THUMB, GamepadButtonMapping.maskForKeyCode(KeyEvent.KEYCODE_BUTTON_THUMBL))
        assertEquals(GamepadButtonMapping.RIGHT_THUMB, GamepadButtonMapping.maskForKeyCode(KeyEvent.KEYCODE_BUTTON_THUMBR))
    }

    @Test
    fun classifiesControllerButtonKeyCodesWithoutDependingOnEventSource() {
        assertTrue(GamepadButtonMapping.isControllerButtonKeyCode(KeyEvent.KEYCODE_BUTTON_MODE))
        assertTrue(GamepadButtonMapping.isControllerButtonKeyCode(KeyEvent.KEYCODE_BUTTON_START))
        assertTrue(GamepadButtonMapping.isControllerButtonKeyCode(KeyEvent.KEYCODE_BUTTON_SELECT))
        assertTrue(GamepadButtonMapping.isControllerButtonKeyCode(KeyEvent.KEYCODE_BUTTON_L2))
        assertTrue(GamepadButtonMapping.isControllerButtonKeyCode(KeyEvent.KEYCODE_BUTTON_R2))
        assertFalse(GamepadButtonMapping.isControllerButtonKeyCode(KeyEvent.KEYCODE_DPAD_CENTER))
        assertFalse(GamepadButtonMapping.isControllerButtonKeyCode(KeyEvent.KEYCODE_ENTER))
    }

    @Test
    fun detectsControllerCapableAndroidSources() {
        assertTrue(AndroidControllerInput.hasControllerSource(InputDevice.SOURCE_GAMEPAD or InputDevice.SOURCE_DPAD))
        assertTrue(AndroidControllerInput.hasControllerSource(InputDevice.SOURCE_JOYSTICK or InputDevice.SOURCE_DPAD))
        assertFalse(AndroidControllerInput.hasControllerSource(InputDevice.SOURCE_DPAD))
        assertFalse(AndroidControllerInput.hasControllerSource(InputDevice.SOURCE_KEYBOARD or InputDevice.SOURCE_DPAD))
    }

    @Test
    fun recognizesStadiaControllerNamesWithDpadOnlySources() {
        assertTrue(AndroidControllerInput.isKnownControllerName("Stadia Controller rev. A"))
        assertTrue(AndroidControllerInput.isKnownControllerName("Google Stadia Controller"))
        assertTrue(AndroidControllerInput.isControllerDevice(InputDevice.SOURCE_DPAD, "Stadia Controller"))
        assertTrue(AndroidControllerInput.isControllerDevice(InputDevice.SOURCE_DPAD, "DualSense Wireless Controller"))
        assertTrue(AndroidControllerInput.isControllerDevice(InputDevice.SOURCE_DPAD, "Xbox Wireless Controller"))
        assertFalse(AndroidControllerInput.isControllerDevice(InputDevice.SOURCE_DPAD, "TV Remote"))
    }

    @Test
    fun resolvesHatOnlyControllerMotionAsLeftStick() {
        val axes = AndroidGamepadAxisMapping.resolve(
            raw = AndroidGamepadRawAxes(hatX = -1f, hatY = 0.75f),
            available = AndroidGamepadAxisAvailability(
                x = false,
                y = false,
                z = false,
                rz = false,
                rx = false,
                ry = false,
                hatX = true,
                hatY = true,
            ),
        )

        assertEquals(-1f, axes.leftX, 0.0001f)
        assertEquals(0.75f, axes.leftY, 0.0001f)
        assertEquals("hat", axes.leftSource)
        assertTrue(axes.hatUsedAsLeftStick)
    }

    @Test
    fun keepsStandardControllerAxesOnExpectedSticks() {
        val axes = AndroidGamepadAxisMapping.resolve(
            AndroidGamepadRawAxes(x = 0.5f, y = -0.25f, z = 0.6f, rz = -0.7f, rx = -0.2f, ry = 0.1f),
        )

        assertEquals(0.5f, axes.leftX, 0.0001f)
        assertEquals(-0.25f, axes.leftY, 0.0001f)
        assertEquals(0.6f, axes.rightX, 0.0001f)
        assertEquals(-0.7f, axes.rightY, 0.0001f)
        assertEquals("x/y", axes.leftSource)
        assertEquals("z/rz", axes.rightSource)
        assertFalse(axes.hatUsedAsLeftStick)
    }

    @Test
    fun mapsControllerActivationKeysToPrimaryGamepadButtonOnlyForControllers() {
        assertEquals(
            GamepadButtonMapping.A,
            GamepadButtonMapping.maskForKeyCode(KeyEvent.KEYCODE_DPAD_CENTER, controllerActivation = true),
        )
        assertEquals(
            GamepadButtonMapping.A,
            GamepadButtonMapping.maskForKeyCode(KeyEvent.KEYCODE_ENTER, controllerActivation = true),
        )
        assertNull(GamepadButtonMapping.maskForKeyCode(KeyEvent.KEYCODE_DPAD_CENTER))
        assertNull(GamepadButtonMapping.maskForKeyCode(KeyEvent.KEYCODE_ENTER))
    }

    @Test
    fun normalizesControllerAForNativeUiActivation() {
        assertEquals(
            KeyEvent.KEYCODE_DPAD_CENTER,
            NativeStreamInputRouter.normalizedAppUiKeyCode(KeyEvent.KEYCODE_BUTTON_A, streamUiActive = false),
        )
    }

    @Test
    fun consumesControllerBAsNativeUiBackNavigation() {
        assertTrue(
            NativeStreamInputRouter.isControllerAppBackKey(
                keyCode = KeyEvent.KEYCODE_BUTTON_B,
                controllerSource = false,
                streamUiActive = false,
            ),
        )
    }

    @Test
    fun clampsStreamSharpnessShaderStrength() {
        assertEquals(0f, streamSharpnessShaderStrength(enabled = false, amount = 1f), 0.0001f)
        assertEquals(0f, streamSharpnessShaderStrength(enabled = true, amount = -1f), 0.0001f)
        assertEquals(0.28f, streamSharpnessShaderStrength(enabled = true, amount = 2f), 0.0001f)
    }
}

package com.opencloudgaming.opennow

import android.view.KeyEvent
import org.junit.Assert.assertEquals
import org.junit.Assert.assertNotNull
import org.junit.Test

class InputEncoderKeyboardTest {
    @Test
    fun mapsNumberRowKeysWhenAndroidReportsNoScanCode() {
        val one = InputEncoder.mapKeyboardPayload(keyCode = KeyEvent.KEYCODE_1, unicode = 0, scanCode = 0, timestampUs = 0L)
        val zero = InputEncoder.mapKeyboardPayload(keyCode = KeyEvent.KEYCODE_0, unicode = 0, scanCode = 0, timestampUs = 0L)

        assertNotNull(one)
        assertEquals(0x31, one?.keycode)
        assertEquals(0x0002, one?.scancode)
        assertNotNull(zero)
        assertEquals(0x30, zero?.keycode)
        assertEquals(0x000b, zero?.scancode)
    }

    @Test
    fun mapsNumpadDigitsWhenAndroidReportsNoScanCode() {
        val numpadOne = InputEncoder.mapKeyboardPayload(keyCode = KeyEvent.KEYCODE_NUMPAD_1, unicode = 0, scanCode = 0, timestampUs = 0L)
        val numpadZero = InputEncoder.mapKeyboardPayload(keyCode = KeyEvent.KEYCODE_NUMPAD_0, unicode = 0, scanCode = 0, timestampUs = 0L)

        assertNotNull(numpadOne)
        assertEquals(0x61, numpadOne?.keycode)
        assertEquals(0x004f, numpadOne?.scancode)
        assertNotNull(numpadZero)
        assertEquals(0x60, numpadZero?.keycode)
        assertEquals(0x0052, numpadZero?.scancode)
    }

}

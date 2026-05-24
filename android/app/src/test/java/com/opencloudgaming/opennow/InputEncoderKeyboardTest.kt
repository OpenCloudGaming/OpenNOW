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

    @Test
    fun mapsOverlayTextCharactersLikeDesktopTextInput() {
        val upperD = InputEncoder.mapTextCharToKeySpec('D')
        val lowerA = InputEncoder.mapTextCharToKeySpec('a')
        val space = InputEncoder.mapTextCharToKeySpec(' ')
        val colon = InputEncoder.mapTextCharToKeySpec(':')

        assertNotNull(upperD)
        assertEquals(0x44, upperD?.keycode)
        assertEquals(0x0020, upperD?.scancode)
        assertEquals(true, upperD?.shift)
        assertNotNull(lowerA)
        assertEquals(0x41, lowerA?.keycode)
        assertEquals(0x001e, lowerA?.scancode)
        assertEquals(false, lowerA?.shift)
        assertNotNull(space)
        assertEquals(0x20, space?.keycode)
        assertEquals(0x0039, space?.scancode)
        assertNotNull(colon)
        assertEquals(0xba, colon?.keycode)
        assertEquals(0x0027, colon?.scancode)
        assertEquals(true, colon?.shift)
    }

}

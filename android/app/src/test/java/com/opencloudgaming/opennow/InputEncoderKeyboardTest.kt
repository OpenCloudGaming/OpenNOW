package com.opencloudgaming.opennow

import android.view.KeyEvent
import org.junit.Assert.assertArrayEquals
import org.junit.Assert.assertEquals
import org.junit.Assert.assertNotNull
import org.junit.Test

class InputEncoderKeyboardTest {
    @Test
    fun mapsPhysicalSpaceToCanonicalHostKeyDespiteOemScanCode() {
        val space = InputEncoder.mapKeyboardPayload(
            keyCode = KeyEvent.KEYCODE_SPACE,
            unicode = ' '.code,
            scanCode = 0x123,
            timestampUs = 0L,
        )

        assertNotNull(space)
        assertEquals(0x20, space?.keycode)
        assertEquals(0x0039, space?.scancode)
    }

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
    fun mapsOverlayEscapeWhenAndroidReportsNoScanCode() {
        val escape = InputEncoder.mapKeyboardPayload(
            keyCode = KeyEvent.KEYCODE_ESCAPE,
            unicode = 0,
            scanCode = 0,
            timestampUs = 0L,
        )

        assertNotNull(escape)
        assertEquals(0x1b, escape?.keycode)
        assertEquals(0x0001, escape?.scancode)
    }

    @Test
    fun mapsOverlayBackspaceUsedByClearWhenAndroidReportsNoScanCode() {
        val backspace = InputEncoder.mapKeyboardPayload(
            keyCode = KeyEvent.KEYCODE_DEL,
            unicode = 0,
            scanCode = 0,
            timestampUs = 0L,
        )

        assertNotNull(backspace)
        assertEquals(0x08, backspace?.keycode)
        assertEquals(0x000e, backspace?.scancode)
    }

    @Test
    fun mapsOverlayTextCharactersLikeDesktopTextInput() {
        val upperD = InputEncoder.mapTextCharToKeySpec('D')
        val lowerA = InputEncoder.mapTextCharToKeySpec('a')
        val space = InputEncoder.mapTextCharToKeySpec(' ')
        val colon = InputEncoder.mapTextCharToKeySpec(':')
        val atSign = InputEncoder.mapTextCharToKeySpec('@')

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
        assertNotNull(atSign)
        assertEquals(0x32, atSign?.keycode)
        assertEquals(0x0003, atSign?.scancode)
        assertEquals(true, atSign?.shift)
    }

    @Test
    fun encodesUnicodeTextWithOfficialSendUnicodeFraming() {
        val packet = InputEncoder().encodeTextInput("язык 🙂 ß").single()

        assertEquals(0x22, packet[0].toInt())
        assertEquals(
            InputEncoder.INPUT_TEXT,
            java.nio.ByteBuffer.wrap(packet).order(java.nio.ByteOrder.LITTLE_ENDIAN).getInt(1),
        )
        assertArrayEquals("язык 🙂 ß".toByteArray(Charsets.UTF_8), packet.copyOfRange(5, packet.size))
    }

    @Test
    fun chunksUnicodeTextWithoutSplittingUtf8Characters() {
        val packets = InputEncoder().encodeTextInput("a".repeat(1015) + "🙂b")

        assertEquals(2, packets.size)
        assertEquals("a".repeat(1015), packets[0].copyOfRange(5, packets[0].size).toString(Charsets.UTF_8))
        assertEquals("🙂b", packets[1].copyOfRange(5, packets[1].size).toString(Charsets.UTF_8))
    }

}

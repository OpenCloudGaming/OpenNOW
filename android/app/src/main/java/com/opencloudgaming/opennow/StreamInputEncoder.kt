package com.opencloudgaming.opennow

import android.os.SystemClock
import android.view.KeyEvent
import kotlinx.coroutines.cancel
import org.webrtc.AudioTrack
import org.webrtc.RtpSender
import java.nio.ByteBuffer
import java.nio.ByteOrder

class InputEncoder {
    private var protocolVersion = 3
    private val gamepadSequences = mutableMapOf<Int, Int>()

    fun setProtocolVersion(version: Int) {
        protocolVersion = version.coerceAtLeast(1)
    }

    fun resetGamepadSequences() {
        gamepadSequences.clear()
    }

    fun encodeHeartbeat(): ByteArray = ByteBuffer.allocate(4).order(ByteOrder.LITTLE_ENDIAN).putInt(INPUT_HEARTBEAT).array()

    fun encodeKeyDown(key: KeyboardPayload): ByteArray = encodeKey(INPUT_KEY_DOWN, key)
    fun encodeKeyUp(key: KeyboardPayload): ByteArray = encodeKey(INPUT_KEY_UP, key)

    fun encodeMouseMove(dx: Int, dy: Int): ByteArray {
        val bytes = ByteArray(22)
        ByteBuffer.wrap(bytes).order(ByteOrder.LITTLE_ENDIAN).putInt(INPUT_MOUSE_REL)
        ByteBuffer.wrap(bytes).order(ByteOrder.BIG_ENDIAN)
            .putShort(4, dx.coerceIn(-32768, 32767).toShort())
            .putShort(6, dy.coerceIn(-32768, 32767).toShort())
            .putShort(8, 0.toShort())
            .putInt(10, 0)
            .putLong(14, timestampUs())
        return wrapMouseMove(bytes)
    }

    /** Match desktop's i16 splitting so coalesced fast motion cannot be clipped on the wire. */
    internal fun encodeMouseMoves(dx: Int, dy: Int): Sequence<ByteArray> = sequence {
        var remainingX = dx
        var remainingY = dy
        while (remainingX != 0 || remainingY != 0) {
            val x = remainingX.coerceIn(Short.MIN_VALUE.toInt(), Short.MAX_VALUE.toInt())
            val y = remainingY.coerceIn(Short.MIN_VALUE.toInt(), Short.MAX_VALUE.toInt())
            yield(encodeMouseMove(x, y))
            remainingX -= x
            remainingY -= y
        }
    }

    /** Absolute host cursor position, matching the desktop GFN local-cursor encoder. */
    fun encodeMouseAbsolute(x: Int, y: Int, width: Int, height: Int): ByteArray {
        val bytes = ByteArray(26)
        ByteBuffer.wrap(bytes).order(ByteOrder.LITTLE_ENDIAN).putInt(INPUT_MOUSE_ABS)
        ByteBuffer.wrap(bytes).order(ByteOrder.BIG_ENDIAN)
            .putShort(4, x.coerceIn(0, 65535).toShort())
            .putShort(6, y.coerceIn(0, 65535).toShort())
            .putShort(8, 0.toShort())
            .putShort(10, width.coerceIn(1, 65535).toShort())
            .putShort(12, height.coerceIn(1, 65535).toShort())
            .putInt(14, 0)
            .putLong(18, timestampUs())
        return wrapMouseMove(bytes)
    }

    fun encodeMouseButton(type: Int, button: Int): ByteArray {
        val bytes = ByteArray(18)
        ByteBuffer.wrap(bytes).order(ByteOrder.LITTLE_ENDIAN).putInt(type)
        bytes[4] = button.coerceIn(1, 5).toByte()
        bytes[5] = 0
        ByteBuffer.wrap(bytes).order(ByteOrder.BIG_ENDIAN).putInt(6, 0).putLong(10, timestampUs())
        return wrapSingle(bytes)
    }

    fun encodeMouseWheel(delta: Int): ByteArray {
        val bytes = ByteArray(22)
        ByteBuffer.wrap(bytes).order(ByteOrder.LITTLE_ENDIAN).putInt(INPUT_MOUSE_WHEEL)
        ByteBuffer.wrap(bytes).order(ByteOrder.BIG_ENDIAN)
            .putShort(4, 0.toShort())
            .putShort(6, delta.coerceIn(-32768, 32767).toShort())
            .putShort(8, 0.toShort())
            .putInt(10, 0)
            .putLong(14, timestampUs())
        return wrapSingle(bytes)
    }

    /**
     * A batch of finger updates, one packet per input event.
     *
     * Layout, taken from the official web client's encoder. Note the opcode is little-endian while
     * everything after it is big-endian — the same split every other packet here uses.
     *
     * ```
     * 0..3    opcode 24            uint32 LE
     * 4..5    payload size         uint16 BE   = 8 + 16 * count
     * 6..7    count                uint16 BE
     * 8+      records, 16 bytes each:
     *           +0     slot        uint8
     *           +1     phase       uint8       1=down 2=up 4=move 8=cancel
     *           +2..3  x           uint16 BE   0..65535 across the video area
     *           +4..5  y           uint16 BE
     *           +6     radiusX     uint8
     *           +7     radiusY     uint8
     *           +8..15 timestamp   int64 BE    microseconds
     * ```
     *
     * Returns null for an empty batch so callers cannot send a header describing nothing.
     */
    internal fun encodeTouchBatch(touches: List<TouchRecord>, nowUs: Long = timestampUs()): ByteArray? {
        if (touches.isEmpty()) return null
        val count = minOf(touches.size, MAX_TOUCH_RECORDS_PER_BATCH)
        val payloadSize = 8 + 16 * count
        val bytes = ByteArray(payloadSize)
        ByteBuffer.wrap(bytes).order(ByteOrder.LITTLE_ENDIAN).putInt(INPUT_TOUCH)
        val be = ByteBuffer.wrap(bytes).order(ByteOrder.BIG_ENDIAN)
        be.putShort(4, payloadSize.toShort())
        be.putShort(6, count.toShort())
        for (index in 0 until count) {
            val touch = touches[index]
            val offset = 8 + 16 * index
            bytes[offset] = touch.slot.toByte()
            bytes[offset + 1] = touch.phase.toByte()
            be.putShort(offset + 2, touch.x.coerceIn(0, TOUCH_COORDINATE_MAX).toShort())
            be.putShort(offset + 4, touch.y.coerceIn(0, TOUCH_COORDINATE_MAX).toShort())
            bytes[offset + 6] = touch.radiusX.coerceIn(0, 255).toByte()
            bytes[offset + 7] = touch.radiusY.coerceIn(0, 255).toByte()
            // 0 means "stamp it here", so the router does not have to reach for the same clock
            // every other packet in this encoder uses.
            be.putLong(offset + 8, if (touch.timestampUs != 0L) touch.timestampUs else nowUs)
        }
        return wrapSingle(bytes, nowUs)
    }

    fun encodeHapticsEnabled(enabled: Boolean): ByteArray {
        val bytes = ByteArray(6)
        ByteBuffer.wrap(bytes).order(ByteOrder.LITTLE_ENDIAN).putInt(INPUT_HAPTICS_ENABLED)
        ByteBuffer.wrap(bytes).order(ByteOrder.BIG_ENDIAN).putShort(4, (if (enabled) 1 else 0).toShort())
        return wrapSingle(bytes)
    }

    /** Official GFN SendUnicode framing. These packets are already single-message framed. */
    fun encodeTextInput(text: String): List<ByteArray> {
        val utf8 = text.toByteArray(Charsets.UTF_8)
        val chunks = mutableListOf<ByteArray>()
        var offset = 0
        while (offset < utf8.size) {
            val chunkLength = textInputChunkLength(utf8, offset)
            if (chunkLength <= 0) break
            val bytes = ByteArray(TEXT_INPUT_HEADER_BYTES + chunkLength)
            bytes[0] = 0x22
            ByteBuffer.wrap(bytes).order(ByteOrder.LITTLE_ENDIAN).putInt(1, INPUT_TEXT)
            utf8.copyInto(
                destination = bytes,
                destinationOffset = TEXT_INPUT_HEADER_BYTES,
                startIndex = offset,
                endIndex = offset + chunkLength,
            )
            chunks += bytes
            offset += chunkLength
        }
        return chunks
    }

    fun encodeGamepadState(
        controllerId: Int,
        buttons: Int,
        leftTrigger: Int,
        rightTrigger: Int,
        leftStickX: Int,
        leftStickY: Int,
        rightStickX: Int,
        rightStickY: Int,
        bitmap: Int,
        partiallyReliable: Boolean,
        timestampUs: Long = timestampUs(),
    ): ByteArray {
        val bytes = ByteArray(38)
        val le = ByteBuffer.wrap(bytes).order(ByteOrder.LITTLE_ENDIAN)
        le.putInt(0, INPUT_GAMEPAD)
        le.putShort(4, 26.toShort())
        le.putShort(6, (controllerId and 0x03).toShort())
        le.putShort(8, bitmap.toShort())
        le.putShort(10, 20.toShort())
        le.putShort(12, buttons.toShort())
        le.putShort(14, ((leftTrigger and 0xff) or ((rightTrigger and 0xff) shl 8)).toShort())
        le.putShort(16, leftStickX.toShort())
        le.putShort(18, leftStickY.toShort())
        le.putShort(20, rightStickX.toShort())
        le.putShort(22, rightStickY.toShort())
        le.putShort(24, 0.toShort())
        le.putShort(26, 85.toShort())
        le.putShort(28, 0.toShort())
        le.putLong(30, timestampUs)
        return if (partiallyReliable) wrapGamepadPartiallyReliable(bytes, controllerId) else wrapGamepadReliable(bytes)
    }

    private fun encodeKey(type: Int, key: KeyboardPayload): ByteArray {
        val bytes = ByteArray(18)
        ByteBuffer.wrap(bytes).order(ByteOrder.LITTLE_ENDIAN).putInt(type)
        ByteBuffer.wrap(bytes).order(ByteOrder.BIG_ENDIAN)
            .putShort(4, key.keycode.toShort())
            .putShort(6, key.modifiers.toShort())
            .putShort(8, key.scancode.toShort())
            .putLong(10, key.timestampUs)
        return wrapSingle(bytes)
    }

    private fun wrapSingle(payload: ByteArray, nowUs: Long = timestampUs()): ByteArray {
        if (protocolVersion <= 2) return payload
        return ByteArray(10 + payload.size).also {
            it[0] = 0x23
            ByteBuffer.wrap(it).order(ByteOrder.BIG_ENDIAN).putLong(1, nowUs)
            it[9] = 0x22
            payload.copyInto(it, 10)
        }
    }

    private fun wrapMouseMove(payload: ByteArray): ByteArray {
        if (protocolVersion <= 2) return payload
        return ByteArray(12 + payload.size).also {
            it[0] = 0x23
            ByteBuffer.wrap(it).order(ByteOrder.BIG_ENDIAN).putLong(1, timestampUs())
            it[9] = 0x21
            ByteBuffer.wrap(it).order(ByteOrder.BIG_ENDIAN).putShort(10, payload.size.toShort())
            payload.copyInto(it, 12)
        }
    }

    private fun wrapGamepadReliable(payload: ByteArray): ByteArray {
        if (protocolVersion <= 2) return payload
        return ByteArray(12 + payload.size).also {
            it[0] = 0x23
            ByteBuffer.wrap(it).order(ByteOrder.BIG_ENDIAN).putLong(1, timestampUs())
            it[9] = 0x21
            ByteBuffer.wrap(it).order(ByteOrder.BIG_ENDIAN).putShort(10, payload.size.toShort())
            payload.copyInto(it, 12)
        }
    }

    private fun wrapGamepadPartiallyReliable(payload: ByteArray, index: Int): ByteArray {
        if (protocolVersion <= 2) return payload
        val seq = gamepadSequences[index] ?: 1
        gamepadSequences[index] = (seq + 1) and 0xffff
        return ByteArray(16 + payload.size).also {
            it[0] = 0x23
            val be = ByteBuffer.wrap(it).order(ByteOrder.BIG_ENDIAN)
            be.putLong(1, timestampUs())
            it[9] = 0x26
            it[10] = (index and 0xff).toByte()
            be.putShort(11, seq.toShort())
            it[13] = 0x21
            be.putShort(14, payload.size.toShort())
            payload.copyInto(it, 16)
        }
    }

    data class KeyboardPayload(
        val keycode: Int,
        val scancode: Int,
        val modifiers: Int,
        val timestampUs: Long = timestampUs(),
    )

    data class TextKeySpec(
        val keycode: Int,
        val scancode: Int,
        val shift: Boolean = false,
    ) {
        fun toKeyboardPayload(modifiers: Int): KeyboardPayload =
            KeyboardPayload(keycode, scancode, modifiers)
    }

    companion object {
        const val INPUT_HEARTBEAT = 2
        const val INPUT_KEY_DOWN = 3
        const val INPUT_KEY_UP = 4
        const val INPUT_MOUSE_ABS = 5
        const val INPUT_MOUSE_REL = 7
        const val INPUT_MOUSE_BUTTON_DOWN = 8
        const val INPUT_MOUSE_BUTTON_UP = 9
        const val INPUT_MOUSE_WHEEL = 10
        const val INPUT_GAMEPAD = 12
        const val INPUT_HAPTICS_ENABLED = 13
        const val INPUT_TEXT = 23

        /**
         * Native multi-touch. The host turns these into a Windows digitizer, which is what makes
         * touch-aware games switch to their mobile UI on their own.
         */
        const val INPUT_TOUCH = 24

        fun mapKeyEvent(event: KeyEvent): KeyboardPayload? {
            if (event.action != KeyEvent.ACTION_DOWN && event.action != KeyEvent.ACTION_UP) return null
            return mapKeyboardPayload(
                keyCode = event.keyCode,
                unicode = event.unicodeChar,
                scanCode = event.scanCode,
                shift = event.isShiftPressed,
                ctrl = event.isCtrlPressed,
                alt = event.isAltPressed,
                meta = event.isMetaPressed,
                capsLock = event.isCapsLockOn,
                numLock = event.isNumLockOn,
            )
        }

        internal fun mapKeyboardPayload(
            keyCode: Int,
            unicode: Int,
            scanCode: Int,
            shift: Boolean = false,
            ctrl: Boolean = false,
            alt: Boolean = false,
            meta: Boolean = false,
            capsLock: Boolean = false,
            numLock: Boolean = false,
            timestampUs: Long = timestampUs(),
        ): KeyboardPayload? {
            val vk = virtualKey(keyCode, unicode)
            val resolvedScanCode = if (scanCode > 0) scanCode else fallbackScanCode(keyCode)
            if (vk == null || resolvedScanCode == null) return null
            var modifiers = 0
            if (shift) modifiers = modifiers or 0x01
            if (ctrl) modifiers = modifiers or 0x02
            if (alt) modifiers = modifiers or 0x04
            if (meta) modifiers = modifiers or 0x08
            if (capsLock) modifiers = modifiers or 0x10
            if (numLock) modifiers = modifiers or 0x20
            return KeyboardPayload(vk, resolvedScanCode, modifiers, timestampUs)
        }

        /** Use committed text for symbols; virtual keys describe positions on the host layout. */
        internal fun keyboardTextFallbackChar(
            unicodeChar: Int,
            baseUnicodeChar: Int,
            mapped: Boolean,
            altGraph: Boolean,
            shortcut: Boolean = false,
            keyCode: Int = KeyEvent.KEYCODE_UNKNOWN,
        ): Char? {
            if (shortcut) return null
            val resolved = if (unicodeChar == 0) when (keyCode) {
                KeyEvent.KEYCODE_AT -> '@'.code
                KeyEvent.KEYCODE_POUND -> '#'.code
                KeyEvent.KEYCODE_STAR -> '*'.code
                KeyEvent.KEYCODE_PLUS -> '+'.code
                else -> 0
            } else unicodeChar
            if (resolved < 0x20 || resolved == 0x7f || resolved > Char.MAX_VALUE.code) return null
            val char = resolved.toChar()
            // Mapped physical keys must retain their down/up semantics for gameplay and ordinary
            // typing. Unicode is only the fallback for keys with no usable mapping or for an
            // AltGr composition whose committed character differs from the physical base key.
            return char.takeIf { !mapped || (altGraph && resolved != baseUnicodeChar) }
        }

        internal fun mapTextCharToKeySpec(char: Char): TextKeySpec? {
            val mapped = when (char) {
                in 'a'..'z' -> textKeySpecFromAndroidKeyCode(KeyEvent.KEYCODE_A + (char - 'a'))
                in 'A'..'Z' -> textKeySpecFromAndroidKeyCode(KeyEvent.KEYCODE_A + (char - 'A'), shift = true)
                in '0'..'9' -> textKeySpecFromAndroidKeyCode(KeyEvent.KEYCODE_0 + (char - '0'))
                '\n', '\r' -> textKeySpecFromAndroidKeyCode(KeyEvent.KEYCODE_ENTER)
                else -> textBaseKeyCodes[char]?.let(::textKeySpecFromAndroidKeyCode)
                    ?: textShiftedKeyCodes[char]?.let { textKeySpecFromAndroidKeyCode(it, shift = true) }
            }
            return mapped
        }

        internal fun shiftLeftPayload(modifiers: Int): KeyboardPayload =
            KeyboardPayload(0xa0, fallbackScanCode(KeyEvent.KEYCODE_SHIFT_LEFT) ?: 0x002a, modifiers)

        private fun textKeySpecFromAndroidKeyCode(keyCode: Int, shift: Boolean = false): TextKeySpec? {
            val payload = mapKeyboardPayload(
                keyCode = keyCode,
                unicode = 0,
                scanCode = 0,
                shift = shift,
                timestampUs = 0L,
            ) ?: return null
            return TextKeySpec(payload.keycode, payload.scancode, shift)
        }

        private val textBaseKeyCodes = mapOf(
            ' ' to KeyEvent.KEYCODE_SPACE,
            '-' to KeyEvent.KEYCODE_MINUS,
            '=' to KeyEvent.KEYCODE_EQUALS,
            '[' to KeyEvent.KEYCODE_LEFT_BRACKET,
            ']' to KeyEvent.KEYCODE_RIGHT_BRACKET,
            '\\' to KeyEvent.KEYCODE_BACKSLASH,
            ';' to KeyEvent.KEYCODE_SEMICOLON,
            '\'' to KeyEvent.KEYCODE_APOSTROPHE,
            ',' to KeyEvent.KEYCODE_COMMA,
            '.' to KeyEvent.KEYCODE_PERIOD,
            '/' to KeyEvent.KEYCODE_SLASH,
            '`' to KeyEvent.KEYCODE_GRAVE,
        )

        private val textShiftedKeyCodes = mapOf(
            '!' to KeyEvent.KEYCODE_1,
            '@' to KeyEvent.KEYCODE_2,
            '#' to KeyEvent.KEYCODE_3,
            '$' to KeyEvent.KEYCODE_4,
            '%' to KeyEvent.KEYCODE_5,
            '^' to KeyEvent.KEYCODE_6,
            '&' to KeyEvent.KEYCODE_7,
            '*' to KeyEvent.KEYCODE_8,
            '(' to KeyEvent.KEYCODE_9,
            ')' to KeyEvent.KEYCODE_0,
            '_' to KeyEvent.KEYCODE_MINUS,
            '+' to KeyEvent.KEYCODE_EQUALS,
            '{' to KeyEvent.KEYCODE_LEFT_BRACKET,
            '}' to KeyEvent.KEYCODE_RIGHT_BRACKET,
            '|' to KeyEvent.KEYCODE_BACKSLASH,
            ':' to KeyEvent.KEYCODE_SEMICOLON,
            '"' to KeyEvent.KEYCODE_APOSTROPHE,
            '<' to KeyEvent.KEYCODE_COMMA,
            '>' to KeyEvent.KEYCODE_PERIOD,
            '?' to KeyEvent.KEYCODE_SLASH,
            '~' to KeyEvent.KEYCODE_GRAVE,
        )

        private fun textInputChunkLength(bytes: ByteArray, offset: Int): Int {
            val remaining = bytes.size - offset
            if (remaining <= TEXT_INPUT_CHUNK_MAX_BYTES) return remaining

            var end = offset + TEXT_INPUT_CHUNK_MAX_BYTES
            repeat(4) {
                if ((bytes[end].toInt() and 0xc0) != 0x80) return end - offset
                end -= 1
            }
            return 0
        }

        private const val TEXT_INPUT_CHUNK_MAX_BYTES = 1016
        private const val TEXT_INPUT_HEADER_BYTES = 5

        private fun virtualKey(keyCode: Int, unicode: Int): Int? =
            when (keyCode) {
                KeyEvent.KEYCODE_ENTER -> 0x0d
                KeyEvent.KEYCODE_ESCAPE -> 0x1b
                KeyEvent.KEYCODE_DEL -> 0x08
                KeyEvent.KEYCODE_TAB -> 0x09
                KeyEvent.KEYCODE_SPACE -> 0x20
                KeyEvent.KEYCODE_DPAD_LEFT -> 0x25
                KeyEvent.KEYCODE_DPAD_UP -> 0x26
                KeyEvent.KEYCODE_DPAD_RIGHT -> 0x27
                KeyEvent.KEYCODE_DPAD_DOWN -> 0x28
                KeyEvent.KEYCODE_PAGE_UP -> 0x21
                KeyEvent.KEYCODE_PAGE_DOWN -> 0x22
                KeyEvent.KEYCODE_FORWARD_DEL -> 0x2e
                KeyEvent.KEYCODE_INSERT -> 0x2d
                KeyEvent.KEYCODE_MOVE_HOME -> 0x24
                KeyEvent.KEYCODE_MOVE_END -> 0x23
                KeyEvent.KEYCODE_SHIFT_LEFT,
                KeyEvent.KEYCODE_SHIFT_RIGHT,
                -> 0x10
                KeyEvent.KEYCODE_CTRL_LEFT,
                KeyEvent.KEYCODE_CTRL_RIGHT,
                -> 0x11
                KeyEvent.KEYCODE_ALT_LEFT,
                KeyEvent.KEYCODE_ALT_RIGHT,
                -> 0x12
                KeyEvent.KEYCODE_CAPS_LOCK -> 0x14
                KeyEvent.KEYCODE_NUM_LOCK -> 0x90
                KeyEvent.KEYCODE_SCROLL_LOCK -> 0x91
                KeyEvent.KEYCODE_MINUS -> 0xbd
                KeyEvent.KEYCODE_EQUALS -> 0xbb
                KeyEvent.KEYCODE_LEFT_BRACKET -> 0xdb
                KeyEvent.KEYCODE_RIGHT_BRACKET -> 0xdd
                KeyEvent.KEYCODE_BACKSLASH -> 0xdc
                KeyEvent.KEYCODE_SEMICOLON -> 0xba
                KeyEvent.KEYCODE_APOSTROPHE -> 0xde
                KeyEvent.KEYCODE_COMMA -> 0xbc
                KeyEvent.KEYCODE_PERIOD -> 0xbe
                KeyEvent.KEYCODE_SLASH -> 0xbf
                KeyEvent.KEYCODE_GRAVE -> 0xc0
                in KeyEvent.KEYCODE_A..KeyEvent.KEYCODE_Z -> 0x41 + (keyCode - KeyEvent.KEYCODE_A)
                in KeyEvent.KEYCODE_0..KeyEvent.KEYCODE_9 -> 0x30 + (keyCode - KeyEvent.KEYCODE_0)
                in KeyEvent.KEYCODE_NUMPAD_0..KeyEvent.KEYCODE_NUMPAD_9 -> 0x60 + (keyCode - KeyEvent.KEYCODE_NUMPAD_0)
                in KeyEvent.KEYCODE_F1..KeyEvent.KEYCODE_F12 -> 0x70 + (keyCode - KeyEvent.KEYCODE_F1)
                else -> unicode.takeIf { it in 1..255 }?.let { Character.toUpperCase(it.toChar()).code }
            }

        private fun fallbackScanCode(keyCode: Int): Int? =
            when (keyCode) {
                KeyEvent.KEYCODE_A -> 0x001e
                KeyEvent.KEYCODE_B -> 0x0030
                KeyEvent.KEYCODE_C -> 0x002e
                KeyEvent.KEYCODE_D -> 0x0020
                KeyEvent.KEYCODE_E -> 0x0012
                KeyEvent.KEYCODE_F -> 0x0021
                KeyEvent.KEYCODE_G -> 0x0022
                KeyEvent.KEYCODE_H -> 0x0023
                KeyEvent.KEYCODE_I -> 0x0017
                KeyEvent.KEYCODE_J -> 0x0024
                KeyEvent.KEYCODE_K -> 0x0025
                KeyEvent.KEYCODE_L -> 0x0026
                KeyEvent.KEYCODE_M -> 0x0032
                KeyEvent.KEYCODE_N -> 0x0031
                KeyEvent.KEYCODE_O -> 0x0018
                KeyEvent.KEYCODE_P -> 0x0019
                KeyEvent.KEYCODE_Q -> 0x0010
                KeyEvent.KEYCODE_R -> 0x0013
                KeyEvent.KEYCODE_S -> 0x001f
                KeyEvent.KEYCODE_T -> 0x0014
                KeyEvent.KEYCODE_U -> 0x0016
                KeyEvent.KEYCODE_V -> 0x002f
                KeyEvent.KEYCODE_W -> 0x0011
                KeyEvent.KEYCODE_X -> 0x002d
                KeyEvent.KEYCODE_Y -> 0x0015
                KeyEvent.KEYCODE_Z -> 0x002c
                KeyEvent.KEYCODE_1 -> 0x0002
                KeyEvent.KEYCODE_2 -> 0x0003
                KeyEvent.KEYCODE_3 -> 0x0004
                KeyEvent.KEYCODE_4 -> 0x0005
                KeyEvent.KEYCODE_5 -> 0x0006
                KeyEvent.KEYCODE_6 -> 0x0007
                KeyEvent.KEYCODE_7 -> 0x0008
                KeyEvent.KEYCODE_8 -> 0x0009
                KeyEvent.KEYCODE_9 -> 0x000a
                KeyEvent.KEYCODE_0 -> 0x000b
                KeyEvent.KEYCODE_NUMPAD_7 -> 0x0047
                KeyEvent.KEYCODE_NUMPAD_8 -> 0x0048
                KeyEvent.KEYCODE_NUMPAD_9 -> 0x0049
                KeyEvent.KEYCODE_NUMPAD_4 -> 0x004b
                KeyEvent.KEYCODE_NUMPAD_5 -> 0x004c
                KeyEvent.KEYCODE_NUMPAD_6 -> 0x004d
                KeyEvent.KEYCODE_NUMPAD_1 -> 0x004f
                KeyEvent.KEYCODE_NUMPAD_2 -> 0x0050
                KeyEvent.KEYCODE_NUMPAD_3 -> 0x0051
                KeyEvent.KEYCODE_NUMPAD_0 -> 0x0052
                KeyEvent.KEYCODE_ENTER -> 0x001c
                KeyEvent.KEYCODE_NUMPAD_ENTER -> 0x011c
                KeyEvent.KEYCODE_ESCAPE -> 0x0001
                KeyEvent.KEYCODE_SPACE -> 0x0039
                KeyEvent.KEYCODE_TAB -> 0x000f
                KeyEvent.KEYCODE_DEL -> 0x000e
                KeyEvent.KEYCODE_DPAD_LEFT -> 0x014b
                KeyEvent.KEYCODE_DPAD_UP -> 0x0148
                KeyEvent.KEYCODE_DPAD_RIGHT -> 0x014d
                KeyEvent.KEYCODE_DPAD_DOWN -> 0x0150
                KeyEvent.KEYCODE_PAGE_UP -> 0x0149
                KeyEvent.KEYCODE_PAGE_DOWN -> 0x0151
                KeyEvent.KEYCODE_FORWARD_DEL -> 0x0153
                KeyEvent.KEYCODE_INSERT -> 0x0152
                KeyEvent.KEYCODE_MOVE_HOME -> 0x0147
                KeyEvent.KEYCODE_MOVE_END -> 0x014f
                KeyEvent.KEYCODE_SHIFT_LEFT -> 0x002a
                KeyEvent.KEYCODE_SHIFT_RIGHT -> 0x0036
                KeyEvent.KEYCODE_CTRL_LEFT -> 0x001d
                KeyEvent.KEYCODE_CTRL_RIGHT -> 0x011d
                KeyEvent.KEYCODE_ALT_LEFT -> 0x0038
                KeyEvent.KEYCODE_ALT_RIGHT -> 0x0138
                KeyEvent.KEYCODE_CAPS_LOCK -> 0x003a
                KeyEvent.KEYCODE_NUM_LOCK -> 0x0145
                KeyEvent.KEYCODE_SCROLL_LOCK -> 0x0046
                KeyEvent.KEYCODE_MINUS -> 0x000c
                KeyEvent.KEYCODE_EQUALS -> 0x000d
                KeyEvent.KEYCODE_LEFT_BRACKET -> 0x001a
                KeyEvent.KEYCODE_RIGHT_BRACKET -> 0x001b
                KeyEvent.KEYCODE_BACKSLASH -> 0x002b
                KeyEvent.KEYCODE_SEMICOLON -> 0x0027
                KeyEvent.KEYCODE_APOSTROPHE -> 0x0028
                KeyEvent.KEYCODE_COMMA -> 0x0033
                KeyEvent.KEYCODE_PERIOD -> 0x0034
                KeyEvent.KEYCODE_SLASH -> 0x0035
                KeyEvent.KEYCODE_GRAVE -> 0x0029
                else -> null
            }
    }
}

internal class InputSessionClock(
    private val elapsedRealtimeNanos: () -> Long = SystemClock::elapsedRealtimeNanos,
) {
    @Volatile
    private var startedAtNanos = 0L

    fun start() {
        startedAtNanos = elapsedRealtimeNanos()
    }

    fun timestampUs(): Long {
        val startedAt = startedAtNanos
        if (startedAt == 0L) return 0L
        return (elapsedRealtimeNanos() - startedAt).coerceAtLeast(0L) / 1_000L
    }
}

private val inputSessionClock = InputSessionClock()

/** Reset the input clock when the host handshake completes, matching the desktop GFN client. */
internal fun startInputSessionClock() {
    inputSessionClock.start()
}

internal fun timestampUs(): Long = inputSessionClock.timestampUs()

/** Protocol v3 outer headers carry the send-time session clock, not device uptime. */
internal fun restampProtocolV3OuterTimestamp(packet: ByteArray, nowUs: Long = timestampUs()): Boolean {
    if (packet.size < 9 || packet[0] != 0x23.toByte()) return false
    ByteBuffer.wrap(packet).order(ByteOrder.BIG_ENDIAN).putLong(1, nowUs.coerceAtLeast(0L))
    return true
}

internal fun shouldRunControllerMouseLoop(
    controllerMouseAssistActive: Boolean,
    controllerMouseEmulationActive: Boolean,
): Boolean = controllerMouseAssistActive || controllerMouseEmulationActive

internal fun shouldCaptureMicrophone(
    mode: MicrophoneMode,
    permissionGranted: Boolean,
): Boolean = mode != MicrophoneMode.Disabled && permissionGranted

internal fun isDisposedRtpSenderFailure(error: IllegalStateException): Boolean =
    error.message == "RtpSender has been disposed."

internal fun advancedCodecRestartSettleDelayMs(codec: VideoCodec, hadStableMedia: Boolean): Long =
    if (hadStableMedia && codec != VideoCodec.H264) ANDROID_CODEC_RESTART_SETTLE_MS else 0L

internal const val GFN_MICROPHONE_MID = "3"
internal const val MICROPHONE_STREAM_ID = "mic"
internal const val MICROPHONE_TRACK_ID = "mic"
internal const val DEFAULT_INPUT_PROTOCOL_VERSION = 2
internal const val INPUT_HANDSHAKE_MARKER = 0x0e
internal const val INPUT_HANDSHAKE_MAGIC_WORD = 526
internal const val ICE_DISCONNECTED_GRACE_MS = 3500L
internal const val ICE_FAILED_RECONNECT_DELAY_MS = 250L
internal const val SIGNALING_RECONNECT_DELAY_MS = 1000L
private const val ANDROID_CODEC_RESTART_SETTLE_MS = 180L
internal const val MAX_TRANSPORT_RECONNECT_ATTEMPTS = 3
internal const val MAX_TRANSIENT_SIGNALING_RETRIES = 3
internal const val OFFER_TIMEOUT_MS = 12_000L
internal const val MEDIA_STALL_KEYFRAME_AFTER_MS = 5_000L
internal const val MEDIA_STALL_KEYFRAME_INTERVAL_MS = 2_500L
internal const val MEDIA_STALL_RESTART_AFTER_MS = 10_000L
// Low-power TV MediaCodec implementations can open an advanced decoder several
// seconds before they produce their first frame. Keep the pre-TV-optimization
// startup window so a slow H.265/AV1 decoder is not mistaken for a dead one and
// immediately replaced by the safe-codec profile.
internal const val TV_MEDIA_STALL_KEYFRAME_AFTER_MS = 5_000L
internal const val TV_MEDIA_STALL_KEYFRAME_INTERVAL_MS = 2_500L
internal const val TV_MEDIA_STALL_RESTART_AFTER_MS = 14_000L
internal const val FIRST_VIDEO_FRAME_TIMEOUT_MS = 8_000L
internal const val STABLE_TRANSPORT_PROGRESS_SAMPLES = 3
internal const val GAMEPAD_GUIDE_AUTO_RELEASE_MS = 160L
internal const val STEAM_MENU_MODIFIER_DELAY_MS = 40L
internal const val STREAM_TEXT_SEND_MAX_CHARS = 4096
internal const val STREAM_TEXT_SEND_ATTEMPTS = 3
internal const val STREAM_TEXT_PACKET_DELAY_MS = 4L
internal const val STREAM_TEXT_RETRY_DELAY_MS = 16L
internal const val BYTES_PER_MEBIBYTE = 1024L * 1024L
internal const val LOW_POWER_TV_MEMORY_LIMIT_BYTES = 3L * 1024L * BYTES_PER_MEBIBYTE

internal fun Any?.statsDouble(): Double? =
    when (this) {
        is Number -> toDouble()
        is String -> toDoubleOrNull()
        else -> null
    }

internal fun Any?.statsLong(): Long? =
    when (this) {
        is Number -> toLong()
        is String -> toLongOrNull()
        else -> null
    }

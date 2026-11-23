// Adapted from OpenStroid Android. See android/app/src/sideload/assets/licenses/OpenStroid-NOTICE.txt.
package com.opencloudgaming.opennow.boosteroid

import android.os.SystemClock
import android.view.KeyEvent

internal class BoosteroidKeyMapper {

    data class KeyboardPayload(
        val keycode: Int,
        val scancode: Int,
        val modifiers: Int,
        val timestampUs: Long = monotonicTimestampUs(),
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
            timestampUs: Long = monotonicTimestampUs(),
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
                KeyEvent.KEYCODE_SHIFT_LEFT -> 0xa0
                KeyEvent.KEYCODE_SHIFT_RIGHT -> 0xa1
                KeyEvent.KEYCODE_CTRL_LEFT -> 0xa2
                KeyEvent.KEYCODE_CTRL_RIGHT -> 0xa3
                KeyEvent.KEYCODE_ALT_LEFT -> 0xa4
                KeyEvent.KEYCODE_ALT_RIGHT -> 0xa5
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

private fun monotonicTimestampUs(): Long = SystemClock.elapsedRealtimeNanos() / 1000L

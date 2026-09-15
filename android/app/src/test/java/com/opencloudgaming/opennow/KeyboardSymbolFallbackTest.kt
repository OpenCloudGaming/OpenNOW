package com.opencloudgaming.opennow

import android.view.KeyEvent
import org.junit.Assert.assertEquals
import org.junit.Assert.assertNull
import org.junit.Test

/**
 * Symbols were reaching the host as nothing at all, or as the wrong key, while letters worked.
 * See [InputEncoder.keyboardTextFallbackChar].
 */
class KeyboardSymbolFallbackTest {
    private fun fallback(
        unicodeChar: Int,
        baseUnicodeChar: Int = unicodeChar,
        mapped: Boolean = true,
        altGraph: Boolean = false,
    ) = InputEncoder.keyboardTextFallbackChar(unicodeChar, baseUnicodeChar, mapped, altGraph)

    @Test
    fun anUnmappableSymbolKeyIsSentAsText() {
        // KEYCODE_AT has no fallback scancode, so mapKeyboardPayload returns null and the key
        // used to be dropped on the floor.
        assertEquals('@', fallback(unicodeChar = '@'.code, mapped = false))
    }

    @Test
    fun anAltGraphComposedSymbolIsSentAsTextRatherThanAsCtrlAltLetter() {
        // German layout: AltGr+Q is '@'. The key maps — to VK_Q — which is the wrong character.
        assertEquals(
            '@',
            fallback(unicodeChar = '@'.code, baseUnicodeChar = 'q'.code, mapped = true, altGraph = true),
        )
    }

    @Test
    fun ordinaryMappedKeysStayOnThePhysicalKeyPath() {
        assertNull(fallback(unicodeChar = '@'.code, mapped = true))
        assertNull(fallback(unicodeChar = 'a'.code, mapped = true))
    }

    @Test
    fun altGraphThatChangesNothingStaysOnTheKeyPath() {
        // Ctrl+Alt held over a key whose character did not change is a shortcut, not a composition.
        assertNull(fallback(unicodeChar = 'f'.code, baseUnicodeChar = 'f'.code, altGraph = true))
    }

    @Test
    fun shortcutsThatResolveToNoCharacterAreNeverDiverted() {
        // Android reports unicodeChar 0 for Ctrl+Alt+F on a US layout.
        assertNull(fallback(unicodeChar = 0, baseUnicodeChar = 'f'.code, altGraph = true))
        assertNull(fallback(unicodeChar = 0, mapped = false))
    }

    @Test
    fun controlCharactersKeepTheirOwnKeys() {
        // Enter, Tab and Backspace must stay key presses; as text the host would type a raw
        // control byte instead of pressing the key.
        assertNull(fallback(unicodeChar = '\n'.code, mapped = false))
        assertNull(fallback(unicodeChar = '\t'.code, mapped = false))
        assertNull(fallback(unicodeChar = 0x08, mapped = false))
        assertNull(fallback(unicodeChar = 0x1b, mapped = false))
        assertNull(fallback(unicodeChar = 0x7f, mapped = false))
    }

    @Test
    fun spaceIsNotTreatedAsAControlCharacter() {
        assertEquals(' ', fallback(unicodeChar = ' '.code, mapped = false))
    }

    @Test
    fun mappedPhysicalSpaceStaysOnTheGameplayKeyPath() {
        assertNull(
            InputEncoder.keyboardTextFallbackChar(
                unicodeChar = ' '.code,
                baseUnicodeChar = ' '.code,
                mapped = true,
                altGraph = false,
                keyCode = KeyEvent.KEYCODE_SPACE,
            ),
        )
    }

    @Test
    fun everyAsciiSymbolSurvivesTheUnmappedPath() {
        val symbols = "!@#$%^&*()_+-=[]{}|;':\",./<>?`~"
        symbols.forEach { symbol ->
            assertEquals("symbol $symbol", symbol, fallback(unicodeChar = symbol.code, mapped = false))
        }
    }

    @Test
    fun nonBmpCharactersAreNotTruncatedIntoTheWrongGlyph() {
        // An emoji arrives as a surrogate pair; halving it would send a lone surrogate.
        assertNull(fallback(unicodeChar = 0x1F600, mapped = false))
    }
    @Test
    fun mappedSymbolsStayOnThePhysicalKeyPath() {
        "!@#$%^&*()_+-=[]{}|;':\",./<>?`~".forEach {
            assertNull("symbol $it", fallback(it.code, mapped = true))
        }
    }

    @Test
    fun controlAndMetaShortcutsStayAsKeyEvents() {
        assertNull(InputEncoder.keyboardTextFallbackChar('+'.code, '='.code, true, false, shortcut = true))
        assertNull(InputEncoder.keyboardTextFallbackChar('!'.code, '1'.code, true, false, shortcut = true))
    }

    @Test
    fun dedicatedSymbolKeysWorkEvenWithoutAndroidUnicodeMetadata() {
        assertEquals('@', InputEncoder.keyboardTextFallbackChar(0, 0, false, false,
            keyCode = android.view.KeyEvent.KEYCODE_AT))
    }
}

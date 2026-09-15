package com.opencloudgaming.opennow

import android.view.KeyEvent
import org.junit.Assert.assertEquals
import org.junit.Assert.assertFalse
import org.junit.Assert.assertNull
import org.junit.Assert.assertTrue
import org.junit.Test

class AndroidKeyboardShortcutTest {
    @Test
    fun capturesAndMatchesPhysicalKeyboardChord() {
        assertEquals(
            DEFAULT_ANDROID_STREAM_MENU_SHORTCUT,
            androidKeyboardShortcutFromKey(
                keyCode = KeyEvent.KEYCODE_G,
                ctrlPressed = true,
                shiftPressed = true,
            ),
        )
        assertTrue(
            matchesAndroidKeyboardShortcut(
                keyCode = KeyEvent.KEYCODE_G,
                ctrlPressed = true,
                shiftPressed = true,
                configuredShortcut = DEFAULT_ANDROID_STREAM_MENU_SHORTCUT,
            ),
        )
        assertFalse(
            matchesAndroidKeyboardShortcut(
                keyCode = KeyEvent.KEYCODE_G,
                ctrlPressed = true,
                shiftPressed = true,
                configuredShortcut = "Ctrl+Alt+G",
            ),
        )
    }

    @Test
    fun shiftMenuDoesNotMatchDefaultButCanBeExplicitlyBound() {
        assertFalse(
            matchesAndroidKeyboardShortcut(
                keyCode = KeyEvent.KEYCODE_MENU,
                shiftPressed = true,
                configuredShortcut = DEFAULT_ANDROID_STREAM_MENU_SHORTCUT,
            ),
        )
        assertTrue(
            matchesAndroidKeyboardShortcut(
                keyCode = KeyEvent.KEYCODE_MENU,
                shiftPressed = true,
                configuredShortcut = "Shift+Menu",
            ),
        )
    }

    @Test
    fun disabledShortcutNeverMatches() {
        assertFalse(
            matchesAndroidKeyboardShortcut(
                keyCode = KeyEvent.KEYCODE_G,
                ctrlPressed = true,
                shiftPressed = true,
                configuredShortcut = DISABLED_ANDROID_STREAM_MENU_SHORTCUT,
            ),
        )
    }

    @Test
    fun modifierOnlyRepeatAndControllerEventsAreNotCaptured() {
        assertNull(androidKeyboardShortcutFromKey(KeyEvent.KEYCODE_SHIFT_LEFT, shiftPressed = true))
        assertNull(
            androidKeyboardShortcutFromKey(KeyEvent.KEYCODE_G, repeatCount = 1),
        )
        assertNull(
            androidKeyboardShortcutFromKey(KeyEvent.KEYCODE_BUTTON_A, controllerInputDevice = true),
        )
    }

    @Test
    fun normalizesPersistedAliasesAndUnknownKeyCodesForDisplay() {
        assertEquals("Ctrl+Shift+G", androidKeyboardShortcutDisplay("control + shift + g"))
        assertEquals("Alt+Key999", androidKeyboardShortcutDisplay("Alt+Key999"))
        assertEquals(DISABLED_ANDROID_STREAM_MENU_SHORTCUT, androidKeyboardShortcutDisplay("disabled"))
    }
}

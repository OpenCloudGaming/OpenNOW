package com.opencloudgaming.opennow

import kotlinx.serialization.decodeFromString
import kotlinx.serialization.encodeToString
import android.view.KeyEvent
import org.junit.Assert.assertEquals
import org.junit.Assert.assertFalse
import org.junit.Assert.assertNotNull
import org.junit.Assert.assertNull
import org.junit.Assert.assertTrue
import org.junit.Test

class KeyboardOverlayCatalogTest {
    @Test
    fun heldModifiersRemainActiveWhenDirectionKeysChange() {
        val diagonal = setOf(
            KeyEvent.KEYCODE_W,
            KeyEvent.KEYCODE_A,
            KeyEvent.KEYCODE_SHIFT_LEFT,
            KeyEvent.KEYCODE_CTRL_LEFT,
        )
        val straight = diagonal - KeyEvent.KEYCODE_A

        assertEquals(0x03, keyboardModifierMaskForKeys(diagonal))
        assertEquals(0x03, keyboardModifierMaskForKeys(straight))
        assertEquals(0x02, keyboardModifierMaskForKeys(straight - KeyEvent.KEYCODE_SHIFT_LEFT))
    }

    @Test
    fun catalogCoversLettersDigitsFunctionKeysAndEveryOptionIsEncodable() {
        val actions = keyboardButtonCatalog.map(KeyboardButtonOption::id).toSet()
        ('a'..'z').forEach { assertTrue("Missing letter $it", "letter_$it" in actions) }
        (0..9).forEach { assertTrue("Missing digit $it", "digit_$it" in actions) }
        (1..12).forEach { assertTrue("Missing F$it", "function_f$it" in actions) }
        listOf(
            "page_up", "page_down", "tab", "space", "escape", "backspace", "delete",
            "insert", "home", "end", "arrow_up", "arrow_down", "arrow_left", "arrow_right",
            "shift_left", "shift_right", "ctrl_left", "ctrl_right", "alt_left", "alt_right",
            "meta_left", "meta_right", "print_screen", "pause_break", "mouse_left", "mouse_middle", "mouse_right",
        ).forEach { assertTrue("Missing $it", it in actions) }

        val unencodable = keyboardButtonCatalog.mapNotNull { option ->
            option.keyCode?.takeIf { keyCode ->
                InputEncoder.mapKeyboardPayload(keyCode = keyCode, unicode = 0, scanCode = 0, timestampUs = 0L) == null
            }?.let { option.id }
        }
        assertTrue("Catalog actions without an encoder mapping: $unencodable", unencodable.isEmpty())
        assertEquals(setOf(1, 2, 3), keyboardButtonCatalog.mapNotNull(KeyboardButtonOption::mouseButton).toSet())
    }

    @Test
    fun userCanAddTheSameActionMoreThanOnceAndRemoveOneWithoutLeavingStyleOrOffsetBehind() {
        val first = newKeyboardOverlayButton("letter_w")!!
        val second = newKeyboardOverlayButton("letter_w")!!
        assertTrue(first.id != second.id)

        val touch = AndroidTouchSettings()
            .withKeyboardButtonAdded(first)
            .withKeyboardButtonAdded(second)
            .withButtonAppearance(first.appearanceKey(), TouchButtonAppearance(label = "Sprint"))
            .withOffset("${first.positionKey()}_landscape", 28f, -40f)
        assertEquals(9, touch.keyboardButtons.size)
        assertEquals(7, touch.keyboardButtons.first { it.id == first.id }.layoutIndex)
        assertEquals(8, touch.keyboardButtons.first { it.id == second.id }.layoutIndex)

        val restored = OpenNowJson.decodeFromString<AndroidTouchSettings>(OpenNowJson.encodeToString(touch))
        assertEquals(touch.keyboardButtons, restored.keyboardButtons)
        assertEquals("Sprint", restored.buttonAppearances.getValue(first.appearanceKey()).label)
        assertEquals(TouchOffset(28f, -40f), restored.offsets["${first.positionKey()}_landscape"])

        val remaining = restored.withKeyboardButtonRemoved(first.id)
        assertFalse(remaining.keyboardButtons.any { it.id == first.id })
        assertTrue(remaining.keyboardButtons.any { it.id == second.id })
        assertFalse(remaining.buttonAppearances.containsKey(first.appearanceKey()))
        assertFalse(remaining.offsets.containsKey("${first.positionKey()}_landscape"))
        assertEquals(8, remaining.keyboardButtons.first { it.id == second.id }.layoutIndex)
        val third = newKeyboardOverlayButton("letter_e")!!
        assertEquals(9, remaining.withKeyboardButtonAdded(third).keyboardButtons.first { it.id == third.id }.layoutIndex)
    }

    @Test
    fun removingOneButtonKeepsOffsetsForIdsThatShareItsPrefix() {
        val settings = AndroidTouchSettings(
            keyboardButtons = defaultKeyboardOverlayButtons() + listOf(
                KeyboardOverlayButton("custom", "letter_a"),
                KeyboardOverlayButton("custom_more", "letter_b"),
            ),
            offsets = mapOf(
                "keyboard_button_custom_landscape" to TouchOffset(1f, 2f),
                "keyboard_button_custom_more_landscape" to TouchOffset(3f, 4f),
            ),
        )

        val remaining = settings.withKeyboardButtonRemoved("custom")
        assertFalse(remaining.offsets.containsKey("keyboard_button_custom_landscape"))
        assertEquals(TouchOffset(3f, 4f), remaining.offsets["keyboard_button_custom_more_landscape"])
    }

    @Test
    fun normalizationKeepsOnlyKnownActionsAndActiveButtonAppearances() {
        val valid = newKeyboardOverlayButton("page_down")!!
        val settings = AndroidTouchSettings(
            keyboardButtons = defaultKeyboardOverlayButtons() + listOf(
                valid,
                KeyboardOverlayButton("duplicate", "letter_a"),
                KeyboardOverlayButton("duplicate", "letter_b"),
                KeyboardOverlayButton("unknown_action", "not_a_key"),
                KeyboardOverlayButton("bad/id", "letter_c"),
            ),
            buttonAppearances = mapOf(
                valid.appearanceKey() to TouchButtonAppearance(label = "Next page"),
                "keyboard_custom_orphan" to TouchButtonAppearance(label = "Orphan"),
            ),
            offsets = mapOf(
                "${valid.positionKey()}_portrait" to TouchOffset(5f, 8f),
                "keyboard_button_removed_user_portrait" to TouchOffset(9f, 9f),
            ),
        ).normalizedTouchControls()

        assertEquals(9, settings.keyboardButtons.size)
        assertEquals("Next page", settings.buttonAppearances.getValue(valid.appearanceKey()).label)
        assertFalse(settings.buttonAppearances.containsKey("keyboard_custom_orphan"))
        assertTrue(settings.offsets.containsKey("${valid.positionKey()}_portrait"))
        assertFalse(settings.offsets.containsKey("keyboard_button_removed_user_portrait"))
    }

    @Test
    fun defaultLayoutSlotsAreSeparateAndLegacyGroupTranslationIsPreserved() {
        val base = (0..9).map { index -> keyboardButtonDefaultOffset(index, spacingDp = 55f) }
        assertEquals(base.size, base.toSet().size)
        val moved = keyboardButtonDefaultOffset(0, spacingDp = 55f, migratedGroupOffset = TouchOffset(20f, 12f))
        assertEquals(TouchOffset(-90f, -98f), moved)
        assertEquals(TouchOffset(), keyboardButtonDefaultOffset(-1, spacingDp = 55f))
    }

    @Test
    fun oldSettingsGetTheOriginalSevenButtonsAndInvalidPickerValuesAreRejected() {
        val restored = OpenNowJson.decodeFromString<AndroidTouchSettings>("{}")
        assertFalse(restored.keyboardModeEnabled)
        assertEquals(7, restored.keyboardButtons.size)
        assertNull(newKeyboardOverlayButton("not_a_catalog_action"))
        assertNotNull(newKeyboardOverlayButton("function_f12"))
    }

    @Test
    fun customKeyboardButtonListHasNoExtraButtonSlotLimit() {
        val manyButtons = AndroidTouchSettings(
            keyboardButtons = defaultKeyboardOverlayButtons() + (0 until 100).map { index ->
                KeyboardOverlayButton("custom_$index", "letter_${'a' + (index % 26)}")
            },
        ).normalizedTouchControls()
        assertEquals(107, manyButtons.keyboardButtons.size)
    }

    @Test
    fun normalizationBoundsUntrustedButtonLists() {
        val raw = (0..300).map { index -> KeyboardOverlayButton("custom_$index", "letter_a") }
        val normalized = AndroidTouchSettings(keyboardButtons = raw).normalizedTouchControls()
        assertEquals(MAX_KEYBOARD_OVERLAY_BUTTONS, normalized.keyboardButtons.size)
        assertEquals(normalized, normalized.withKeyboardButtonAdded(newKeyboardOverlayButton("letter_b")!!))
    }

    @Test
    fun wasdStickReleasesAtCenterAndRejectsInvalidCoordinates() {
        assertEquals(setOf(KeyEvent.KEYCODE_W, KeyEvent.KEYCODE_D), keyboardStickKeyCodes(0.8f, -0.8f))
        assertEquals(emptySet<Int>(), keyboardStickKeyCodes(0.2f, -0.2f))
        assertEquals(emptySet<Int>(), keyboardStickKeyCodes(Float.NaN, 0f))
    }
}

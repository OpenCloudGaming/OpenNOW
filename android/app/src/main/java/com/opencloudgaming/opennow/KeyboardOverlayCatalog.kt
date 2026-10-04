package com.opencloudgaming.opennow

import android.view.KeyEvent
import java.util.UUID

internal enum class KeyboardButtonCategory {
    Mouse,
    Letters,
    Numbers,
    FunctionKeys,
    Navigation,
    Modifiers,
    Symbols,
    Numpad,
}

internal data class KeyboardButtonOption(
    val id: String,
    val label: String,
    val capLabel: String,
    val category: KeyboardButtonCategory,
    val keyCode: Int? = null,
    val mouseButton: Int? = null,
)

private fun keyOption(
    id: String,
    label: String,
    capLabel: String,
    category: KeyboardButtonCategory,
    keyCode: Int,
) = KeyboardButtonOption(id, label, capLabel, category, keyCode = keyCode)

private fun mouseOption(id: String, label: String, capLabel: String, button: Int) =
    KeyboardButtonOption(id, label, capLabel, KeyboardButtonCategory.Mouse, mouseButton = button)

/**
 * The catalog contains regular PC keyboard keys that the native stream protocol can encode,
 * plus the three standard mouse buttons. The picker can add the same action more than once.
 */
internal val keyboardButtonCatalog: List<KeyboardButtonOption> by lazy {
    buildList {
        add(mouseOption("mouse_left", "Mouse left button", "LMB", 1))
        add(mouseOption("mouse_middle", "Mouse middle button", "MMB", 2))
        add(mouseOption("mouse_right", "Mouse right button", "RMB", 3))

        addAll(('A'..'Z').map { letter ->
            val index = letter - 'A'
            keyOption("letter_${letter.lowercaseChar()}", letter.toString(), letter.toString(), KeyboardButtonCategory.Letters,
                KeyEvent.KEYCODE_A + index)
        })
        addAll((0..9).map { digit ->
            keyOption("digit_$digit", digit.toString(), digit.toString(), KeyboardButtonCategory.Numbers,
                KeyEvent.KEYCODE_0 + digit)
        })
        addAll((1..12).map { number ->
            keyOption("function_f$number", "F$number", "F$number", KeyboardButtonCategory.FunctionKeys,
                KeyEvent.KEYCODE_F1 + number - 1)
        })

        add(keyOption("escape", "Escape", "ESC", KeyboardButtonCategory.Navigation, KeyEvent.KEYCODE_ESCAPE))
        add(keyOption("tab", "Tab", "TAB", KeyboardButtonCategory.Navigation, KeyEvent.KEYCODE_TAB))
        add(keyOption("enter", "Enter", "ENTER", KeyboardButtonCategory.Navigation, KeyEvent.KEYCODE_ENTER))
        add(keyOption("space", "Space", "SPACE", KeyboardButtonCategory.Navigation, KeyEvent.KEYCODE_SPACE))
        add(keyOption("backspace", "Backspace", "BKSP", KeyboardButtonCategory.Navigation, KeyEvent.KEYCODE_DEL))
        add(keyOption("delete", "Delete", "DEL", KeyboardButtonCategory.Navigation, KeyEvent.KEYCODE_FORWARD_DEL))
        add(keyOption("insert", "Insert", "INS", KeyboardButtonCategory.Navigation, KeyEvent.KEYCODE_INSERT))
        add(keyOption("home", "Home", "HOME", KeyboardButtonCategory.Navigation, KeyEvent.KEYCODE_MOVE_HOME))
        add(keyOption("end", "End", "END", KeyboardButtonCategory.Navigation, KeyEvent.KEYCODE_MOVE_END))
        add(keyOption("page_up", "Page Up", "PGUP", KeyboardButtonCategory.Navigation, KeyEvent.KEYCODE_PAGE_UP))
        add(keyOption("page_down", "Page Down", "PGDN", KeyboardButtonCategory.Navigation, KeyEvent.KEYCODE_PAGE_DOWN))
        add(keyOption("arrow_up", "Arrow Up", "↑", KeyboardButtonCategory.Navigation, KeyEvent.KEYCODE_DPAD_UP))
        add(keyOption("arrow_down", "Arrow Down", "↓", KeyboardButtonCategory.Navigation, KeyEvent.KEYCODE_DPAD_DOWN))
        add(keyOption("arrow_left", "Arrow Left", "←", KeyboardButtonCategory.Navigation, KeyEvent.KEYCODE_DPAD_LEFT))
        add(keyOption("arrow_right", "Arrow Right", "→", KeyboardButtonCategory.Navigation, KeyEvent.KEYCODE_DPAD_RIGHT))
        add(keyOption("print_screen", "Print Screen", "PRTSC", KeyboardButtonCategory.Navigation, KeyEvent.KEYCODE_SYSRQ))
        add(keyOption("pause_break", "Pause / Break", "PAUSE", KeyboardButtonCategory.Navigation, KeyEvent.KEYCODE_BREAK))
        add(keyOption("menu", "Menu / Application", "MENU", KeyboardButtonCategory.Navigation, KeyEvent.KEYCODE_MENU))
        add(keyOption("clear", "Clear", "CLEAR", KeyboardButtonCategory.Navigation, KeyEvent.KEYCODE_CLEAR))

        add(keyOption("shift_left", "Left Shift", "SHIFT", KeyboardButtonCategory.Modifiers, KeyEvent.KEYCODE_SHIFT_LEFT))
        add(keyOption("shift_right", "Right Shift", "SHFT R", KeyboardButtonCategory.Modifiers, KeyEvent.KEYCODE_SHIFT_RIGHT))
        add(keyOption("ctrl_left", "Left Ctrl", "CTRL", KeyboardButtonCategory.Modifiers, KeyEvent.KEYCODE_CTRL_LEFT))
        add(keyOption("ctrl_right", "Right Ctrl", "CTRL R", KeyboardButtonCategory.Modifiers, KeyEvent.KEYCODE_CTRL_RIGHT))
        add(keyOption("alt_left", "Left Alt", "ALT", KeyboardButtonCategory.Modifiers, KeyEvent.KEYCODE_ALT_LEFT))
        add(keyOption("alt_right", "Right Alt / AltGr", "ALT R", KeyboardButtonCategory.Modifiers, KeyEvent.KEYCODE_ALT_RIGHT))
        add(keyOption("meta_left", "Left Meta / Windows", "WIN", KeyboardButtonCategory.Modifiers, KeyEvent.KEYCODE_META_LEFT))
        add(keyOption("meta_right", "Right Meta / Windows", "WIN R", KeyboardButtonCategory.Modifiers, KeyEvent.KEYCODE_META_RIGHT))
        add(keyOption("caps_lock", "Caps Lock", "CAPS", KeyboardButtonCategory.Modifiers, KeyEvent.KEYCODE_CAPS_LOCK))
        add(keyOption("num_lock", "Num Lock", "NUM", KeyboardButtonCategory.Modifiers, KeyEvent.KEYCODE_NUM_LOCK))
        add(keyOption("scroll_lock", "Scroll Lock", "SCRL", KeyboardButtonCategory.Modifiers, KeyEvent.KEYCODE_SCROLL_LOCK))

        val symbols = listOf(
            Triple("minus", "-", KeyEvent.KEYCODE_MINUS),
            Triple("equals", "=", KeyEvent.KEYCODE_EQUALS),
            Triple("left_bracket", "[", KeyEvent.KEYCODE_LEFT_BRACKET),
            Triple("right_bracket", "]", KeyEvent.KEYCODE_RIGHT_BRACKET),
            Triple("backslash", "\\", KeyEvent.KEYCODE_BACKSLASH),
            Triple("semicolon", ";", KeyEvent.KEYCODE_SEMICOLON),
            Triple("apostrophe", "'", KeyEvent.KEYCODE_APOSTROPHE),
            Triple("comma", ",", KeyEvent.KEYCODE_COMMA),
            Triple("period", ".", KeyEvent.KEYCODE_PERIOD),
            Triple("slash", "/", KeyEvent.KEYCODE_SLASH),
            Triple("grave", "`", KeyEvent.KEYCODE_GRAVE),
        )
        addAll(symbols.map { (id, label, keyCode) ->
            keyOption("symbol_$id", label, label, KeyboardButtonCategory.Symbols, keyCode)
        })

        addAll((0..9).map { digit ->
            keyOption("numpad_$digit", "Numpad $digit", "NUM $digit", KeyboardButtonCategory.Numpad,
                KeyEvent.KEYCODE_NUMPAD_0 + digit)
        })
        add(keyOption("numpad_enter", "Numpad Enter", "NUM ENT", KeyboardButtonCategory.Numpad, KeyEvent.KEYCODE_NUMPAD_ENTER))
        add(keyOption("numpad_add", "Numpad +", "NUM +", KeyboardButtonCategory.Numpad, KeyEvent.KEYCODE_NUMPAD_ADD))
        add(keyOption("numpad_subtract", "Numpad −", "NUM −", KeyboardButtonCategory.Numpad, KeyEvent.KEYCODE_NUMPAD_SUBTRACT))
        add(keyOption("numpad_multiply", "Numpad ×", "NUM ×", KeyboardButtonCategory.Numpad, KeyEvent.KEYCODE_NUMPAD_MULTIPLY))
        add(keyOption("numpad_divide", "Numpad ÷", "NUM ÷", KeyboardButtonCategory.Numpad, KeyEvent.KEYCODE_NUMPAD_DIVIDE))
        add(keyOption("numpad_decimal", "Numpad decimal", "NUM .", KeyboardButtonCategory.Numpad, KeyEvent.KEYCODE_NUMPAD_DOT))
        add(keyOption("numpad_comma", "Numpad comma", "NUM ,", KeyboardButtonCategory.Numpad, KeyEvent.KEYCODE_NUMPAD_COMMA))
        add(keyOption("numpad_equals", "Numpad =", "NUM =", KeyboardButtonCategory.Numpad, KeyEvent.KEYCODE_NUMPAD_EQUALS))
    }
}

private val keyboardButtonCatalogById: Map<String, KeyboardButtonOption> by lazy {
    keyboardButtonCatalog.associateBy(KeyboardButtonOption::id)
}

internal fun keyboardButtonOption(actionId: String): KeyboardButtonOption? = keyboardButtonCatalogById[actionId]

internal fun keyboardButtonCategoryLabelRes(category: KeyboardButtonCategory): Int = when (category) {
    KeyboardButtonCategory.Mouse -> R.string.keyboard_overlay_category_mouse
    KeyboardButtonCategory.Letters -> R.string.keyboard_overlay_category_letters
    KeyboardButtonCategory.Numbers -> R.string.keyboard_overlay_category_numbers
    KeyboardButtonCategory.FunctionKeys -> R.string.keyboard_overlay_category_functions
    KeyboardButtonCategory.Navigation -> R.string.keyboard_overlay_category_navigation
    KeyboardButtonCategory.Modifiers -> R.string.keyboard_overlay_category_modifiers
    KeyboardButtonCategory.Symbols -> R.string.keyboard_overlay_category_symbols
    KeyboardButtonCategory.Numpad -> R.string.keyboard_overlay_category_numpad
}

/** Modifier bits travel with each virtual key packet, including movement key releases. */
internal fun keyboardModifierMaskForKeys(pressedKeyCodes: Set<Int>): Int {
    var modifiers = 0
    if (KeyEvent.KEYCODE_SHIFT_LEFT in pressedKeyCodes || KeyEvent.KEYCODE_SHIFT_RIGHT in pressedKeyCodes) {
        modifiers = modifiers or 0x01
    }
    if (KeyEvent.KEYCODE_CTRL_LEFT in pressedKeyCodes || KeyEvent.KEYCODE_CTRL_RIGHT in pressedKeyCodes) {
        modifiers = modifiers or 0x02
    }
    if (KeyEvent.KEYCODE_ALT_LEFT in pressedKeyCodes || KeyEvent.KEYCODE_ALT_RIGHT in pressedKeyCodes) {
        modifiers = modifiers or 0x04
    }
    if (KeyEvent.KEYCODE_META_LEFT in pressedKeyCodes || KeyEvent.KEYCODE_META_RIGHT in pressedKeyCodes) {
        modifiers = modifiers or 0x08
    }
    return modifiers
}

internal const val DEFAULT_KEYBOARD_LMB_ID = "default_lmb"
internal const val DEFAULT_KEYBOARD_MMB_ID = "default_mmb"
internal const val DEFAULT_KEYBOARD_RMB_ID = "default_rmb"
internal const val DEFAULT_KEYBOARD_SHIFT_ID = "default_shift"
internal const val DEFAULT_KEYBOARD_CTRL_ID = "default_ctrl"
internal const val DEFAULT_KEYBOARD_F_ID = "default_f"
internal const val DEFAULT_KEYBOARD_Q_ID = "default_q"
internal const val MAX_KEYBOARD_OVERLAY_BUTTONS = 128

internal fun defaultKeyboardOverlayButtons(): List<KeyboardOverlayButton> = listOf(
    KeyboardOverlayButton(DEFAULT_KEYBOARD_LMB_ID, "mouse_left", layoutIndex = 0),
    KeyboardOverlayButton(DEFAULT_KEYBOARD_MMB_ID, "mouse_middle", layoutIndex = 1),
    KeyboardOverlayButton(DEFAULT_KEYBOARD_RMB_ID, "mouse_right", layoutIndex = 2),
    KeyboardOverlayButton(DEFAULT_KEYBOARD_SHIFT_ID, "shift_left", layoutIndex = 3),
    KeyboardOverlayButton(DEFAULT_KEYBOARD_CTRL_ID, "ctrl_left", layoutIndex = 4),
    KeyboardOverlayButton(DEFAULT_KEYBOARD_F_ID, "letter_f", layoutIndex = 5),
    KeyboardOverlayButton(DEFAULT_KEYBOARD_Q_ID, "letter_q", layoutIndex = 6),
)

internal fun KeyboardOverlayButton.appearanceKey(): String = when (id) {
    DEFAULT_KEYBOARD_LMB_ID -> "keyboard_lmb"
    DEFAULT_KEYBOARD_MMB_ID -> "keyboard_mmb"
    DEFAULT_KEYBOARD_RMB_ID -> "keyboard_rmb"
    DEFAULT_KEYBOARD_SHIFT_ID -> "keyboard_shift"
    DEFAULT_KEYBOARD_CTRL_ID -> "keyboard_ctrl"
    DEFAULT_KEYBOARD_F_ID -> "keyboard_f"
    DEFAULT_KEYBOARD_Q_ID -> "keyboard_q"
    else -> "keyboard_custom_$id"
}

internal fun KeyboardOverlayButton.positionKey(): String = "keyboard_button_$id"

internal fun newKeyboardOverlayButton(actionId: String): KeyboardOverlayButton? {
    if (keyboardButtonOption(actionId) == null) return null
    return KeyboardOverlayButton("user_${UUID.randomUUID()}", actionId)
}

internal fun normalizeKeyboardOverlayButtons(buttons: List<KeyboardOverlayButton>): List<KeyboardOverlayButton> {
    val seen = mutableSetOf<String>()
    val seenSlots = mutableSetOf<Int>()
    var nextCustomSlot = 7
    val validButtons = buttons.filter { button ->
        button.id.length in 1..64 && button.id.all { it.isLetterOrDigit() || it == '_' || it == '-' } &&
            keyboardButtonOption(button.actionId) != null && seen.add(button.id)
    }.take(MAX_KEYBOARD_OVERLAY_BUTTONS)
    return validButtons.map { button ->
        val legacySlot = when (button.id) {
            DEFAULT_KEYBOARD_LMB_ID -> 0
            DEFAULT_KEYBOARD_MMB_ID -> 1
            DEFAULT_KEYBOARD_RMB_ID -> 2
            DEFAULT_KEYBOARD_SHIFT_ID -> 3
            DEFAULT_KEYBOARD_CTRL_ID -> 4
            DEFAULT_KEYBOARD_F_ID -> 5
            DEFAULT_KEYBOARD_Q_ID -> 6
            else -> null
        }
        val savedSlot = button.layoutIndex.takeIf { it in 7..10_000 && it !in seenSlots }
        val slot = legacySlot ?: savedSlot ?: run {
            while (nextCustomSlot in seenSlots) nextCustomSlot++
            nextCustomSlot++
            nextCustomSlot - 1
        }
        seenSlots += slot
        button.copy(layoutIndex = slot)
    }
}

internal fun AndroidTouchSettings.withKeyboardButtonAdded(button: KeyboardOverlayButton): AndroidTouchSettings {
    if (normalizeKeyboardOverlayButtons(listOf(button)).isEmpty() || keyboardButtons.any { it.id == button.id }) return this
    val normalizedButtons = normalizeKeyboardOverlayButtons(keyboardButtons)
    if (normalizedButtons.size >= MAX_KEYBOARD_OVERLAY_BUTTONS) return this
    val nextSlot = (normalizedButtons.maxOfOrNull(KeyboardOverlayButton::layoutIndex) ?: 6) + 1
    return copy(keyboardButtons = normalizedButtons + button.copy(layoutIndex = nextSlot))
}

internal fun AndroidTouchSettings.withKeyboardButtonRemoved(id: String): AndroidTouchSettings {
    val button = keyboardButtons.firstOrNull { it.id == id } ?: return this
    val positionKey = button.positionKey()
    return copy(
        keyboardButtons = keyboardButtons.filterNot { it.id == id },
        buttonAppearances = buttonAppearances - button.appearanceKey(),
        offsets = offsets - "${positionKey}_landscape" - "${positionKey}_portrait",
    )
}

/** Slot offsets are translations from a bottom-right anchor; every key remains independently movable. */
internal fun keyboardButtonDefaultOffset(index: Int, spacingDp: Float, migratedGroupOffset: TouchOffset = TouchOffset()): TouchOffset {
    if (index < 0 || !spacingDp.isFinite() || spacingDp <= 0f) return TouchOffset()
    val (column, row) = when (index) {
        0 -> 0 to 2
        1 -> 1 to 2
        2 -> 2 to 2
        3 -> 1 to 1
        4 -> 2 to 1
        5 -> 1 to 0
        6 -> 2 to 0
        else -> {
            val additionIndex = index - 7
            (additionIndex % 3) to (3 + additionIndex / 3)
        }
    }
    val x = (column - 2) * spacingDp
    val y = -row * spacingDp
    val migration = if (index < 7) migratedGroupOffset else TouchOffset()
    return TouchOffset(x + migration.x, y + migration.y)
}

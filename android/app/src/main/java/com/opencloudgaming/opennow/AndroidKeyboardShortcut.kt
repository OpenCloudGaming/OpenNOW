package com.opencloudgaming.opennow

import android.view.KeyEvent

internal const val DEFAULT_ANDROID_STREAM_MENU_SHORTCUT = "Ctrl+Shift+G"
internal const val DISABLED_ANDROID_STREAM_MENU_SHORTCUT = "Disabled"

private data class AndroidKeyboardShortcut(
    val keyCode: Int,
    val ctrl: Boolean,
    val alt: Boolean,
    val shift: Boolean,
    val meta: Boolean,
)

/**
 * Turns a physical key press into the stable value persisted in [AppSettings]. Modifier-only and
 * controller events are ignored so opening the editor with a gamepad cannot replace the binding.
 */
internal fun androidKeyboardShortcutFromEvent(event: KeyEvent): String? {
    return androidKeyboardShortcutFromKey(
        keyCode = event.keyCode,
        action = event.action,
        repeatCount = event.repeatCount,
        controllerInputDevice = AndroidControllerInput.isControllerEvent(event.source, event.deviceId),
        ctrlPressed = event.isCtrlPressed,
        altPressed = event.isAltPressed,
        shiftPressed = event.isShiftPressed,
        metaPressed = event.isMetaPressed,
    )
}

internal fun androidKeyboardShortcutFromKey(
    keyCode: Int,
    action: Int = KeyEvent.ACTION_DOWN,
    repeatCount: Int = 0,
    controllerInputDevice: Boolean = false,
    ctrlPressed: Boolean = false,
    altPressed: Boolean = false,
    shiftPressed: Boolean = false,
    metaPressed: Boolean = false,
): String? {
    if (action != KeyEvent.ACTION_DOWN || repeatCount != 0 || controllerInputDevice) return null
    if (keyCode == KeyEvent.KEYCODE_BACK || keyCode.isShortcutModifierKey()) return null
    val key = shortcutKeyLabel(keyCode) ?: return null
    return canonicalShortcut(
        key = key,
        ctrl = ctrlPressed,
        alt = altPressed,
        shift = shiftPressed,
        meta = metaPressed,
    )
}

internal fun matchesAndroidKeyboardShortcut(event: KeyEvent, configuredShortcut: String): Boolean {
    return matchesAndroidKeyboardShortcut(
        keyCode = event.keyCode,
        ctrlPressed = event.isCtrlPressed,
        altPressed = event.isAltPressed,
        shiftPressed = event.isShiftPressed,
        metaPressed = event.isMetaPressed,
        configuredShortcut = configuredShortcut,
    )
}

internal fun matchesAndroidKeyboardShortcut(
    keyCode: Int,
    ctrlPressed: Boolean = false,
    altPressed: Boolean = false,
    shiftPressed: Boolean = false,
    metaPressed: Boolean = false,
    configuredShortcut: String,
): Boolean {
    if (configuredShortcut.equals(DISABLED_ANDROID_STREAM_MENU_SHORTCUT, ignoreCase = true)) return false
    val shortcut = parseAndroidKeyboardShortcut(configuredShortcut) ?: return false
    return keyCode == shortcut.keyCode &&
        ctrlPressed == shortcut.ctrl &&
        altPressed == shortcut.alt &&
        shiftPressed == shortcut.shift &&
        metaPressed == shortcut.meta
}

internal fun androidKeyboardShortcutDisplay(configuredShortcut: String): String =
    parseAndroidKeyboardShortcut(configuredShortcut)?.let { shortcut ->
        canonicalShortcut(
            key = shortcutKeyLabel(shortcut.keyCode) ?: "Key${shortcut.keyCode}",
            ctrl = shortcut.ctrl,
            alt = shortcut.alt,
            shift = shortcut.shift,
            meta = shortcut.meta,
        )
    } ?: if (configuredShortcut.equals(DISABLED_ANDROID_STREAM_MENU_SHORTCUT, ignoreCase = true)) {
        DISABLED_ANDROID_STREAM_MENU_SHORTCUT
    } else {
        DEFAULT_ANDROID_STREAM_MENU_SHORTCUT
    }

private fun parseAndroidKeyboardShortcut(raw: String): AndroidKeyboardShortcut? {
    val tokens = raw.split('+').map(String::trim).filter(String::isNotEmpty)
    if (tokens.isEmpty()) return null
    var ctrl = false
    var alt = false
    var shift = false
    var meta = false
    var keyCode: Int? = null
    tokens.forEach { token ->
        when (token.lowercase()) {
            "ctrl", "control" -> if (ctrl) return null else ctrl = true
            "alt" -> if (alt) return null else alt = true
            "shift" -> if (shift) return null else shift = true
            "meta", "cmd", "command" -> if (meta) return null else meta = true
            else -> {
                if (keyCode != null) return null
                keyCode = shortcutKeyCode(token) ?: return null
            }
        }
    }
    return AndroidKeyboardShortcut(keyCode ?: return null, ctrl, alt, shift, meta)
}

private fun canonicalShortcut(
    key: String,
    ctrl: Boolean,
    alt: Boolean,
    shift: Boolean,
    meta: Boolean,
): String = buildList {
    if (ctrl) add("Ctrl")
    if (alt) add("Alt")
    if (shift) add("Shift")
    if (meta) add("Meta")
    add(key)
}.joinToString("+")

private fun shortcutKeyLabel(keyCode: Int): String? = when (keyCode) {
    in KeyEvent.KEYCODE_A..KeyEvent.KEYCODE_Z -> ('A'.code + keyCode - KeyEvent.KEYCODE_A).toChar().toString()
    in KeyEvent.KEYCODE_0..KeyEvent.KEYCODE_9 -> ('0'.code + keyCode - KeyEvent.KEYCODE_0).toChar().toString()
    in KeyEvent.KEYCODE_F1..KeyEvent.KEYCODE_F12 -> "F${keyCode - KeyEvent.KEYCODE_F1 + 1}"
    KeyEvent.KEYCODE_SPACE -> "Space"
    KeyEvent.KEYCODE_ENTER -> "Enter"
    KeyEvent.KEYCODE_NUMPAD_ENTER -> "NumpadEnter"
    KeyEvent.KEYCODE_TAB -> "Tab"
    KeyEvent.KEYCODE_ESCAPE -> "Escape"
    KeyEvent.KEYCODE_FORWARD_DEL -> "Delete"
    KeyEvent.KEYCODE_DEL -> "Backspace"
    KeyEvent.KEYCODE_INSERT -> "Insert"
    KeyEvent.KEYCODE_HOME -> "Home"
    KeyEvent.KEYCODE_MOVE_END -> "End"
    KeyEvent.KEYCODE_PAGE_UP -> "PageUp"
    KeyEvent.KEYCODE_PAGE_DOWN -> "PageDown"
    KeyEvent.KEYCODE_DPAD_UP -> "Up"
    KeyEvent.KEYCODE_DPAD_DOWN -> "Down"
    KeyEvent.KEYCODE_DPAD_LEFT -> "Left"
    KeyEvent.KEYCODE_DPAD_RIGHT -> "Right"
    KeyEvent.KEYCODE_MENU -> "Menu"
    else -> keyCode.takeIf { it > KeyEvent.KEYCODE_UNKNOWN }?.let { "Key$it" }
}

private fun shortcutKeyCode(label: String): Int? {
    val normalized = label.trim().lowercase()
    if (normalized.length == 1) {
        val char = normalized.single()
        if (char in 'a'..'z') return KeyEvent.KEYCODE_A + (char - 'a')
        if (char in '0'..'9') return KeyEvent.KEYCODE_0 + (char - '0')
    }
    if (normalized.startsWith("f")) {
        val functionNumber = normalized.drop(1).toIntOrNull()
        if (functionNumber != null && functionNumber in 1..12) {
            return KeyEvent.KEYCODE_F1 + functionNumber - 1
        }
    }
    if (normalized.startsWith("key")) {
        return normalized.drop(3).toIntOrNull()?.takeIf { it > KeyEvent.KEYCODE_UNKNOWN }
    }
    return when (normalized) {
        "space" -> KeyEvent.KEYCODE_SPACE
        "enter" -> KeyEvent.KEYCODE_ENTER
        "numpadenter" -> KeyEvent.KEYCODE_NUMPAD_ENTER
        "tab" -> KeyEvent.KEYCODE_TAB
        "escape", "esc" -> KeyEvent.KEYCODE_ESCAPE
        "delete" -> KeyEvent.KEYCODE_FORWARD_DEL
        "backspace" -> KeyEvent.KEYCODE_DEL
        "insert" -> KeyEvent.KEYCODE_INSERT
        "home" -> KeyEvent.KEYCODE_HOME
        "end" -> KeyEvent.KEYCODE_MOVE_END
        "pageup" -> KeyEvent.KEYCODE_PAGE_UP
        "pagedown" -> KeyEvent.KEYCODE_PAGE_DOWN
        "up" -> KeyEvent.KEYCODE_DPAD_UP
        "down" -> KeyEvent.KEYCODE_DPAD_DOWN
        "left" -> KeyEvent.KEYCODE_DPAD_LEFT
        "right" -> KeyEvent.KEYCODE_DPAD_RIGHT
        "menu" -> KeyEvent.KEYCODE_MENU
        else -> null
    }
}

private fun Int.isShortcutModifierKey(): Boolean =
    this == KeyEvent.KEYCODE_CTRL_LEFT ||
        this == KeyEvent.KEYCODE_CTRL_RIGHT ||
        this == KeyEvent.KEYCODE_ALT_LEFT ||
        this == KeyEvent.KEYCODE_ALT_RIGHT ||
        this == KeyEvent.KEYCODE_SHIFT_LEFT ||
        this == KeyEvent.KEYCODE_SHIFT_RIGHT ||
        this == KeyEvent.KEYCODE_META_LEFT ||
        this == KeyEvent.KEYCODE_META_RIGHT

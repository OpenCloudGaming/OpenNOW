pragma Singleton
import QtQuick

QtObject {
    readonly property var assets: ({
        "A": "xbox_button_a",
        "B": "xbox_button_b",
        "X": "xbox_button_x",
        "Y": "xbox_button_y",
        "LB": "xbox_lb",
        "RB": "xbox_rb",
        "LT": "xbox_lt",
        "RT": "xbox_rt",
        "MENU": "xbox_button_menu",
        "VIEW": "xbox_button_view",
        "GUIDE": "xbox_guide",
        "controller": "controller_xboxseries",
        "xbox": "controller_xboxseries",
        "playstation": "controller_playstation5",
        "keyboard": "keyboard"
    })

    function sourceFor(glyph: string, ink: color): url {
        return assetSource(assets.hasOwnProperty(glyph) ? assets[glyph] : "", ink)
    }

    readonly property var keyboardAliases: ({
        "ctrl": Qt.platform.os === "osx" ? "command" : "ctrl",
        "control": Qt.platform.os === "osx" ? "command" : "ctrl",
        "shift": "shift_icon", "alt": Qt.platform.os === "osx" ? "option" : "alt",
        "cmd": "command", "command": "command", "⌘": "command", "option": "option",
        "win": "win", "super": "win", "meta": Qt.platform.os === "osx" ? "ctrl" : "win",
        "esc": "escape", "escape": "escape", "enter": "enter", "return": "return",
        "tab": "tab_icon", "space": "space_icon", "backspace": "backspace_icon",
        "del": "delete", "delete": "delete", "ins": "insert", "insert": "insert",
        "home": "home", "end": "end", "pgup": "page_up", "pageup": "page_up",
        "page up": "page_up", "pgdown": "page_down", "pgdn": "page_down",
        "pagedown": "page_down", "page down": "page_down", "pause": "pause",
        "print": "printscreen", "printscreen": "printscreen",
        "up": "arrow_up", "↑": "arrow_up", "down": "arrow_down", "↓": "arrow_down",
        "left": "arrow_left", "←": "arrow_left", "right": "arrow_right", "→": "arrow_right",
        "arrows": "arrows", "↑ ↓": "arrows_vertical",
        "/": "slash_forward", "\\": "slash_back", "?": "question", ",": "comma",
        ".": "period", "+": "plus", "-": "minus", "=": "equals", ";": "semicolon",
        "'": "apostrophe", "[": "bracket_open", "]": "bracket_close", "`": "tilde"
    })

    function keyboardAsset(key: string): string {
        const normalized = key.trim().toLowerCase()
        if (/^[a-z0-9]$/.test(normalized) || /^f([1-9]|1[0-2])$/.test(normalized))
            return "keyboard_" + normalized
        return keyboardAliases.hasOwnProperty(normalized) ? "keyboard_" + keyboardAliases[normalized] : ""
    }

    function keyboardSourceFor(key: string, ink: color): url {
        return assetSource(keyboardAsset(key), ink)
    }

    function keysFor(shortcut: string): list<string> {
        const sequence = shortcut.trim()
        if (!sequence)
            return []
        if (keyboardAsset(sequence))
            return [sequence]
        const keys = sequence.endsWith("++")
            ? sequence.slice(0, -2).split("+").concat(["+"])
            : sequence.split(/[+\s]+/)
        return keys.length <= 5 && keys.every(key => keyboardAsset(key.trim()) !== "")
            ? keys.map(key => key.trim()) : [sequence]
    }

    function assetSource(asset: string, ink: color): url {
        if (!asset)
            return ""
        const dark = 0.299 * ink.r + 0.587 * ink.g + 0.114 * ink.b < 0.5
        return "qrc:/qt/qml/OpenNOW/res/input-prompts/" + asset + (dark ? "-dark.svg" : ".svg")
    }
}

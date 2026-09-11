import QtQuick
import QtTest
import OpenNOW

TestCase {
    id: testCase
    name: "KeyboardIcons"
    when: windowShown
    visible: true
    width: 800
    height: 240

    Component { id: imageComponent; Image { width: 32; height: 32 } }
    Component { id: glyphComponent; KeyboardGlyph {} }
    Component { id: hintComponent; DesktopKeyHint {} }
    Component { id: buttonComponent; DesktopSettingsButton { text: "Ctrl+F12"; keySequence: text } }
    Component { id: desktopButtonComponent; DesktopButton { text: "Play" } }

    function init() {
        ShellStore.settings = {appTheme: "dark"}
    }

    function keyItems(glyph) {
        return Array.from(glyph.children).filter(child => child.modelData !== undefined)
    }

    function test_assets_data() {
        const keys = Object.keys(InputPromptIcons.keyboardAliases).concat("ABCDEFGHIJKLMNOPQRSTUVWXYZ0123456789".split(""))
        for (let i = 1; i <= 12; ++i)
            keys.push("F" + i)
        const rows = []
        for (const key of keys) {
            rows.push({tag: key + "-white", key: key, ink: "white"})
            rows.push({tag: key + "-dark", key: key, ink: "#111827"})
        }
        return rows
    }

    function test_assets(data) {
        const image = createTemporaryObject(imageComponent, testCase, {
            source: InputPromptIcons.keyboardSourceFor(data.key, data.ink)
        })
        verify(image.source.toString().indexOf("/keyboard_") >= 0)
        tryCompare(image, "status", Image.Ready)
        compare(image.source.toString().endsWith("-dark.svg"), data.ink !== "white")
    }

    function test_chords_data() {
        return [
            {tag: "plus-delimited", input: "Ctrl+Shift+K", keys: ["Ctrl", "Shift", "K"]},
            {tag: "space-delimited", input: "Ctrl  K", keys: ["Ctrl", "K"]},
            {tag: "trimmed", input: " Ctrl + K ", keys: ["Ctrl", "K"]},
            {tag: "literal-plus", input: "+", keys: ["+"]},
            {tag: "modified-plus", input: "Ctrl++", keys: ["Ctrl", "+"]},
            {tag: "modified-comma", input: "Ctrl+,", keys: ["Ctrl", ","]},
            {tag: "all-modifiers", input: "Ctrl+Shift+Alt+Meta+A", keys: ["Ctrl", "Shift", "Alt", "Meta", "A"]},
            {tag: "page-up", input: "Page Up", keys: ["Page Up"]},
            {tag: "navigation", input: "↑ ↓", keys: ["↑ ↓"]},
            {tag: "unavailable-key", input: "Ctrl+F13", keys: ["Ctrl+F13"]},
            {tag: "unknown-key", input: "Media Play", keys: ["Media Play"]},
            {tag: "multi-stroke", input: "Ctrl+K, Ctrl+C", keys: ["Ctrl+K, Ctrl+C"]},
            {tag: "empty", input: "", keys: []}
        ]
    }

    function test_chords(data) {
        compare(JSON.stringify(InputPromptIcons.keysFor(data.input)), JSON.stringify(data.keys))
    }

    function test_fallbackAndDeviceSeparation() {
        for (const key of ["F13", "toString", "constructor", "Media Play"]) {
            compare(InputPromptIcons.keyboardSourceFor(key, "white").toString(), "")
            const glyph = createTemporaryObject(glyphComponent, testCase, {shortcut: key})
            const item = keyItems(glyph)[0]
            compare(item.children[1].visible, true)
            compare(item.children[1].children[0].text, key)
        }
        verify(InputPromptIcons.sourceFor("A", "white").toString().endsWith("xbox_button_a.svg"))
        verify(InputPromptIcons.keyboardSourceFor("A", "white").toString().endsWith("keyboard_a.svg"))
        compare(InputPromptIcons.sourceFor("+", "white").toString(), "")
    }

    function test_platformModifiers() {
        const mac = Qt.platform.os === "osx"
        compare(InputPromptIcons.keyboardAsset("Ctrl"), mac ? "keyboard_command" : "keyboard_ctrl")
        compare(InputPromptIcons.keyboardAsset("Meta"), mac ? "keyboard_ctrl" : "keyboard_win")
        compare(InputPromptIcons.keyboardAsset("Alt"), mac ? "keyboard_option" : "keyboard_alt")
    }

    function test_sizingAndThemeChanges() {
        const glyph = createTemporaryObject(glyphComponent, testCase, {shortcut: "Ctrl+K", keySize: 24})
        const items = keyItems(glyph)
        compare(items.length, 2)
        for (const item of items)
            tryCompare(item.children[0], "status", Image.Ready)
        tryCompare(glyph, "height", 24)
        compare(Math.round(items[0].width), Qt.platform.os === "osx" ? 24 : 32)
        verify(glyph.width > 48)
        ShellStore.settings = {appTheme: "light"}
        tryVerify(() => items.every(item => item.children[0].source.toString().endsWith("-dark.svg")))
        glyph.keySize = 18
        tryCompare(glyph, "height", 18)
        verify(glyph.width < 48)
    }

    function test_translatedAccessibleLabel() {
        const hint = createTemporaryObject(hintComponent, testCase, {keyText: "Entrée", shortcut: "Enter", label: "Jouer"})
        const glyph = hint.children[0]
        compare(glyph.Accessible.name, "Entrée")
        verify(keyItems(glyph)[0].children[0].source.toString().endsWith("keyboard_enter.svg"))
        const button = createTemporaryObject(desktopButtonComponent, testCase, {shortcutText: "Entrée", shortcutSequence: "Enter"})
        const buttonGlyph = button.contentItem.children[0].children[3]
        compare(buttonGlyph.Accessible.name, "Entrée")
        verify(keyItems(buttonGlyph)[0].children[0].source.toString().endsWith("keyboard_enter.svg"))
    }

    function test_shortcutButtonActivation() {
        const button = createTemporaryObject(buttonComponent, testCase)
        let clicks = 0
        button.clicked.connect(() => ++clicks)
        button.forceActiveFocus()
        keyClick(Qt.Key_Space)
        compare(clicks, 1)
        compare(button.text, "Ctrl+F12")
        verify(button.implicitWidth >= 84)
    }
}

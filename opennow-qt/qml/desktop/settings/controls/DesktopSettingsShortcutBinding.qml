import QtQuick
import OpenNOW

QtObject {
    readonly property var defaults: ({
        shortcutToggleStats: "Ctrl+N", shortcutTogglePointerLock: "F8",
        shortcutToggleFullscreen: "F11", shortcutStopStream: "Ctrl+Shift+Q",
        shortcutToggleAntiAfk: "Ctrl+Shift+K", shortcutToggleMicrophone: "Ctrl+Shift+M",
        shortcutScreenshot: "Ctrl+F11", shortcutToggleRecording: "F12",
        shortcutSaveClip: "Ctrl+F12"
    })
    readonly property var commands: [
        {key: "shortcutToggleStats", title: qsTr("Stats overlay")},
        {key: "shortcutToggleFullscreen", title: qsTr("Toggle fullscreen")},
        {key: "shortcutTogglePointerLock", title: qsTr("Grab or release the mouse")},
        {key: "shortcutScreenshot", title: qsTr("Screenshot the stream")},
        {key: "shortcutToggleMicrophone", title: qsTr("Toggle microphone")},
        {key: "shortcutToggleRecording", title: qsTr("Toggle recording")},
        {key: "shortcutSaveClip", title: qsTr("Save replay clip")},
        {key: "shortcutToggleAntiAfk", title: qsTr("Toggle Anti-AFK")},
        {key: "shortcutStopStream", title: qsTr("End the session")}
    ]

    function value(key) {
        return String(ShellStore.settings[key] ?? defaults[key] ?? "")
    }

    function isDefault(key) {
        return AppController.normalizeShortcut(value(key)) === AppController.normalizeShortcut(defaults[key])
    }

    function title(key) {
        const command = commands.find(item => item.key === key)
        return command ? command.title : ""
    }

    function owner(chord, exceptKey) {
        const normalized = AppController.normalizeShortcut(chord)
        if (normalized === "")
            return ""
        return Object.keys(defaults).find(key => key !== exceptKey
            && AppController.normalizeShortcut(value(key)) === normalized) || ""
    }

    function changedBindings() {
        const changes = {}
        for (const key of Object.keys(defaults)) {
            if (!isDefault(key))
                changes[key] = defaults[key]
        }
        return changes
    }

    function validateChord(key, chord) {
        if (chord === "Ctrl+G")
            return {error: qsTr("Ctrl+G is reserved for the in-stream Guide."), reason: "reserved"}
        if (chord === "Shift+F3")
            return {error: qsTr("That shortcut is reserved for stream statistics."), reason: "reserved"}
        const conflict = owner(chord, key)
        if (conflict !== "")
            return {error: qsTr("That shortcut is already assigned."), reason: "conflict", owner: conflict, attempted: chord}
        return {chord: chord}
    }

    function validate(key, event) {
        const chord = AppController.shortcutFromKey(event.key, event.modifiers)
        if (!chord)
            return {error: qsTr("Press a letter, number, function key, or navigation key."), reason: "invalid"}
        if ((event.key >= Qt.Key_A && event.key <= Qt.Key_Z
                || event.key >= Qt.Key_0 && event.key <= Qt.Key_9)
                && !(event.modifiers & (Qt.ControlModifier | Qt.ShiftModifier | Qt.AltModifier | Qt.MetaModifier)))
            return {error: qsTr("Add Ctrl, Shift, Alt, or Meta to letter and number shortcuts."), reason: "invalid"}
        return validateChord(key, chord)
    }
}

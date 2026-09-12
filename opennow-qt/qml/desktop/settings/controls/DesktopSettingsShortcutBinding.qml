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

    function value(key) {
        return String(ShellStore.settings[key] || defaults[key] || "")
    }

    function validate(key, event) {
        const chord = AppController.shortcutFromKey(event.key, event.modifiers)
        if (!chord)
            return {error: qsTr("Press a letter, number, function key, or navigation key.")}
        if ((event.key >= Qt.Key_A && event.key <= Qt.Key_Z
                || event.key >= Qt.Key_0 && event.key <= Qt.Key_9)
                && !(event.modifiers & (Qt.ControlModifier | Qt.ShiftModifier | Qt.AltModifier | Qt.MetaModifier)))
            return {error: qsTr("Add Ctrl, Shift, Alt, or Meta to letter and number shortcuts.")}
        if (chord === "Ctrl+G")
            return {error: qsTr("Ctrl+G is reserved for the in-stream Guide.")}
        if ((chord === "F3" && key !== "shortcutToggleStats") || chord === "Shift+F3")
            return {error: qsTr("That shortcut is reserved for stream statistics.")}
        for (const otherKey of Object.keys(defaults)) {
            if (otherKey !== key && AppController.normalizeShortcut(value(otherKey)) === chord)
                return {error: qsTr("That shortcut is already assigned.")}
        }
        return {chord: chord}
    }
}

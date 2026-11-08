import QtQuick
import QtQuick.Controls
import QtQuick.Window
import OpenNOW

QtObject {
    property var host: null
    property var dialog: null
    property int phase: 0
    property int key: 0
    property int modifiers: 0
    property int waited: 0
    property bool settling: false
    onKeyChanged: if (key !== 0) settling = true

    function check(value, message) { if (!value) throw new Error("Clock skew acceptance: " + message) }
    function waitFor(condition, message) {
        if (condition) { waited = 0; return true }
        check(++waited < 80, message)
        return false
    }
    function run(host) {
        this.host = host
        for (const object of host.Window.window.contentData) {
            if (object.objectName === "clockSkewDialog") dialog = object
        }
        check(dialog !== null, "missing the actual Main.qml dialog")
        ShellStore.streamCreateRequestId = "clock-skew-deferred"
        ShellStore.clockSkewDismissed = false
        ShellStore.clockSkewMs = 7 * 60 * 60 * 1000
        check(!ShellStore.clockSkewSessionSafe && !dialog.visible, "a warning interrupted session preparation")
        return true
    }
    function advance() {
        if (settling) { settling = false; return 0 }
        if (phase === 0) {
            check(!dialog.visible, "a deferred warning opened early")
            ShellStore.streamCreateRequestId = ""
            phase++
            return 0
        }
        if (phase === 1) {
            check(ShellStore.clockSkewSessionSafe, "the fixture is not session-safe")
            if (!waitFor(dialog.opened, "a safe idle shell did not show its clock warning")) return 0
            check(dialog.activeFocus, "the clock warning did not own keyboard focus")
            check(dialog.contentItem.textFormat === Text.PlainText, "the warning is not plain text")
            key = Qt.Key_Escape
            phase++
            return 0
        }
        if (phase === 2) {
            check(!dialog.visible && ShellStore.clockSkewDismissed, "Escape did not dismiss the warning in ShellStore")
            ShellStore.clockSkewMs = -12 * 60 * 1000
            ShellStore.clockSkewDismissed = false
            host.Window.window.showFullScreen()
            phase++
            return 0
        }
        if (phase === 3) {
            if (!waitFor(dialog.opened && host.Window.window.visibility === Window.FullScreen,
                    "the new warning did not open in fullscreen")) return 0
            key = Qt.Key_Return
            phase++
            return 0
        }
        if (phase === 4) {
            check(!dialog.visible && ShellStore.clockSkewDismissed, "Return did not dismiss the fullscreen warning")
            ShellStore.clockSkewDismissed = false
            phase++
            return 0
        }
        if (phase === 5) {
            if (!waitFor(dialog.opened && dialog.activeFocus, "a repeated warning did not regain focus")) return 0
            console.log("CLOCK SKEW PASS deferred session, idle warning, Escape rejection, new fullscreen warning, Return")
            return 1
        }
        return -1
    }
}

import QtQuick
import QtQuick.Controls
import QtQuick.Window
import OpenNOW

QtObject {
    property var host: null
    property var dialog: null
    property var warning: null
    property var loader: null
    property string order: "update-first"
    property int phase: 0
    property int key: 0
    property int modifiers: 0
    property int padButton: -1
    property var padButtons: ({})
    property int settling: 0
    property int waited: 0
    property int stableTurns: 0
    property int focusChanges: 0
    property var stableItem: null
    property Connections focusObserver: Connections {
        target: host ? host.Window.window : null
        function onActiveFocusItemChanged() { focusChanges++ }
    }

    function check(value, message) { if (!value) throw new Error("Clock overlap " + order + ": " + message) }
    function window() { return host.Window.window }
    function namedChild(item, name) {
        if (item.objectName === name) return item
        for (const child of item.children || []) {
            const found = namedChild(child, name)
            if (found) return found
        }
        return null
    }
    function inside(owner) {
        for (let item = window().activeFocusItem; item; item = item.parent) {
            if (item === owner) return true
        }
        return false
    }
    function waitFor(condition, message) {
        if (condition) { waited = 0; return true }
        check(++waited < 80, message)
        return false
    }
    function stable(owner) {
        check(owner === dialog ? dialog.activeFocus : inside(owner), "focus escaped the current modal")
        if (stableTurns++ === 0) {
            stableItem = window().activeFocusItem
            focusChanges = 0
        }
        window().restoreShellFocus()
        check(window().activeFocusItem === stableItem && focusChanges === 0, "modal focus oscillated across event-loop turns")
        if (stableTurns < 12) return false
        stableTurns = 0
        return true
    }
    function press(value) { key = value; settling = 2; phase++; return 0 }
    function pad(value) { padButton = value; settling = 5; phase++; return 0 }
    function clockOwnsInput() {
        check(dialog.opened && dialog.activeFocus, "the clock did not regain focus")
        check(window().consoleModalFocusOwner === dialog.contentItem, "the visible clock has no console input sentinel")
        check(!loader.enabled && !window().accountShortcutsEnabled && !window().consoleLaunchAllowed,
            "background console input remains enabled behind the clock")
        check(AppController.route === "home" && AppController.overlay === "", "input navigated behind the clock")
    }
    function run(host) {
        this.host = host
        for (const object of window().contentData) {
            if (object.objectName === "clockSkewDialog") dialog = object
        }
        warning = namedChild(host, "consoleSystemWarning")
        loader = namedChild(host, "mainRouteLoader")
        check(dialog && warning && loader && !window().desktopSurfaceActive, "missing real console warning components")
        check(ShellStore.clockSkewSessionSafe && ShellStore.updaterSessionSafe, "the fixture is not session-safe")
        if (order === "update-first") ShellStore.updaterFailureMessage = "Fixture update failure"
        else ShellStore.clockSkewMs = 7 * 60 * 60 * 1000
        return true
    }
    function advance() {
        if (settling > 0) { settling--; return 0 }
        if (phase === 0) {
            if (!waitFor(ControllerInput.controllerCount > 0 && window().active,
                    "the actual virtual SDL controller is not ready")) return 0
            if (!waitFor(order === "update-first" ? warning.opened && inside(warning) : dialog.opened && dialog.activeFocus,
                    "the first warning did not take focus")) return 0
            if (order === "update-first") ShellStore.clockSkewMs = 7 * 60 * 60 * 1000
            else ShellStore.updaterFailureMessage = "Fixture update failure"
            check(!dialog.visible && !ShellStore.clockSkewDismissed, "the clock overlapped the console update sheet instead of deferring")
            phase++
            return 0
        }
        if (phase === 1) {
            if (!waitFor(warning.opened && inside(warning), "the update sheet lost priority")) return 0
            check(window().consoleModalFocusOwner === warning && !dialog.visible && !loader.enabled,
                "the update sheet does not exclusively own console input")
            if (!stable(warning)) return 0
            return order === "update-first" ? press(Qt.Key_Return) : pad(padButtons.east)
        }
        if (phase === 2) {
            if (!waitFor(dialog.opened && dialog.activeFocus, "the deferred clock did not return after update dismissal")) return 0
            check(ShellStore.updaterFailureMessage === "" && !ShellStore.clockSkewDismissed,
                "dismissing the update also dismissed the clock")
            clockOwnsInput()
            if (!stable(dialog)) return 0
            return press(Qt.Key_PageDown)
        }
        if (phase === 3) { clockOwnsInput(); return pad(padButtons.rightShoulder) }
        if (phase === 4) { clockOwnsInput(); return pad(padButtons.start) }
        if (phase === 5) {
            clockOwnsInput()
            return order === "update-first" ? pad(padButtons.south) : press(Qt.Key_Escape)
        }
        if (phase === 6) {
            if (!waitFor(!dialog.visible && ShellStore.clockSkewDismissed && loader.enabled
                    && window().consoleModalFocusOwner === null, "clock dismissal did not release console input")) return 0
            AppController.showOverlay("guide-session")
            ShellStore.clockSkewDismissed = false
            check(!dialog.visible, "the clock overlapped an existing console overlay")
            phase++
            return 0
        }
        if (phase === 7) {
            const overlay = namedChild(host, "fallbackOverlayHost")
            if (!waitFor(inside(overlay), "the guide did not keep modal focus")) return 0
            check(!ShellStore.clockSkewDismissed && !dialog.visible, "guide deferral dismissed the clock")
            return pad(padButtons.east)
        }
        if (phase === 8) {
            if (!waitFor(dialog.opened && dialog.activeFocus && AppController.overlay === "",
                    "the clock did not return after controller B closed the guide")) return 0
            clockOwnsInput()
            window().close()
            check(window().visible && AppController.overlay === "application-quit-confirm" && !dialog.visible,
                "quit confirmation did not take priority over the clock")
            phase++
            return 0
        }
        if (phase === 9) {
            if (!waitFor(inside(warning), "quit confirmation did not take focus")) return 0
            check(!ShellStore.clockSkewDismissed && !window().applicationCloseConfirmed, "quit deferral changed dismissal or quit state")
            return pad(padButtons.east)
        }
        if (phase === 10) {
            if (!waitFor(dialog.opened && dialog.activeFocus, "the clock did not return after cancelling quit")) return 0
            clockOwnsInput()
            ShellStore.streamCreateRequestId = "clock-overlap-session-safety"
            check(!dialog.visible && !ShellStore.clockSkewDismissed, "session preparation did not defer the clock")
            phase++
            return 0
        }
        if (phase === 11) { ShellStore.streamCreateRequestId = ""; phase++; return 0 }
        if (phase === 12) {
            if (!waitFor(dialog.opened && dialog.activeFocus, "the safe idle clock did not return")) return 0
            clockOwnsInput()
            if (!stable(dialog)) return 0
            console.log("CLOCK OVERLAP PASS " + order + " stable priority, real SDL A/B/shoulder/Start, keyboard, guide, quit, session safety")
            return 1
        }
        return -1
    }
}

import QtQuick
import OpenNOW

QtObject {
    id: fixture
    property int selectedSection: 10
    property int key: 0
    property int modifiers: 0
    property int phase: 0
    property Item interactivePage: null
    property QtObject probe: null
    property int callsBeforeStrip: 0
    property int probeKey: Qt.Key_F9
    property int probeModifiers: Qt.ControlModifier | Qt.AltModifier
    property Component probeComponent: Component {
        Shortcut {
            property int fired: 0
            sequence: "Ctrl+Alt+F9"
            context: Qt.ApplicationShortcut
            onActivated: fired += 1
        }
    }
    property QtObject client: QtObject {
        property string state: "ready"
        property string lastError: ""
        property var calls: []
        signal responseReceived(string requestId, var result)
        signal requestFailed(string requestId, string code, string message)
        signal eventReceived(string name, var payload)
        function markUiReady() {}
        function logShellDiagnostic(message) {}
        function request(method, params, timeout) {
            const id = "shortcuts-fixture-" + (calls.length + 1)
            calls = calls.concat([{id: id, method: method, params: params}])
            return id
        }
        function cancel(id) { return true }
    }
    property Component pageComponent: Component {
        DesktopSettingsShortcutsPage { availableWidth: 1028; settingsScreen: fixture }
    }
    function check(ok, message) { if (!ok) throw new Error("Shortcuts: " + message) }
    function find(item, name) {
        if (item.objectName === name) return item
        for (const child of item.children || []) {
            const found = find(child, name)
            if (found) return found
        }
        return null
    }
    function shortcutCalls() { return client.calls.filter(call => call.method === "settings.shortcuts.update") }
    function lastCall() { return shortcutCalls()[shortcutCalls().length - 1] }
    function canonical(map) { return JSON.stringify(Object.keys(map).sort().map(name => [name, map[name]])) }
    function same(left, right) { return canonical(left) === canonical(right) }
    function respond(bindings) {
        const call = lastCall()
        check(same(call.params.bindings, bindings), "transaction payload " + JSON.stringify(call.params.bindings))
        client.responseReceived(call.id, {bindings: bindings})
        check(ShellStore.shortcutUpdateRequestId === "", "confirmed transactions must release the pending request")
    }

    function run(parent) {
        ShellStore.settings = {}
        const page = pageComponent.createObject(parent)
        check(page !== null, "shortcuts page must load")
        const row = key => find(page, "shortcutCommand-" + key)
        for (const key of ["shortcutToggleStats", "shortcutToggleFullscreen", "shortcutTogglePointerLock", "shortcutScreenshot",
                "shortcutToggleMicrophone", "shortcutToggleRecording", "shortcutSaveClip", "shortcutToggleAntiAfk", "shortcutStopStream"])
            check(row(key) && row(key).visible, key + " must be editable in place")
        check(shortcutCalls().length === 0, "opening the page saves nothing")
        check(find(page, "shortcutRow-sessionMenu").visible && !find(page, "shortcutBinding-guide"),
            "the in-stream Guide stays visible and locked")
        check(find(page, "shortcutResetAll").enabled === false, "reset all is idle while every binding is default")

        const stats = row("shortcutToggleStats")
        stats.beginCapture()
        check(stats.capturing && page.captureKey === "shortcutToggleStats", "selecting a binding captures in place")
        row("shortcutToggleFullscreen").beginCapture()
        check(!stats.capturing && page.captureKey === "shortcutToggleFullscreen", "only one row captures at a time")
        row("shortcutToggleFullscreen").cancel()
        check(page.captureKey === "", "cancel ends capture")
        stats.beginCapture()
        stats.captureKey(Qt.Key_Control, Qt.ControlModifier, false, false)
        check(stats.heldModifiers === "Ctrl" && shortcutCalls().length === 0, "held modifiers preview without saving")
        stats.captureKey(Qt.Key_S, Qt.ControlModifier | Qt.ShiftModifier, true, false)
        check(shortcutCalls().length === 0, "auto-repeat is ignored")
        stats.captureKey(Qt.Key_Return, Qt.NoModifier, false, true)
        check(shortcutCalls().length === 0 && stats.capturing && stats.message !== "", "controller buttons are never recorded")
        stats.captureKey(Qt.Key_K, Qt.NoModifier, false, false)
        check(shortcutCalls().length === 0 && stats.tone === "amber", "bare letters are rejected")
        stats.captureKey(Qt.Key_G, Qt.ControlModifier, false, false)
        check(shortcutCalls().length === 0 && stats.tone === "amber"
            && stats.message === qsTr("Ctrl+G is reserved for the in-stream Guide."), "Ctrl+G stays reserved")
        stats.captureKey(Qt.Key_S, Qt.ControlModifier | Qt.ShiftModifier, false, false)
        check(stats.saving && ShellStore.settings.shortcutToggleStats === undefined,
            "capture waits for the core instead of applying optimistically")
        check(ShellStore.updateShortcuts({shortcutSaveClip: ""}) === "" && shortcutCalls().length === 1,
            "a second transaction cannot overlap the pending one")
        respond({shortcutToggleStats: "Ctrl+Shift+S"})
        check(ShellStore.settings.shortcutToggleStats === "Ctrl+Shift+S" && !stats.capturing && stats.changed,
            "a confirmed capture persists and ends capture")
        check(find(page, "shortcutResetAll").enabled && page.changedCount === 1, "changed bindings enable reset all")

        stats.beginCapture()
        stats.captureKey(Qt.Key_F11, Qt.ControlModifier, false, false)
        check(stats.tone === "danger" && stats.conflictOwner === "shortcutScreenshot"
            && stats.message === qsTr("%1 is already used by %2.").arg("Ctrl+F11").arg(qsTr("Screenshot the stream")),
            "conflicts name the command that owns the chord")
        check(shortcutCalls().length === 1, "a conflict saves nothing until the user chooses")
        stats.useHere()
        const failed = lastCall()
        check(same(failed.params.bindings, {shortcutToggleStats: "Ctrl+F11", shortcutScreenshot: ""}),
            "use here moves the chord and unbinds the owner in one transaction")
        client.requestFailed(failed.id, "invalid_setting", "Could not save settings: disk full")
        check(ShellStore.settings.shortcutToggleStats === "Ctrl+Shift+S" && ShellStore.settings.shortcutScreenshot === undefined,
            "a failed transaction leaves both bindings untouched")
        check(stats.capturing && !stats.saving && stats.tone === "danger", "a failed save keeps the row open instead of completing")
        check(stats.message === "Could not save settings: disk full" && stats.conflictOwner === "shortcutScreenshot",
            "the failure is shown in the row and use here can be retried")
        stats.useHere()
        respond({shortcutToggleStats: "Ctrl+F11", shortcutScreenshot: ""})
        check(ShellStore.settings.shortcutScreenshot === "" && row("shortcutScreenshot").unset
            && page.allShortcutGroups()[0].rows.find(item => item.setting === "shortcutScreenshot").k === qsTr("Not set"),
            "the previous owner becomes Not set")
        check(!ShellStore.streamShortcutBindings()["screenshot"][0], "an unbound command stops consuming its old key in the stream")

        stats.beginCapture()
        stats.captureKey(Qt.Key_Escape, Qt.NoModifier, false, false)
        check(!stats.capturing && ShellStore.settings.shortcutToggleStats === "Ctrl+F11", "Escape restores the old binding")

        const recording = row("shortcutToggleRecording")
        recording.clearBinding()
        respond({shortcutToggleRecording: ""})
        check(recording.unset && recording.stateText === qsTr("Cleared · %1 now reaches the game").arg("F12"),
            "clearing persists an empty binding")

        ShellStore.applySetting("shortcutToggleFullscreen", "F12")
        recording.resetToDefault()
        check(recording.conflictOwner === "shortcutToggleFullscreen" && shortcutCalls().length === 4,
            "reset reports a taken default instead of creating a duplicate")
        recording.useHere()
        respond({shortcutToggleRecording: "F12", shortcutToggleFullscreen: ""})
        check(recording.current === "F12" && !recording.changed, "reset can reclaim its default")

        stats.resetToDefault()
        respond({shortcutToggleStats: "Ctrl+N"})
        check(!stats.changed && stats.stateText === qsTr("Default restored"), "reset restores the default")

        page.filter = "unset"
        check(row("shortcutScreenshot").visible && row("shortcutToggleFullscreen").visible && !stats.visible,
            "the Not set filter shows only unbound commands")
        check(!find(page, "shortcutRow-sessionMenu").visible, "fixed rows are hidden by state filters")
        page.filter = "changed"
        check(!row("shortcutToggleRecording").visible && row("shortcutScreenshot").visible, "the Changed filter hides defaults")
        page.filter = "all"
        find(page, "shortcutSearch").text = "rec"
        check(recording.visible && !row("shortcutToggleFullscreen").visible, "search filters by command name")
        find(page, "shortcutSearch").text = "ctrl+shift+q"
        check(row("shortcutStopStream").visible && !recording.visible, "search filters by binding")
        find(page, "shortcutSearch").text = "no such command"
        check(find(page, "shortcutEmptyState").visible, "an empty search explains itself")
        find(page, "shortcutSearch").text = ""

        check(page.changedCount === 2, "screenshot and fullscreen remain changed")
        page.requestResetAll()
        check(page.confirmingReset && shortcutCalls().length === 6, "reset all asks before saving")
        page.cancelResetAll()
        check(!page.confirmingReset && shortcutCalls().length === 6, "reset all can be cancelled")
        page.requestResetAll()
        page.confirmResetAll()
        client.requestFailed(lastCall().id, "settings_write_failed", "Could not save settings: read-only")
        check(page.confirmingReset && page.resetAllError === "Could not save settings: read-only" && page.changedCount === 2,
            "a failed reset all keeps the confirmation open and changes nothing")
        page.confirmResetAll()
        respond({shortcutToggleFullscreen: "F11", shortcutScreenshot: "Ctrl+F11"})
        check(page.changedCount === 0 && !page.confirmingReset, "reset all restores every default in one transaction")

        page.destroy()
        interactivePage = pageComponent.createObject(parent)
        probe = probeComponent.createObject(parent)
        check(interactivePage !== null && probe !== null, "interactive capture fixture must load")
        return true
    }

    function press(nextKey, nextModifiers, nextPhase) {
        key = nextKey
        modifiers = nextModifiers
        phase = nextPhase
        return 0
    }

    function advance() {
        const stats = find(interactivePage, "shortcutCommand-shortcutToggleStats")
        const bindingButton = find(interactivePage, "shortcutBinding-shortcutToggleStats")
        if (phase === 0) {
            stats.beginCapture()
            check(stats.capturing && bindingButton.activeFocus, "capture focuses the listening binding")
            return press(probeKey, probeModifiers, 1)
        }
        if (phase === 1) {
            check(probe.fired === 0, "an application shortcut fired while the binding was listening")
            respond({shortcutToggleStats: "Ctrl+Alt+F9"})
            stats.beginCapture()
            return press(Qt.Key_Tab, Qt.NoModifier, 2)
        }
        if (phase === 2) {
            check(stats.capturing && find(interactivePage, "shortcutClearInStrip-shortcutToggleStats").activeFocus,
                "Tab moves from the listening binding to the capture strip")
            callsBeforeStrip = shortcutCalls().length
            return press(probeKey, probeModifiers, 3)
        }
        if (phase === 3) {
            check(probe.fired === 0 && stats.capturing && shortcutCalls().length === callsBeforeStrip,
                "an application shortcut fired from the capture strip")
            return press(Qt.Key_Escape, Qt.NoModifier, 4)
        }
        if (phase === 4) {
            check(!stats.capturing && ShellStore.settings.shortcutToggleStats === "Ctrl+Alt+F9",
                "Escape on the capture strip cancels without saving")
            ShellStore.applySetting("shortcutToggleStats", "Ctrl+N")
            return press(probeKey, probeModifiers, 5)
        }
        if (phase === 5) {
            check(probe.fired === 1, "the probe shortcut fires once capture has ended")
            interactivePage.destroy()
            probe.destroy()
            return 1
        }
        return 0
    }
}

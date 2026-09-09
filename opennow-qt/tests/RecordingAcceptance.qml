import QtQuick
import OpenNOW

QtObject {
    id: fixture
    property int selectedSection: 12
    property QtObject runtime: QtObject {
        property bool running: true
        property string lastError: ""
        property var commands: []
        signal presentationError(string message)
        signal responseReceived(var response)
        signal eventReceived(var event)
        signal callbacksDropped(int count)
        function start() { return true }
        function send(command) { commands = commands.concat([command]); return true }
    }
    property QtObject client: QtObject {
        property string state: "stopped"
        property string lastError: ""
        property var calls: []
        signal responseReceived(string requestId, var result)
        signal requestFailed(string requestId, string code, string message)
        signal eventReceived(string name, var payload)
        function logShellDiagnostic(message) {}
        function request(method, params, timeout) {
            const id = "recording-fixture-" + (calls.length + 1)
            calls = calls.concat([{id: id, method: method, params: params}])
            return id
        }
        function cancel(id) { return true }
    }
    property Component pageComponent: Component {
        DesktopSettingsRecordingPage { availableWidth: 960; settingsScreen: fixture }
    }
    property Component bindingComponent: Component { DesktopSettingsShortcutBinding {} }
    property Component statusComponent: Component { StreamCaptureStatus {} }
    function boolSetting(key, fallback) { return ShellStore.settings[key] ?? fallback }
    function valueSetting(key, fallback) { return ShellStore.settings[key] ?? fallback }
    function setSetting(key, value) { ShellStore.applySetting(key, value) }
    function check(ok, message) { if (!ok) throw new Error("Recording: " + message) }
    function find(item, name) {
        if (item.objectName === name) return item
        for (const child of item.children || []) {
            const found = find(child, name)
            if (found) return found
        }
        return null
    }
    function run(parent) {
        ShellStore.settings = {}
        ShellStore.streamerStartRequestId = "fixture-blocked"
        ShellStore.streamInputPauseRequestId = "fixture-blocked"
        ShellStore.activeSession = {sessionId: "recording-fixture", phase: "ready", status: 2}
        ShellStore.streamer = {status: "streaming"}
        const page = pageComponent.createObject(parent)
        check(page !== null, "Recording page must load")
        const toggle = find(page, "replayBufferEnabledToggle")
        check(toggle && !toggle.checked && !ShellStore.streamReplayEnabled, "replay must default off")
        check(find(page, "replayBufferSecondsChoice").value === 30, "default duration")
        check(find(page, "replayBufferMemoryChoice").value === 256, "default memory limit")
        find(page, "recordingStreamSettings").clicked()
        check(selectedSection === 3, "source settings must link to Stream")
        check(find(page, "editRecordingShortcut").text === "F12", "recording binding must be visible")
        check(find(page, "editSaveClipShortcut").text === "Ctrl+F12", "clip binding must be visible")
        const binding = bindingComponent.createObject(parent)
        const status = statusComponent.createObject(parent)
        ShellStore.streamRecordingElapsedMs = 65000
        ShellStore.streamRecordingActive = true
        check(status.elapsedText === "01:05" && find(status, "streamRecordingIndicator").visible,
            "recording must show a visible elapsed indicator")
        ShellStore.streamCaptureAnnounced(qsTr("Clip saved"))
        check(status.notice === qsTr("Clip saved") && !status.activeFocus,
            "clip notices must be visible without taking focus")
        status.notice = ""
        ShellStore.streamCaptureAnnounced(qsTr("Clip saved"))
        check(status.notice === qsTr("Clip saved"), "repeated capture notices must be delivered")
        ShellStore.streamRecordingActive = false
        check(binding.validate("shortcutSaveClip", {key: Qt.Key_F12, modifiers: Qt.ControlModifier}).chord === "Ctrl+F12", "clip default must validate")
        check(binding.validate("shortcutSaveClip", {key: Qt.Key_F12, modifiers: Qt.NoModifier}).error, "recording collision must be rejected")
        check(binding.validate("shortcutSaveClip", {key: Qt.Key_G, modifiers: Qt.ControlModifier}).error, "Guide is reserved")
        check(binding.validate("shortcutSaveClip", {key: Qt.Key_F3, modifiers: Qt.NoModifier}).error, "stats alias is reserved")
        ShellStore.applyStreamShortcutAction("save-clip")
        check(client.calls.length === 0, "disabled shortcut must not request a file")
        toggle.clicked()
        check(toggle.checked && !ShellStore.streamReplayEnabled && runtime.commands.length === 0, "opt-in must not start a new runtime or encoder mid-session")
        find(page, "replayBufferSecondsChoice").selected(60)
        check(ShellStore.settings.replayBufferSeconds === 60, "duration must be editable")
        find(page, "replayBufferMemoryChoice").selected(128)
        check(ShellStore.settings.replayBufferMemoryMiB === 128, "memory limit must be editable")
        ShellStore.streamReplayEnabled = true
        ShellStore.saveStreamClip()
        const target = ShellStore.mediaClipTargetRequestId
        check(target !== "" && ShellStore.streamClipBusy, "clip must allocate a bounded target")
        ShellStore.saveStreamClip()
        check(client.calls.length === 1, "duplicate saves must be suppressed")
        client.responseReceived(target, {path: "/recording-fixture/clip.mkv"})
        const command = runtime.commands[runtime.commands.length - 1]
        check(command.type === "clip-save" && command.outputPath === "/recording-fixture/clip.mkv", "native must own export")
        ShellStore.acceptNativeResponse({id: command.id, type: "clip-saving"})
        check(ShellStore.streamClipBusy, "acknowledgement is not completion")
        ShellStore.acceptNativeEvent({type: "clip-state", requestId: "stale", state: "saved"})
        check(ShellStore.streamClipBusy, "stale completion must be ignored")
        ShellStore.acceptNativeEvent({type: "clip-state", requestId: command.id, state: "saved", path: command.outputPath})
        check(!ShellStore.streamClipBusy && ShellStore.mediaMessage === qsTr("Clip saved"), "completion must release the pending save")
        ShellStore.saveStreamClip()
        const cancelledTarget = ShellStore.mediaClipTargetRequestId
        toggle.clicked()
        check(!ShellStore.streamReplayEnabled && !ShellStore.streamClipBusy, "disable must clear pending work")
        check(runtime.commands[runtime.commands.length - 1].type === "replay-stop", "disable must stop native replay immediately")
        const before = runtime.commands.length
        client.responseReceived(cancelledTarget, {path: "/recording-fixture/cancelled.mkv"})
        check(runtime.commands.length === before, "late target cannot restart a disabled buffer")
        ShellStore.streamClipRequestId = "old-session"
        ShellStore.nativeRequests = {"old-session": {operation: "clip-save"}}
        ShellStore.resetStreamReplay()
        ShellStore.acceptNativeResponse({id: "old-session", type: "error", message: "stale error"})
        ShellStore.acceptNativeEvent({type: "clip-state", requestId: "old-session", state: "failed", message: "stale error"})
        check(ShellStore.mediaMessage !== "stale error", "old session failure cannot replace current state")
        binding.destroy()
        status.destroy()
        page.destroy()
        return true
    }
}

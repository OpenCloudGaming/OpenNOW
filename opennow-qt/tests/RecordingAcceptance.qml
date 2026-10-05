import QtQuick
import OpenNOW

QtObject {
    id: fixture
    property int selectedSection: 12
    property int recordingNotices: 0
    property Connections recordingAnnouncements: Connections {
        target: ShellStore
        function onStreamCaptureAnnounced(message) { fixture.recordingNotices += 1 }
    }
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
        function markUiReady() {}
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
    property Component shortcutsComponent: Component {
        DesktopSettingsShortcutsPage { availableWidth: 960; settingsScreen: fixture }
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
    function beginManualRecording(sessionId) {
        ShellStore.activeSession = {sessionId: sessionId, phase: "ready", status: 2}
        ShellStore.streamer = {status: "streaming"}
        ShellStore.streamerStopExpected = false
        ShellStore.toggleStreamRecording()
        const target = ShellStore.mediaRecordingTargetRequestId
        check(target !== "", "manual recording must allocate a target")
        client.responseReceived(target, {path: "/recording-fixture/" + target + ".mkv"})
        const command = runtime.commands[runtime.commands.length - 1]
        check(command.type === "recording-start", "manual capture must use the native recorder")
        return command
    }
    function runManualRecording(status) {
        ShellStore.streamRecordingActive = false
        recordingNotices = 0
        let start = beginManualRecording("manual-one")
        ShellStore.acceptNativeEvent({type: "recording-state", requestId: start.id,
            state: "saved", path: start.outputPath,
            completion: {kind: "cut", reason: "discontinuity"}})
        const cutMessage = ShellStore.mediaMessage
        check(cutMessage === qsTr("Recording saved early because the stream was interrupted")
            && status.notice === cutMessage && !ShellStore.streamRecordingActive,
            "early terminal events must display the cut through the existing notice")
        ShellStore.acceptNativeResponse({id: start.id, type: "recording-started"})
        check(!ShellStore.streamRecordingActive && ShellStore.streamRecordingStartRequestId === "",
            "late start acknowledgement must not resurrect completed recording")
        ShellStore.acceptNativeEvent({type: "recording-state", requestId: start.id,
            state: "failed", message: "duplicate failure"})
        check(ShellStore.mediaMessage === cutMessage && recordingNotices === 1,
            "duplicate terminal events must not overwrite or announce twice")
        const oldStart = start
        start = beginManualRecording("manual-two")
        ShellStore.acceptNativeResponse({id: start.id, type: "recording-started"})
        check(ShellStore.streamRecordingActive, "acknowledged manual recording must run")
        ShellStore.acceptNativeEvent({type: "recording-state", requestId: oldStart.id,
            state: "saved", path: oldStart.outputPath, completion: {kind: "complete"}})
        ShellStore.acceptNativeEvent({type: "recording-state", state: "failed", message: "uncorrelated"})
        ShellStore.acceptNativeEvent({type: "recording-state", requestId: start.id,
            state: "saved", path: start.outputPath})
        check(ShellStore.streamRecordingActive && recordingNotices === 1,
            "stale, uncorrelated and metadata-free events must not complete current capture")
        ShellStore.toggleStreamRecording()
        const stop = runtime.commands[runtime.commands.length - 1]
        check(stop.type === "recording-stop", "manual stop must use the existing command")
        ShellStore.acceptNativeEvent({type: "recording-state", requestId: start.id,
            state: "saved", path: start.outputPath, completion: {kind: "cut", reason: "queue-overflow"}})
        const overflowMessage = ShellStore.mediaMessage
        ShellStore.acceptNativeResponse({id: stop.id, type: "recording-stopped", requestId: start.id,
            path: start.outputPath, completion: {kind: "complete"}})
        check(ShellStore.mediaMessage === overflowMessage && recordingNotices === 2
            && ShellStore.streamRecordingStopRequestId === "",
            "stop acknowledgement must release the command without replacing an earlier cut")
        start = beginManualRecording("manual-three")
        ShellStore.acceptNativeResponse({id: start.id, type: "recording-started"})
        ShellStore.toggleStreamRecording()
        const fallbackStop = runtime.commands[runtime.commands.length - 1]
        ShellStore.acceptNativeResponse({id: fallbackStop.id, type: "recording-stopped", path: start.outputPath})
        check(!ShellStore.streamRecordingActive && ShellStore.mediaMessage === qsTr("Recording saved")
            && recordingNotices === 3, "authoritative older stop responses retain normal-save semantics")
        ShellStore.acceptNativeEvent({type: "recording-state", requestId: start.id,
            state: "saved", path: start.outputPath, completion: {kind: "complete"}})
        check(recordingNotices === 3, "event after stop response must not announce again")
        start = beginManualRecording("manual-four")
        ShellStore.acceptNativeResponse({id: start.id, type: "recording-started"})
        ShellStore.activeSession = null
        ShellStore.acceptNativeEvent({type: "recording-state", requestId: start.id,
            state: "saved", path: start.outputPath, completion: {kind: "cut", reason: "interrupted"}})
        check(ShellStore.mediaMessage === qsTr("Recording saved early because the stream stopped")
            && recordingNotices === 4, "finalization must survive session teardown")
        start = beginManualRecording("manual-five")
        ShellStore.acceptNativeEvent({type: "recording-state", requestId: start.id,
            state: "failed", message: "failed to finalize Matroska recording"})
        ShellStore.acceptNativeResponse({id: start.id, type: "recording-started"})
        check(!ShellStore.streamRecordingActive && ShellStore.lastError === "failed to finalize Matroska recording"
            && status.notice === ShellStore.lastError && recordingNotices === 5,
            "storage failures must remain visible failures without an active indicator")
        ShellStore.toggleStreamRecording()
        const cancelledTarget = ShellStore.mediaRecordingTargetRequestId
        ShellStore.activeSession = null
        const commandCount = runtime.commands.length
        client.responseReceived(cancelledTarget, {path: "/recording-fixture/too-late.mkv"})
        check(ShellStore.mediaRecordingTargetRequestId === "" && runtime.commands.length === commandCount,
            "target allocation after session teardown cannot start recording")
    }
    function runDroppedAcknowledgements() {
        let start = beginManualRecording("dropped-acknowledgements")
        ShellStore.acceptNativeEvent({type: "recording-state", requestId: start.id,
            state: "saved", path: start.outputPath, completion: {kind: "cut", reason: "interrupted"}})
        check(ShellStore.streamRecordingStartRequestId === "",
            "terminal completion must permit retry without a start acknowledgement")
        const oldStart = start
        start = beginManualRecording("dropped-acknowledgements")
        ShellStore.acceptNativeResponse({id: oldStart.id, type: "recording-started"})
        check(!ShellStore.streamRecordingActive && ShellStore.streamRecordingStartRequestId === start.id,
            "old acknowledgement must not activate or clear a newer attempt")
        ShellStore.acceptNativeResponse({id: start.id, type: "recording-started"})
        ShellStore.toggleStreamRecording()
        const stop = runtime.commands[runtime.commands.length - 1]
        ShellStore.acceptNativeEvent({type: "recording-state", requestId: start.id,
            state: "saved", path: start.outputPath, completion: {kind: "cut", reason: "queue-overflow"}})
        check(ShellStore.streamRecordingStopRequestId === "",
            "terminal completion must permit retry without a stop acknowledgement")
        const stoppedStart = start
        start = beginManualRecording("dropped-acknowledgements")
        ShellStore.acceptNativeResponse({id: stop.id, type: "recording-stopped", requestId: stoppedStart.id,
            path: stoppedStart.outputPath, completion: {kind: "complete"}})
        check(!ShellStore.streamRecordingActive && !ShellStore.streamRecordingAttempt.completed
            && ShellStore.streamRecordingStartRequestId === start.id,
            "late stop acknowledgement must not finish the retry")
        ShellStore.acceptNativeResponse({id: start.id, type: "recording-started"})
        ShellStore.acceptNativeEvent({type: "recording-state", requestId: start.id,
            state: "saved", path: start.outputPath, completion: {kind: "complete"}})
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
        check(find(page, "recordingShortcutHint").keyText === "F12", "recording binding must be visible")
        check(find(page, "replayShortcutHint").keyText === "Ctrl+F12", "clip binding must be visible")
        find(page, "recordingKeyboardShortcuts").clicked()
        check(selectedSection === 10, "capture shortcut editing must link to the central shortcuts page")
        const shortcuts = shortcutsComponent.createObject(parent)
        check(shortcuts !== null, "central shortcut settings must load")
        const editableBindings = shortcuts.allShortcutGroups().reduce((keys, group) => keys.concat(group.rows.map(row => row.setting)), [])
        check(["shortcutToggleStats", "shortcutToggleRecording", "shortcutSaveClip"].every(key => editableBindings.indexOf(key) >= 0),
            "statistics and capture bindings must remain editable in the central shortcut settings")
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
        check(binding.validate("shortcutSaveClip", {key: Qt.Key_F3, modifiers: Qt.NoModifier}).chord === "F3", "F3 must be available for gameplay or a custom binding")
        ShellStore.applySetting("shortcutToggleStats", "")
        ShellStore.applySetting("shortcutToggleRecording", "")
        ShellStore.applySetting("shortcutSaveClip", "")
        check(binding.value("shortcutToggleStats") === "" && binding.value("shortcutToggleRecording") === "", "cleared shortcuts must not inherit defaults")
        check(!ShellStore.streamShortcutBindings()["toggle-recording"][0]
            && !ShellStore.streamShortcutBindings()["save-clip"][0], "cleared capture bindings must reach the video item")
        check(shortcuts.allShortcutGroups()[0].rows.find(row => row.setting === "shortcutToggleRecording").k === qsTr("Not set"), "central shortcuts page must show a cleared binding")
        check(!find(page, "recordingShortcutHint").visible && !find(page, "replayShortcutHint").visible, "recording page must not advertise disabled shortcuts")
        ShellStore.applySetting("shortcutToggleStats", "Ctrl+N")
        ShellStore.applySetting("shortcutToggleRecording", "F12")
        ShellStore.applySetting("shortcutSaveClip", "Ctrl+F12")
        shortcuts.destroy()
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
        runManualRecording(status)
        runDroppedAcknowledgements()
        binding.destroy()
        status.destroy()
        page.destroy()
        return true
    }
}

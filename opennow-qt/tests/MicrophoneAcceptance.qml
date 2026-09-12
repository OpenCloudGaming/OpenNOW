import QtQuick
import OpenNOW

QtObject {
    id: fixture
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
    property Component audioComponent: Component {
        DesktopSettingsAudioPage { availableWidth: 1000; settingsScreen: fixture }
    }
    function valueSetting(key, fallback) { return ShellStore.settings[key] ?? fallback }
    function setSetting(key, value) { ShellStore.setSetting(key, value) }
    function check(ok, message) { if (!ok) throw new Error("Microphone: " + message) }
    function find(item, name) {
        if (item.objectName === name) return item
        for (const child of item.children || []) {
            const result = find(child, name)
            if (result) return result
        }
        return null
    }
    function run(parent) {
        ShellStore.streamInputPauseRequestId = "fixture-blocked"
        ShellStore.streamerStartRequestId = "fixture-blocked"
        ShellStore.activeSession = {sessionId: "microphone-fixture", phase: "ready", status: 2}
        ShellStore.settings = {microphoneMode: "voice-activity"}
        ShellStore.nativeRuntimeReady = true
        ShellStore.acceptNativeCapabilities({protocolVersion: 6, supportsMicrophone: false})
        const audio = audioComponent.createObject(parent)
        const open = find(audio, "settingsOption-voice-activity")
        check(open && !open.enabled, "unsupported builds must not offer capture")
        ShellStore.acceptNativeCapabilities({protocolVersion: 6, supportsMicrophone: true})
        check(find(audio, "settingsOption-voice-activity").enabled, "supported builds must offer opt-in")
        check(runtime.commands.length === 1 && runtime.commands[0].type === "audioDevices",
            "opening Audio settings must only discover output devices")
        ShellStore.acceptNativeResponse({id: runtime.commands[0].id, type: "audioDevices", devices: []})
        runtime.commands = []
        check(ShellStore.prepareMicrophoneStart("microphone-fixture", "voice-activity"),
            "new opted-in sessions must start enabled")
        ShellStore.sessionMicrophoneMode = "disabled"
        ShellStore.streamer = {status: "streaming", capabilities: {supportsMicrophone: true}}
        const before = runtime.commands.length
        ShellStore.toggleMicrophone()
        check(runtime.commands.length === before, "disabled sessions must not open capture")
        check(ShellStore.streamShortcutBindings()["toggle-microphone"].length === 0,
            "disabled sessions must not advertise a shortcut")
        ShellStore.sessionMicrophoneMode = "voice-activity"
        ShellStore.streamer = {status: "streaming", capabilities: {}}
        check(!ShellStore.microphoneCanToggle, "build support must not substitute for negotiation")
        ShellStore.nativeRequests = {"fixture-start": {operation: "start"}}
        ShellStore.acceptNativeResponse({id: "fixture-start", type: "ok", capabilities: {}})
        check(!ShellStore.microphoneCanToggle, "start response cannot inherit build microphone support")
        ShellStore.streamer = {status: "streaming", capabilities: {supportsMicrophone: true}}
        ShellStore.acceptNativeEvent({type: "microphone-state", state: "ready", enabled: true})
        check(ShellStore.microphoneEnabled && ShellStore.microphoneLabel === qsTr("Open microphone"),
            "native ready state must reach the shell")
        ShellStore.applyStreamShortcutAction("toggle-microphone")
        let command = runtime.commands[runtime.commands.length - 1]
        check(command.type === "microphone-set" && command.enabled === false,
            "shortcut must explicitly mute without a restart")
        check(!ShellStore.prepareMicrophoneStart("microphone-fixture", "voice-activity"),
            "reconnect must retain mute even before its acknowledgement")
        check(ShellStore.microphoneEnabled, "command submission must not invent native state")
        const pendingCount = runtime.commands.length
        ShellStore.toggleMicrophone()
        check(runtime.commands.length === pendingCount, "pending commands must be bounded")
        ShellStore.acceptNativeEvent({type: "microphone-state", state: "muted", enabled: false})
        ShellStore.acceptNativeResponse({id: command.id, type: "ok"})
        check(!ShellStore.microphoneEnabled && ShellStore.microphoneCanToggle, "mute acknowledgement")
        ShellStore.toggleMicrophone()
        command = runtime.commands[runtime.commands.length - 1]
        check(command.type === "microphone-set" && command.enabled === true, "explicit unmute")
        check(!ShellStore.prepareMicrophoneStart("microphone-fixture", "voice-activity"),
            "unacknowledged unmute must not reopen capture after reconnect")
        ShellStore.acceptNativeEvent({type: "microphone-state", state: "error", enabled: false,
            message: "Permission denied"})
        ShellStore.acceptNativeResponse({id: command.id, type: "error", message: "Permission denied"})
        check(ShellStore.microphoneState === "error" && !ShellStore.microphoneEnabled
            && ShellStore.microphoneDescription === "Permission denied", "capture failures must remain visible")
        check(!ShellStore.prepareMicrophoneStart("microphone-fixture", "voice-activity"),
            "failed capture must remain muted after reconnect")
        ShellStore.toggleMicrophone()
        command = runtime.commands[runtime.commands.length - 1]
        ShellStore.acceptNativeEvent({type: "microphone-state", state: "error", enabled: false})
        ShellStore.acceptNativeResponse({id: command.id, type: "ok"})
        check(!ShellStore.prepareMicrophoneStart("microphone-fixture", "voice-activity"),
            "an unmute acknowledgement must not overwrite an earlier capture failure")
        ShellStore.toggleMicrophone()
        command = runtime.commands[runtime.commands.length - 1]
        ShellStore.acceptNativeEvent({type: "microphone-state", state: "ready", enabled: true})
        ShellStore.acceptNativeResponse({id: command.id, type: "ok"})
        check(ShellStore.prepareMicrophoneStart("microphone-fixture", "voice-activity"),
            "acknowledged unmute must restore the enabled recovery state")
        ShellStore.acceptNativeEvent({type: "microphone-state", state: "error", enabled: false})
        check(!ShellStore.prepareMicrophoneStart("microphone-fixture", "voice-activity"),
            "later capture failures must fail closed on reconnect")
        check(ShellStore.prepareMicrophoneStart("new-session", "voice-activity"),
            "another opted-in session must not inherit the old mute")
        ShellStore.settings = {microphoneMode: "disabled"}
        check(ShellStore.microphoneCanToggle, "next-session settings must not strand a live mute control")
        ShellStore.activeSession = null
        check(!ShellStore.microphoneCanToggle && !ShellStore.microphoneEnabled,
            "session teardown must disable microphone controls")
        check(runtime.commands.every(command => command.type === "microphone-set"),
            "microphone controls must never restart transport")
        ShellStore.activeSession = {sessionId: "microphone-fixture", phase: "ready", status: 2}
        ShellStore.acceptNativeEvent({type: "microphone-state", state: "ready", enabled: true})
        const beforeRefresh = runtime.commands.length
        ShellStore.refreshAudioOutputDevices()
        check(runtime.commands.length === beforeRefresh + 1
            && runtime.commands[beforeRefresh].type === "audioDevices"
            && ShellStore.microphoneEnabled, "output discovery must not change active capture")
        ShellStore.acceptNativeResponse({id: runtime.commands[beforeRefresh].id, type: "error",
            message: "Output discovery failed"})
        check(ShellStore.microphoneEnabled && ShellStore.microphoneCanToggle,
            "output discovery failure must not mute microphone or stop the session")
        audio.destroy()
        return true
    }
}

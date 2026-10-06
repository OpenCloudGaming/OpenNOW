import QtQuick
import OpenNOW

QtObject {
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
            const id = "fixture-" + (calls.length + 1)
            calls = calls.concat([{id:id, method:method, params:params}])
            return id
        }
        function cancel(id) { return true }
    }
    property Component consoleSettings: Component { SettingsScreen { visible: false; selectedSection: 4 } }
    function check(ok, message) { if (!ok) throw new Error("Network test: " + message) }
    function find(item, name) {
        if (item.objectName === name) return item
        for (const child of item.children || []) {
            const found = find(child, name)
            if (found) return found
        }
        return null
    }
    function lastWrite() { return client.calls[client.calls.length - 1] }
    function run(parent) {
        client.state = "ready"
        ShellStore.settings = ({})
        const desktop = find(parent, "desktopSettingsScreen")
        check(desktop !== null, "the desktop settings screen must be present")
        desktop.advancedOpen = true
        const compatibility = find(desktop, "webrtcCompatibilityModeChoice")
        check(compatibility !== null, "the network page must expose the transport choice")
        check(compatibility.title === qsTr("WebRTC compatibility mode"), "compatibility is not labeled as alliance-only")
        check(compatibility.description.indexOf(qsTr("Automatic uses WebRTC for signed-in alliance accounts and NVST for NVIDIA accounts. Choose WebRTC or NVST to override either account type. Applies to new sessions only; existing sessions keep their transport.")) >= 0,
            "automatic routing and the new-session restriction must be explained")
        check(compatibility.description.indexOf(qsTr("WebRTC is for compatibility only. Expect lower performance than NVST. Supports H.264 and H.265, SDR, and stereo. H.265 supports 10-bit color and the selected resolution and frame rate. No microphone or clipboard text. Requires direct UDP connectivity.")) >= 0,
            "the performance warning and restrictions must be visible before choosing compatibility")
        check(compatibility.value === "auto" && compatibility.enabled,
            "compatibility defaults to automatic and is selectable before launch")
        check(compatibility.items.length === 3
            && compatibility.items[0].value === "auto" && compatibility.items[0].label === qsTr("Automatic")
            && compatibility.items[1].value === "on" && compatibility.items[1].label === qsTr("WebRTC")
            && compatibility.items[2].value === "off" && compatibility.items[2].label === qsTr("NVST"),
            "the choice exposes automatic routing and both explicit overrides")
        const owner = ShellStore.settingsOwnerState
        for (const mode of ["on", "off", "auto"]) {
            const previous = compatibility.value
            compatibility.expanded = true
            const option = find(compatibility, "settingsChoice-" + mode)
            check(option !== null && option.enabled, "the transport option must be selectable: " + mode)
            option.clicked()
            const compatibilityWrite = lastWrite()
            check(compatibilityWrite.method === "settings.set"
                && compatibilityWrite.params.key === "webrtcCompatibilityMode"
                && compatibilityWrite.params.value === mode, "transport persists through the settings owner: " + mode)
            check(compatibility.value === previous && !compatibility.enabled && ShellStore.streamBusy,
                "launch remains blocked until the transport write is acknowledged: " + mode)
            owner.colorRequestId = "transport-capabilities-" + mode
            owner.colorDescriptors = [{value:"10bit_420", disabled:false}]
            client.eventReceived("settings.changed", {key:"webrtcCompatibilityMode", value:mode})
            check(ShellStore.streamBusy && !compatibility.enabled,
                "a settings event alone must not release the pending-write guard: " + mode)
            client.responseReceived(compatibilityWrite.id, {key:"webrtcCompatibilityMode", value:mode})
            check(compatibility.value === mode && compatibility.enabled && !ShellStore.streamBusy,
                "the acknowledged setting releases the launch guard: " + mode)
            check(owner.colorRequestId === "" && owner.colorDescriptors.length === 0
                && !owner.acceptResponse("transport-capabilities-" + mode, {colorQualities:[{value:"10bit_420", disabled:false}]}),
                "a transport change invalidates stale capability choices: " + mode)
            compatibility.expanded = true
            ShellStore.pendingLaunchParams = {appId:"fixture"}
            check(!compatibility.enabled && !option.enabled,
                "transport cannot change while a launch is pending: " + mode)
            ShellStore.pendingLaunchParams = null
            ShellStore.streamCreateRequestId = "transport-launch-fixture"
            check(!compatibility.enabled && !option.enabled,
                "transport cannot change during session creation: " + mode)
            ShellStore.streamCreateRequestId = ""
            ShellStore.activeSession = {sessionId:"compatibility-fixture", status:2}
            check(!compatibility.enabled && !option.enabled && compatibility.value === mode,
                "transport cannot change an allocated seat: " + mode)
            ShellStore.activeSession = null
            check(compatibility.enabled && option.enabled,
                "transport becomes selectable after the seat ends: " + mode)
            compatibility.expanded = false
        }
        compatibility.expanded = true
        find(compatibility, "settingsChoice-on").clicked()
        const rejectedWrite = lastWrite()
        client.requestFailed(rejectedWrite.id, "settings_write_failed", "Fixture denied transport persistence")
        check(compatibility.value === "auto" && compatibility.enabled && !ShellStore.streamBusy,
            "a rejected override retains automatic routing and releases the launch guard")
        const toggle = find(desktop, "renewNetworkTestToggle")
        check(toggle !== null, "the network page must expose the network test opt-in")
        check(!toggle.checked, "the network test must ship off")

        toggle.clicked()
        let write = lastWrite()
        check(write.method === "settings.set" && write.params.key === "networkTest"
            && write.params.value === true, "the desktop toggle persists the opt-in")
        client.eventReceived("settings.changed", {key:"networkTest", value:true})
        client.responseReceived(write.id, {key:"networkTest", value:true})
        check(toggle.checked && ShellStore.settings.networkTest === true,
            "the desktop toggle reflects the saved value")

        const consolePage = consoleSettings.createObject(parent)
        const row = consolePage.settingsModel().find(item => item.key === "networkTest")
        check(row !== undefined && row.toggle === true && row.v === "On",
            "the console exposes the same opt-in")
        consolePage.activate(row)
        write = lastWrite()
        check(write.method === "settings.set" && write.params.key === "networkTest"
            && write.params.value === false, "the console writes the same opt-in")
        client.eventReceived("settings.changed", {key:"networkTest", value:false})
        client.responseReceived(write.id, {key:"networkTest", value:false})
        check(!toggle.checked, "the desktop toggle reflects console changes")
        consolePage.destroy()
        return true
    }
}

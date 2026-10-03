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
        const compatibility = find(desktop, "allianceWebrtcCompatibilityToggle")
        check(compatibility !== null, "the network page must expose alliance compatibility")
        check(!compatibility.checked && compatibility.enabled, "compatibility ships off and is selectable before launch")
        compatibility.clicked()
        let compatibilityWrite = lastWrite()
        check(compatibilityWrite.method === "settings.set"
            && compatibilityWrite.params.key === "allianceWebrtcCompatibility"
            && compatibilityWrite.params.value === true, "compatibility persists through the settings owner")
        check(!compatibility.checked && !compatibility.enabled && ShellStore.streamBusy,
            "launch remains blocked until the compatibility write is acknowledged")
        client.eventReceived("settings.changed", {key:"allianceWebrtcCompatibility", value:true})
        client.responseReceived(compatibilityWrite.id, {key:"allianceWebrtcCompatibility", value:true})
        check(compatibility.checked && compatibility.enabled && !ShellStore.streamBusy,
            "the acknowledged setting releases the launch guard")
        ShellStore.pendingLaunchParams = {appId:"fixture"}
        check(!compatibility.enabled, "compatibility cannot change while a launch is pending")
        ShellStore.pendingLaunchParams = null
        ShellStore.activeSession = {sessionId:"compatibility-fixture", status:2}
        check(!compatibility.enabled, "compatibility cannot change an allocated seat")
        ShellStore.activeSession = null
        check(compatibility.enabled, "compatibility becomes selectable after the seat ends")
        compatibility.clicked()
        compatibilityWrite = lastWrite()
        check(compatibilityWrite.params.key === "allianceWebrtcCompatibility"
            && compatibilityWrite.params.value === false, "compatibility can be disabled for the next launch")
        client.eventReceived("settings.changed", {key:"allianceWebrtcCompatibility", value:false})
        client.responseReceived(compatibilityWrite.id, {key:"allianceWebrtcCompatibility", value:false})
        check(!compatibility.checked, "compatibility returns to the NVST default")
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

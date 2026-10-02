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

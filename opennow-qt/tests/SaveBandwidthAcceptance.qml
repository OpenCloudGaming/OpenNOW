import QtQuick
import OpenNOW

QtObject {
    property Component consoleSettings: Component { SettingsScreen { visible: false; selectedSection: 1 } }
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
            const id = "fixture-" + (calls.length + 1)
            calls = calls.concat([{id:id, method:method, params:params}])
            return id
        }
        function cancel(id) { return true }
    }
    function check(ok, message) { if (!ok) throw new Error("Save bandwidth: " + message) }
    function find(item, name) {
        if (item.objectName === name) return item
        for (const child of item.children || []) {
            const found = find(child, name)
            if (found) return found
        }
        return null
    }
    function run(parent) {
        ShellStore.settings = Object.assign({}, ShellStore.settings, {resolution: "1920x1080", fps: 60})
        const toggle = find(parent, "saveBandwidthToggle")
        check(toggle, "desktop exposes the bandwidth-saving toggle")
        check(!toggle.checked, "desktop preference defaults off, so fixed quality stays the wire default")
        ShellStore.settings = Object.assign({}, ShellStore.settings, {saveBandwidth:true})
        const consolePage = consoleSettings.createObject(parent)
        const consoleRow = () => consolePage.settingsModel().find(item => item.key === "saveBandwidth")
        check(consoleRow() && consoleRow().toggle && consoleRow().v === "On"
            && consoleRow().t === "Save bandwidth", "console exposes the same preference")
        for (const enabled of [false, true]) {
            toggle.clicked()
            const write = client.calls[client.calls.length - 1]
            check(write.method === "settings.set" && write.params.key === "saveBandwidth"
                && write.params.value === enabled, "desktop requests the persisted preference")
            client.eventReceived("settings.changed", {key:"saveBandwidth", value:enabled})
            client.responseReceived(write.id, {key:"saveBandwidth", value:enabled})
            check(toggle.checked === enabled && ShellStore.settings.saveBandwidth === enabled,
                "desktop reflects the saved value")
            check(consoleRow().v === (enabled ? "On" : "Off"), "console reflects desktop changes")
        }
        consolePage.activate(consoleRow())
        let write = client.calls[client.calls.length - 1]
        check(write.method === "settings.set" && write.params.key === "saveBandwidth"
            && write.params.value === false, "console writes the same preference")
        client.eventReceived("settings.changed", {key:"saveBandwidth", value:false})
        client.responseReceived(write.id, {key:"saveBandwidth", value:false})
        check(!toggle.checked, "desktop reflects console changes")
        consolePage.activate(consoleRow())
        write = client.calls[client.calls.length - 1]
        check(write.method === "settings.set" && write.params.key === "saveBandwidth"
            && write.params.value === true, "console writes the preference back on")
        client.eventReceived("settings.changed", {key:"saveBandwidth", value:true})
        client.responseReceived(write.id, {key:"saveBandwidth", value:true})
        check(toggle.checked && consoleRow().v === "On",
            "both surfaces agree once the preference is enabled")
        consolePage.destroy()
        return true
    }
}

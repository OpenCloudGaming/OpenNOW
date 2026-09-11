import QtQuick
import OpenNOW

QtObject {
    property Component consoleSettings: Component { SettingsScreen { visible: false; selectedSection: 0 } }
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
    function check(ok, message) { if (!ok) throw new Error("Persistent in-game settings: " + message) }
    function find(item, name) {
        if (item.objectName === name) return item
        for (const child of item.children || []) {
            const found = find(child, name)
            if (found) return found
        }
        return null
    }
    function run(parent) {
        client.state = "ready"
        ShellStore.settings = ({})
        const toggle = find(parent, "persistentInGameSettingsToggle")
        check(toggle && !toggle.checked, "desktop preference defaults off")
        const consolePage = consoleSettings.createObject(parent)
        const row = consolePage.settingsModel().find(item => item.key === "enablePersistingInGameSettings")
        check(row && row.toggle && row.v === "Off" && row.t === "Persistent in-game settings",
            "console exposes in-game settings rather than cloud saves")
        for (const enabled of [true, false]) {
            toggle.clicked()
            const write = client.calls[client.calls.length - 1]
            check(write.method === "settings.set" && write.params.key === "enablePersistingInGameSettings"
                && write.params.value === enabled, "desktop requests the persisted preference")
            client.eventReceived("settings.changed", {key:"enablePersistingInGameSettings", value:enabled})
            check(toggle.checked === enabled && ShellStore.settings.enablePersistingInGameSettings === enabled,
                "desktop reflects the saved value")
            check(consolePage.settingsModel().find(item => item.key === "enablePersistingInGameSettings").v
                === (enabled ? "On" : "Off"), "console reflects desktop changes")
        }
        consolePage.activate(consolePage.settingsModel().find(item => item.key === "enablePersistingInGameSettings"))
        const write = client.calls[client.calls.length - 1]
        check(write.method === "settings.set" && write.params.key === "enablePersistingInGameSettings"
            && write.params.value === true, "console writes the same preference")
        client.eventReceived("settings.changed", {key:"enablePersistingInGameSettings", value:true})
        check(toggle.checked, "desktop reflects console changes")
        consolePage.destroy()

        ShellStore.nativeRuntimeReady = true
        ShellStore.authSession = {accountId:"fixture"}
        for (const support of [undefined, false, true]) {
            for (const selectedIndex of [0, 1]) {
                for (const directConsoleMode of [false, true]) {
                    ShellStore.selectedGame = {
                        title:"Fixture", launchAppId:"12345", selectedVariantIndex:selectedIndex,
                        variants:[
                            {appId:"12345", supportsInGameSettingsPersistence:support},
                            {appId:"67890", supportsInGameSettingsPersistence:!support}
                        ]
                    }
                    ShellStore.launchSelectedGame(directConsoleMode)
                    const request = client.calls[client.calls.length - 1]
                    const expectedSupport = selectedIndex === 0 ? support === true : !support
                    check(request.method === "session.remote.list"
                        && request.params.supportsInGameSettingsPersistence === expectedSupport,
                        "launch uses the selected storefront's support flag")
                    client.responseReceived(request.id, {sessions:[]})
                    const create = client.calls[client.calls.length - 1]
                    check(create.method === "session.create"
                        && create.params.supportsInGameSettingsPersistence === expectedSupport,
                        "session creation preserves game support")
                    client.responseReceived(create.id, {session:null})
                    check(!ShellStore.streamBusy, "launch requests settle")
                }
            }
        }
        return true
    }
}

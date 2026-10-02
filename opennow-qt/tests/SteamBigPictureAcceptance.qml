import QtQuick
import OpenNOW

QtObject {
    property Component consoleSettings: Component { SettingsScreen { visible: false; selectedSection: 2 } }
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
    function check(ok, message) { if (!ok) throw new Error("Steam Big Picture: " + message) }
    function inspect() {
        const id = ShellStore.launchInspectRequestId
        check(id !== "", "launch inspection is required")
        const pending = ShellStore.pendingLaunchParams
        client.responseReceived(id, {appId:pending.catalogAppId,variantId:pending.variantId,game:ShellStore.selectedGame,decision:{status:"ready"}})
    }
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
        ShellStore.settings = {controllerMode:true, launchInConsoleMode:true}
        const toggle = find(parent, "steamBigPictureToggle")
        check(toggle && !toggle.checked, "desktop setting defaults off despite controller and console preferences")
        toggle.clicked()
        check(ShellStore.settings.steamBigPictureMode === true && toggle.checked, "desktop toggle opts in")
        const write = client.calls[client.calls.length - 1]
        check(write.method === "settings.set" && write.params.key === "steamBigPictureMode" && write.params.value === true,
            "desktop toggle requests persistence")
        client.eventReceived("settings.changed", {key:"steamBigPictureMode", value:true})
        client.responseReceived(write.id, {key:"steamBigPictureMode", value:true})
        toggle.clicked()
        check(ShellStore.settings.steamBigPictureMode === false && !toggle.checked, "desktop toggle opts out")
        const disabledWrite = client.calls[client.calls.length - 1]
        check(disabledWrite.method === "settings.set" && disabledWrite.params.value === false,
            "desktop opt-out requests persistence")
        client.eventReceived("settings.changed", {key:"steamBigPictureMode", value:false})
        client.responseReceived(disabledWrite.id, {key:"steamBigPictureMode", value:false})
        const consolePage = consoleSettings.createObject(parent)
        const row = consolePage.settingsModel().find(item => item.key === "steamBigPictureMode")
        check(row && row.toggle && row.v === "Off", "console Video page exposes the same default-off preference")
        consolePage.activate(row)
        const consoleWrite = client.calls[client.calls.length - 1]
        check(consoleWrite.method === "settings.set" && consoleWrite.params.key === "steamBigPictureMode"
            && consoleWrite.params.value === true, "console toggle requests the same persisted opt-in")
        client.eventReceived("settings.changed", {key:"steamBigPictureMode", value:true})
        client.responseReceived(consoleWrite.id, {key:"steamBigPictureMode", value:true})
        check(consolePage.settingsModel().find(item => item.key === "steamBigPictureMode").v === "On" && toggle.checked,
            "both controls reflect the persisted value")
        consolePage.destroy()

        ShellStore.nativeRuntimeReady = true
        ShellStore.authSession = {accountId:"fixture"}
        ShellStore.selectedGame = {id:"fixture-parent",title:"Fixture", launchAppId:"12345", variants:[{id:"12345",store:"STEAM",libraryStatus:"MANUAL",librarySelected:true,inLibrary:true}]}
        for (const enabled of [undefined, false, true, false]) {
            for (const controller of [false, true]) {
                for (const consoleMode of [false, true]) {
                    for (const directConsoleMode of [false, true]) {
                        ShellStore.settings = {
                            steamBigPictureMode:enabled,
                            controllerMode:controller,
                            launchInConsoleMode:consoleMode
                        }
                        ShellStore.launchSelectedGame(directConsoleMode)
                        inspect()
                        const request = client.calls[client.calls.length - 1]
                        check(request.method === "session.remote.list", "launch checks existing sessions first")
                        check(request.params.appLaunchMode === (enabled === true ? "gamepadFriendly" : "default"),
                            "only explicit Big Picture opt-in selects gamepad-friendly mode")
                        client.responseReceived(request.id, {sessions:[]})
                        inspect()
                        const create = client.calls[client.calls.length - 1]
                        check(create.method === "session.create" && create.params.appLaunchMode === request.params.appLaunchMode,
                            "session creation preserves the selected launch mode")
                        client.responseReceived(create.id, {session:null})
                        check(!ShellStore.streamBusy, "completed requests allow the next launch")
                    }
                }
            }
        }
        return true
    }
}

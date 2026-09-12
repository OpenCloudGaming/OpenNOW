import QtQuick
import OpenNOW

QtObject {
    id: fixture
    property string savedDevice: ""
    property Component pageComponent: Component {
        DesktopSettingsAudioPage {
            availableWidth: 960
            settingsScreen: fixture
        }
    }
    function valueSetting(key, fallback) { return key === "audioOutputDevice" ? savedDevice : fallback }
    function setSetting(key, value) {
        check(key === "audioOutputDevice", "selector must write the native audio setting")
        savedDevice = value
    }
    function check(ok, message) { if (!ok) throw new Error("Audio output: " + message) }
    function find(item, name) {
        if (item.objectName === name) return item
        for (const child of item.children || []) {
            const result = find(child, name)
            if (result) return result
        }
        return null
    }
    function run(parent) {
        ShellStore.nativeRuntimeReady = false
        ShellStore.audioOutputDevices = [{id: "USB Headphones", name: "USB Headphones"}]
        const page = pageComponent.createObject(parent)
        check(page !== null, "page must load")
        const choice = find(page, "audioOutputDeviceChoice")
        check(choice.value === "" && choice.items[0].value === "", "default must remain selectable")
        choice.expanded = true
        find(page, "settingsChoice-USB Headphones").clicked()
        check(savedDevice === "USB Headphones" && choice.value === savedDevice, "fixed device must be saved")
        ShellStore.audioOutputDevices = []
        check(choice.items.length === 2 && choice.items[1].disabled, "missing saved device must stay visible")
        check(savedDevice === "USB Headphones", "refresh must not replace the saved choice")
        choice.expanded = true
        find(page, "settingsChoice-").clicked()
        check(savedDevice === "" && choice.value === "", "system default must be restorable")
        ShellStore.audioOutputDevicesBusy = true
        ShellStore.nativeRequests = {"audio-test": {operation: "audioDevices"}}
        ShellStore.acceptNativeResponse({id: "audio-test", type: "audioDevices", devices: [{id: "DAC", name: "DAC"}]})
        check(!ShellStore.audioOutputDevicesBusy && choice.items[1].value === "DAC", "native response must populate the selector")
        ShellStore.audioOutputDevicesBusy = true
        ShellStore.nativeRequests = {"audio-error": {operation: "audioDevices"}}
        ShellStore.acceptNativeResponse({id: "audio-error", type: "error", message: "Audio unavailable"})
        check(!ShellStore.audioOutputDevicesBusy && ShellStore.audioOutputDevicesError === "Audio unavailable", "enumeration failure must be visible")
        check(choice.items.length === 1, "failed enumeration must clear stale devices")
        ShellStore.audioOutputDevicesBusy = true
        ShellStore.audioOutputDevicesRequestId = "audio-timeout"
        ShellStore.nativeRequests = {"audio-timeout": {operation: "audioDevices"}}
        ShellStore.audioOutputDevicesTimeout.triggered()
        check(!ShellStore.audioOutputDevicesBusy && ShellStore.audioOutputDevicesError !== "", "timeout must allow retry")
        ShellStore.acceptNativeResponse({id: "audio-timeout", type: "audioDevices", devices: [{id: "stale", name: "stale"}]})
        check(choice.items.length === 1, "late timed-out response must be ignored")
        page.destroy()
        return true
    }
}

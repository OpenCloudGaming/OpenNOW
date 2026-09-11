import QtQuick
import OpenNOW

QtObject {
    property QtObject input: QtObject {
        property var availableControllers: [
            {instanceId: 1, slot: 1, name: "DualSense Wireless Controller", family: "playstation", powerState: "charging", batteryPercent: 65, charging: true},
            {instanceId: 2, slot: 2, name: "Xbox Wireless Controller", family: "xbox", powerState: "onBattery", batteryPercent: 70, charging: false},
            {instanceId: 3, slot: 3, name: "USB Gamepad", family: "generic", powerState: "noBattery", batteryPercent: -1, charging: false},
            {instanceId: 4, slot: 4, name: "Xbox 360 Controller", family: "xbox", powerState: "unknown", batteryPercent: -1, charging: false}
        ]
        property int inputControllerId: 0
        property var controllers: inputControllerId === 0 ? availableControllers
            : availableControllers.filter(controller => controller.instanceId === inputControllerId)
        property int controllerCount: controllers.length
        property bool shellCaptureEnabled: true
        property bool inputSuspended: false
        property int leftStickDeadzone: 24
        property int rightStickDeadzone: 27
        property int vibrationIntensity: 100
        signal controllerActivity()
        signal controllerActivityDetailed(string device, string control, int value)
    }

    function check(ok, message) { if (!ok) throw new Error("Controller metadata: " + message) }
    function find(item, name) {
        if (item.objectName === name) return item
        for (const child of item.children || []) {
            const found = find(child, name)
            if (found) return found
        }
        return null
    }
    function run(parent) {
        const page = find(parent, "desktopControllerSettings")
        check(page, "controls page must be visible")
        ShellStore.applySetting("controllerMode", false)
        check(input.inputSuspended && !input.shellCaptureEnabled,
            "disabled shell navigation must not transfer controller input or rumble to gameplay")
        ShellStore.applySetting("controllerMode", true)
        for (const setting of [
            ["controllerLeftStickDeadzone", "leftStickDeadzone", 24, 50],
            ["controllerRightStickDeadzone", "rightStickDeadzone", 27, 50],
            ["controllerVibrationIntensity", "vibrationIntensity", 100, 100]
        ]) {
            const slider = find(page, setting[0] + "Slider")
            check(slider && slider.from === 0 && slider.to === setting[3], "controller tuning range")
            for (const value of [0, setting[3], setting[2]]) {
                slider.committed(value)
                check(ShellStore.settings[setting[0]] === value, "controller tuning saved")
                check(input[setting[1]] === value, "controller tuning reaches input owner")
                check(slider.value === value, "controller tuning reflects saved value")
            }
        }
        for (const controller of input.availableControllers) {
            const row = find(page, "controllerRow-" + controller.instanceId)
            check(row && row.title === controller.name, "device name must be preserved")
            check(row.glyph === (controller.family === "generic" ? "controller" : controller.family), "family icon")
            check(row.description.indexOf(page.batteryLabel(controller)) >= 0, "battery description")
        }
        const cases = [
            ["onBattery", 0, "Battery 0%"], ["onBattery", 100, "Battery 100%"],
            ["onBattery", -1, "On battery"], ["onBattery", 101, "On battery"],
            ["charging", 65, "Charging · 65%"], ["charging", -1, "Charging"],
            ["charged", -1, "Fully charged"], ["noBattery", 0, "Wired power"],
            ["unknown", 0, "Battery unavailable"]
        ]
        for (const entry of cases)
            check(page.batteryLabel({powerState: entry[0], batteryPercent: entry[1]}) === entry[2], "power state " + entry[0])
        check(page.controllerGlyph({family: "unsupported"}) === "controller", "generic fallback")
        const choice = find(page, "controllerSourceChoice")
        check(choice && choice.items.length === 5, "all sources must be selectable")
        choice.selected(2)
        check(input.inputControllerId === 2 && choice.glyph === "xbox", "selected source branding")
        choice.selected(1)
        check(choice.glyph === "playstation", "PlayStation source branding")
        choice.selected(0)
        check(choice.glyph === "controller", "multiplayer generic icon")
        const devices = input.availableControllers
        input.availableControllers = []
        check(choice.items.length === 1 && input.controllers.length === 0, "disconnect clears device choices")
        input.availableControllers = devices
        choice.expanded = true
        return true
    }
}

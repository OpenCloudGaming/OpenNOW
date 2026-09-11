import QtQuick
import OpenNOW

QtObject {
    function check(ok, message) {
        if (!ok) throw new Error("GPU settings: " + message)
    }

    function find(parent, name) {
        if (parent.objectName === name) return parent
        for (const child of parent.children || []) {
            const match = find(child, name)
            if (match) return match
        }
        return null
    }

    function run(parent) {
        const argumentIndex = Qt.application.arguments.indexOf("--smoke-gpu-count")
        const count = Number(Qt.application.arguments[argumentIndex + 1])
        check(GraphicsDevices.selectorVisible === (count >= 2), "physical GPU visibility rule")
        check(GraphicsDevices.choices.length === count + 1, "only hardware choices plus Automatic")
        const desktop = find(parent, "desktopSettingsScreen")
        if (desktop) {
            const selector = find(parent, "graphicsProcessorSelector")
            check(selector !== null, "desktop selector exists")
            check(selector.visible === (count >= 2), "desktop row is hidden below two GPUs")
            if (count >= 2) {
                check(selector.items.length === count + 1, "desktop lists every GPU")
                check(selector.value === "", "Automatic is initially selected")
                selector.expanded = true
            }
        } else {
            const consoleScreen = find(parent, "consoleSettingsScreen")
            check(consoleScreen !== null, "console settings exists")
            const rows = consoleScreen.settingsModel().filter(row => row.key === "windowsGpuDeviceId")
            check(rows.length === (count >= 2 ? 1 : 0), "console row is omitted below two GPUs")
            if (count >= 2) {
                check(rows[0].values.length === count + 1, "console lists every GPU")
                consoleScreen.openChoices(rows[0])
            }
        }
        return true
    }
}

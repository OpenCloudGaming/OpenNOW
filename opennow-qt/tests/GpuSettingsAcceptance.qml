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
                check(selector.items[0].codecs.join(",") === "h264,h265", "Automatic indexes a decoding GPU")
                check(String(selector.items[0].detail).indexOf("HD Graphics 620") >= 0, "Automatic names the decoding GPU")
                check(selector.items[1].codecs.length === 0, "discrete fixture has no hardware decoder")
                check(String(selector.items[1].detail).indexOf("No hardware decoder") >= 0, "discrete detail lists no decoder")
                check(selector.items[2].codecs.join(",") === "h264,h265", "integrated fixture lists H.264 and H.265")
                selector.expanded = true
            }
            desktop.advancedOpen = true
        } else {
            const consoleScreen = find(parent, "consoleSettingsScreen")
            check(consoleScreen !== null, "console settings exists")
            const rows = consoleScreen.settingsModel().filter(row => row.key === "windowsGpuDeviceId")
            check(rows.length === (count >= 2 ? 1 : 0), "console row is omitted below two GPUs")
            if (count >= 2) {
                check(rows[0].values.length === count + 1, "console lists every GPU")
                check(String(rows[0].labels[1]).indexOf("No hardware decoder") >= 0, "console lists the discrete decode result")
                check(String(rows[0].labels[2]).indexOf("H.264") >= 0, "console lists integrated decode codecs")
                consoleScreen.openChoices(rows[0])
            }
        }
        return true
    }
}

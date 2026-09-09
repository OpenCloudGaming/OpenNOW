import QtQuick
import OpenNOW

QtObject {
    property Component desktopStream: Component { DesktopStreamScreen { visible: false } }
    property Component consoleStream: Component { StreamScreen { visible: false } }
    property Component consoleSettings: Component { SettingsScreen { visible: false; selectedSection: 1 } }

    function check(ok, message) { if (!ok) throw new Error("Upscaling: " + message) }
    function find(item, name) {
        if (item.objectName === name) return item
        for (const child of item.children || []) {
            const result = find(child, name)
            if (result) return result
        }
        return null
    }
    function run(parent) {
        const mac = Qt.platform.os === "osx"
        ShellStore.settings = Object.assign({}, ShellStore.settings, {upscaling: "off", fps: 60})
        const row = find(parent, "upscalingSettingsRow")
        const selector = find(parent, "upscalingSelector")
        check(row && row.visible === mac, "desktop setting is visible only on macOS")
        check(selector && selector.options.length === 2 && selector.selectedIndex === 0, "exactly Off and MetalFX; default Off")
        const settings = consoleSettings.createObject(parent)
        const consoleRow = settings.settingsModel().find(item => item.key === "upscaling")
        check(Boolean(consoleRow) === mac, "console setting exists only on macOS")
        if (mac) check(consoleRow.values.join(",") === "off,metalfx", "console choices match desktop")
        const desktop = desktopStream.createObject(parent)
        const console = consoleStream.createObject(parent)
        const surfaces = [find(desktop, "streamSurfaceHost"), find(console, "streamSurfaceHost")]
        check(surfaces.every(surface => surface && !surface.metalFxUpscaling), "both surfaces default off")
        selector.selected(1, selector.options[1])
        check(ShellStore.settings.upscaling === "metalfx" && selector.selectedIndex === 1, "selection persists exact MetalFX value")
        check(surfaces.every(surface => surface.metalFxUpscaling === mac), "only macOS enables native upscaling")
        check(ShellStore.settings.fps === 60, "upscaling does not change source FPS")
        selector.selected(0, selector.options[0])
        check(ShellStore.settings.upscaling === "off", "Off selection persists")
        check(surfaces.every(surface => !surface.metalFxUpscaling), "both surfaces return to normal scaling")
        check(find(desktop, "streamSurfaceHost") === surfaces[0]
            && find(console, "streamSurfaceHost") === surfaces[1], "toggling never replaces the presenter")
        if (Qt.application.arguments.indexOf("--screenshot") >= 0)
            row.visible = true
        ShellStore.lastError = ""
        settings.destroy()
        desktop.destroy()
        console.destroy()
        return true
    }
}

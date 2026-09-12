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
        const mode = mac ? "metalfx" : "fsr1"
        ShellStore.settings = Object.assign({}, ShellStore.settings, {upscaling: "off", fps: 60})
        const row = find(parent, "upscalingSettingsRow")
        const selector = find(parent, "upscalingSelector")
        check(row && row.visible, "desktop upscaling setting is visible on every platform")
        check(selector && selector.options.length === 2 && selector.selectedIndex === 0, "exactly two choices; default Off")
        check(selector.options.map(item => item.value).join(",") === "off," + mode, "desktop offers the platform upscaler")
        const clarityRow = find(parent, "upscalingSharpnessRow")
        const denoiseRow = find(parent, "upscalingDenoiseRow")
        const clarity = find(parent, "upscalingSharpnessSlider")
        const denoise = find(parent, "upscalingDenoiseSlider")
        check(clarityRow && denoiseRow && clarityRow.visible && denoiseRow.visible === mac, "Clarity is cross-platform; Noise Reduction remains macOS-only")
        check(!clarityRow.enabled && !denoiseRow.enabled, "enhancement controls are disabled when upscaling is off")
        check(clarity.from === 0 && clarity.to === 15 && clarity.stepSize === 1 && clarity.value === 10, "Clarity matches Mac range and default")
        check(denoise.from === 0 && denoise.to === 20 && denoise.stepSize === 1 && denoise.value === 0, "Noise Reduction matches Mac range and default")
        const settings = consoleSettings.createObject(parent)
        const consoleRow = settings.settingsModel().find(item => item.key === "upscaling")
        check(Boolean(consoleRow), "console setting exists on every platform")
        check(consoleRow.values.join(",") === "off," + mode, "console choices match desktop")
        const desktop = desktopStream.createObject(parent)
        const console = consoleStream.createObject(parent)
        const surfaces = [find(desktop, "streamSurfaceHost"), find(console, "streamSurfaceHost")]
        check(surfaces.every(surface => surface && !surface.metalFxUpscaling && !surface.fsrUpscaling), "both surfaces default off")
        selector.selected(1, selector.options[1])
        check(ShellStore.settings.upscaling === mode && selector.selectedIndex === 1, "selection persists the exact upscaler value")
        check(surfaces.every(surface => surface.metalFxUpscaling === mac && surface.fsrUpscaling === !mac), "both surfaces enable only the selected platform upscaler")
        check(clarityRow.enabled && denoiseRow.enabled === mac, "upscaling enables only applicable enhancement controls")
        clarity.committed(15)
        denoise.committed(20)
        check(ShellStore.settings.upscalingSharpness === 15 && ShellStore.settings.upscalingDenoise === 20, "slider changes persist")
        check(surfaces.every(surface => surface.upscalingSharpness === 15 && surface.upscalingDenoise === 20), "both existing surfaces receive live values")
        if (mac) {
            const controls = settings.settingsModel().filter(item => item.key === "upscalingSharpness" || item.key === "upscalingDenoise")
            check(controls.length === 2 && controls[0].values.length === 16 && controls[1].values.length === 21, "console exposes the same integer ranges")
            check(controls.every(item => !item.info), "console controls are enabled with MetalFX")
        } else {
            const controls = settings.settingsModel().filter(item => item.key === "upscalingSharpness" || item.key === "upscalingDenoise")
            check(controls.length === 1 && controls[0].values.length === 16 && !controls[0].info, "FSR console exposes the enabled clarity range without denoise")
        }
        check(ShellStore.settings.fps === 60, "upscaling does not change source FPS")
        selector.selected(0, selector.options[0])
        check(ShellStore.settings.upscaling === "off", "Off selection persists")
        check(surfaces.every(surface => !surface.metalFxUpscaling && !surface.fsrUpscaling), "both surfaces return to normal scaling")
        check(ShellStore.settings.upscalingSharpness === 15 && ShellStore.settings.upscalingDenoise === 20, "Off retains enhancement preferences")
        clarity.committed(0)
        denoise.committed(0)
        check(surfaces.every(surface => surface.upscalingSharpness === 0 && surface.upscalingDenoise === 0), "zero disables each enhancement without using the default")
        check(find(desktop, "streamSurfaceHost") === surfaces[0]
            && find(console, "streamSurfaceHost") === surfaces[1], "toggling never replaces the presenter")
        if (Qt.application.arguments.indexOf("--screenshot") >= 0) {
            row.visible = true
            clarityRow.visible = true
            selector.selected(1, selector.options[1])
            clarity.committed(10)
        }
        ShellStore.lastError = ""
        settings.destroy()
        desktop.destroy()
        console.destroy()
        return true
    }
}

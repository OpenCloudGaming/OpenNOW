import QtQuick
import OpenNOW

QtObject {
    property Component desktopStream: Component { DesktopStreamScreen { visible: false } }
    property Component consoleStream: Component { StreamScreen { visible: false } }
    property Component statsComponent: Component { DesktopStreamStats { visible: false } }

    function check(ok, message) { if (!ok) throw new Error("Frame generation: " + message) }
    function find(item, name) {
        if (item.objectName === name) return item
        for (const child of item.children || []) {
            const result = find(child, name)
            if (result) return result
        }
        return null
    }
    function run(parent) {
        ShellStore.settings = Object.assign({}, ShellStore.settings, {fps: 60, frameGeneration: "off"})
        const two = find(parent, "settingsOption-2x")
        check(two && two.enabled, "2x setting is selectable")
        check(!find(parent, "settingsOption-3x"), "no 3x option")
        const desktop = desktopStream.createObject(parent)
        const console = consoleStream.createObject(parent)
        const surfaces = [find(desktop, "streamSurfaceHost"), find(console, "streamSurfaceHost")]
        check(surfaces.every(surface => surface && !surface.frameGeneration), "both surfaces default off")
        two.clicked()
        check(ShellStore.settings.frameGeneration === "2x", "selection stores the exact 2x value")
        check(surfaces.every(surface => surface.frameGeneration), "both surfaces enable local generation")
        check(ShellStore.settings.fps === 60, "local generation does not raise the stream FPS")
        ShellStore.applySetting("frameGeneration", "off")
        check(surfaces.every(surface => !surface.frameGeneration), "both surfaces return to off")
        check(find(desktop, "streamSurfaceHost") === surfaces[0]
            && find(console, "streamSurfaceHost") === surfaces[1], "the presenter is never replaced")
        ShellStore.applySetting("frameGeneration", "2x")
        const stats = statsComponent.createObject(parent)
        ShellStore.streamer = {status: "streaming", framesPerSecond: 60}
        stats.frameGenerationStats = {status: "active", outputFps: 117}
        check(stats.cards.find(card => card.field === "framesPerSecond").value === 60,
            "source FPS remains the received measurement")
        check(stats.cards.find(card => card.field === "frameGenerationOutputFps").value === 117,
            "output FPS uses measured swaps, not source times two")
        stats.frameGenerationStats = {status: "overloaded", outputFps: 59}
        check(stats.cards.find(card => card.field === "frameGenerationOutputFps").value === 59,
            "fallback is reflected in the output measurement")
        ShellStore.streamer = {status: "streaming", framesPerSecond: 120}
        stats.frameGenerationStats = {status: "source-rate-limit", outputFps: 120}
        check(stats.frameGenerationState() === qsTr("120 FPS generation limit"),
            "the source-rate limit has an explicit status")
        check(stats.cards.find(card => card.field === "framesPerSecond").value === 120
            && stats.cards.find(card => card.field === "frameGenerationOutputFps").value === 120,
            "a 120 FPS source stays at 120 rather than claiming 240")
        if (Qt.application.arguments.indexOf("--screenshot") >= 0) {
            stats.anchors.fill = parent
            stats.z = 10000
            stats.expanded = true
            stats.visible = true
        } else {
            ShellStore.streamer = {status: "stopped"}
            stats.destroy()
        }
        ShellStore.lastError = ""
        desktop.destroy()
        console.destroy()
        return true
    }
}

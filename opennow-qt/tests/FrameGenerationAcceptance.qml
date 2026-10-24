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
        // Levels stay locked until the capability test has allowed them for the current
        // preset (resolution, frame rate, GPU, engine).
        ShellStore.settings = Object.assign({}, ShellStore.settings,
            {fps: 60, resolution: "1920x1080", frameGeneration: "off", frameGenerationTargetFps: 0, frameGenerationProfiles: {}})
        const two = find(parent, "settingsOption-2x")
        const three = find(parent, "settingsOption-3x")
        const four = find(parent, "settingsOption-4x")
        check(two && three && four, "2x, 3x and 4x levels exist")
        check(!two.enabled && !three.enabled && !four.enabled, "levels are locked before a capability test")
        check(!find(parent, "frameGenerationTestButton") || find(parent, "frameGenerationTestButton").enabled,
            "the capability test can be started")
        // A saved level without a passing profile must not run: the surfaces stay off.
        ShellStore.settings = Object.assign({}, ShellStore.settings, {frameGeneration: "2x"})
        check(ShellStore.frameGenerationFactor() === 0, "an untested preset never generates frames")
        ShellStore.settings = Object.assign({}, ShellStore.settings, {frameGeneration: "off"})
        ShellStore.ensureFrameGenerationGpu()
        // A measurement unlocks the levels that fit the selected rate on the display: at
        // 60 FPS with 2.1 ms per frame every level fits the GPU, and a 180 Hz display shows
        // 2x (120) and 3x (180) but not 4x (240).
        const profiles = {}
        profiles[ShellStore.settingsOwnerState.frameGenerationProfileKey()] = {
            engine: "motion", gpu: ShellStore.settingsOwnerState.frameGenerationGpu, width: 1920, height: 1080,
            fps: 60, refreshHz: 180, p50Ms: 1.5, p95Ms: 2.1, maxMs: 3, frames: 180, gpuTimed: true,
            engineMaxFactor: 4, allowed: [2, 3], testedAt: "acceptance", version: 1}
        ShellStore.settings = Object.assign({}, ShellStore.settings,
            {frameGenerationProfiles: profiles, frameGenerationDisplayHz: 180})
        check(two.enabled && three.enabled, "a passing profile unlocks the allowed levels")
        check(!four.enabled, "levels the profile rules out stay locked")
        const desktop = desktopStream.createObject(parent)
        const console = consoleStream.createObject(parent)
        const surfaces = [find(desktop, "streamSurfaceHost"), find(console, "streamSurfaceHost")]
        check(surfaces.every(surface => surface && !surface.frameGeneration), "both surfaces default off")
        two.clicked()
        check(ShellStore.settings.frameGeneration === "2x", "selection stores the exact 2x value")
        check(surfaces.every(surface => surface.frameGeneration), "both surfaces enable local generation")
        check(ShellStore.settings.fps === 60, "local generation does not raise the stream FPS")
        // The in-stream toggle (Ctrl+G tile and shortcut) switches off and restores the choice.
        check(ShellStore.frameGenerationQuickVisible(), "the stream menu tile shows while generation is on")
        ShellStore.toggleFrameGeneration()
        check(ShellStore.frameGenerationFactor() === 0 && surfaces.every(surface => !surface.frameGeneration),
            "the toggle switches generation off at once")
        check(ShellStore.frameGenerationQuickVisible(), "the tile stays so it can be switched back on")
        ShellStore.toggleFrameGeneration()
        check(ShellStore.settings.frameGeneration === "2x" && ShellStore.frameGenerationFactor() === 2,
            "the toggle restores the 2x level")
        ShellStore.applyStreamShortcutAction("toggle-frame-generation")
        check(ShellStore.frameGenerationFactor() === 0 && ShellStore.streamControlMessage === qsTr("Frame generation off"),
            "the shortcut switches generation off and reports it")
        ShellStore.applyStreamShortcutAction("toggle-frame-generation")
        check(ShellStore.frameGenerationFactor() === 2, "the shortcut restores generation")
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
        check(stats.frameGenerationState() === qsTr("Source rate too high"),
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

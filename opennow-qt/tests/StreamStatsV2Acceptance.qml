import QtQuick
import OpenNOW

QtObject {
    property Component statsComponent: Component { DesktopStreamStats { visible: false } }

    function check(ok, message) { if (!ok) throw new Error("Stream stats V2: " + message) }
    function find(item, name) {
        if (item.objectName === name) return item
        for (const child of item.children || []) {
            const result = find(child, name)
            if (result) return result
        }
        return null
    }
    function run(parent) {
        ShellStore.settings = Object.assign({}, ShellStore.settings, {
            statsOverlayScale: 1, statsOverlayOpacity: 94, statsOverlayPosition: "top-right",
            statsShowGraphs: true, statsShowPing: true, statsShowFps: true,
            statsShowBitrate: true, statsShowJitter: true, statsShowDrops: true,
            statsShowPacketLoss: true, statsShowDecode: true, statsShowLatency: true,
            statsShowRegion: true, statsShowVideo: true, statsShowClock: true,
            maxBitrateMbps: 100, frameGeneration: "off", shortcutToggleStats: "F3"
        })
        ShellStore.activeSession = {sessionId:"stats-v2-fixture", regionName:"EU-WEST", serverLocation:"Amsterdam", rigName:"RTX 5080",
            negotiatedStreamProfile:{codec:"AV1", resolution:"2560x1440", colorQuality:"10bit_420", enableHdr:true}}
        ShellStore.runtimeStreamProfile = {maxBitrateMbps:75}
        ShellStore.streamStartedAtMs = Date.now() - 6130000
        ShellStore.streamer = {status:"streaming", framesPerSecond:120, pingMs:9, latencyMs:31,
            bitrateMbps:74.6, jitterMs:1.2, packetLossPercent:0, decodeTimeMs:2.1, mediaBackend:"Vulkan"}
        const stats = statsComponent.createObject(parent, {width:parent.width, height:parent.height})
        check(stats !== null, "overlay created")
        check(stats.allocatedBitrateMbps === 75, "allocation uses prepared session, not editable settings")
        check(Math.abs(stats.bitrateUsage - 74.6 / 75) < 0.000001, "bar uses measured / allocated bitrate")
        check(stats.healthKnown && !stats.degraded, "zero packet loss is healthy")
        check(stats.videoText === "AV1 · 2560×1440 · 10-bit 4:2:0 · HDR", "real negotiated profile fields format correctly")
        check(stats.featureBadges.length === 1 && stats.featureBadges[0].text === "HDR", "only enabled features are advertised")
        ShellStore.acceptNativeEvent({type:"telemetry", bitrateMbps:120, packetLossPercent:1.8})
        check(stats.bitrateUsage === 1 && stats.degraded, "overshoot is bounded and packet loss degrades health")
        ShellStore.streamer = Object.assign({}, ShellStore.streamer, {bitrateMbps:null, packetLossPercent:null})
        check(stats.bitrateUsage === 0 && !stats.healthKnown, "missing measurements are not a full bar or healthy status")
        ShellStore.runtimeStreamProfile = {}
        ShellStore.settings = Object.assign({}, ShellStore.settings, {maxBitrateMbps:0})
        check(stats.allocatedBitrateMbps === 0 && stats.bitrateUsage === 0, "unknown allocation cannot divide by zero")
        ShellStore.runtimeStreamProfile = {maxBitrateMbps:75}
        ShellStore.settings = Object.assign({}, ShellStore.settings, {maxBitrateMbps:100})
        ShellStore.acceptNativeEvent({type:"telemetry", bitrateMbps:74.6, packetLossPercent:0})
        for (let i = 0; i < 75; ++i) stats.sampleHistory()
        check(stats.history.framesPerSecond.length === 60, "history is bounded to sixty samples")
        ShellStore.streamer = Object.assign({}, ShellStore.streamer, {status:"starting"})
        check(Object.keys(stats.history).length === 0 && !stats.healthKnown, "restart clears history and health")
        ShellStore.streamer = Object.assign({}, ShellStore.streamer, {status:"streaming"})
        stats.pointerLocked = true
        check(!stats.enabled, "stats never capture pointer input during gameplay")
        stats.pointerLocked = false
        check(stats.enabled, "unlocked pointer restores controls")
        for (const position of ["top-left", "top-right", "bottom-left", "bottom-right"]) {
            ShellStore.settings = Object.assign({}, ShellStore.settings, {statsOverlayPosition:position, statsOverlayScale:1.5})
            const panel = find(stats, "expandedStatsPanel")
            check(panel && panel.width === Math.min(parent.width - 48, 630), "panel honors overlay scaling")
            check(panel.x >= 0 && panel.x + panel.width <= parent.width, "panel stays inside the viewport")
        }
        ShellStore.settings = Object.assign({}, ShellStore.settings, {statsOverlayPosition:"top-right",
            statsOverlayScale:Qt.application.arguments.indexOf("--smoke-stats-scaled") >= 0 ? 1.5 : 1})
        if (Qt.application.arguments.indexOf("--smoke-stats-degraded") >= 0) {
            ShellStore.acceptNativeEvent({type:"telemetry", framesPerSecond:112, pingMs:38, latencyMs:94,
                bitrateMbps:58.2, jitterMs:4.6, packetLossPercent:1.8})
        }
        stats.expanded = Qt.application.arguments.indexOf("--smoke-stats-compact") < 0
        stats.visible = true
        for (let i = 0; i < 8; ++i) {
            ShellStore.acceptNativeEvent({type:"telemetry", jitterMs:stats.degraded ? 4.6 : 1.2,
                pingMs:(stats.degraded ? 38 : 9) + i % 3,
                framesPerSecond:(stats.degraded ? 112 : 120) - i % 2,
                latencyMs:(stats.degraded ? 94 : 31) + i % 3})
            stats.sampleHistory()
        }
        ShellStore.acceptNativeEvent({type:"telemetry", pingMs:stats.degraded ? 38 : 9,
            framesPerSecond:stats.degraded ? 112 : 120, latencyMs:stats.degraded ? 94 : 31})
        if (Qt.application.arguments.indexOf("--screenshot") >= 0) {
            const samples = stats.history
            const expanded = stats.expanded
            stats.destroy()
            AppController.showOverlay(expanded ? "desktop-stream-stats-expanded" : "desktop-stream-stats")
            const host = find(parent, "desktopStreamOverlayHost")
            const renderedStats = find(host, "desktopStreamStats")
            check(renderedStats !== null, "production overlay is mounted")
            renderedStats.history = samples
            if (Qt.application.arguments.indexOf("--smoke-stats-toasts") >= 0) {
                ShellStore.acceptNativeEvent({type:"telemetry", packetLossPercent:0})
                const toasts = find(host, "desktopStreamToasts")
                check(toasts !== null, "production toast stack is mounted")
                toasts.controllers = [{instanceId:2, slot:2, name:"Xbox Wireless Controller",
                    family:"xbox", powerState:"onBattery", batteryPercent:82}]
                for (const loss of [0, 0.2, 0.1, 0.5, 0.3, 0.7, 0.3, 0.6, 0.4, 0.5])
                    ShellStore.acceptNativeEvent({type:"telemetry", packetLossPercent:loss})
                check(toasts.controllerNotice !== null && toasts.lossNotice, "controller and packet-loss notices are visible")
                check(toasts.lossHistory.length <= 12, "toast history remains bounded")
                check(toasts.y >= renderedStats.topRightInset, "toasts respect the actual stats geometry")
            }
        } else {
            stats.destroy()
        }
        return true
    }
}

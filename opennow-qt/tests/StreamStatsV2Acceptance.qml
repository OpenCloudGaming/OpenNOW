import QtQuick
import QtQuick.Window
import OpenNOW

QtObject {
    id: fixture
    property real sampleTime: 100000
    property Component statsComponent: Component { DesktopStreamStats { visible: false } }
    property Component toastComponent: Component { DesktopStreamToasts {} }

    function sample(loss, elapsed = 100) {
        sampleTime += elapsed
        ShellStore.acceptNativeEvent({type: "telemetry", packetLossPercent: loss})
    }

    function sustainedLoss(loss) {
        sample(loss)
        sample(loss, 2000)
    }

    function checkConnectionHealth(stats, parent) {
        const health = ShellStore.connectionHealth
        const host = find(parent, "desktopStreamOverlayHost")
        check(host !== null, "production toast host is mounted")
        host.connectionNotificationsEnabled = false
        const toast = toastComponent.createObject(parent)
        check(toast !== null, "connection toast created")
        sample(0)
        sample(0.5)
        check(!stats.degraded && !toast.lossNotice, "first spike never warns")
        sampleTime += 2000
        ShellStore.acceptNativeEvent({type: "telemetry", pingMs: 20})
        ShellStore.acceptNativeEvent({type: "log", packetLossPercent: 5})
        ShellStore.streamer = Object.assign({}, ShellStore.streamer, {packetLossPercent: 8})
        check(!stats.degraded && !toast.lossNotice, "unrelated events and cached fields cannot sustain loss")
        check(stats.read("packetLossPercent") === 0.5
            && stats.cards.find(card => card.key === "PacketLoss").value === 0.5
            && toast.lastLoss === 0.5,
            "stats and toast retain the last genuine sample instead of cached packet loss")
        sample(0.5, 0)
        check(stats.degraded && toast.lossNotice, "fresh loss at two seconds warns in both surfaces")
        sample(0.1)
        check(stats.degraded, "recovery threshold is strictly below 0.1 percent")
        sample(0.3)
        check(stats.degraded, "hysteresis retains unstable between thresholds")
        toast.visible = false
        toast.visible = true
        check(!toast.lossNotice, "overlay reopening does not replay a claimed warning")
        toast.visible = false
        toast.destroy()
        const recreated = toastComponent.createObject(parent)
        check(!recreated.lossNotice, "overlay recreation does not replay a claimed warning")
        sample(0.04)
        check(stats.healthText === qsTr("Stream healthy") && !stats.degraded && !recreated.lossNotice,
            "loss rounded to 0.0 immediately recovers both surfaces")
        sustainedLoss(0.5)
        check(stats.degraded && !recreated.lossNotice, "recovered episode still respects cooldown")
        sampleTime += 5000
        health.expire(sampleTime)
        check(!stats.healthKnown && !recreated.lossNotice, "stale input becomes unknown without a warning")
        check(stats.read("packetLossPercent") === null && recreated.lastLoss === null,
            "stale packet loss becomes unavailable in both surfaces")
        sample(0.5)
        sample(0.49, 2000)
        sample(0.5)
        sample(0.5, 1999)
        check(!stats.degraded, "flapping interrupts the pending sustained-loss duration")
        sample(0.5, 1)
        check(stats.degraded, "exactly two seconds at the threshold is unstable")
        for (const invalid of [null, undefined, "", "0.5", true, -1, 101, NaN, Infinity]) {
            sample(invalid)
            check(!stats.healthKnown && !recreated.lossNotice, "invalid input clears health and the active toast")
            check(stats.read("packetLossPercent") === null && recreated.lastLoss === null,
                "invalid loss cannot replace the unavailable metric with a coerced value")
        }
        sample(0.5)
        sample(0.5, -1000)
        check(!stats.degraded, "clock rollback cannot fabricate sustained duration")
        sample(0.5, 1999)
        check(!stats.degraded, "rollback restarts the pending interval")
        sample(0.5, 1)
        check(stats.degraded, "fresh samples can establish health after rollback")
        ShellStore.streamerStopExpected = true
        check(!stats.healthKnown && !recreated.lossNotice, "stop request clears health immediately")
        sample(2)
        check(!stats.healthKnown, "stopping cannot accept a new health sample")
        ShellStore.streamerStopExpected = false
        ShellStore.streamer = Object.assign({}, ShellStore.streamer, {status: "starting"})
        ShellStore.streamer = Object.assign({}, ShellStore.streamer, {status: "streaming"})
        sample(0.5)
        check(!stats.degraded, "restart cannot inherit a bad interval")
        ShellStore.activeSession = Object.assign({}, ShellStore.activeSession, {sessionId: "health-new-session"})
        check(!stats.healthKnown, "new session discards cached health")
        ShellStore.acceptNativeEvent({type: "telemetry", sessionId: "stats-v2-fixture", packetLossPercent: 0})
        check(!stats.healthKnown, "old-session telemetry cannot establish health")
        sample(0)
        sample(0.49)
        check(stats.healthKnown && !stats.degraded, "healthy state survives the hysteresis band")
        sampleTime += 30000
        sample(0.5)
        check(!stats.degraded, "a stale gap cannot count towards a bad interval")
        sample(0.5, 2000)
        check(stats.degraded && recreated.lossNotice, "a new sustained episode may warn after cooldown")
        sample(0.5, -60000)
        check(!stats.degraded && !recreated.lossNotice && health.lastNoticeAt === null,
            "rollback clears a future-dated cooldown without inheriting bad duration")
        sample(0.5, 2000)
        check(stats.degraded && recreated.lossNotice, "fresh sustained loss can notify after clock rollback")
        sample(0.04)
        check(!recreated.lossNotice && !stats.degraded, "fresh recovery clears an active loss toast")
        recreated.visible = false
        recreated.destroy()
        ShellStore.activeSession = Object.assign({}, ShellStore.activeSession, {sessionId: "stats-v2-fixture"})
        sample(0)
        host.connectionNotificationsEnabled = true
    }

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
            bitrateMbps:74.6, receiveBitrateMbps:82.1, jitterMs:1.2, packetLossPercent:0, decodeTimeMs:2.1, decoderResidenceMs:6.4,
            mediaBackend:"Vulkan"}
        ShellStore.connectionHealth.clock = () => fixture.sampleTime
        sample(0)
        const stats = statsComponent.createObject(parent, {width:parent.width, height:parent.height})
        check(stats !== null, "overlay created")
        check(stats.allocatedBitrateMbps === 75, "allocation uses prepared session, not editable settings")
        check(Math.abs(stats.bitrateUsage - 74.6 / 75) < 0.000001, "bar uses measured / allocated bitrate")
        check(stats.cards.find(card => card.field === "bitrateMbps").label === qsTr("VIDEO BITRATE")
            && stats.cards.find(card => card.field === "receiveBitrateMbps").label === qsTr("STREAM UDP RECEIVE")
            && stats.ledgerCards.some(card => card.field === "receiveBitrateMbps" && card.value === 82.1)
            && stats.ledgerDetail(stats.cards.find(card => card.field === "receiveBitrateMbps"))
                === qsTr("known session peer · UDP datagram bytes")
            && stats.report().includes("STREAM UDP RECEIVE: 82.1 Mbps"),
            "video and peer-filtered socket rates remain distinct in the expanded panel and report")
        const compactReceive = find(stats, "compactSocketReceive")
        const compactBar = find(stats, "compactStatsBar")
        check(compactReceive && compactReceive.text === qsTr("UDP RX") + " 82.1"
            && compactBar && compactBar.x >= 0 && compactBar.x + compactBar.width <= stats.width,
            "default compact F3 shows the measured socket rate within the viewport")
        ShellStore.acceptNativeEvent({type:"telemetry", receiveBitrateMbps:null})
        check(stats.read("receiveBitrateMbps") === null && stats.report().includes("STREAM UDP RECEIVE: N/A Mbps"),
            "unavailable socket samples must not retain a previous rate")
        ShellStore.acceptNativeEvent({type:"telemetry", receiveBitrateMbps:82.1})
        check(stats.healthKnown && !stats.degraded, "zero packet loss is healthy")
        check(stats.videoText === "AV1 · 2560×1440 · 10-bit 4:2:0 · HDR", "real negotiated profile fields format correctly")
        check(stats.featureBadges.length === 1 && stats.featureBadges[0].text === "HDR", "only enabled features are advertised")
        checkConnectionHealth(stats, parent)
        ShellStore.acceptNativeEvent({type:"log", event:"queue-dropped", unit:"frames", count:23})
        ShellStore.acceptNativeEvent({type:"log", event:"queue-dropped", unit:"packets", count:1})
        ShellStore.acceptNativeEvent({type:"telemetry", bitrateMbps:120, packetLossPercent:1.8})
        check(!stats.degraded, "an isolated loss spike does not degrade health")
        sample(1.8, 2000)
        check(stats.bitrateUsage === 1 && stats.degraded, "overshoot is bounded and packet loss degrades health")
        ShellStore.acceptNativeEvent({type:"telemetry", bitrateMbps:null, packetLossPercent:null})
        check(stats.bitrateUsage === 0 && !stats.healthKnown, "missing measurements are not a full bar or healthy status")
        ShellStore.runtimeStreamProfile = {}
        ShellStore.settings = Object.assign({}, ShellStore.settings, {maxBitrateMbps:0})
        check(stats.allocatedBitrateMbps === 0 && stats.bitrateUsage === 0, "unknown allocation cannot divide by zero")
        ShellStore.runtimeStreamProfile = {maxBitrateMbps:75}
        ShellStore.settings = Object.assign({}, ShellStore.settings, {maxBitrateMbps:100})
        ShellStore.acceptNativeEvent({type:"telemetry", bitrateMbps:74.6, packetLossPercent:0})
        check(!stats.degraded && stats.read("videoDropCount") === 23
            && stats.read("audioPacketDropCount") === 1,
            "fresh healthy telemetry preserves cumulative video and audio drops")
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
            sample(1.8, 2000)
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
            check(renderedStats.swapStats.gated !== true,
                "an open overlay never gates the Qt swap measurement")
            ShellStore.currentStreamInputPaused = true
            check(renderedStats.swapStats.gated !== true,
                "suspended input for a local overlay never gates the Qt swap measurement")
            ShellStore.currentStreamInputPaused = false
            if (Qt.application.arguments.indexOf("--smoke-stats-gated") >= 0) {
                renderedStats.swapStats = ({gated: true, gateSource: "minimized"})
                check(renderedStats.report().split("\n").includes(qsTr("SWAP GATE") + ": " + qsTr("Window minimized")),
                    "a minimized window names the gate source instead of a duration")
            } else {
                renderedStats.swapStats = ({p50Ms: 4.2, p95Ms: 9.6, maxMs: 14.1, windowSamples: 256,
                    swappedFramesTotal: 12345, gated: false})
                check(renderedStats.report().split("\n").includes(qsTr("QT SUBMIT TO SWAP") + ": 4.2 ms"),
                    "the production overlay renders the measured Qt submit-to-swap value")
                check(renderedStats.swapStats.swappedFramesTotal === 12345 && renderedStats.swapStats.p95Ms === 9.6,
                    "the production overlay carries the cumulative swap total and percentiles")
            }
            if (Qt.application.arguments.indexOf("--smoke-stats-toasts") >= 0) {
                ShellStore.acceptNativeEvent({type:"telemetry", packetLossPercent:0})
                const toasts = find(host, "desktopStreamToasts")
                check(toasts !== null, "production toast stack is mounted")
                toasts.controllers = [{instanceId:2, slot:2, name:"Xbox Wireless Controller",
                    family:"xbox", powerState:"onBattery", batteryPercent:82}]
                sampleTime += 30000
                for (const loss of [0, 0.2, 0.1, 0.5, 0.3, 0.7, 0.3, 0.6, 0.4, 0.5])
                    sample(loss)
                sample(0.5, 2000)
                check(toasts.controllerNotice !== null && toasts.lossNotice, "controller and packet-loss notices are visible")
                check(toasts.lossHistory.length <= 12, "toast history remains bounded")
                check(toasts.y >= renderedStats.topRightInset, "toasts respect the actual stats geometry")
                if (Qt.application.arguments.indexOf("--smoke-stats-recovered") >= 0) {
                    sample(0.04)
                    check(!toasts.lossNotice && renderedStats.healthText === qsTr("Stream healthy"),
                        "production overlay immediately clears the rounded-zero loss warning")
                    check(renderedStats.read("videoDropCount") === 23
                        && renderedStats.read("audioPacketDropCount") === 1,
                        "recovery leaves historical frame and audio queue drops visible")
                }
                if (Qt.application.arguments.indexOf("--smoke-stats-closed") >= 0)
                    AppController.showOverlay("")
            }
            if (Qt.application.arguments.indexOf("--smoke-stats-fullscreen") >= 0)
                parent.Window.window.showFullScreen()
        } else {
            stats.destroy()
        }
        return true
    }
}

import QtQuick
import OpenNOW

QtObject {
    property Component statsComponent: Component { DesktopStreamStats { visible: false } }

    function check(ok, message) { if (!ok) throw new Error("Stream stats: " + message) }
    function checkMetric(stats, key, label, expected) {
        const card = stats.cards.find(item => item.key === key)
        const compact = stats.compactMetrics.find(item => item.text.startsWith(label + " "))
        const line = stats.report().split("\n").find(item => item.startsWith(label + ": "))
        if (expected === null) {
            check(!card && !compact && !line, key + " is absent from cards, compact text, and report")
        } else {
            check(card && card.value === expected, key + " preserves its measured value")
            const value = stats.format(expected, card.decimals) + " " + card.unit
            check(compact && compact.text === label + " " + value, key + " compact value")
            check(line === label + ": " + value, key + " report value")
        }
    }
    function run(parent) {
        ShellStore.settings = Object.assign({}, ShellStore.settings, {
            statsShowPing: true, statsShowDecode: true, statsShowLatency: true,
            statsShowGraphs: false, frameGeneration: "off"
        })
        ShellStore.activeSession = null
        ShellStore.streamer = {status: "streaming"}
        const stats = statsComponent.createObject(parent)
        check(stats !== null, "overlay created")
        stats.expanded = Qt.application.arguments.indexOf("--smoke-stats-expanded") >= 0
        checkMetric(stats, "Decode", qsTr("DECODE"), null)
        checkMetric(stats, "Latency", qsTr("LATENCY"), null)
        check(stats.cards.find(item => item.key === "Ping").value === null,
            "missing ping remains unavailable rather than a fabricated value")
        ShellStore.acceptNativeEvent({type: "telemetry", pingMs: 23, decodeTimeMs: null, latencyMs: null})
        check(ShellStore.streamer.pingMs === 23, "native ping reaches ShellStore")
        checkMetric(stats, "Ping", qsTr("PING"), 23)
        checkMetric(stats, "Decode", qsTr("DECODE"), null)
        checkMetric(stats, "Latency", qsTr("LATENCY"), null)
        ShellStore.acceptNativeEvent({type: "telemetry", pingMs: 0, decodeTimeMs: 0, latencyMs: 0})
        checkMetric(stats, "Ping", qsTr("PING"), 0)
        checkMetric(stats, "Decode", qsTr("DECODE"), 0)
        checkMetric(stats, "Latency", qsTr("LATENCY"), 0)
        ShellStore.acceptNativeEvent({type: "telemetry", decodeTimeMs: 2.5, latencyMs: 18})
        checkMetric(stats, "Decode", qsTr("DECODE"), 2.5)
        checkMetric(stats, "Latency", qsTr("LATENCY"), 18)
        ShellStore.settings = Object.assign({}, ShellStore.settings, {
            statsShowPing: false, statsShowDecode: false, statsShowLatency: false
        })
        checkMetric(stats, "Ping", qsTr("PING"), null)
        checkMetric(stats, "Decode", qsTr("DECODE"), null)
        checkMetric(stats, "Latency", qsTr("LATENCY"), null)
        ShellStore.settings = Object.assign({}, ShellStore.settings, {
            statsShowPing: true, statsShowDecode: true, statsShowLatency: true
        })
        checkMetric(stats, "Decode", qsTr("DECODE"), 2.5)
        checkMetric(stats, "Latency", qsTr("LATENCY"), 18)
        ShellStore.acceptNativeEvent({type: "telemetry", decodeTimeMs: null})
        checkMetric(stats, "Decode", qsTr("DECODE"), null)
        checkMetric(stats, "Latency", qsTr("LATENCY"), 18)
        ShellStore.acceptNativeEvent({type: "telemetry", decodeTimeMs: 1.5, latencyMs: null})
        checkMetric(stats, "Decode", qsTr("DECODE"), 1.5)
        checkMetric(stats, "Latency", qsTr("LATENCY"), null)
        ShellStore.acceptNativeEvent({type: "telemetry", pingMs: null, decodeTimeMs: "invalid", latencyMs: "invalid"})
        check(ShellStore.streamer.pingMs === null, "an unavailable native ping clears the previous sample")
        check(stats.cards.find(item => item.key === "Ping").value === null,
            "an unavailable native ping clears the displayed value")
        checkMetric(stats, "Decode", qsTr("DECODE"), null)
        checkMetric(stats, "Latency", qsTr("LATENCY"), null)
        ShellStore.streamer = {status: "starting", pingMs: 23, decodeTimeMs: 2.5, latencyMs: 18}
        checkMetric(stats, "Decode", qsTr("DECODE"), null)
        checkMetric(stats, "Latency", qsTr("LATENCY"), null)
        check(stats.cards.find(item => item.key === "Ping").value === null,
            "non-streaming telemetry is unavailable")
        ShellStore.acceptStreamerSnapshot({status: "streaming", pingMs: 31})
        checkMetric(stats, "Ping", qsTr("PING"), 31)
        ShellStore.acceptStreamerSnapshot({status: "streaming"})
        check(stats.cards.find(item => item.key === "Ping").value === null,
            "replacement snapshots without ping do not retain an old measurement")
        ShellStore.acceptNativeEvent({type: "telemetry", pingMs: 23, framesPerSecond: 60,
            bitrateMbps: 24.5, jitterMs: 0.4, packetLossPercent: 0, decodeTimeMs: null, latencyMs: null})
        ShellStore.activeSession = {regionName: "SYNTHETIC FIXTURE · NOT LIVE",
            negotiatedStreamProfile: {codec: "h264", width: 1920, height: 1080}}
        checkMetric(stats, "Ping", qsTr("PING"), 23)
        if (Qt.application.arguments.indexOf("--screenshot") >= 0) {
            stats.anchors.fill = parent
            stats.z = 10000
            stats.visible = true
        } else {
            stats.destroy()
        }
        return true
    }
}

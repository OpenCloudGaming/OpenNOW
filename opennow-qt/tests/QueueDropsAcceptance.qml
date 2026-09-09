import QtQuick
import OpenNOW

QtObject {
    property QtObject runtime: QtObject {
        property bool running: true
        property string lastError: ""
        signal presentationError(string message)
        signal responseReceived(var response)
        signal eventReceived(var event)
        signal callbacksDropped(int count)
        function start() { return true }
        function send(command) { return true }
    }
    property Component statsComponent: Component {
        DesktopStreamStats { anchors.fill: parent; expanded: true }
    }
    property Component reportComponent: Component {
        SessionReportOverlay {}
    }
    property QtObject client: QtObject {
        property string state: "stopped"
        property string lastError: ""
        property var calls: []
        signal responseReceived(string requestId, var result)
        signal requestFailed(string requestId, string code, string message)
        signal eventReceived(string name, var payload)
        function logShellDiagnostic(message) {}
        function request(method, params, timeout) {
            const id = "drop-fixture-" + (calls.length + 1)
            calls = calls.concat([{id: id, method: method, params: params}])
            return id
        }
        function cancel(id) { return true }
    }

    function check(ok, message) {
        if (!ok) throw new Error("Queue drop acceptance: " + message)
    }

    function drop(unit, count, extra) {
        ShellStore.acceptNativeEvent(Object.assign({type: "log", event: "queue-dropped",
            unit: unit, count: count}, extra || {}))
    }

    function run(parent) {
        ShellStore.streamerStartRequestId = "fixture-blocked"
        ShellStore.activeSession = {sessionId: "drop-fixture", zone: "Fixture region"}
        ShellStore.streamer = {status: "streaming", framesPerSecond: 60, queueDropCount: 0}
        drop("frames", 7)
        drop("samples", 4800, {sampleRate: 48000, channels: 2})
        drop("packets", 2)
        runtime.callbacksDropped(3)
        drop("items", 4)
        check(ShellStore.streamDropCounts.videoDropCount === 7, "video counts only frames")
        check(ShellStore.streamDropCounts.audioDiscardedMs === 50, "4800 stereo samples are 50 ms")
        check(ShellStore.streamDropCounts.audioPacketDropCount === 2, "unknown audio duration stays in packets")
        check(ShellStore.streamDropCounts.callbackDropCount === 3, "callbacks are separate")
        check(ShellStore.streamDropCounts.otherQueueDropCount === 4, "unknown sources stay visible")
        drop("frames", -5)
        drop("frames", 1.5)
        drop("frames", Infinity)
        drop("frames", NaN)
        check(ShellStore.streamDropCounts.videoDropCount === 7, "invalid counters do not poison totals")
        drop("samples", 5, {sampleRate: 0, channels: 2})
        check(ShellStore.streamDropCounts.audioDiscardedMs === 50, "invalid audio format is not guessed")
        check(ShellStore.streamDropCounts.otherQueueDropCount === 9, "invalid format remains unclassified")
        ShellStore.activeSession = {sessionId: "drop-fixture", zone: "Fixture region"}
        ShellStore.streamer = {status: "streaming", framesPerSecond: 60}
        check(ShellStore.streamDropCounts.videoDropCount === 7, "same-session reconnect preserves totals")
        ShellStore.streamStartedAtMs = Date.now() - 1280000
        ShellStore.stopStreamingSession()
        check(ShellStore.lastSessionReport.drops.audioDiscardedMs === 50, "report captures typed counters")
        drop("frames", 1)
        check(ShellStore.lastSessionReport.drops.videoDropCount === 8, "shutdown flush updates retained report")
        ShellStore.activeSession = null
        ShellStore.streamer = {status: "stopped"}
        check(ShellStore.streamDropCounts.videoDropCount === 8, "stopping retains diagnostics")
        client.state = "ready"
        ShellStore.exportDiagnostics()
        const exported = client.calls[client.calls.length - 1]
        check(exported.method === "diagnostics.export", "export uses the diagnostics boundary")
        check(exported.params.embeddedStream.drops.videoDropCount === 8, "export includes Qt-owned counters")
        check(exported.params.lastSessionReport.drops.audioDiscardedMs === 50, "export retains completed session")
        client.state = "stopped"
        ShellStore.activeSession = {sessionId: "next-fixture", zone: "Fixture region"}
        check(ShellStore.streamDropCounts.videoDropCount === 0, "new session starts at zero")
        drop("frames", 2)
        check(ShellStore.lastSessionReport.drops.videoDropCount === 8, "new session cannot change previous report")
        ShellStore.settings = {statsShowDrops: true, statsShowGraphs: false}
        ShellStore.streamer = {status: "streaming", framesPerSecond: 60}
        drop("samples", 4800, {sampleRate: 48000, channels: 2})
        const stats = statsComponent.createObject(parent)
        check(stats !== null, "stats load")
        check(stats.cards.some(card => card.field === "videoDropCount" && card.value === 2), "video card uses typed total")
        check(stats.cards.some(card => card.field === "audioDiscardedMs" && card.value === 50), "audio card uses milliseconds")
        check(stats.cards.every(card => card.field !== "queueDropCount"), "mixed-unit total is not displayed")
        ShellStore.settings = {statsShowDrops: false}
        check(stats.cards.every(card => card.key !== "Drops"), "existing visibility preference covers all drop cards")
        ShellStore.settings = {statsShowDrops: true, statsShowGraphs: false}
        if (Qt.application.arguments.indexOf("--smoke-queue-report") >= 0) {
            stats.destroy()
            const report = reportComponent.createObject(parent)
            check(report !== null, "session report loads")
            check(report.dropValue("videoDropCount", "frames", 0) === "8 frames", "report shows retained frame count")
            check(report.dropValue("audioDiscardedMs", "ms", 1) === "50.0 ms", "report shows retained audio duration")
        }
        return true
    }
}

import QtQuick
import OpenNOW

QtObject {
    property Component statsComponent: Component {
        DesktopStreamStats { width: 1280; height: 720 }
    }
    property QtObject client: QtObject {
        property string state: "stopped"
        property string lastError: ""
        property var calls: []
        signal responseReceived(string requestId, var result)
        signal requestFailed(string requestId, string code, string message)
        signal eventReceived(string name, var payload)
        function markUiReady() {}
        function logShellDiagnostic(message) {} // No filesystem writes from the isolated mock.
        function request(method, params, timeout) {
            const id = "fixture-" + (calls.length + 1)
            calls = calls.concat([{id:id, method:method, params:params}])
            return id
        }
        function cancel(id) { return true }
    }
    function check(ok, message) { if (!ok) throw new Error("Stream recovery: " + message) }
    function run(parent) {
        client.state = "stopped"
        ShellStore.streamerStartRequestId = "fixture-blocked"
        ShellStore.streamInputPauseRequestId = "fixture-blocked"
        // This is another reply for the same seat, not the first session.
        ShellStore.activeSession = {sessionId:"fixture", phase:"ready", status:2}
        ShellStore.streamerRestartAttempts = 2
        ShellStore.sessionReconnectAttempts = 2
        ShellStore.acceptStreamingSession({sessionId:"fixture", phase:"ready", status:2})
        check(ShellStore.sessionReconnectAttempts === 2, "seat claim must not reset video retries")
        ShellStore.acceptStreamerSnapshot({sessionId:"fixture", status:"streaming"})
        check(ShellStore.streamerRestartAttempts === 2, "transport startup must not reset video retries")
        client.state = "ready"
        ShellStore.sessionReconnectAttempts = ShellStore.maximumSessionReconnectAttempts
        for (let i = 0; i < 20; ++i)
            ShellStore.acceptStreamerSnapshot({sessionId:"fixture", status:"error", message:"decoder failed"})
        check(ShellStore.streamState === "error", "exhausted recovery must stop")
        check(ShellStore.sessionClaimRequestId === "", "no claims after budget exhaustion")
        check(ShellStore.streamMessage === "decoder failed", "retain the failure message")
        ShellStore.streamerRestartTimer.stop()
        ShellStore.streamerRecoveryExhausted = false
        ShellStore.sessionReconnectAttempts = 0
        ShellStore.streamer = {status:"stopped"}
        ShellStore.streamerStartRequestId = ""
        ShellStore.streamerPrepareRequestId = ""
        ShellStore.recoverStreamingSession("connection lost")
        check(client.calls[client.calls.length - 1].method === "session.remote.list", "discover before resume")
        let id = ShellStore.recoveryDiscoveryRequestId
        client.responseReceived(id, {sessions:[{sessionId:"unrelated"}]})
        check(ShellStore.sessionClaimRequestId === "", "never resume another game")
        ShellStore.streamerRestartTimer.stop()
        ShellStore.recoverStreamingSession("retry")
        id = ShellStore.recoveryDiscoveryRequestId
        client.responseReceived(id, {sessions:[{sessionId:"fixture", streamingBaseUrl:"https://example.invalid"}]})
        check(client.calls[client.calls.length - 1].method === "session.claim", "claim the original session")
        id = ShellStore.sessionClaimRequestId
        client.responseReceived(id, {session:{sessionId:"fixture", status:3, phase:"resuming", resumePending:true}})
        check(ShellStore.streamerPrepareRequestId === "", "resume acknowledgement is not readiness")
        ShellStore.streamPollTimer.stop()
        ShellStore.pollStreamingSession()
        check(client.calls[client.calls.length - 1].method === "session.poll", "poll the resumed seat")
        id = ShellStore.streamPollRequestId
        client.responseReceived(id, {session:{sessionId:"fixture", status:6, phase:"resuming", resumePending:true}})
        check(ShellStore.streamerPrepareRequestId === "", "wait through transient cleanup")
        ShellStore.streamPollTimer.stop()
        ShellStore.pollStreamingSession()
        id = ShellStore.streamPollRequestId
        client.requestFailed(id, "network_error", "connection still offline")
        check(ShellStore.streamerPrepareRequestId === "", "network failure must not start native media")
        check(ShellStore.streamPollTimer.running, "retry transient resume poll failure")
        ShellStore.streamPollTimer.stop()
        ShellStore.nativeRuntimeReady = true
        ShellStore.pollStreamingSession()
        id = ShellStore.streamPollRequestId
        client.responseReceived(id, {session:{sessionId:"fixture", status:2, phase:"ready", resumePending:false}})
        check(client.calls.some(call => call.method === "streamer.prepare"), "prepare only after ready poll")
        ShellStore.cancelSessionRecovery()
        ShellStore.streamer = {status:"stopped"}
        ShellStore.recoverStreamingSession("cancel fixture")
        id = ShellStore.recoveryDiscoveryRequestId
        ShellStore.cancelSessionRecovery()
        const callsBeforeLateDiscovery = client.calls.length
        client.responseReceived(id, {sessions:[{sessionId:"fixture"}]})
        check(client.calls.length === callsBeforeLateDiscovery, "cancelled discovery must not resume")
        ShellStore.sessionRecoveryPending = true
        ShellStore.streamerStopRequestId = "fixture-stalled-stop"
        ShellStore.recoveryStopTimer.triggered()
        check(ShellStore.streamState === "error" && !ShellStore.sessionRecoveryPending,
              "stalled native cleanup must not wait forever")
        check(ShellStore.streamerStopRequestId === "fixture-stalled-stop",
              "stalled cleanup must retain native resource ownership")
        ShellStore.streamerStopRequestId = ""
        client.state = "stopped"
        ShellStore.streamer = {sessionId:"fixture", status:"streaming"}
        ShellStore.acceptNativeEvent({type:"status", event:"first-frame", status:"streaming", backend:"fixture"})
        check(ShellStore.streamerRestartAttempts === 0 && ShellStore.sessionReconnectAttempts === 0,
              "video recovery must restore retry budgets")
        ShellStore.settings = ({})
        ShellStore.activeSession = {zone:"EU-Southeast", serverLocation:null}
        const stats = statsComponent.createObject(parent)
        for (const expanded of [false, true]) {
            stats.expanded = expanded
            stats.pointerLocked = true
            check(!stats.enabled, "locked stats must not receive pointer input")
            stats.pointerLocked = false
            check(stats.enabled, "unlocked stats remain interactive")
        }
        check(stats.region === "EU-Southeast", "show the actual session zone")
        ShellStore.activeSession = {zone:"EU-Southeast", serverLocation:"SOF"}
        check(stats.region === "SOF", "prefer server-assigned location")
        ShellStore.acceptNativeEvent({type:"telemetry", jitterMs:1.5, packetLossPercent:0.25,
            pingMs:null, decodeTimeMs:null, latencyMs:null})
        check(stats.read("jitterMs") === 1.5 && stats.read("packetLossPercent") === 0.25,
              "native measurements reach stats")
        check(stats.read("pingMs") === null && stats.read("decodeTimeMs") === null,
              "missing measurements must not become zero")
        check(stats.compactMetrics.length === stats.cards.length + 2,
              "compact stats must include every enabled metric plus video and region")
        ShellStore.settings = {statsShowPacketLoss:false}
        check(stats.cards.every(card => card.key !== "PacketLoss"), "honor hidden metrics")
        stats.destroy()
        ShellStore.activeSession = null
        ShellStore.streamer = null
        return true
    }
}

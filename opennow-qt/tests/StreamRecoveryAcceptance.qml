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
    function checkQueuedPollRetries() {
        client.state = "ready"
        const queued = {sessionId:"queue-fixture",status:1,phase:"queued",queuePosition:21,
            connectionInfo:null,resourcePath:null}
        ShellStore.streamer = {status:"stopped"}
        ShellStore.acceptStreamingSession(queued)
        ShellStore.sessionReconnectAttempts = 2
        for (let cycle = 0; cycle < 3; ++cycle) {
            for (let failure = 1; failure <= 4; ++failure) {
                ShellStore.streamPollTimer.stop()
                ShellStore.pollStreamingSession()
                const id = ShellStore.streamPollRequestId
                check(id !== "" && client.calls[client.calls.length - 1].method === "session.poll",
                    "queued seat remains pollable")
                client.requestFailed(id, "network_error", "intermittent network failure")
                check(ShellStore.streamPollFailureAttempts === failure
                    && ShellStore.streamState === "reconnecting" && ShellStore.streamPollTimer.running,
                    "intermittent failures retry without ending the queue")
            }
            ShellStore.streamPollTimer.stop()
            ShellStore.pollStreamingSession()
            client.responseReceived(ShellStore.streamPollRequestId, {session:queued})
            check(ShellStore.streamPollFailureAttempts === 0 && ShellStore.streamState === "queued"
                && ShellStore.activeSession.queuePosition === 21 && ShellStore.streamPollTimer.running,
                "successful queue poll resets only the consecutive failure budget")
            check(ShellStore.sessionReconnectAttempts === 2,
                "successful queue poll does not reset native video recovery")
        }
        for (let failure = 1; failure <= ShellStore.maximumStreamPollFailureAttempts + 1; ++failure) {
            ShellStore.streamPollTimer.stop()
            ShellStore.pollStreamingSession()
            client.requestFailed(ShellStore.streamPollRequestId, "network_error", "connection unavailable")
            check(ShellStore.streamState === (failure <= ShellStore.maximumStreamPollFailureAttempts
                    ? "reconnecting" : "error"), "consecutive poll failures remain bounded")
        }
        check(!ShellStore.streamPollTimer.running && ShellStore.sessionReconnectAttempts === 2,
            "exhausted poll retries stop polling without consuming video retries")
        ShellStore.acceptStreamingSession({sessionId:"new-queue",status:1,phase:"queued",queuePosition:10})
        check(ShellStore.streamPollFailureAttempts === 0, "a replacement seat starts with a fresh poll budget")
        ShellStore.streamPollTimer.stop()
    }
    function checkOwnedTerminations() {
        const owner = {generation:7,userId:"account-a",providerIdpId:"provider-a"}
        ShellStore.authGeneration = 9
        ShellStore.authSession = {user:{userId:"account-a",displayName:"Account A"},provider:{idpId:"provider-a"}}
        ShellStore.streamer = {status:"stopped"}
        ShellStore.activeSession = {sessionId:"fixture",status:3,ownerScope:owner,marker:"original"}
        for (const path of ["response", "event"]) {
            for (const payload of [
                {scope:owner,session:{sessionId:"fixture",status:1,marker:"stale"}},
                {scope:owner,session:null,termination:{source:"untrusted",status:7,sessionId:"fixture",resumable:false}},
                {scope:owner,session:null,termination:{source:"cloudmatch-http",httpStatus:503,sessionId:"fixture",resumable:false}},
                {scope:owner,session:null,termination:{source:"cloudmatch-http",httpStatus:404,sessionId:"fixture",resumable:true}},
                {scope:owner,session:null,termination:{source:"cloudmatch-http",httpStatus:404,sessionId:"other-seat",resumable:false}},
                {scope:{generation:7,userId:"account-b",providerIdpId:"provider-a"},session:null,
                    termination:{source:"cloudmatch-http",httpStatus:404,sessionId:"fixture",resumable:false}},
                {scope:{generation:7,userId:"account-a",providerIdpId:"provider-b"},session:null,
                    termination:{source:"cloudmatch-http",httpStatus:404,sessionId:"fixture",resumable:false}}
            ]) {
                ShellStore.streamPollRequestId = "terminal-fixture"
                if (path === "response") client.responseReceived("terminal-fixture", payload)
                else client.eventReceived("session.changed", payload)
                check(ShellStore.activeSession && ShellStore.activeSession.marker === "original",
                      path + " rejects stale ordinary, foreign-owner, foreign-seat, and non-authoritative terminal results")
            }
        }
        for (const selected of ["account-a", "account-b"]) {
            ShellStore.authSession = {user:{userId:selected,displayName:selected},provider:{idpId:"provider-a"}}
            for (const firstPath of ["response", "event"]) {
                for (const payload of [
                    {scope:owner,session:{sessionId:"fixture",status:7}},
                    {scope:owner,session:null,termination:{source:"cloudmatch-session-status",status:7,sessionId:"fixture",resumable:false}},
                    {scope:owner,session:null,termination:{source:"cloudmatch-http",httpStatus:404,sessionId:"fixture",resumable:false}}
                ]) {
                    ShellStore.activeSession = {sessionId:"fixture",status:3,ownerScope:owner}
                    ShellStore.streamState = "ready"
                    ShellStore.streamPollRequestId = "terminal-fixture"
                    ShellStore.streamerPrepareRequestId = "preparing-fixture"
                    ShellStore.sessionRecoveryPending = true
                    if (firstPath === "response") client.responseReceived("terminal-fixture", payload)
                    else client.eventReceived("session.changed", payload)
                    check(!ShellStore.activeSession && ShellStore.streamState === "idle"
                          && ShellStore.streamPollRequestId === "" && ShellStore.streamerPrepareRequestId === ""
                          && !ShellStore.sessionRecoveryPending && !ShellStore.streamPollTimer.running,
                          firstPath + " authoritatively ends the exact owned seat despite a newer auth generation")
                    ShellStore.activeSession = {sessionId:"replacement",status:3,
                        ownerScope:{generation:9,userId:"account-a",providerIdpId:"provider-a"}}
                    if (firstPath === "response") client.eventReceived("session.changed", payload)
                    else client.responseReceived("terminal-fixture", payload)
                    check(ShellStore.activeSession && ShellStore.activeSession.sessionId === "replacement",
                          "duplicate terminal delivery cannot end a replacement seat")
                }
            }
        }
        ShellStore.authSession = null
        ShellStore.authGeneration = 0
    }
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
        const recoveryProbe = client.calls[client.calls.length - 1]
        check(recoveryProbe.method === "session.poll", "probe the exact seat before resume")
        check(recoveryProbe.params.sessionId === "fixture" && recoveryProbe.params.recoveryMode === true,
              "recovery probe retains the original seat identity")
        let id = ShellStore.recoveryDiscoveryRequestId
        client.responseReceived(id, {session:{sessionId:"unrelated"}})
        check(ShellStore.sessionClaimRequestId === "", "never resume another game")
        ShellStore.streamerRestartTimer.stop()
        ShellStore.recoverStreamingSession("retry")
        id = ShellStore.recoveryDiscoveryRequestId
        client.responseReceived(id, {session:{sessionId:"fixture", streamingBaseUrl:"https://example.invalid"}})
        check(client.calls[client.calls.length - 1].method === "session.claim", "claim the original session")
        check(client.calls[client.calls.length - 1].params.sessionId === "fixture", "claim only the probed seat")
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
        client.responseReceived(id, {session:{sessionId:"fixture"}})
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
        checkQueuedPollRetries()
        checkOwnedTerminations()
        ShellStore.activeSession = null
        ShellStore.streamer = null
        return true
    }
}

import QtQuick
import OpenNOW

QtObject {
    id: fixture
    property var window
    property var surface
    property int step: 0
    property var recording
    property bool interactive: false
    property bool stopFailure: false
    property bool postEndClose: false
    property int originalVisibility: 0
    property QtObject runtime: QtObject {
        property bool running: true
        property string lastError: ""
        property var commands: []
        signal presentationError(string message, string errorCode)
        signal responseReceived(var response)
        signal eventReceived(var event)
        signal sessionTimerReceived(int timerType, double secondsLeft, bool active)
        signal callbacksDropped(int count)
        function start() { return true }
        function send(command) { commands = commands.concat([command]); return true }
    }
    property QtObject client: QtObject {
        property string state: "ready"
        property bool allocationsSettled: true
        property string lastError: ""
        property var calls: []
        signal responseReceived(string requestId, var result)
        signal requestFailed(string requestId, string code, string message)
        signal eventReceived(string name, var payload)
        function markUiReady() {}
        function logShellDiagnostic(message) {}
        function request(method, params, timeout) {
            const id = "quit-fixture-" + (calls.length + 1)
            calls = calls.concat([{id: id, method: method, params: params}])
            return id
        }
        function cancel(id) { return true }
    }
    function check(ok, message) {
        if (!ok) {
            console.error("Session quit: " + message)
            Qt.exit(1)
            throw new Error(message)
        }
    }
    function find(item, name) {
        if (item.objectName === name) return item
        for (const child of item.children || []) {
            const found = find(child, name)
            if (found) return found
        }
        return null
    }
    function run(parent) {
        window = parent
        originalVisibility = window.visibility
        if (postEndClose) {
            checkPostEndClose()
            return
        }
        surface = find(parent.contentItem, "streamSurfaceHost")
        ShellStore.settings = {confirmQuitWhenIdle: false, showSessionReport: false}
        ShellStore.streamerStartRequestId = "fixture-blocked"
        ShellStore.streamInputPauseRequestId = "fixture-blocked"
        ShellStore.pendingLaunchParams = null
        ShellStore.activeSession = {sessionId: "quit-session", phase: "ready", status: 2}
        ShellStore.streamState = "streaming"
        ShellStore.streamer = {status: "streaming"}
        ShellStore.streamerStopExpected = false
        ShellStore.toggleStreamRecording()
        client.responseReceived(ShellStore.mediaRecordingTargetRequestId, {path: "/quit-fixture.mkv"})
        recording = runtime.commands[runtime.commands.length - 1]
        check(recording.type === "recording-start", "recording was not started")
        ShellStore.acceptNativeResponse({id: recording.id, type: "recording-started"})
        ShellStore.streamerStartRequestId = ""
        client.allocationsSettled = false
        steps.start()
        console.info("SESSION_QUIT_READY")
    }
    function checkCloseGuard(message) {
        const visibility = window.visibility
        check(!window.close() && window.visible && window.visibility === visibility
            && AppController.overlay === "application-quit-confirm", message)
        AppController.showOverlay("")
    }
    function checkPostEndClose() {
        const owner = {generation:7, userId:"quit-owner", providerIdpId:"quit-provider"}
        ShellStore.authSession = {user:{userId:owner.userId, displayName:"Quit fixture"}, provider:{idpId:owner.providerIdpId}}
        ShellStore.authGeneration = owner.generation
        ShellStore.settings = {confirmQuitWhenIdle:false, showSessionReport:false}
        ShellStore.activeSessionRequestId = ""
        ShellStore.streamInputPauseRequestId = "fixture-blocked"
        ShellStore.pendingLaunchParams = {appId:"123", title:"Quit fixture"}
        ShellStore.streamCreateRequestId = "create-failed"
        client.requestFailed("create-failed", "network_error", "Create failed")
        check(ShellStore.pendingLaunchParams !== null, "a failed create must retain its retry intent")
        checkCloseGuard("an unresolved launch must still ask")
        for (const ending of ["native", "status-response", "status-event", "http", "stop"]) {
            const seat = {sessionId:"quit-" + ending, status:1, phase:"queued", ownerScope:owner}
            ShellStore.pendingLaunchParams = {appId:"123", title:"Quit fixture"}
            ShellStore.streamCreateRequestId = "create-" + ending
            client.responseReceived(ShellStore.streamCreateRequestId, {scope:owner, session:seat})
            checkCloseGuard(ending + ": a created queued session must still ask")
            ShellStore.streamPollTimer.stop()
            ShellStore.activeSession = Object.assign({}, seat, {status:3, phase:"streaming"})
            ShellStore.streamer = {status:"stopped"}
            checkCloseGuard("disconnected media with a live cloud session must still ask")
            ShellStore.streamer = {sessionId:seat.sessionId, status:"streaming"}
            ShellStore.streamState = "streaming"
            ShellStore.streamerStopExpected = false
            checkCloseGuard(ending + ": a live stream must still ask")
            const termination = ending === "native"
                ? {source:"nvst-server", code:"server-terminated", sessionId:seat.sessionId, resumable:false}
                : {source:"cloudmatch-http", httpStatus:404, sessionId:seat.sessionId, resumable:false}
            if (ending === "stop") {
                ShellStore.stopStreamingSession()
                checkCloseGuard("a pending cloud stop must still ask")
                client.requestFailed(ShellStore.streamStopRequestId, "network_error", "Stop failed")
                checkCloseGuard("a failed cloud stop must retain the live session guard")
                ShellStore.stopStreamingSession()
                client.responseReceived(ShellStore.streamStopRequestId, {session:null, sessionId:seat.sessionId})
            } else if (ending === "native") {
                runtime.eventReceived({type:"error", code:"server-terminated", termination:termination})
                checkCloseGuard("native termination must wait for core cleanup")
                client.responseReceived(ShellStore.sessionEndRequestId, {sessionId:seat.sessionId, cleared:true})
            } else if (ending === "status-event") {
                client.eventReceived("session.changed", {scope:owner, session:Object.assign({}, seat, {status:7})})
            } else {
                ShellStore.streamPollRequestId = "poll-" + ending
                client.responseReceived(ShellStore.streamPollRequestId, ending === "http"
                    ? {scope:owner, session:null, termination:termination}
                    : {scope:owner, session:Object.assign({}, seat, {status:7})})
            }
            check(ShellStore.activeSession === null, ending + ": the cloud session must be terminal")
            checkCloseGuard(ending + ": native teardown must still ask")
            runtime.responseReceived({id:ShellStore.streamerStopRequestId, type:"ok"})
            check(!ShellStore.applicationQuitNeedsConfirmation,
                ending + ": completed session must allow idle opt-out close; pending launch="
                    + JSON.stringify(ShellStore.pendingLaunchParams))
            ShellStore.remoteSessions = [{sessionId:"other-live-seat", status:3}]
            checkCloseGuard("a separately resumable cloud session must still ask")
            ShellStore.remoteSessions = []
            ShellStore.settings = {confirmQuitWhenIdle:true, showSessionReport:false}
            checkCloseGuard("enabled idle confirmation must still ask after ending")
            ShellStore.settings = {confirmQuitWhenIdle:false, showSessionReport:false}
            check(window.visibility === originalVisibility && window.close()
                && !window.visible && AppController.overlay === "",
                ending + ": post-end native close must honor idle opt-out without changing window mode")
            if (ending !== "stop") window.visibility = originalVisibility
        }
        console.info("SESSION_POST_END_CLOSE_PASS")
        Qt.callLater(() => Qt.exit(0))
    }
    function finishRecording() {
        ShellStore.acceptNativeResponse({id: ShellStore.streamerStopRequestId, type: "ok",
            recording: {requestId: recording.id, state: "saved", path: recording.outputPath,
                videoPackets: 3, audioPackets: 2, completion: {kind: "complete"}}})
        check(!ShellStore.streamRecordingActive && (!ShellStore.streamRecordingAttempt
            || ShellStore.streamRecordingAttempt.completed), "recording did not finalize")
    }
    property Timer steps: Timer {
        interval: 150
        repeat: true
        onTriggered: {
            const stopCalls = client.calls.filter(call => call.method === "session.stop")
            check(window.visible, "window closed before graceful completion")
            if (fixture.step < 3 && !window.sessionQuitStopping) {
                check(window.visibility === fixture.originalVisibility, "request or cancel changed window mode")
                check(stopCalls.length === 0 && ShellStore.streamRecordingActive,
                    "request or cancel stopped the cloud session or recording")
                if (surface) check(find(window.contentItem, "streamSurfaceHost") === surface, "video surface was replaced")
            }
            if (fixture.interactive && fixture.step < 3) {
                if (fixture.step === 0) {
                    if (window.sessionQuitPending) fixture.step = 1
                } else if (fixture.step === 1) {
                    if (!window.sessionQuitPending) {
                        check(AppController.overlay === "", "cancel did not dismiss confirmation")
                        fixture.step = 2
                        console.info("SESSION_QUIT_CANCELLED")
                    }
                } else if (window.sessionQuitStopping) fixture.step = 3
                return
            }
            if (fixture.step === 0 || fixture.step === 2) {
                if (!window.sessionQuitPending) return
                check(AppController.overlay === "application-quit-confirm", "Qt confirmation is missing")
                const confirmation = find(window.contentItem, "applicationQuitConfirmation")
                check(confirmation !== null, "confirmation is missing")
                if (fixture.step === 0) {
                    if (window.desktopSurfaceActive) confirmation.cancelRequested()
                    else confirmation.safeRequested()
                    check(!window.sessionQuitPending && AppController.overlay === "", "cancel was not retained")
                    fixture.step = 2
                } else {
                    confirmation.confirmRequested()
                    fixture.step = 3
                }
            } else if (fixture.step === 3) {
                check(window.sessionQuitStopping && stopCalls.length === 1, "confirmation did not stop the cloud session")
                check(ShellStore.streamerStopRequestId !== "", "native stop was not requested")
                if (fixture.stopFailure) {
                    client.requestFailed(stopCalls[0].id, "fixture_stop_failed", "The cloud session could not be stopped")
                    fixture.finishRecording()
                    client.allocationsSettled = true
                    fixture.step = 5
                    console.info("SESSION_QUIT_STOP_FAILED")
                } else {
                    client.responseReceived(stopCalls[0].id, {session: null, sessionId: "quit-session"})
                    fixture.step = 4
                }
            } else if (fixture.step === 4) {
                check(window.sessionQuitStopping && !ShellStore.sessionWorkIdle,
                    "cloud stop bypassed recording finalization")
                fixture.finishRecording()
                fixture.step = 6
            } else if (fixture.step === 6) {
                check(ShellStore.sessionWorkIdle && !window.applicationCloseConfirmed,
                    "recorder completion bypassed an earlier cancelled allocation")
                client.allocationsSettled = true
                console.info("SESSION_QUIT_FINALIZED")
                stop()
            } else {
                check(ShellStore.activeSession !== null && !ShellStore.sessionWorkIdle
                    && !window.applicationCloseConfirmed, "failed cloud stop authorized application closure")
                if (!window.sessionQuitPending) {
                    console.info("SESSION_QUIT_FAILURE_RETAINED")
                    stop()
                    Qt.exit(0)
                }
            }
        }
    }
}

import QtQuick
import OpenNOW

QtObject {
    property var unlockSurface: null
    property var unlockDesktop: null
    property int unlockCommandCount: 0
    property QtObject runtime: QtObject {
        property bool running: false
        property string lastError: ""
        property var commands: []
        signal presentationError(string message)
        signal responseReceived(var response)
        signal eventReceived(var event)
        signal callbacksDropped(int count)
        function start() { return true }
        function send(command) { commands = commands.concat([command]); return true }
    }
    property Component statsComponent: Component {
        DesktopStreamStats { width: 1280; height: 720 }
    }
    property QtObject client: QtObject {
        property string state: "stopped"
        property string lastError: ""
        property var calls: []
        property var cancelled: []
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
        function cancel(id) {
            cancelled = cancelled.concat([id])
            requestFailed(id, "cancelled", "Cancelled")
            return true
        }
    }
    function check(ok, message) { if (!ok) throw new Error("Stream recovery: " + message) }
    function find(item, name) {
        if (item.objectName === name) return item
        for (const child of item.children || []) {
            const result = find(child, name)
            if (result) return result
        }
        return null
    }
    function beginLiveProfileUnlock(parent) {
        ShellStore.cancelSessionRecovery()
        ShellStore.activeSession = null
        AppController.navigate("home")
        const owner = {generation:7,userId:"unlock-owner",providerIdpId:"unlock-provider"}
        ShellStore.authSession = {user:{userId:owner.userId,displayName:"Locked player"},provider:{idpId:owner.providerIdpId}}
        ShellStore.authGeneration = 7
        ShellStore.authRestorePending = false
        ShellStore.addingAccount = false
        ShellStore.authState = "signed-in"
        ShellStore.activeSession = {sessionId:"unlock-seat",status:3,phase:"streaming",ownerScope:owner}
        ShellStore.streamer = {sessionId:"unlock-seat",status:"streaming",firstFrameLatencyMs:1,marker:"surviving-native"}
        ShellStore.streamState = "streaming"
        ShellStore.streamerStopRequestId = ""
        ShellStore.streamerStartRequestId = ""
        ShellStore.streamerPrepareRequestId = ""
        ShellStore.streamStopRequestId = ""
        ShellStore.sessionStopIntentId = ""
        ShellStore.streamerStopExpected = false
        ShellStore.streamerRecoveryExhausted = false
        ShellStore.coreSessionRestoreId = "unlock-seat"
        ShellStore.savedAccounts = [{userId:owner.userId,displayName:"Locked player",hasPin:true}]
        runtime.running = true
        AppController.navigate("stream")
        unlockDesktop = find(parent, "desktopApp")
        unlockSurface = find(parent, "streamSurfaceHost")
        check(unlockDesktop && unlockSurface, "desktop stream exists before explicit profile unlock")
        unlockCommandCount = runtime.commands.length
        ShellStore.authSession = null
        ShellStore.authState = "idle"
        const entry = find(parent, "savedAccountsButton")
        check(entry && entry.visible && entry.width > 0 && entry.height > 0,
            "saved profiles remain reachable when a protected owner cannot auto-restore")
        const entryPosition = entry.mapToItem(unlockDesktop, 0, 0)
        check(entryPosition.x >= 0 && entryPosition.y >= 0
            && entryPosition.x + entry.width <= unlockDesktop.width
            && entryPosition.y + entry.height <= unlockDesktop.height,
            "saved profile entry is on screen without scrolling past provider sign-in")
        entry.clicked()
        check(AppController.route === "accounts" && find(parent, "desktopApp") === unlockDesktop
            && find(parent, "streamSurfaceHost") === unlockSurface,
            "explicit account navigation retains the desktop and native video item")
        const loader = find(parent, "desktopProfileLoader")
        const viewport = find(parent, "desktopProfileViewport")
        check(loader && loader.item && viewport && viewport.scale > 0
            && Math.abs(viewport.width * viewport.scale - unlockDesktop.width) < 1
            && Math.abs(viewport.height * viewport.scale - unlockDesktop.height) < 1,
            "existing account screens scale to the locked desktop stream window")
        check(!unlockDesktop.signInVisible && !unlockDesktop.shellVisible && !unlockSurface.inputEnabled
            && !unlockSurface.captureActive && (ControllerInput.shellCaptureEnabled || ControllerInput.inputSuspended),
            "account selection owns keyboard, pointer, and controller input instead of gameplay")
        ShellStore.openPin("unlock", ShellStore.savedAccounts[0])
        check(AppController.route === "profile-pin" && loader.item && loader.item.heading.indexOf("Locked player") >= 0
            && find(parent, "streamSurfaceHost") === unlockSurface,
            "the existing PIN screen replaces only the profile loader")
        return true
    }
    function finishLiveProfileUnlock(parent) {
        ShellStore.submitPin("1234")
        check(ShellStore.accountSwitchRequestId !== "", "PIN submission uses the existing account switch request")
        const owner = {generation:8,userId:"unlock-owner",providerIdpId:"unlock-provider"}
        client.responseReceived(ShellStore.accountSwitchRequestId, {generation:8,
            session:{user:{userId:owner.userId,displayName:"Locked player"},provider:{idpId:owner.providerIdpId}}})
        check(AppController.route === "stream" && find(parent, "desktopApp") === unlockDesktop
            && find(parent, "streamSurfaceHost") === unlockSurface && unlockSurface.inputEnabled,
            "unlock returns to the same embedded stream rather than home or a new presenter")
        ShellStore.pollStreamingSession()
        client.responseReceived(ShellStore.streamPollRequestId,
            {scope:owner,session:{sessionId:"unlock-seat",status:3,phase:"streaming",ownerScope:owner}})
        check(ShellStore.streamer.marker === "surviving-native" && ShellStore.activeSession.sessionId === "unlock-seat"
            && runtime.commands.slice(unlockCommandCount).every(command => command.type !== "start" && command.type !== "stop"),
            "PIN unlock and exact-seat reconciliation preserve the native media connection")
        return true
    }
    function checkActiveTimerExhaustion() {
        ShellStore.streamPollRequestId = ""
        ShellStore.streamer = {status:"stopped"}
        ShellStore.acceptStreamingSession({sessionId:"active-timer-seat",status:1,phase:"queued"})
        const before = client.calls.length
        for (let tick = 0; tick < ShellStore.maximumStreamPollFailureAttempts + 4; ++tick) {
            ShellStore.streamPollTimer.triggered()
            const id = ShellStore.streamPollRequestId
            if (id !== "") client.requestFailed(id, "network_error", "offline")
        }
        check(!ShellStore.streamPollTimer.running && ShellStore.streamState === "error",
              "a running repeating timer stops when consecutive failures exhaust the budget")
        check(client.calls.length - before === ShellStore.maximumStreamPollFailureAttempts + 1,
              "late timer deliveries cannot send polls after exhaustion")
        ShellStore.acceptStreamingSession({sessionId:"replacement-timer-seat",status:1,phase:"queued"})
        ShellStore.streamPollTimer.triggered()
        check(ShellStore.streamPollRequestId !== "", "a replacement seat has a fresh poll budget")
        client.responseReceived(ShellStore.streamPollRequestId,
            {session:{sessionId:"replacement-timer-seat",status:1,phase:"queued"}})
        ShellStore.streamPollTimer.stop()
    }
    function checkCoreRestartPreservesSession() {
        client.state = "stopped"
        const oldOwner = {generation:7,userId:"restart-owner",providerIdpId:"restart-provider"}
        const auth = {user:{userId:"restart-owner",displayName:"Restart owner"},
            provider:{idpId:"restart-provider"}}
        ShellStore.authSession = auth
        ShellStore.authGeneration = 7
        ShellStore.activeSession = {sessionId:"restart-seat",status:3,phase:"streaming",
            ownerScope:oldOwner,keyboardLayout:"de-DE"}
        ShellStore.streamer = {sessionId:"restart-seat",status:"streaming",marker:"surviving-media"}
        ShellStore.streamState = "streaming"
        ShellStore.streamerPrepareRequestId = ""
        ShellStore.streamerStartRequestId = ""
        ShellStore.streamerStopRequestId = ""
        ShellStore.streamStopRequestId = ""
        ShellStore.streamPollRequestId = ""
        runtime.running = true
        runtime.commands = []
        client.state = "ready"
        if (ShellStore.activeSessionRequestId !== "")
            client.responseReceived(ShellStore.activeSessionRequestId, {session:null})
        check(ShellStore.activeSession && ShellStore.activeSession.sessionId === "restart-seat",
              "a fresh core cache cannot erase a surviving remote seat")
        check(runtime.commands.every(command => command.type !== "stop"),
              "core restart cannot stop healthy native media")
        client.responseReceived(ShellStore.authSessionRequestId,
            {session:auth,generation:1,persistence:"encrypted-file"})
        const request = client.calls.find(call => call.id === ShellStore.streamPollRequestId)
        check(request && request.method === "session.active.get"
            && request.params.sessionId === "restart-seat"
            && request.params.ownerScope.userId === "restart-owner"
            && request.params.ownerScope.providerIdpId === "restart-provider",
            "restore only the exact seat and original owner after authentication is ready")
        client.requestFailed(request.id, "network_error", "temporary discovery failure")
        check(ShellStore.activeSession.sessionId === "restart-seat" && ShellStore.streamPollTimer.running,
              "failed reconciliation retains the seat and retries through bounded polling")
        ShellStore.streamPollTimer.triggered()
        client.responseReceived(ShellStore.streamPollRequestId, {session:null})
        check(ShellStore.activeSession && ShellStore.activeSession.sessionId === "restart-seat"
            && ShellStore.streamPollTimer.running,
            "an unconfirmed null reconciliation cannot end the surviving seat")
        ShellStore.streamPollTimer.triggered()
        const restoredOwner = {generation:1,userId:"restart-owner",providerIdpId:"restart-provider"}
        client.responseReceived(ShellStore.streamPollRequestId, {scope:restoredOwner,
            session:{sessionId:"restart-seat",status:3,phase:"streaming",ownerScope:restoredOwner}})
        check(ShellStore.activeSession.ownerScope.generation === 1
            && ShellStore.streamer.marker === "surviving-media"
            && ShellStore.streamerPrepareRequestId === "" && !ShellStore.streamPollTimer.running,
            "restored control ownership preserves native media without preparing or claiming it again")
        check(runtime.commands.every(command => command.type !== "stop" && command.type !== "start"),
              "reconciliation sends no media lifecycle commands")
        check(ShellStore.activeSession.keyboardLayout === "de-DE",
              "read-only reconciliation preserves the live session's keyboard layout")
        ShellStore.acceptStreamingSession(Object.assign({}, ShellStore.activeSession, {keyboardLayout:null}))
        check(ShellStore.activeSession.keyboardLayout === "de-DE",
              "later core polls with a null layout preserve the launch-time layout")
        client.state = "stopped"
        client.state = "ready"
        client.responseReceived(ShellStore.authSessionRequestId,
            {session:auth,generation:2,persistence:"encrypted-file"})
        const staleId = ShellStore.streamPollRequestId
        ShellStore.streamer = {sessionId:"replacement-seat",status:"streaming",marker:"replacement-media"}
        ShellStore.acceptStreamingSession({sessionId:"replacement-seat",status:3,phase:"streaming",ownerScope:restoredOwner})
        client.responseReceived(staleId, {scope:restoredOwner,
            session:{sessionId:"restart-seat",status:3,ownerScope:restoredOwner}})
        check(client.cancelled.includes(staleId) && ShellStore.activeSession.sessionId === "replacement-seat"
            && ShellStore.streamer.marker === "replacement-media",
            "replacing a seat cancels reconciliation and ignores its late reply")
        client.state = "stopped"
        client.state = "ready"
        client.responseReceived(ShellStore.authSessionRequestId,
            {session:auth,generation:3,persistence:"encrypted-file"})
        const terminalOwner = {generation:3,userId:"restart-owner",providerIdpId:"restart-provider"}
        client.responseReceived(ShellStore.streamPollRequestId, {scope:terminalOwner,session:null,
            termination:{source:"cloudmatch-http",httpStatus:404,sessionId:"replacement-seat",resumable:false}})
        check(!ShellStore.activeSession && ShellStore.coreSessionRestoreId === ""
            && !ShellStore.streamPollTimer.running,
            "authoritative termination still ends the exact seat during reconciliation")
        runtime.running = false
        ShellStore.streamer = {status:"stopped"}
        ShellStore.acceptStreamingSession(null)
        ShellStore.authSession = null
        ShellStore.authGeneration = 0
    }
    function checkReconciliationAuthAndExit() {
        client.state = "stopped"
        const owner = {generation:7,userId:"restore-owner",providerIdpId:"restore-provider"}
        const auth = {user:{userId:"restore-owner",displayName:"Restore owner"},
            provider:{idpId:"restore-provider"}}
        ShellStore.activeSession = {sessionId:"restore-seat",status:3,phase:"streaming",ownerScope:owner}
        ShellStore.authSession = auth
        ShellStore.authGeneration = 7
        ShellStore.streamer = {sessionId:"restore-seat",status:"streaming"}
        ShellStore.streamerStopRequestId = ""
        ShellStore.streamerPrepareRequestId = ""
        ShellStore.streamerStartRequestId = ""
        ShellStore.streamerStopExpected = false
        runtime.running = true
        runtime.commands = []
        client.state = "ready"
        client.responseReceived(ShellStore.authSessionRequestId, {session:auth,generation:1})
        for (let attempt = 0; attempt <= ShellStore.maximumStreamPollFailureAttempts; ++attempt) {
            client.requestFailed(ShellStore.streamPollRequestId, "session_owner_mismatch", "Sign in to the owner account")
            if (attempt < ShellStore.maximumStreamPollFailureAttempts)
                ShellStore.streamPollTimer.triggered()
        }
        check(!ShellStore.streamPollTimer.running, "authentication failures remain bounded")
        const beforeUnchangedAuth = client.calls.length
        client.eventReceived("auth.session.changed", {session:auth,generation:1})
        ShellStore.pollStreamingSession()
        check(client.calls.length === beforeUnchangedAuth, "an unchanged authentication context cannot reset exhausted retries")
        client.eventReceived("auth.session.changed", {session:null,generation:2})
        client.eventReceived("auth.session.changed", {session:auth,generation:3})
        ShellStore.pollStreamingSession()
        check(ShellStore.streamPollRequestId !== "" && ShellStore.streamPollFailureAttempts === 0,
              "returning to the original account re-arms control reconciliation")
        const abandoned = ShellStore.streamPollRequestId
        ShellStore.stopStreamingSession()
        check(client.cancelled.includes(abandoned), "explicit exit retires the in-flight reconciliation request")
        check(!ShellStore.streamPollTimer.running && ShellStore.streamPollFailureAttempts === 0,
              "synchronous cancellation cannot restart polling or consume the failure budget")
        check(ShellStore.streamPollRequestId !== "", "explicit exit reconciles missing control ownership before cleanup")
        const currentOwner = {generation:3,userId:"restore-owner",providerIdpId:"restore-provider"}
        const currentSeat = {sessionId:"restore-seat",status:3,phase:"streaming",ownerScope:currentOwner}
        const callsBeforeRestore = client.calls.length
        client.responseReceived(ShellStore.streamPollRequestId, {scope:currentOwner,session:currentSeat})
        check(ShellStore.streamStopRequestId !== ""
            && client.calls.slice(callsBeforeRestore).some(call => call.method === "session.stop")
            && client.calls.slice(callsBeforeRestore).every(call => call.method !== "streamer.prepare"),
            "restoring ownership after exit ends the cloud session without restarting media")
        client.requestFailed(ShellStore.streamStopRequestId, "network_error", "Temporary stop failure")
        const afterStopFailure = client.calls.length
        ShellStore.acceptStreamingSession(currentSeat)
        ShellStore.streamPollTimer.triggered()
        check(client.calls.length === afterStopFailure && ShellStore.streamerPrepareRequestId === "",
              "a failed stop and late session snapshot cannot restart media")
        ShellStore.stopStreamingSession()
        client.responseReceived(ShellStore.streamStopRequestId, {session:null})
        check(!ShellStore.activeSession, "explicit cleanup can be retried after a stop failure")
        ShellStore.streamerStopRequestId = ""
        ShellStore.streamer = {status:"stopped"}
        ShellStore.authSession = null
        ShellStore.authGeneration = 0
        runtime.running = false
    }
    function checkExitAfterExhaustedRestore() {
        const owner = {generation:1,userId:"cleanup-owner",providerIdpId:"cleanup-provider"}
        ShellStore.activeSession = {sessionId:"cleanup-seat",status:3,phase:"streaming",ownerScope:owner}
        ShellStore.streamer = {sessionId:"cleanup-seat",status:"streaming"}
        ShellStore.coreSessionRestoreId = "cleanup-seat"
        ShellStore.streamPollFailureAttempts = ShellStore.maximumStreamPollFailureAttempts + 1
        ShellStore.streamerStopRequestId = ""
        ShellStore.streamStopRequestId = ""
        ShellStore.streamPollRequestId = ""
        ShellStore.authSessionRequestId = ""
        runtime.running = true
        ShellStore.stopStreamingSession()
        check(ShellStore.streamPollRequestId !== "" && ShellStore.streamPollFailureAttempts === 0
            && ShellStore.sessionStopIntentId === "cleanup-seat",
            "explicit exit starts fresh bounded cleanup after reconciliation was exhausted")
        client.requestFailed(ShellStore.streamPollRequestId, "network_error", "Cleanup unavailable")
        check(ShellStore.streamPollFailureAttempts === 1 && ShellStore.streamPollTimer.running,
              "cleanup failures still consume the bounded retry budget")
        ShellStore.streamer = {status:"stopped"}
        ShellStore.streamerStopRequestId = ""
        ShellStore.acceptStreamingSession(null)
        runtime.running = false
    }
    function checkAuthenticationRecovery() {
        const auth = {user:{userId:"recovery-owner",displayName:"Recovery owner"},provider:{idpId:"recovery-provider"}}
        const owner = {generation:7,userId:"recovery-owner",providerIdpId:"recovery-provider"}
        const currentOwner = Object.assign({}, owner, {generation:8})
        const seat = {sessionId:"auth-recovery-seat",status:3,phase:"streaming",ownerScope:owner}
        for (const phase of ["discovery", "claim", "claim-refresh", "prepare"]) {
            ShellStore.cancelSessionRecovery()
            ShellStore.activeSession = seat
            ShellStore.authSession = auth
            ShellStore.authGeneration = 7
            ShellStore.coreSessionRestoreId = ""
            ShellStore.sessionStopIntentId = ""
            ShellStore.streamerStopRequestId = ""
            ShellStore.streamerPrepareRequestId = ""
            ShellStore.streamerStartRequestId = ""
            ShellStore.streamStopRequestId = ""
            ShellStore.streamPollRequestId = ""
            ShellStore.authSessionRequestId = ""
            ShellStore.streamerRecoveryExhausted = false
            ShellStore.sessionReconnectAttempts = 2
            ShellStore.nativeRuntimeReady = true
            ShellStore.streamer = {sessionId:seat.sessionId,status:"stopped"}
            runtime.commands = []
            if (phase === "prepare") {
                ShellStore.startNativeStreamer()
            } else {
                ShellStore.recoverStreamingSession("recover authentication fixture")
                if (phase === "claim" || phase === "claim-refresh")
                    client.responseReceived(ShellStore.recoveryDiscoveryRequestId,
                        {scope:phase === "claim-refresh" ? currentOwner : owner,session:seat})
            }
            const abandoned = phase === "prepare" ? ShellStore.streamerPrepareRequestId
                : phase.startsWith("claim") ? ShellStore.sessionClaimRequestId : ShellStore.recoveryDiscoveryRequestId
            const attempts = ShellStore.sessionReconnectAttempts
            check(abandoned !== "", phase + " starts a real outstanding operation")
            if (phase === "claim-refresh") {
                check(ShellStore.authSessionRequestId !== "", "newer recovery scope requests authentication synchronization")
                client.responseReceived(ShellStore.authSessionRequestId, {session:auth,generation:8})
            } else {
                client.eventReceived("auth.session.changed", {session:auth,generation:8})
            }
            check(client.cancelled.includes(abandoned) && ShellStore.recoveryDiscoveryRequestId === ""
                && ShellStore.sessionClaimRequestId === "" && ShellStore.streamerPrepareRequestId === ""
                && !ShellStore.sessionRecoveryPending,
                phase + " authentication change retires the operation and its recovery ownership")
            check(ShellStore.streamerRestartTimer.running && ShellStore.sessionReconnectAttempts === attempts,
                phase + " authentication change schedules bounded recovery without resetting its budget")
            client.responseReceived(abandoned, {scope:owner,session:seat})
            ShellStore.streamerRestartTimer.triggered()
            check(ShellStore.recoveryDiscoveryRequestId !== "" && ShellStore.sessionReconnectAttempts === attempts + 1,
                phase + " automatically retries discovery after the matching owner returns")
            const stale = ShellStore.recoveryDiscoveryRequestId
            client.responseReceived(stale, {scope:owner,session:seat})
            check(ShellStore.recoveryDiscoveryRequestId === "" && !ShellStore.sessionRecoveryPending
                && ShellStore.streamerRestartTimer.running,
                phase + " rejected stale discovery releases its slot and schedules another bounded retry")
            ShellStore.retryNativeStreamer()
            check(ShellStore.recoveryDiscoveryRequestId !== "", phase + " explicit Retry is not blocked by the rejected reply")
            client.responseReceived(ShellStore.recoveryDiscoveryRequestId,
                {scope:currentOwner,session:Object.assign({}, seat, {ownerScope:currentOwner})})
            check(ShellStore.sessionClaimRequestId !== "", phase + " current owner can reclaim the same seat")
            const foreign = {user:{userId:"different-owner",displayName:"Other owner"},provider:auth.provider}
            client.eventReceived("auth.session.changed", {session:foreign,generation:9})
            const beforeForeignRetry = client.calls.length
            ShellStore.streamerRestartTimer.triggered()
            ShellStore.retryNativeStreamer()
            ShellStore.acceptStreamingSession(seat)
            check(client.calls.slice(beforeForeignRetry).every(call => ["session.poll", "session.claim", "streamer.prepare"].indexOf(call.method) < 0)
                && ShellStore.sessionClaimRequestId === ""
                && ShellStore.recoveryDiscoveryRequestId === "" && !ShellStore.streamerRestartTimer.running,
                phase + " neither automatic nor explicit Retry can claim a foreign account's seat")
            client.eventReceived("auth.session.changed", {session:auth,generation:10})
            check(ShellStore.streamerRestartTimer.running, phase + " returning owner rearms suspended media recovery")
            ShellStore.sessionReconnectAttempts = ShellStore.maximumSessionReconnectAttempts
            ShellStore.streamerRestartTimer.triggered()
            check(!ShellStore.streamerRestartTimer.running && ShellStore.streamerRecoveryExhausted,
                phase + " authentication recovery still exhausts its bounded retry budget")
        }
        ShellStore.cancelSessionRecovery()
        ShellStore.streamerRecoveryExhausted = false
        ShellStore.sessionReconnectAttempts = 0
        ShellStore.streamer = {sessionId:seat.sessionId,status:"streaming",marker:"healthy-media"}
        const commandsBefore = runtime.commands.length
        client.eventReceived("auth.session.changed", {session:null,generation:11})
        client.eventReceived("auth.session.changed", {session:auth,generation:12})
        check(ShellStore.streamer.marker === "healthy-media" && !ShellStore.streamerRestartTimer.running
            && runtime.commands.length === commandsBefore,
            "account changes do not stop or restart surviving native media")
        runtime.running = true
        ShellStore.recoverStreamingSession("native cleanup during account switch")
        const nativeStop = ShellStore.streamerStopRequestId
        check(nativeStop !== "" && ShellStore.sessionRecoveryPending, "native cleanup owns recovery until its acknowledgement")
        client.eventReceived("auth.session.changed", {session:null,generation:13})
        runtime.responseReceived({id:nativeStop,type:"ok"})
        check(!ShellStore.sessionRecoveryPending && ShellStore.recoveryDiscoveryRequestId === ""
            && ShellStore.sessionRecoveryAwaitingAuth, "native cleanup parks discovery until the owner signs back in")
        client.eventReceived("auth.session.changed", {session:auth,generation:14})
        check(ShellStore.streamerRestartTimer.running, "native cleanup recovery resumes for the returning owner")
        ShellStore.stopStreamingSession()
        client.eventReceived("auth.session.changed", {session:auth,generation:15})
        check(!ShellStore.streamerRestartTimer.running && ShellStore.sessionStopIntentId === seat.sessionId,
            "owner authentication cannot override an explicit stop intent")
        client.responseReceived(ShellStore.streamStopRequestId, {session:null})
        ShellStore.streamer = {status:"stopped"}
        ShellStore.acceptStreamingSession(null)
        ShellStore.authSession = null
        ShellStore.authGeneration = 0
        runtime.running = false
    }
    function checkSavedProfileBootstrap() {
        const profiles = [{userId:"locked-owner",displayName:"Locked profile",hasPin:true}]
        for (const order of ["accounts-first", "auth-first"]) {
            const initialRoute = order === "accounts-first" ? "home" : "sign-in"
            client.state = "stopped"
            ShellStore.activeSession = null
            ShellStore.authSession = null
            ShellStore.savedAccounts = []
            ShellStore.accountsRequestId = ""
            ShellStore.authSessionRequestId = ""
            ShellStore.streamStopRequestId = ""
            ShellStore.streamCreateRequestId = ""
            ShellStore.addingAccount = false
            ShellStore.authState = "idle"
            ShellStore.authRestorePending = true
            AppController.navigate(initialRoute)
            const before = client.calls.length
            client.state = "ready"
            const accountsId = ShellStore.accountsRequestId
            const authId = ShellStore.authSessionRequestId
            check(accountsId !== "" && authId !== "", order + " independently loads saved profiles during authentication bootstrap")
            if (order === "accounts-first") {
                client.responseReceived(accountsId, {accounts:profiles})
                check(AppController.route === initialRoute, "saved profiles cannot interrupt unfinished authentication restoration")
                client.responseReceived(authId, {session:null,generation:1})
            } else {
                client.responseReceived(authId, {session:null,generation:1})
                check(AppController.route === initialRoute, "null authentication waits for the saved profile list")
                client.responseReceived(accountsId, {accounts:profiles})
            }
            check(AppController.route === "accounts" && !ShellStore.signedIn,
                order + " exposes the existing PIN profile selection without bypassing authentication")
            check(client.calls.slice(before).every(call => !["account.subscription.get", "network.regions.list", "account.connections.list"].includes(call.method)),
                "loading saved profiles does not require signed-in network services")
            ShellStore.beginAddAccount()
            ShellStore.refreshSavedAccounts(true)
            client.responseReceived(ShellStore.accountsRequestId, {accounts:profiles})
            check(AppController.route === "sign-in" && ShellStore.addingAccount,
                "a fresh profile list cannot redirect explicit add-account login")
            ShellStore.addingAccount = false
            ShellStore.activeSession = {sessionId:"surviving-seat"}
            ShellStore.refreshSavedAccounts(true)
            client.responseReceived(ShellStore.accountsRequestId, {accounts:profiles})
            check(AppController.route === "sign-in", "saved profile selection cannot reroute an active session")
            ShellStore.activeSession = null
            AppController.navigate("home")
            ShellStore.refreshSavedAccounts(true)
            const stale = ShellStore.accountsRequestId
            ShellStore.logoutRequestId = "logout-bootstrap"
            client.responseReceived("logout-bootstrap", {session:null,generation:2})
            check(client.cancelled.includes(stale) && ShellStore.accountsRequestId !== stale,
                "logout retires an older profile list before refreshing it")
            client.responseReceived(stale, {accounts:profiles})
            client.responseReceived(ShellStore.accountsRequestId, {accounts:[]})
            check(ShellStore.savedAccounts.length === 0 && AppController.route === "home",
                "an empty post-logout profile list cannot resurrect removed accounts")
            ShellStore.accountRemoveRequestId = "remove-bootstrap"
            client.responseReceived("remove-bootstrap", {session:null,generation:3})
            client.responseReceived(ShellStore.accountsRequestId, {accounts:profiles})
            check(AppController.route === "home", "profile removal waits for its authentication refresh")
            client.responseReceived(ShellStore.authSessionRequestId, {session:null,generation:3})
            check(AppController.route === "accounts", "profile removal can expose another saved PIN profile")
        }
        ShellStore.savedAccounts = []
    }
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
        checkActiveTimerExhaustion()
        checkCoreRestartPreservesSession()
        checkReconciliationAuthAndExit()
        checkExitAfterExhaustedRestore()
        checkAuthenticationRecovery()
        checkSavedProfileBootstrap()
        ShellStore.activeSession = null
        ShellStore.streamer = null
        beginLiveProfileUnlock(parent)
        finishLiveProfileUnlock(parent)
        return true
    }
}

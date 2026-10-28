import QtQuick
import QtMultimedia
import OpenNOW

QtObject {
    id: fixture
    property var client
    property var runtime
    property Component adComponent: Component { QueueAdPlayback {} }

    function check(ok, message) { if (!ok) throw new Error(message) }
    function reset() {
        ShellStore.cancelSessionRecovery()
        ShellStore.streamerRestartTimer.stop()
        ShellStore.streamerRecoveryExhausted = false
        ShellStore.sessionRecoveryAwaitingAuth = false
        ShellStore.coreSessionRestoreId = ""
        ShellStore.sessionStopIntentId = ""
        ShellStore.streamStopRequestId = ""
        ShellStore.streamerStartRequestId = ""
        ShellStore.streamerPrepareRequestId = ""
        ShellStore.streamerStopRequestId = ""
        ShellStore.sessionClaimRequestId = ""
        ShellStore.nativeRuntimeReady = true
        ShellStore.nativeRequests = ({})
        ShellStore.streamerStopExpected = false
        ShellStore.streamInputPauseRequestId = "blocked"
        ShellStore.activeSession = null
        ShellStore.streamer = {status:"stopped"}
        runtime.running = true
        client.state = "ready"
    }
    function checkLocalActions() {
        reset()
        for (const sessionId of ["actions-a", "actions-a", "actions-b"]) {
            ShellStore.activeSession = {sessionId:sessionId, status:2, phase:"ready"}
            ShellStore.streamer = {status:"stopped"}
            ShellStore.streamerPrepareRequestId = ""
            const before = [ShellStore.overlayRequestGeneration, ShellStore.screenshotRequestGeneration,
                ShellStore.recordingToggleRequestGeneration, ShellStore.shortcutActionGeneration]
            ShellStore.startNativeStreamer()
            for (const type of ["overlay-request", "screenshot-request", "recording-toggle-request", "shortcut-action"])
                ShellStore.acceptNativeEvent({type:type, action:"toggle-anti-afk"})
            const after = [ShellStore.overlayRequestGeneration, ShellStore.screenshotRequestGeneration,
                ShellStore.recordingToggleRequestGeneration, ShellStore.shortcutActionGeneration]
            check(after.every((value, index) => value > before[index]),
                "QT-01: every local action must be consumed after reconnect/replacement: " + sessionId)
            ShellStore.acceptStreamerSnapshot(Object.assign({}, ShellStore.streamer))
            check(ShellStore.shortcutActionGeneration === after[3], "local actions cannot replay on snapshots")
        }
    }
    function checkStopReplacement() {
        reset()
        ShellStore.activeSession = {sessionId:"stop-a", status:3, phase:"streaming"}
        ShellStore.streamer = {sessionId:"stop-a", status:"streaming"}
        ShellStore.stopNativeStreamer("fixture replacement")
        const stopId = ShellStore.streamerStopRequestId
        check(stopId !== "", "a native stop must be outstanding")
        const before = client.calls.filter(call => call.method === "streamer.prepare").length
        ShellStore.acceptStreamingSession({sessionId:"stop-b", status:2, phase:"ready"})
        check(client.calls.filter(call => call.method === "streamer.prepare").length === before,
            "replacement media cannot overlap old teardown")
        runtime.responseReceived({id:stopId, type:"stopped"})
        check(ShellStore.streamerPrepareRequestId !== "",
            "QT-02: stop ACK must reconcile the ready replacement without another poll")
        const request = client.calls.find(call => call.id === ShellStore.streamerPrepareRequestId)
        check(request.params.session.sessionId === "stop-b", "only the desired seat can prepare")
        runtime.responseReceived({id:stopId, type:"stopped"})
        check(ShellStore.streamerPrepareRequestId === request.id, "duplicate stop ACK must be inert")
        ShellStore.activeSession = null
        client.responseReceived(request.id, {session:{sessionId:"stop-b"}, context:{settings:{}}})
        check(ShellStore.streamerStartRequestId === "", "an abandoned replacement cannot start from a late prepare")
        reset()
        ShellStore.activeSession = {sessionId:"late-a", status:2, phase:"ready"}
        ShellStore.startNativeStreamer()
        client.responseReceived(ShellStore.streamerPrepareRequestId,
            {session:{sessionId:"late-a", status:2, phase:"ready"}, context:{settings:{}}})
        const oldStart = ShellStore.streamerStartRequestId
        ShellStore.stopNativeStreamer("late start fixture")
        const oldStop = ShellStore.streamerStopRequestId
        ShellStore.acceptStreamingSession({sessionId:"late-b", status:2, phase:"ready"})
        runtime.responseReceived({id:oldStop, type:"stopped"})
        const replacement = ShellStore.streamerPrepareRequestId
        runtime.responseReceived({id:oldStart, type:"ok", transport:"nvst"})
        check(ShellStore.streamer.sessionId === "late-b" && ShellStore.streamer.status === "starting"
            && ShellStore.streamerPrepareRequestId === replacement,
            "an old start response cannot claim replacement media")
    }
    function checkAdReports() {
        reset()
        ShellStore.activeSession = {sessionId:"ad-seat", status:1, phase:"queued",
            adState:{sessionAdsRequired:true, sessionAds:[{adId:"ad-a"}]}}
        const ad = {adId:"ad-a"}
        const before = client.calls.length
        ShellStore.reportSessionAd("start", ad, 0, "")
        const startId = ShellStore.sessionAdRequestId
        ShellStore.reportSessionAd("pause", ad, 200, "")
        ShellStore.reportSessionAd("finish", ad, 1000, "")
        client.responseReceived(startId, {})
        let reports = client.calls.slice(before).filter(call => call.method === "session.ad.report")
        check(reports.length === 2 && reports[1].params.action === "pause",
            "QT-03: pause must survive an outstanding start report")
        client.responseReceived(ShellStore.sessionAdRequestId, {})
        reports = client.calls.slice(before).filter(call => call.method === "session.ad.report")
        check(reports.length === 3 && reports[2].params.action === "finish", "terminal report must follow pause")
        const terminal = ShellStore.sessionAdRequestId
        client.requestFailed(terminal, "network_error", "fixture offline")
        check(ShellStore.sessionAdReports.length > 0, "terminal ownership survives RPC failure")
        ShellStore.sessionAdRetryTimer.triggered()
        const retry = client.calls[client.calls.length - 1]
        check(retry.params.action === "finish" && retry.params.adId === "ad-a", "retry the same terminal report")
        client.responseReceived(retry.id, {})
        check(ShellStore.sessionAdReports.length === 0, "terminal ACK releases ownership")
        ShellStore.reportSessionAd("start", {adId:"ad-b"}, 0, "")
        const abandoned = ShellStore.sessionAdRequestId
        ShellStore.reportSessionAd("cancel", {adId:"ad-b"}, 300, "error")
        ShellStore.activeSession = {sessionId:"replacement-ad-seat", adState:{sessionAds:[{adId:"ad-c"}]}}
        check(ShellStore.sessionAdReports.length === 0 && ShellStore.sessionAdRequestId === "",
            "session abandonment explicitly releases old reporting ownership")
        client.responseReceived(abandoned, {session:{sessionId:"ad-seat", status:1, phase:"queued"}})
        check(ShellStore.activeSession.sessionId === "replacement-ad-seat", "late ad ACK cannot restore an abandoned session")
    }
    function checkBoundedAdReports(parent) {
        reset()
        ShellStore.activeSession = {sessionId:"bounded-ad-seat", adState:{sessionAds:[{adId:"bounded-0"}]}}
        for (let adIndex = 0; adIndex < 8; ++adIndex) {
            const ad = {adId:"bounded-" + adIndex}
            check(ShellStore.reportSessionAd("start", ad, 0, ""), "ad admission reserves terminal capacity")
            for (let index = 0; index < 100; ++index)
                ShellStore.reportSessionAd(index % 2 ? "resume" : "pause", ad, index, "")
            check(ShellStore.reportSessionAd("finish", ad, 1000, ""), "terminal capacity cannot be consumed by pause/resume churn")
        }
        check(ShellStore.sessionAdReports.length === 8
            && ShellStore.sessionAdReports.every(report => report.events.length <= 4),
            "ad reporting has a fixed bound under a stalled core")
        ShellStore.activeSession = {sessionId:"bounded-ad-seat", adState:{sessionAds:[{adId:"bounded-next"}]}}
        const player = adComponent.createObject(parent)
        check(player.player === null && player.admissionTimer.running,
            "a saturated report queue backpressures new playback instead of losing its terminal report")
        client.responseReceived(ShellStore.sessionAdRequestId, {})
        client.responseReceived(ShellStore.sessionAdRequestId, {})
        client.responseReceived(ShellStore.sessionAdRequestId, {})
        player.admissionTimer.triggered()
        check(player.player !== null && !player.admissionTimer.running, "terminal ACK releases capacity for waiting playback")
        player.destroy()
    }
    function checkAdIdentity(parent) {
        reset()
        ShellStore.activeSession = {sessionId:"player-seat", adState:{sessionAds:[{adId:"ad-a"}]}}
        const player = adComponent.createObject(parent)
        check(player !== null, "the real Qt Multimedia ad player must mount")
        player.started = true
        player.completed = true
        ShellStore.activeSession = {sessionId:"player-seat", adState:{sessionAds:[{adId:"ad-a", title:"poll"}]}}
        check(player.started && player.completed, "same-ID snapshots must not reset playback flags")
        ShellStore.activeSession = {sessionId:"player-seat", adState:{sessionAds:[{adId:"ad-b"}]}}
        const resetForAd = !player.started && !player.completed
        check(resetForAd, "QT-04: a mounted player's flags must reset for a replacement ad")
        player.started = true
        player.completed = true
        ShellStore.activeSession = {sessionId:"other-player-seat", adState:{sessionAds:[{adId:"ad-b"}]}}
        check(!player.started && !player.completed, "a new session resets even an unchanged ad ID")
        player.destroy()
    }
    function checkPlayerReporting(parent) {
        reset()
        ShellStore.activeSession = {sessionId:"playback-seat", status:1, phase:"queued",
            adState:{sessionAdsRequired:true, sessionAds:[{adId:"play-a"}]}}
        const player = adComponent.createObject(parent)
        const first = player.player
        check(first !== null && !player.started, "loading an ad does not claim playback has started")
        player.toggle()
        check(!player.started && ShellStore.sessionAdRequestId === "", "a delayed play command cannot report resume speculatively")
        first.acceptPlaybackState(MediaPlayer.PlayingState)
        const start = ShellStore.sessionAdRequestId
        first.acceptPlaybackState(MediaPlayer.PausedState)
        first.acceptMediaStatus(MediaPlayer.EndOfMedia)
        check(player.started && player.completed && start !== "", "the store owns a terminal report behind a delayed start")
        ShellStore.activeSession = {sessionId:"playback-seat", status:1, phase:"queued",
            adState:{sessionAdsRequired:true, sessionAds:[{adId:"play-b"}]}}
        const second = player.player
        check(second !== first && !player.started && !player.completed, "the mounted wrapper gives the replacement its own backend")
        const beforeTeardown = client.calls.length
        first.acceptPlaybackState(MediaPlayer.PausedState)
        first.acceptMediaStatus(MediaPlayer.InvalidMedia)
        check(!player.completed && client.calls.length === beforeTeardown,
            "old-source teardown callbacks cannot cancel or pause the replacement ad")
        second.acceptPlaybackState(MediaPlayer.PlayingState)
        second.acceptMediaStatus(MediaPlayer.InvalidMedia)
        check(player.completed, "invalid replacement media transfers its cancel report to the store")
        client.requestFailed(start, "network_error", "delayed start failed")
        ShellStore.sessionAdRetryTimer.triggered()
        const retriedStart = ShellStore.sessionAdRequestId
        const delayed = {adReportIdentity:{sessionId:"playback-seat", adId:"play-a"},
            session:{sessionId:"playback-seat", status:1, phase:"queued",
            adState:{sessionAdsRequired:true, sessionAds:[{adId:"play-a"}]}}}
        client.responseReceived(retriedStart, delayed)
        client.eventReceived("session.changed", delayed)
        check(ShellStore.activeSession.adState.sessionAds[0].adId === "play-b", "a stale ad snapshot cannot replace the current ad")
        const expected = [["pause", "play-a"], ["finish", "play-a"], ["start", "play-b"], ["cancel", "play-b"]]
        for (const pair of expected) {
            const call = client.calls.find(value => value.id === ShellStore.sessionAdRequestId)
            check(call && call.params.action === pair[0] && call.params.adId === pair[1],
                "ordered player report " + pair)
            client.responseReceived(call.id, {})
        }
        check(ShellStore.sessionAdReports.length === 0, "both ad terminal reports remain owned until ACK")
        ShellStore.activeSession = {sessionId:"playback-seat", status:1, phase:"queued",
            adState:{sessionAdsRequired:true, sessionAds:[{adId:"play-b", title:"same-ID poll"}]}}
        check(player.started && player.completed && player.player === second, "same-ID polls never replay a completed ad")
        player.destroy()
    }
    function checkReportCompanionEnvelopes(parent) {
        const seat = adId => ({sessionId:"envelope-seat", status:1, phase:"queued",
            adState:{sessionAdsRequired:true, sessionAds:[{adId:adId}]}})
        const envelope = adId => ({adReportIdentity:{sessionId:"envelope-seat", adId:"envelope-a"},
            session:seat(adId)})
        for (const retry of [false, true]) {
            reset()
            ShellStore.activeSession = seat("envelope-a")
            const player = adComponent.createObject(parent)
            ShellStore.reportSessionAd("start", {adId:"envelope-a"}, 0, "")
            if (retry) {
                client.requestFailed(ShellStore.sessionAdRequestId, "network_error", "delayed ad report")
                ShellStore.sessionAdRetryTimer.triggered()
            }
            const requestId = ShellStore.sessionAdRequestId
            ShellStore.acceptStreamingSession(seat("envelope-b"))
            const current = player.player
            client.responseReceived(requestId, envelope("envelope-a"))
            client.eventReceived("session.changed", envelope("envelope-a"))
            check(ShellStore.activeSession.adState.sessionAds[0].adId === "envelope-b"
                && player.player === current,
                "ad-report response AND companion event cannot roll back the polled replacement, retry=" + retry)
            const owner = {generation:7, userId:"envelope-owner", providerIdpId:"envelope-provider"}
            const live = seat("envelope-b")
            live.ownerScope = owner
            ShellStore.activeSession = live
            const ended = envelope("envelope-a")
            ended.scope = owner
            ended.session.status = 7
            client.responseReceived("abandoned-ad-report", ended)
            client.eventReceived("session.changed", ended)
            check(ShellStore.activeSession && ShellStore.activeSession.adState.sessionAds[0].adId === "envelope-b"
                && player.player === current, "unowned tagged replies cannot bypass identity checks through termination handling")
            player.destroy()
        }
        for (const eventFirst of [false, true]) {
            reset()
            ShellStore.activeSession = seat("envelope-a")
            const player = adComponent.createObject(parent)
            ShellStore.reportSessionAd("finish", {adId:"envelope-a"}, 1000, "")
            const requestId = ShellStore.sessionAdRequestId
            if (eventFirst) client.eventReceived("session.changed", envelope("envelope-b"))
            else client.responseReceived(requestId, envelope("envelope-b"))
            const advanced = player.player
            check(ShellStore.activeSession.adState.sessionAds[0].adId === "envelope-b",
                "a matching report can advance the current ad")
            if (eventFirst) client.responseReceived(requestId, envelope("envelope-b"))
            else client.eventReceived("session.changed", envelope("envelope-b"))
            check(player.player === advanced && ShellStore.sessionAdRequestId === "",
                "the companion delivery cannot recreate the advanced ad")
            for (const identity of [null, {}, {sessionId:"wrong-seat", adId:"envelope-b"},
                    {sessionId:"envelope-seat", adId:"x".repeat(257)}]) {
                client.eventReceived("session.changed", {adReportIdentity:identity, session:seat("forged-ad")})
                check(player.player === advanced, "malformed or foreign report identities cannot adopt a snapshot")
            }
            client.eventReceived("session.changed", {session:seat("ordinary-poll")})
            check(ShellStore.activeSession.adState.sessionAds[0].adId === "ordinary-poll",
                "ordinary untagged session events remain authoritative")
            player.destroy()
        }
    }
    function run(parent, core, nativeRuntime) {
        client = core
        runtime = nativeRuntime
        const failures = []
        for (const probe of [checkLocalActions, checkStopReplacement, checkAdReports, () => checkAdIdentity(parent),
                () => checkPlayerReporting(parent), () => checkBoundedAdReports(parent),
                () => checkReportCompanionEnvelopes(parent)]) {
            try { probe() } catch (error) { failures.push(String(error)) }
        }
        reset()
        runtime.running = false
        check(failures.length === 0, failures.join("\n"))
        return true
    }
}

import QtQuick
import OpenNOW

QtObject {
    property QtObject client: QtObject {
        property string state: "ready"
        property string lastError: ""
        property int sequence: 0
        property var requests: []
        signal responseReceived(string requestId, var result)
        signal requestFailed(string requestId, string code, string message)
        signal eventReceived(string name, var payload)
        function markUiReady() {}
        function logShellDiagnostic(message) {}
        function request(method, params, timeout) {
            const id = "bug-report-" + (++sequence)
            requests.push({id: id, method: method, params: params})
            return id
        }
        function cancel(id) { return true }
    }
    function check(value, message) { if (!value) throw new Error("Bug report acceptance: " + message) }
    function namedChild(item, name) {
        if (item.objectName === name) return item
        for (const child of item.children || []) {
            const found = namedChild(child, name)
            if (found) return found
        }
        return null
    }
    function findText(item, wanted) {
        if (item.text !== undefined && String(item.text) === wanted && item.visible !== false) return item
        for (const child of item.children || []) {
            const found = findText(child, wanted)
            if (found) return found
        }
        return null
    }
    function incidents() {
        return client.requests.filter(item => item.method === "bug_report.incident")
    }
    function tracked(event) {
        return client.requests.filter(item => item.method === "analytics.track" && item.params.event === event)
    }
    function setChoice(value) {
        ShellStore.settings = Object.assign({}, ShellStore.settings, {automaticBugReports: value})
    }

    function run(host) {
        const reports = ShellStore.bugReports
        ShellStore.authGeneration = 3
        ShellStore.authSession = {user: {userId: "bug-fixture", displayName: "Zortos"}, provider: {idpId: "nvidia"}}
        setChoice("unset")
        check(reports.enabled, "an unanswered notice must leave automatic reports on")
        check(reports.noticePending, "signed-in users with no choice must see the notice")

        const app = namedChild(host, "desktopApp")
        check(app !== null, "desktop shell is missing")
        app.bugReportNoticeAllowed = true
        const notice = namedChild(host, "desktopBugReportNotice")
        check(notice !== null && notice.opened, "the first sign-in notice did not open")
        check(findText(notice, "Help fix OpenNOW faster") !== null, "the notice title is missing")

        namedChild(notice, "bugReportNoticeTurnOff").clicked()
        const write = client.requests.filter(item => item.method === "settings.set").pop()
        check(write && write.params.key === "automaticBugReports" && write.params.value === "disabled",
            "Turn off did not persist the opt-out")
        check(write.params.source === "first_run_sheet", "the opt-out did not name the first-run sheet")
        setChoice("disabled")
        check(!notice.opened && !reports.noticePending, "the notice stayed open after a choice")
        reports.reportStreamError("native_stream_error", "decoder lost")
        check(incidents().length === 0, "an opted-out user produced a report")
        check(tracked("session_error").length === 0, "an opted-out user produced usage statistics")

        setChoice("enabled")
        reports.streaming = true
        reports.sessionId = "bug-session"
        reports.observeFrameDrops(200, "other-session")
        reports.observeFrameDrops(200, "bug-session")
        check(incidents().length === 0, "frame drops below the threshold produced a report")
        reports.observeFrameDrops(150, "bug-session")
        const drops = incidents()
        check(drops.length === 1 && drops[0].params.kind === "frame_drops"
            && drops[0].params.metrics.droppedFrames === 350, "sustained frame drops were not reported once")
        reports.observeFrameDrops(500, "bug-session")
        check(incidents().length === 1, "frame drops were reported twice in one session")
        const dropEvents = tracked("frame_drops_detected")
        check(dropEvents.length === 1 && dropEvents[0].params.props.dropped === 350
            && dropEvents[0].params.props.window_s === 60, "frame drops were not counted once")

        reports.observeFirstFrame({game_id: "100", game_title: "Portal 2", codec: "h265",
            resolution: "1920x1080", fps_target: 60, decoder_backend: "vaapi", first_frame_ms: 840})
        reports.observeFirstFrame({game_id: "100"})
        const started = tracked("session_started")
        check(started.length === 1 && started[0].params.props.codec === "h265"
            && started[0].params.uiSurface === "desktop", "the first frame did not start the session once")
        reports.observeTelemetry({sessionId: "bug-session", framesPerSecond: 60, pingMs: 20, packetLossPercent: 0})
        reports.observeTelemetry({sessionId: "other-session", framesPerSecond: 1, pingMs: 900})
        reports.observeTelemetry({sessionId: "bug-session", framesPerSecond: 58, pingMs: 24, packetLossPercent: null})
        reports.observeSessionEnd("bug-session", {game_id: "100", game_title: "Portal 2",
            duration_s: 120, outcome: "clean", recoveries: 0, video_drop_count: 350, decoder_errors: 0})
        reports.observeSessionEnd("bug-session", {outcome: "remote_ended"})
        const ended = tracked("session_ended")
        check(ended.length === 1, "the session ended more than once")
        const summary = ended[0].params.props
        check(summary.outcome === "clean" && summary.avg_fps === 59 && summary.avg_ping_ms === 22
            && summary.avg_packet_loss_pct === 0 && summary.video_drop_count === 350,
            "the session summary did not average the stream statistics")

        reports.reportStreamError("Native Stream Error!", "Bearer secret")
        const stream = incidents().pop()
        check(stream.params.kind === "stream_error" && stream.params.code === "native_stream_error_",
            "stream error codes must be normalized for the core contract")
        const streamError = tracked("session_error").pop()
        check(streamError && streamError.params.props.stage === "stream"
            && streamError.params.props.code === "native_stream_error_", "stream errors were not counted")

        client.eventReceived("bug_report.changed", {state: "sent", kind: "library_error",
            code: "network_error", reportId: "br-7F3A21", issueStatus: "investigating", game: "Portal 2"})
        check(reports.latest && reports.latest.reportId === "br-7F3A21", "the sent event was not kept")
        return true
    }

    function verifyRendered(host) {
        const toast = namedChild(host, "desktopBugReportToast")
        check(toast !== null && toast.visible, "the report toast is not visible")
        check(findText(toast, "Bug reported to the developer") !== null, "the toast title is missing")
        check(findText(toast, "REF br-7F3A21 · Portal 2") !== null, "the toast reference is missing")
        check(findText(toast, "Your library couldn't load. The developer's assistant is looking into it.") !== null,
            "the investigating copy is missing")
        return true
    }
}

import QtQuick
import QtQuick.Window
import OpenNOW

QtObject {
    property var host: null
    property int phase: 0
    property int key: 0
    property int modifiers: 0
    property bool settling: false
    onKeyChanged: if (key !== 0) settling = true
    readonly property bool noticePreview: Qt.application.arguments.indexOf("--bug-report-notice-preview") >= 0
    readonly property bool noticeCheck: Qt.application.arguments.indexOf("--bug-report-notice-check") >= 0
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
        if (Qt.application.arguments.indexOf("--bug-report-stream-preview") >= 0) {
            ShellStore.authRestorePending = false
            ShellStore.authGeneration = 3
            ShellStore.authSession = {user: {userId: "bug-fixture", displayName: "Layout fixture"}}
            setChoice("enabled")
            ShellStore.activeSession = {sessionId: "report-fixture", status: 2}
            ShellStore.streamState = "streaming"
            ShellStore.streamer = {status: "streaming", sessionId: "report-fixture", firstFrameLatencyMs: 1}
            AppController.navigate("stream")
            client.eventReceived("bug_report.changed", {state: "sent", kind: "frame_drops",
                reportId: "br-7F3A21", game: "Portal 2"})
            return true
        }
        if (noticePreview || noticeCheck) {
            this.host = host
            ShellStore.authRestorePending = false
            ShellStore.authGeneration = 3
            ShellStore.authSession = {user: {userId: "bug-fixture", displayName: "Layout fixture"}, provider: {idpId: "nvidia"}}
            setChoice("unset")
            const scaleIndex = Qt.application.arguments.indexOf("--bug-report-scale")
            DesktopTokens.uiScale = scaleIndex >= 0 ? Number(Qt.application.arguments[scaleIndex + 1]) : 1
            ShellStore.settings = Object.assign({}, ShellStore.settings, {desktopUiScale: DesktopTokens.uiScale})
            namedChild(host, "desktopApp").bugReportNoticeAllowed = true
            AppController.navigate("home")
            if (noticeCheck) {
                const notice = namedChild(host, "desktopBugReportNotice")
                AppController.showOverlay("friends")
                check(!notice.opened, "notice stacked over an existing shell overlay")
                AppController.showOverlay("")
                ShellStore.queueSelector.opened = true
                check(!notice.opened, "notice stacked over a queue selection")
                ShellStore.queueSelector.opened = false
                AppController.navigate("game-detail")
                check(!notice.opened, "notice stacked over game details")
                AppController.navigate("home")
            }
            return true
        }
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
        if (Qt.application.arguments.indexOf("--bug-report-stream-preview") >= 0) {
            check(namedChild(host, "streamBugReportToast").visible, "stream report toast is not visible")
            return true
        }
        if (noticePreview || noticeCheck) {
            const notice = namedChild(host, "desktopBugReportNotice")
            check(notice !== null && notice.opened, "the preview notice is not open")
            for (const name of ["bugReportNoticeTurnOff", "bugReportNoticeKeep"]) {
                const button = namedChild(notice, name)
                const corner = button.mapToItem(host, 0, 0)
                console.log("NOTICE BUTTON", name, corner.x, corner.y, button.width, button.height, "HOST", host.width, host.height)
            }
            if (Qt.application.arguments.indexOf("--bug-report-notice-bottom") >= 0) {
                const scroll = namedChild(host, "bugReportNoticeScroll")
                scroll.contentItem.contentY = Math.max(0, scroll.contentHeight - scroll.height)
            }
            return true
        }
        const toast = namedChild(host, "desktopBugReportToast")
        check(toast !== null && toast.visible, "the report toast is not visible")
        check(findText(toast, "Bug reported to the developer") !== null, "the toast title is missing")
        check(findText(toast, "REF br-7F3A21 · Portal 2") !== null, "the toast reference is missing")
        check(findText(toast, "Your library couldn't load. The developer's assistant is looking into it.") !== null,
            "the investigating copy is missing")
        return true
    }

    function insideNotice(item) {
        const notice = namedChild(host, "desktopBugReportNotice")
        for (let owner = item; owner; owner = owner.parent) {
            if (owner === notice) return true
        }
        return false
    }

    function advance() {
        if (settling) { settling = false; return 0 }
        const app = namedChild(host, "desktopApp")
        if (phase === 0) {
            const sheet = namedChild(host, "bugReportNoticeSheet")
            const top = sheet.mapToItem(host, 0, 0)
            check(top.x >= 0 && top.y >= 0 && top.x + sheet.width <= host.width
                && top.y + sheet.height <= host.height, "notice sheet exceeds the window")
            const scroll = namedChild(host, "bugReportNoticeScroll")
            check(scroll.height > 0 && scroll.clip, "notice body has no bounded scroll viewport")
            for (const name of ["bugReportNoticeTurnOff", "bugReportNoticeKeep"]) {
                const button = namedChild(host, name)
                const corner = button.mapToItem(host, 0, 0)
                check(corner.y >= 0 && corner.y + button.height <= host.height,
                    "notice action is outside the window: " + name)
            }
            check(insideNotice(host.Window.window.activeFocusItem), "notice did not retain initial focus")
            key = Qt.Key_K
            modifiers = Qt.ControlModifier
            phase = 1
            return 0
        }
        if (phase === 1) {
            console.log("NOTICE CTRL-K", "palette", app.commandOpen, "focus", host.Window.window.activeFocusItem.objectName)
            check(!app.commandOpen, "Ctrl+K opened a covered command palette")
            check(insideNotice(host.Window.window.activeFocusItem), "Ctrl+K moved focus behind the notice")
            phase = 2
        }
        const blocked = [Qt.Key_PageDown, Qt.Key_PageUp, Qt.Key_Menu, Qt.Key_Y, Qt.Key_X, Qt.Key_F10]
        if (phase >= 2 && phase < 2 + blocked.length) {
            check(AppController.route === "home" && AppController.overlay === ""
                && insideNotice(host.Window.window.activeFocusItem), "a covered shell shortcut escaped the notice")
            modifiers = Qt.NoModifier
            key = blocked[phase - 2]
            phase++
            return 0
        }
        if (phase === 8) {
            check(AppController.route === "home" && AppController.overlay === ""
                && insideNotice(host.Window.window.activeFocusItem), "a covered shell shortcut escaped the notice")
            key = Qt.Key_Tab
            phase++
            return 0
        }
        const turnOff = namedChild(host, "bugReportNoticeTurnOff")
        const keep = namedChild(host, "bugReportNoticeKeep")
        if (phase === 9) {
            console.log("NOTICE TAB focus", host.Window.window.activeFocusItem.objectName)
            check(turnOff.activeFocus, "Tab did not reach the opt-out action")
            key = Qt.Key_Tab
            phase++
            return 0
        }
        if (phase === 10) {
            check(keep.activeFocus, "Tab escaped the consent actions")
            key = Qt.Key_Backtab
            modifiers = Qt.ShiftModifier
            phase++
            return 0
        }
        if (phase === 11) {
            check(turnOff.activeFocus, "Backtab escaped the consent actions")
            modifiers = Qt.NoModifier
            key = Qt.Key_Right
            phase++
            return 0
        }
        if (phase === 12) {
            check(keep.activeFocus, "controller Right did not reach Keep")
            key = Qt.Key_Left
            phase++
            return 0
        }
        if (phase === 13) {
            check(turnOff.activeFocus, "controller Left did not reach Turn off")
            host.Window.window.width = 1600
            host.Window.window.height = 900
            phase++
            return 0
        }
        if (phase === 14) {
            app.restoreShellFocus()
            check(turnOff.activeFocus, "resize or shell restoration stole modal focus")
            key = Qt.Key_Return
            phase++
            return 0
        }
        if (phase === 15) {
            const write = client.requests.filter(item => item.method === "settings.set").pop()
            check(write && write.params.value === "disabled" && write.params.source === "first_run_sheet",
                "controller-equivalent opt-out did not persist")
            client.eventReceived("settings.changed", {key: "automaticBugReports", value: "disabled"})
            client.responseReceived(write.id, {key: "automaticBugReports", value: "disabled"})
            phase++
            return 0
        }
        if (phase === 16) {
            check(!namedChild(host, "desktopBugReportNotice").opened
                && !insideNotice(host.Window.window.activeFocusItem), "closing notice did not restore shell focus")
            console.log("NOTICE PASS bounded layout, shortcuts, Tab/Backtab, controller arrows, resize, opt-out, focus restoration")
            return 1
        }
        return -1
    }
}

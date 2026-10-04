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
        setChoice("disabled")
        check(!notice.opened && !reports.noticePending, "the notice stayed open after a choice")
        reports.reportStreamError("native_stream_error", "decoder lost")
        check(incidents().length === 0, "an opted-out user produced a report")

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

        reports.reportStreamError("Native Stream Error!", "Bearer secret")
        const stream = incidents().pop()
        check(stream.params.kind === "stream_error" && stream.params.code === "native_stream_error_",
            "stream error codes must be normalized for the core contract")

        client.eventReceived("bug_report.changed", {state: "sent", kind: "library_error",
            code: "network_error", reference: "br-7F3A21", game: "Portal 2"})
        check(reports.latest && reports.latest.reference === "br-7F3A21", "the sent event was not kept")
        return true
    }

    function verifyRendered(host) {
        const toast = namedChild(host, "desktopBugReportToast")
        check(toast !== null && toast.visible, "the report toast is not visible")
        check(findText(toast, "Bug reported to the developer") !== null, "the toast title is missing")
        check(findText(toast, "REF br-7F3A21 · Portal 2") !== null, "the toast reference is missing")
        return true
    }
}

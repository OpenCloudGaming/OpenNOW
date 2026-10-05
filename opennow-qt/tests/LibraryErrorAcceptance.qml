import QtQuick
import OpenNOW

QtObject {
    property string message: "Zain can only be reached from inside its home country and network. Turn off any VPN, proxy or custom DNS (such as 1.1.1.1 or 8.8.8.8) and try again. Details: Server info failed: error sending request (dns)"
    property QtObject client: QtObject {
        property string state: "ready"
        property string lastError: ""
        property int sequence: 0
        signal responseReceived(string id, var result)
        signal requestFailed(string id, string code, string message)
        signal eventReceived(string name, var payload)
        function markUiReady() {}
        function logShellDiagnostic(message) {}
        function request(method, params, timeout) { return "library-error-" + (++sequence) }
        function cancel(id) { return true }
    }
    function check(value, detail) {
        if (!value) throw new Error("Library error layout: " + detail)
    }
    function findText(item, value) {
        if (item.text === value) return item
        for (const child of item.children || []) {
            const found = findText(child, value)
            if (found) return found
        }
        return null
    }
    function run(host) {
        ShellStore.authRestorePending = false
        ShellStore.authSession = {user: {userId: "library-error-fixture"}, provider: {idpId: "library-error-provider"}}
        ShellStore.settings = Object.assign({}, ShellStore.settings, {automaticBugReports: "disabled", enableHdr: false})
        ShellStore.catalogGames = []
        ShellStore.catalogSource = "account-library"
        ShellStore.catalogRequestId = "library-error-request"
        AppController.navigate("library")
        client.requestFailed("library-error-request", "network_error", message)
        return true
    }
    function verifyRendered(host) {
        const text = findText(host, message)
        check(text && text.visible && text.width > 0 && text.height > 0, "the production error message is missing")
        const state = text.parent
        const panel = state.parent
        const title = findText(state, qsTr("Couldn’t reach the catalog"))
        const retry = findText(state, qsTr("Try again"))
        check(title && retry && retry.visible && retry.enabled, "the error title or retry action is missing")
        for (const item of [title, text, retry]) {
            const point = item.mapToItem(panel, 0, 0)
            check(point.x >= 0 && point.y >= 0
                && point.x + item.width <= panel.width && point.y + item.height <= panel.height,
                "error content overflows the library panel")
        }
        check(!text.truncated && text.contentWidth <= text.width + 1 && text.contentHeight <= text.height + 1,
            "the failure message is clipped instead of wrapped")
        check(ShellStore.catalogState === "error" && ShellStore.lastError === message,
            "the core failure did not reach the rendered error state")
        console.log("Library error layout passed after rendering")
        return true
    }
}

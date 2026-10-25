import QtQuick
import QtQuick.Window
import OpenNOW

QtObject {
    property var host: null
    property int phase: 0
    property int key: 0
    property int systemKey: 0
    property int modifiers: 0
    property bool settling: false
    property string accountState: ""
    property QtObject client: QtObject {
        property string state: "ready"
        property string lastError: ""
        property int sequence: 0
        signal responseReceived(string requestId, var result)
        signal requestFailed(string requestId, string code, string message)
        signal eventReceived(string name, var payload)
        function markUiReady() {}
        function logShellDiagnostic(message) {}
        function request(method, params, timeout) { return "stream-end-" + (++sequence) }
        function cancel(id) { return true }
    }
    onKeyChanged: if (key !== 0) settling = true
    onSystemKeyChanged: if (systemKey !== 0) settling = true

    function check(value, message) { if (!value) throw new Error("Stream end notice acceptance: " + message) }
    function namedChild(item, name) {
        if (item.objectName === name) return item
        for (const child of item.children || []) {
            const found = namedChild(child, name)
            if (found) return found
        }
        return null
    }
    function need(name) {
        const item = namedChild(host, name)
        check(item !== null, "missing " + name)
        return item
    }
    function inside(name) {
        const owner = need(name)
        for (let item = host.Window.window.activeFocusItem; item; item = item.parent) {
            if (item === owner) return true
        }
        return false
    }
    function accountSnapshot() {
        return JSON.stringify([AppController.route, AppController.overlay, ShellStore.accountSwitchRequestId,
            ShellStore.accountSwitchConfirmTarget, ShellStore.pendingAccountSwitch, ShellStore.accountMessage])
    }
    function press(value, keyModifiers) {
        modifiers = keyModifiers || Qt.NoModifier
        key = value
        phase++
        return 0
    }

    function run(host) {
        this.host = host
        ShellStore.authRestorePending = false
        ShellStore.authGeneration = 3
        ShellStore.authSession = {user: {userId: "notice-fixture", displayName: "Notice fixture"}}
        ShellStore.selectedGame = {title: "Portal 2"}
        AppController.navigate("game-detail")
        check(!need("desktopStreamEndNotice").opened, "a null notice opened the dialog")
        ShellStore.streamEndNotice = {kind: "error", code: 2147680265, codeText: "0x80030009"}
        return true
    }

    function advance() {
        if (settling) { settling = false; return 0 }
        if (phase === 16)
            check(accountSnapshot() === accountState, "Ctrl+2 switched accounts behind the notice")
        const notice = need("desktopStreamEndNotice")
        const dismiss = need("streamEndNoticeDismiss")
        const copy = need("streamEndNoticeCopy")
        if (phase === 0) {
            check(notice.opened, "an error notice did not open over game details")
            const card = need("streamEndNoticeCard")
            const corner = card.mapToItem(host, 0, 0)
            check(corner.x >= 0 && corner.y >= 0 && corner.x + card.width <= host.width
                && corner.y + card.height <= host.height, "the notice card exceeds the window")
            check(dismiss.activeFocus, "Dismiss did not take the initial focus")
            check(need("streamEndNoticeTitle").text === "The stream ended unexpectedly", "error headline is wrong")
            check(need("streamEndNoticeBody").text.indexOf("Portal 2") >= 0, "the body does not name the game")
            check(need("streamEndNoticeCodeRow").visible
                && need("streamEndNoticeCode").text === "Error code: 0x80030009", "the error code row is missing")
            check(!need("streamEndNoticeQuietCode").visible, "an error showed the quiet code line")
            need("desktopGameModal").forceActiveFocus()
            phase = 1
            return 0
        }
        if (phase === 1) {
            check(inside("desktopStreamEndNotice"), "game details kept focus behind the notice")
            return press(Qt.Key_K, Qt.ControlModifier)
        }
        if (phase === 2) {
            check(!need("desktopApp").commandOpen && inside("desktopStreamEndNotice"), "Ctrl+K escaped the notice")
            return press(Qt.Key_Up)
        }
        if (phase === 3) {
            check(copy.activeFocus, "controller Up did not reach Copy")
            AppController.writeClipboardText("before")
            return press(Qt.Key_Return)
        }
        if (phase === 4) {
            check(AppController.readClipboardText() === "0x80030009", "Copy did not put the code on the clipboard")
            check(copy.text === "Copied", "Copy gave no feedback")
            check(ShellStore.streamEndNotice !== null && notice.opened, "Copy dismissed the notice")
            return press(Qt.Key_Tab)
        }
        if (phase === 5) {
            check(dismiss.activeFocus, "Tab did not return to Dismiss")
            return press(Qt.Key_Escape)
        }
        if (phase === 6) {
            check(ShellStore.streamEndNotice === null && !notice.opened, "Escape did not dismiss the notice")
            check(AppController.route === "game-detail", "Escape also closed game details")
            check(inside("desktopGameModal"), "dismissing did not return focus to game details")
            ShellStore.streamEndNotice = {kind: "idle", code: 2147680263, codeText: "0x80030007"}
            phase = 7
            return 0
        }
        if (phase === 7) {
            check(notice.opened && dismiss.activeFocus, "the idle notice did not open with focus")
            check(need("streamEndNoticeTitle").text === "Closed after inactivity", "idle headline is wrong")
            check(!need("streamEndNoticeCodeRow").visible, "idle showed the copyable error code")
            check(need("streamEndNoticeQuietCode").visible
                && need("streamEndNoticeQuietCode").text === "Code 0x80030007", "idle lost its quiet code")
            check(copy.text === "Copy", "Copied feedback leaked into a new notice")
            check(dismiss.text === "Got it", "idle primary action is wrong")
            return press(Qt.Key_Up)
        }
        if (phase === 8) {
            check(dismiss.activeFocus, "Up focused a hidden Copy button")
            return press(Qt.Key_Return)
        }
        if (phase === 9) {
            check(ShellStore.streamEndNotice === null, "controller A did not dismiss the idle notice")
            ShellStore.selectedGame = {}
            ShellStore.streamEndNotice = {kind: "time-limit", code: 2147680268, codeText: "0x8003000C"}
            phase = 10
            return 0
        }
        if (phase === 10) {
            check(need("streamEndNoticeTitle").text === "Session time limit reached", "time-limit headline is wrong")
            check(need("streamEndNoticeBody").text.indexOf("Your game ran") === 0, "time-limit has no game fallback")
            return press(Qt.Key_Back)
        }
        if (phase === 11) {
            check(ShellStore.streamEndNotice === null, "controller Back did not dismiss")
            ShellStore.streamEndNotice = {kind: "error", code: null, codeText: ""}
            phase = 12
            return 0
        }
        if (phase === 12) {
            check(notice.opened && !need("streamEndNoticeCodeRow").visible && !copy.visible,
                "an empty code still showed the code row")
            check(need("streamEndNoticeBody").text === "The cloud server closed your session. You can start it again from here.",
                "the error has no game fallback")
            return press(Qt.Key_Tab)
        }
        if (phase === 13) {
            check(dismiss.activeFocus, "Tab reached a hidden Copy button")
            dismiss.clicked()
            phase = 14
            return 0
        }
        if (phase === 14) {
            check(ShellStore.streamEndNotice === null && !notice.opened, "clicking Dismiss did not dismiss")
            ShellStore.savedAccounts = [
                {userId: "notice-fixture", displayName: "Notice fixture"},
                {userId: "second-fixture", displayName: "Second fixture", hasPin: true}]
            ShellStore.streamEndNotice = {kind: "error", code: 2147680265, codeText: "0x80030009"}
            phase = 15
            return 0
        }
        if (phase === 15) {
            check(notice.opened && dismiss.activeFocus, "the notice did not reopen with focus")
            accountState = accountSnapshot()
            modifiers = Qt.ControlModifier
            systemKey = Qt.Key_2
            phase = 16
            return 0
        }
        if (phase === 16) {
            check(inside("desktopStreamEndNotice"), "Ctrl+2 moved focus behind the notice")
            AppController.showOverlay("application-quit-confirm")
            phase = 17
            return 0
        }
        if (phase === 17) {
            check(inside("applicationQuitConfirmation"), "the notice took focus from the quit confirmation")
            return press(Qt.Key_Escape)
        }
        if (phase === 18) {
            check(AppController.overlay === "", "Escape did not cancel the quit confirmation")
            check(ShellStore.streamEndNotice !== null && notice.opened, "Escape on the quit confirmation dismissed the notice")
            check(dismiss.activeFocus, "the notice did not take focus back after the quit confirmation closed")
            console.log("STREAM END NOTICE PASS focus, Ctrl+K, copy, Tab, Escape, controller A/Back, fallbacks, empty code, account shortcuts, quit confirmation")
            return 1
        }
        return -1
    }
}

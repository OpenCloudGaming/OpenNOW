import QtQuick
import QtQuick.Window
import OpenNOW

QtObject {
    property var host: null
    property int phase: 0
    property int key: 0
    property int modifiers: 0
    property bool settling: false
    property int waited: 0
    onKeyChanged: if (key !== 0) settling = true

    function check(value, message) { if (!value) throw new Error("Console stream end notice acceptance: " + message) }
    function namedChild(item, name) {
        if (!item) return null
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
    function window() { return host.Window.window }
    function inside(name) {
        const owner = need(name)
        for (let item = window().activeFocusItem; item; item = item.parent) {
            if (item === owner) return true
        }
        return false
    }
    function openNotices() {
        const desktop = namedChild(host, "desktopStreamEndNotice")
        return (desktop && desktop.opened ? 1 : 0) + (need("consoleStreamEndNotice").opened ? 1 : 0)
    }
    function press(value) {
        modifiers = Qt.NoModifier
        key = value
        phase++
        return 0
    }
    function waitFor(condition, message) {
        if (condition) { waited = 0; return true }
        check(++waited < 120, message)
        return false
    }

    function run(host) {
        this.host = host
        ShellStore.authRestorePending = false
        ShellStore.authGeneration = 3
        ShellStore.authSession = {user: {userId: "console-notice", displayName: "Console notice"}}
        ShellStore.selectedGame = {title: "<b>Portal 2 & Friends</b> <i>Co-op</i>"}
        AppController.navigate("game-detail")
        check(!window().desktopSurfaceActive, "the fixture did not start on the console surface")
        check(!need("consoleStreamEndNotice").opened, "a null notice opened the console dialog")
        ShellStore.streamEndNotice = {kind: "error", code: 2147680265, codeText: "0x80030009"}
        return true
    }

    function advance() {
        if (settling) { settling = false; return 0 }
        const notice = need("consoleStreamEndNotice")
        const dismiss = need("consoleStreamEndNoticeDismiss")
        const copy = need("consoleStreamEndNoticeCopy")
        if (phase === 0) {
            if (!waitFor(notice.opened && dismiss.activeFocus, "the console notice did not open with Dismiss focused"))
                return 0
            check(openNotices() === 1, "more than one stream-end notice is open")
            check(window().consoleModalFocusOwner === notice, "the console notice does not own modal input")
            check(!window().accountShortcutsEnabled, "account shortcuts stay live behind the console notice")
            const card = need("consoleStreamEndNoticeCard")
            const corner = card.mapToItem(host, 0, 0)
            check(corner.x >= 0 && corner.y >= 0, "the console card starts outside the window")
            check(need("consoleStreamEndNoticeTitle").text === "The stream ended unexpectedly", "error headline is wrong")
            check(need("consoleStreamEndNoticeBody").text.indexOf("Portal 2") >= 0, "the body does not name the game")
            check(need("consoleStreamEndNoticeBody").textFormat === Text.PlainText, "the console body interprets an untrusted title as markup")
            check(need("consoleStreamEndNoticeBody").text.indexOf(ShellStore.selectedGame.title) >= 0,
                "the console body changed the literal title")
            check(need("consoleStreamEndNoticeCodeRow").visible
                && need("consoleStreamEndNoticeCode").text === "Error code: 0x80030009", "the error code row is missing")
            return press(Qt.Key_Up)
        }
        if (phase === 1) {
            check(copy.activeFocus, "D-pad Up did not reach Copy")
            AppController.writeClipboardText("before")
            return press(Qt.Key_Return)
        }
        if (phase === 2) {
            check(AppController.readClipboardText() === "0x80030009", "A on Copy did not copy the code")
            check(copy.text === "Copied", "Copy gave no feedback")
            check(ShellStore.streamEndNotice !== null && notice.opened, "A on Copy dismissed the notice")
            return press(Qt.Key_Down)
        }
        if (phase === 3) {
            check(dismiss.activeFocus, "D-pad Down did not return to Dismiss")
            return press(Qt.Key_Return)
        }
        if (phase === 4) {
            check(ShellStore.streamEndNotice === null, "A on Dismiss did not dismiss the notice")
            if (!waitFor(!inside("consoleStreamEndNotice") && window().consoleModalFocusOwner === null,
                    "focus stayed in the closed console notice"))
                return 0
            ShellStore.streamEndNotice = {kind: "idle", code: 2147680263, codeText: "0x80030007"}
            phase = 5
            return 0
        }
        if (phase === 5) {
            if (!waitFor(notice.opened && dismiss.activeFocus, "the idle notice did not open with focus"))
                return 0
            check(need("consoleStreamEndNoticeTitle").text === "Closed after inactivity", "idle headline is wrong")
            check(!need("consoleStreamEndNoticeCodeRow").visible, "idle showed the copyable code")
            check(need("consoleStreamEndNoticeQuietCode").visible
                && need("consoleStreamEndNoticeQuietCode").text === "Code 0x80030007", "idle lost its quiet code")
            return press(Qt.Key_Escape)
        }
        if (phase === 6) {
            check(ShellStore.streamEndNotice === null, "B did not dismiss the idle notice")
            check(AppController.route === "game-detail", "B also left game details")
            ShellStore.selectedGame = {}
            ShellStore.streamEndNotice = {kind: "error", code: null, codeText: ""}
            phase = 7
            return 0
        }
        if (phase === 7) {
            if (!waitFor(notice.opened && dismiss.activeFocus, "the empty-code notice did not open"))
                return 0
            check(!need("consoleStreamEndNoticeCodeRow").visible && !copy.visible, "an empty code still showed the code row")
            check(need("consoleStreamEndNoticeBody").text
                === "The cloud server closed your session. You can start it again from here.", "no game fallback")
            AppController.writeClipboardText("untouched")
            return press(Qt.Key_X)
        }
        if (phase === 8) {
            check(AppController.readClipboardText() === "untouched", "X copied an empty code")
            check(notice.opened && dismiss.activeFocus, "X moved focus or closed the empty-code notice")
            return press(Qt.Key_Back)
        }
        if (phase === 9) {
            check(ShellStore.streamEndNotice === null, "controller Back did not dismiss")
            ShellStore.streamEndNotice = {kind: "time-limit", code: 2147680268, codeText: "0x8003000C"}
            phase = 10
            return 0
        }
        if (phase === 10) {
            if (!waitFor(notice.opened, "the time-limit notice did not open on console"))
                return 0
            window().applyConsoleSurface(false)
            phase = 11
            return 0
        }
        if (phase === 11) {
            const desktop = namedChild(host, "desktopStreamEndNotice")
            if (!waitFor(window().desktopSurfaceActive && desktop && desktop.opened,
                    "switching to desktop did not show the desktop notice"))
                return 0
            check(!notice.opened && openNotices() === 1, "switching to desktop left two notices open")
            window().applyConsoleSurface(true)
            phase = 12
            return 0
        }
        if (phase === 12) {
            if (!waitFor(!window().desktopSurfaceActive && notice.opened, "switching back to console did not show the console notice"))
                return 0
            check(openNotices() === 1, "switching to console left two notices open")
            check(need("consoleStreamEndNoticeTitle").text === "Session time limit reached", "the switch lost the notice")
            if (!waitFor(dismiss.activeFocus, "the console notice did not regain focus after the switch"))
                return 0
            return press(Qt.Key_Escape)
        }
        if (phase === 13) {
            check(ShellStore.streamEndNotice === null && openNotices() === 0, "B after the switch did not dismiss")
            console.log("CONSOLE STREAM END NOTICE PASS open, A/B/X/Back, copy, fallbacks, empty code, desktop/console switch")
            return 1
        }
        return -1
    }
}

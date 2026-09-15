import QtQuick
import OpenNOW

QtObject {
    property var surface: null
    property var initialFocus: null
    property bool initialInput: false
    property string initialOverlay: ""
    property string initialRoute: ""
    readonly property bool menuOpen: Qt.application.arguments.indexOf("--smoke-color-format-overlay") >= 0

    function check(ok, message) {
        if (!ok) throw new Error("Color format acceptance: " + message)
    }

    function find(item, name) {
        if (item.objectName === name) return item
        for (const child of item.children || []) {
            const result = find(child, name)
            if (result) return result
        }
        return null
    }

    function decoder(extra) {
        ShellStore.acceptNativeEvent(Object.assign({type: "log", event: "color-format-changed",
            requestedColorQuality: "10bit_444", actualColorQuality: "10bit_420",
            source: "decoder", sessionId: String((ShellStore.activeSession || {}).sessionId || "")}, extra || {}))
    }

    function session(id, source) {
        ShellStore.activeSession = {sessionId: id, phase: "ready", status: 2,
            negotiatedStreamProfile: {colorQuality: "10bit_420", bitDepth: 10, chromaFormat: 0,
                bitDepthSource: "request", chromaFormatSource: source || "request"}}
        ShellStore.streamer = {status: "streaming", sessionId: id, firstFrameLatencyMs: 1}
        ShellStore.streamerStopExpected = false
        ShellStore.streamStopRequestId = ""
    }

    function firstFrame() {
        ShellStore.acceptNativeEvent({type: "status", event: "first-frame", status: "streaming"})
    }

    function run(parent) {
        ShellStore.streamerStartRequestId = "fixture-blocked"
        ShellStore.streamInputPauseRequestId = "fixture-blocked"
        session("validation")
        for (const value of [null, "", "12bit_420", "10bit_422", {}, 10]) {
            decoder({actualColorQuality: value})
            decoder({requestedColorQuality: value})
        }
        decoder({source: "server"})
        decoder({type: "status"})
        decoder({sessionId: "stale"})
        decoder({sessionId: ""})
        decoder({sessionId: null})
        decoder({sessionId: undefined})
        check(ShellStore.streamColorFormat === null, "invalid and stale observations are ignored")
        ShellStore.streamerStopExpected = true
        decoder()
        check(ShellStore.streamColorNotice === null, "stopping suppresses notices")
        ShellStore.streamerStopExpected = false
        decoder({actualColorQuality: "10bit_444"})
        check(ShellStore.streamColorFormat.actualColorQuality === "10bit_444"
            && ShellStore.streamColorNotice === null, "matching formats are retained without a notice")
        ShellStore.streamRequestedColorQuality = "10bit_444"
        firstFrame()
        check(ShellStore.streamColorNotice === null, "request metadata is not server evidence")
        const settings = ShellStore.settings
        ShellStore.settings = Object.assign({}, settings, {colorQuality: "8bit_420"})
        decoder({requestedColorQuality: "10bit_420"})
        check(ShellStore.streamColorNotice.source === "decoder"
            && ShellStore.streamColorNotice.requestedColorQuality === "10bit_444"
            && ShellStore.streamColorNotice.actualColorQuality === "10bit_420",
            "output compares captured user intent, not accepted runtime color or mutable preferences")
        ShellStore.settings = settings
        session("resumed", "finalized")
        firstFrame()
        check(ShellStore.streamColorNotice === null, "resumed sessions do not guess original intent")
        session("server", "finalized")
        ShellStore.streamRequestedColorQuality = "10bit_444"
        firstFrame()
        check(ShellStore.streamColorNotice.source === "server", "finalized chroma proves server fallback")
        const notice = ShellStore.streamColorNotice
        decoder()
        firstFrame()
        check(ShellStore.streamColorNotice === notice, "decoder and recovery events do not replace a notice")
        check(ShellStore.streamColorFormat.source === "decoder", "latest actual output remains available")
        decoder({actualColorQuality: "10bit_444"})
        check(ShellStore.streamColorFormat.actualColorQuality === "10bit_444"
            && ShellStore.streamColorNotice === notice, "output recovery updates statistics without another popup")
        ShellStore.activeSession = null
        decoder()
        check(ShellStore.streamColorNotice === null && ShellStore.streamColorFormat === null,
            "session end clears state and ignores late events")
        session("visible-color-format", "finalized")
        if (menuOpen) AppController.showOverlay("desktop-stream-menu")
        return true
    }

    function notify(parent) {
        surface = find(parent, "streamSurfaceHost")
        check(surface && surface.visible, "stream surface is alive before the notice")
        initialFocus = parent.Window.window.activeFocusItem
        initialInput = surface.inputEnabled
        initialOverlay = AppController.overlay
        initialRoute = AppController.route
        if (Qt.application.arguments.indexOf("--smoke-color-format-server") >= 0) {
            ShellStore.streamRequestedColorQuality = "10bit_444"
            firstFrame()
        } else {
            ShellStore.activeSession = Object.assign({}, ShellStore.activeSession,
                {negotiatedStreamProfile: {colorQuality: "10bit_444"}})
            decoder()
            const stats = find(parent, "desktopStreamStats")
            check(stats && stats.videoText.indexOf("10-bit 4:2:0") >= 0,
                "statistics show decoded output rather than requested chroma")
            check(ShellStore.activeSession.negotiatedStreamProfile.colorQuality === "10bit_444",
                "decoder observations do not mutate server negotiation")
            const observed = ShellStore.streamColorFormat
            ShellStore.streamColorFormat = Object.assign({}, observed, {sessionId: "stale"})
            check(stats.videoText.indexOf("10-bit 4:4:4") >= 0, "statistics ignore stale output formats")
            ShellStore.streamColorFormat = observed
        }
        return true
    }

    function verifyRendered(parent) {
        const toast = find(parent, "streamColorFormatToast")
        if (menuOpen) {
            check(toast && !toast.visible && ShellStore.streamColorNotice !== null
                && !ShellStore.streamColorNoticeShown, "notice waits behind the existing stream menu")
            check(initialOverlay === "desktop-stream-menu" && !initialInput,
                "the menu owns input before the notice")
        } else {
            check(toast && toast.visible && toast.width > 0 && toast.height > 0, "format toast is rendered")
        }
        check(find(parent, "streamSurfaceHost") === surface && surface.visible,
            "the same video surface stays visible")
        check(!toast.activeFocus && parent.Window.window.activeFocusItem === initialFocus,
            "notice does not take keyboard focus")
        check(surface.inputEnabled === initialInput && AppController.overlay === initialOverlay
            && AppController.route === initialRoute, "notice does not change gameplay routing")
        check(ShellStore.streamer.status === "streaming", "media remains streaming")
        return true
    }
}

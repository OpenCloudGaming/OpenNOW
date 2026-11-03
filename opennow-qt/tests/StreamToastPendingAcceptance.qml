import QtQuick
import OpenNOW

QtObject {
    id: fixture
    property int key: 0
    property int modifiers: 0
    property int tick: 0
    property bool released: false
    property Item parent: null
    property Item video: null
    property var window: null
    property int hiddenTicks: 220
    property int lossFeedTicks: 10
    property int resumeTicks: 10
    property int releaseTick: 0
    property real heldColorLifetime: -1
    property bool overflowQueue: Qt.application.arguments.indexOf("--toast-overflow-queue") >= 0
    property bool sawQueuedHdr: false
    property bool sawPresentedHdr: false
    property bool sawPresentedLoss: false
    property int presentationWaitTick: -1

    property QtObject settingsClient: QtObject {
        property int serial: 0
        function request(method, params, timeout) {
            return method === "settings.set" ? "stream-toast-pending-setting-" + (++serial) : ""
        }
        function cancel(id) {}
    }

    function require(condition, message) {
        if (!condition) throw new Error("Stream toast pending: " + message)
    }

    function find(item, name) {
        if (!item) return null
        if (item.objectName === name) return item
        for (const child of item.children || []) {
            const result = find(child, name)
            if (result) return result
        }
        return null
    }

    function column() {
        const item = find(window.contentItem, "desktopStreamToasts")
        require(item !== null, "the stream toast column is in the tree")
        return item
    }

    function shown() {
        return ShellStore.settings.hdrNoticeShownGameIds || []
    }

    function health() {
        return ShellStore.connectionHealth
    }

    function feedLoss() {
        ShellStore.acceptNativeEvent({type: "telemetry", sessionId: "stream-toast-pending",
            packetLossPercent: 1.5})
    }

    function run(parent) {
        window = parent.Window.window
        fixture.parent = parent
        ShellStore.streamerStartRequestId = "fixture-blocked"
        ShellStore.streamInputPauseRequestId = "fixture-blocked"
        const owner = ShellStore.settingsOwnerState
        ShellStore.settings = Object.assign({}, ShellStore.settings,
            {enableHdr: true, hdrNoticeShownGameIds: []})
        owner.confirmedSettings = Object.assign({}, owner.settings)
        owner.coreClient = settingsClient
        owner.ready = true
        const title = "Hollow Knight: Silksong — Collector's Edition Deluxe Bundle (Pre-Order)"
        ShellStore.selectedGame = {id: "hollow-knight-silksong", title: title, imageUrl: ""}
        ShellStore.activeSession = {sessionId: "stream-toast-pending", appId: "9001",
            catalogAppId: "hollow-knight-silksong", phase: "ready", status: 2,
            hdrDowngrade: "game-unsupported",
            negotiatedStreamProfile: {colorQuality: "8bit_420", bitDepth: 8, chromaFormat: 0,
                enableHdr: false, enableHdrSource: "request"}}
        ShellStore.streamer = {status: "streaming", sessionId: "stream-toast-pending"}
        ShellStore.streamState = "streaming"
        ShellStore.streamerStopExpected = false
        ShellStore.streamStopRequestId = ""
        ShellStore.streamColorNotice = null
        ShellStore.streamColorNoticeShown = false
        video = find(parent, "streamSurfaceHost")
        require(video && video.visible, "the stream surface is alive")
        ShellStore.streamColorNotice = {sessionId: "stream-toast-pending", source: "server",
            actualColorQuality: "8bit_420", requestedColorQuality: "10bit_444",
            receivedAtMs: Date.now()}
        require(ShellStore.streamColorNoticeShown === true,
            "the colour notice fits on its own and is presented first")
        ShellStore.acceptNativeEvent({type: "status", event: "first-frame", status: "streaming"})
        if (!overflowQueue) column().maximumHeight = 0
        return true
    }

    function assertPending() {
        const toasts = column()
        if (tick % 20 === 1)
            console.log("PENDING_STATE tick=" + tick + " visible=" + toasts.visible
                + " pending=" + toasts.pendingNotices + " stack=" + toasts.stackHeight
                + " hdr=" + toasts.hdrNotice + " hdrLife=" + toasts.hdrLifetime
                + " colorLife=" + toasts.colorLifetime
                + " loss=" + toasts.lossNotice + " lossLife=" + toasts.lossLifetime
                + " lossStatus=" + health().status + " lastNoticeAt=" + health().lastNoticeAt
                + " y=" + toasts.y + " shown=" + JSON.stringify(shown()))
        require(toasts.visible === false, "a stack that cannot fit stays hidden")
        require(toasts.hdrNotice === true, "the HDR notice is still queued")
        require(toasts.hdrLifetime === 0, "the HDR display lifetime has not started")
        require(shown().length === 0, "the game is not recorded as shown while it is unseen")
        require(health().lastNoticeAt === null,
            "the loss claim is not consumed while the stack is hidden")
        require(toasts.lossNotice === false, "the loss notice is not raised while the stack is hidden")
        require(toasts.lossLifetime === 0, "the loss display lifetime has not started")
        if (heldColorLifetime < 0) heldColorLifetime = toasts.colorLifetime
        require(Math.abs(toasts.colorLifetime - heldColorLifetime) < 0.001,
            "the interrupted notice's lifetime does not advance while the column is hidden")
        if (tick > lossFeedTicks * 10)
            require(health().status === "unstable",
                "the loss is sustained, so a claim would be taken if the stack were shown")
    }

    function assertPresented() {
        const toasts = column()
        require(toasts.visible === true, "the stack shows once the overlay frees the space")
        const toast = find(window.contentItem, "streamHdrDowngradeToast")
        require(toast !== null && toast.visible, "the HDR notice is presented")
        require(shown().length === 1 && shown()[0] === "hollow-knight-silksong",
            "the game is recorded once, after it is presented")
        require(health().lastNoticeAt !== null,
            "the loss claim is taken once the stack is shown")
        const lossToast = find(window.contentItem, "streamPacketLossToast")
        require(lossToast !== null && lossToast.visible, "the loss notice is presented")
    }

    function resumed() {
        return column().colorLifetime < heldColorLifetime
    }

    function advance() {
        ++tick
        if (overflowQueue) return advanceOverflow()
        if (!released) {
            assertPending()
            if (tick % lossFeedTicks === 1) feedLoss()
            if (tick < hiddenTicks) return 0
            assertPending()
            require(AppController.showOverlay("") === true, "the stats overlay closes")
            const toasts = column()
            toasts.maximumHeight = Qt.binding(() => Math.max(0, toasts.parent.height - toasts.y - 24))
            feedLoss()
            released = true
            releaseTick = tick
            return 0
        }
        if (resumed()) {
            assertPresented()
            return 1
        }
        require(tick - releaseTick < resumeTicks,
            "the interrupted colour notice resumes once the space returns")
        return 0
    }

    function advanceOverflow() {
        const toasts = column()
        const color = find(window.contentItem, "streamColorFormatToast")
        const hdr = find(window.contentItem, "streamHdrDowngradeToast")
        const loss = find(window.contentItem, "streamPacketLossToast")
        if (tick % lossFeedTicks === 1) feedLoss()
        require(toasts.y + toasts.height <= toasts.parent.height - 24,
            "the visible subset stays within the available stream space")
        if (toasts.colorNotice) {
            require(color.visible, "the first fitting toast remains visible when another notice is queued")
            require(!hdr.visible && toasts.hdrLifetime === 0 && shown().length === 0,
                "a queued HDR notice is neither timed nor recorded as shown")
            sawQueuedHdr = true
        } else if (toasts.hdrNotice) {
            require(hdr.visible,
                "the queued HDR notice is presented after the first toast expires without resizing")
            if (shown().length === 0) return waitForPresentation()
            require(shown().length === 1, "the presented HDR notice is recorded exactly once")
            presentationWaitTick = -1
            sawPresentedHdr = true
        } else if (toasts.lossNotice) {
            require(loss.visible, "the queued packet-loss notice fits after the preceding notices expire")
            if (health().lastNoticeAt === null) return waitForPresentation()
            presentationWaitTick = -1
            sawPresentedLoss = true
        } else if (sawQueuedHdr && sawPresentedHdr && sawPresentedLoss) {
            require(video.visible && ShellStore.activeSession.sessionId === "stream-toast-pending",
                "draining notices preserves the video surface and session")
            return 1
        }
        require(tick < 560, "the bounded toast queue drains without changing the window or overlay")
        return 0
    }

    function waitForPresentation() {
        if (presentationWaitTick < 0) presentationWaitTick = tick
        require(tick - presentationWaitTick < resumeTicks,
            "the visible notice is claimed after its visibility binding settles")
        return 0
    }
}

import QtQuick
import OpenNOW

Column {
    id: root
    objectName: "desktopStreamToasts"
    property bool active: visible && telemetry.status === "streaming"
    property bool connectionNotificationsEnabled: true
    property var controllers: []
    property var telemetry: ShellStore.streamer || ({})
    property string sessionId: String((ShellStore.activeSession || {}).sessionId || "")
    property var knownControllerIds: []
    property var controllerNotice: null
    readonly property var health: ShellStore.connectionHealth
    readonly property var lossHistory: active ? health.history : []
    readonly property var lastLoss: health.lastLoss
    property bool lossNotice: false
    property bool decoderNotice: false
    property string decoderNoticeSessionId: ""
    property real decoderLifetime: 0
    property real controllerLifetime: 0
    property real lossLifetime: 0
    property var colorFormat: ShellStore.streamColorNotice
    property bool colorNotice: false
    property real colorLifetime: 0
    property var hdrDowngrade: ShellStore.streamHdrNotice
    property bool hdrNotice: false
    property real hdrLifetime: 0
    readonly property var bugReport: ShellStore.bugReports.latest
    property bool bugReportNotice: false
    property real bugReportLifetime: 0
    property bool antiAfkNotice: false
    property real antiAfkLifetime: 0
    width: Math.min(384, parent ? Math.max(0, parent.width - 48) : 384)
    spacing: 12
    property real maximumHeight: Infinity
    readonly property var pendingStack: [
        antiAfkNotice ? antiAfkToast : null,
        decoderNotice ? decoderToast : null,
        colorNotice ? colorToast : null,
        hdrNotice ? hdrToast : null,
        bugReportNotice && bugReport !== null ? bugReportToast : null,
        connectionNotificationsEnabled && controllerNotice !== null ? controllerToast : null,
        connectionNotificationsEnabled && lossNotice ? lossToast : null
    ].filter(notice => notice !== null)
    readonly property int pendingNotices: pendingStack.length
    readonly property var displayedStack: {
        const notices = []
        let usedHeight = 0
        for (const notice of pendingStack) {
            const nextHeight = usedHeight + (notices.length > 0 ? spacing : 0) + notice.height
            if (nextHeight > maximumHeight) continue
            notices.push(notice)
            usedHeight = nextHeight
        }
        return notices
    }
    readonly property real stackHeight: displayedStack.length === 0 ? 0
        : displayedStack.reduce((total, notice) => total + notice.height, 0)
            + spacing * (displayedStack.length - 1)

    function reset() {
        controllerAnimation.stop()
        lossAnimation.stop()
        colorAnimation.stop()
        colorNotice = false
        hdrAnimation.stop()
        hdrNotice = false
        bugReportAnimation.stop()
        bugReportNotice = false
        antiAfkAnimation.stop()
        antiAfkNotice = false
        controllerNotice = null
        lossNotice = false
        decoderAnimation.stop()
        decoderNotice = false
        knownControllerIds = controllers.map(controller => controller.instanceId)
    }

    function observeControllers() {
        if (!active || !connectionNotificationsEnabled) {
            knownControllerIds = controllers.map(controller => controller.instanceId)
            return
        }
        for (const controller of controllers) {
            if (knownControllerIds.indexOf(controller.instanceId) >= 0) continue
            controllerNotice = controller
            controllerAnimation.restart()
        }
        knownControllerIds = controllers.map(controller => controller.instanceId)
        if (controllerNotice) {
            const connected = controllers.find(controller => controller.instanceId === controllerNotice.instanceId)
            if (connected) controllerNotice = connected
            else {
                controllerAnimation.stop()
                controllerNotice = null
            }
        }
    }

    function observeTelemetry() {
        if (!active || !connectionNotificationsEnabled) return
        if (health.status !== "unstable") {
            lossAnimation.stop()
            lossNotice = false
            return
        }
        if (!visible || (!lossNotice && !health.canClaimNotice())) return
        lossNotice = true
        presentLossNotice()
    }

    function presentLossNotice() {
        if (!lossNotice || !lossToast.visible || lossAnimation.running) return
        if (!health.claimNotice()) {
            lossNotice = false
            return
        }
        lossAnimation.restart()
    }

    function observeDecoder() {
        if (!active || !visible || !health.decoderLimited || decoderNoticeSessionId === sessionId) return
        decoderNotice = true
        presentDecoderNotice()
    }

    function presentDecoderNotice() {
        if (!decoderToast.visible || decoderNoticeSessionId === sessionId) return
        decoderNoticeSessionId = sessionId
        decoderAnimation.restart()
    }

    Connections {
        target: root.health
        function onStatusChanged() { root.observeTelemetry() }
        function onSampleAccepted() { root.observeTelemetry() }
        function onDecoderLimitedChanged() { root.observeDecoder() }
    }

    onControllersChanged: observeControllers()
    function observeColorFormat() {
        if (!active || !colorFormat || colorFormat.sessionId !== sessionId
                || ShellStore.streamColorNoticeShown || ShellStore.streamerStopExpected) return
        colorNotice = true
        presentColorNotice()
    }

    function presentColorNotice() {
        if (!colorToast.visible || ShellStore.streamColorNoticeShown) return
        ShellStore.streamColorNoticeShown = true
        colorAnimation.restart()
    }

    function observeHdrDowngrade() {
        if (!active || !hdrDowngrade || hdrDowngrade.sessionId !== sessionId
                || ShellStore.streamerStopExpected || ShellStore.streamHdrNoticeShown) return
        hdrNotice = true
        presentHdrNotice()
    }

    function presentHdrNotice() {
        if (!hdrToast.visible || ShellStore.streamHdrNoticeShown) return
        if (!ShellStore.claimHdrNotice(hdrDowngrade)) {
            hdrNotice = false
            return
        }
        hdrAnimation.restart()
    }

    function colorLabel(value) {
        switch (value) {
        case "8bit_420": return qsTr("8-bit 4:2:0")
        case "8bit_444": return qsTr("8-bit 4:4:4")
        case "10bit_420": return qsTr("10-bit 4:2:0")
        case "10bit_444": return qsTr("10-bit 4:4:4")
        default: return ""
        }
    }

    onColorFormatChanged: observeColorFormat()
    onHdrDowngradeChanged: observeHdrDowngrade()
    Connections {
        target: ShellStore.bugReports
        function onGenerationChanged() {
            if (!root.active || !root.bugReport || root.bugReport.state !== "sent") return
            root.bugReportNotice = true
            bugReportAnimation.restart()
        }
    }
    Connections {
        target: ShellStore
        function onAntiAfkEnabledChanged() {
            if (!root.active || !ShellStore.activeSession) return
            root.antiAfkNotice = true
            antiAfkAnimation.restart()
        }
    }
    onActiveChanged: {
        reset()
        if (active) {
            observeTelemetry()
            observeDecoder()
            observeColorFormat()
            observeHdrDowngrade()
        }
    }
    function syncLifetimeRunning() {
        for (const [notice, animation] of [[controllerToast, controllerAnimation], [lossToast, lossAnimation],
                [decoderToast, decoderAnimation], [bugReportToast, bugReportAnimation],
                [antiAfkToast, antiAfkAnimation], [hdrToast, hdrAnimation], [colorToast, colorAnimation]]) {
            if (notice.visible) {
                if (animation.paused) animation.resume()
            } else if (animation.running && !animation.paused) {
                animation.pause()
            }
        }
    }
    onVisibleChanged: {
        syncLifetimeRunning()
        observeTelemetry()
        observeDecoder()
        presentColorNotice()
        presentHdrNotice()
        presentLossNotice()
    }
    onSessionIdChanged: reset()
    Component.onCompleted: {
        reset()
        if (active) {
            observeTelemetry()
            observeDecoder()
            observeColorFormat()
            observeHdrDowngrade()
        }
    }

    NumberAnimation {
        id: controllerAnimation
        target: root; property: "controllerLifetime"
        from: 1; to: 0; duration: 4000
        onFinished: root.controllerNotice = null
        onRunningChanged: root.syncLifetimeRunning()
    }
    NumberAnimation {
        id: lossAnimation
        target: root; property: "lossLifetime"
        from: 1; to: 0; duration: 4000
        onFinished: root.lossNotice = false
        onRunningChanged: root.syncLifetimeRunning()
    }
    NumberAnimation {
        id: decoderAnimation
        target: root; property: "decoderLifetime"
        from: 1; to: 0; duration: 8000
        onFinished: root.decoderNotice = false
        onRunningChanged: root.syncLifetimeRunning()
    }
    NumberAnimation {
        id: bugReportAnimation
        target: root; property: "bugReportLifetime"
        from: 1; to: 0; duration: 5000
        onFinished: root.bugReportNotice = false
        onRunningChanged: root.syncLifetimeRunning()
    }
    NumberAnimation {
        id: antiAfkAnimation
        target: root; property: "antiAfkLifetime"
        from: 1; to: 0; duration: 4000
        onFinished: root.antiAfkNotice = false
        onRunningChanged: root.syncLifetimeRunning()
    }
    NumberAnimation {
        id: hdrAnimation
        target: root; property: "hdrLifetime"
        from: 1; to: 0; duration: 4000
        onFinished: root.hdrNotice = false
        onRunningChanged: root.syncLifetimeRunning()
    }
    NumberAnimation {
        id: colorAnimation
        target: root; property: "colorLifetime"
        from: 1; to: 0; duration: 4000
        onFinished: root.colorNotice = false
        onRunningChanged: root.syncLifetimeRunning()
    }

    DesktopStreamToast {
        id: antiAfkToast
        objectName: "streamAntiAfkToast"
        width: root.width
        visible: root.active && root.displayedStack.indexOf(antiAfkToast) >= 0
        onVisibleChanged: root.syncLifetimeRunning()
        formatNotice: true
        title: ShellStore.antiAfkEnabled ? qsTr("Anti-AFK on") : qsTr("Anti-AFK off")
        subtitle: ShellStore.antiAfkEnabled
            ? qsTr("A short keypress every 4 minutes keeps the session from timing out.")
            : qsTr("GeForce NOW can end the session after a period of inactivity.")
        lifetimeFraction: root.antiAfkLifetime
    }

    DesktopStreamToast {
        id: decoderToast
        objectName: "streamDecoderLimitedToast"
        width: root.width
        visible: root.active && root.displayedStack.indexOf(decoderToast) >= 0 && !ShellStore.streamerStopExpected
        onVisibleChanged: {
            root.syncLifetimeRunning()
            root.presentDecoderNotice()
        }
        warning: true
        formatNotice: true
        title: qsTr("Decoder can't keep up")
        subtitle: qsTr("This device is dropping frames while decoding. Lower the resolution or frame rate in Settings.")
        lifetimeFraction: root.decoderLifetime
    }

    DesktopStreamToast {
        id: colorToast
        objectName: "streamColorFormatToast"
        width: root.width
        visible: root.active && root.displayedStack.indexOf(colorToast) >= 0 && !ShellStore.streamerStopExpected
        onVisibleChanged: {
            root.syncLifetimeRunning()
            root.presentColorNotice()
        }
        formatNotice: true
        title: qsTr("Stream color format changed")
        subtitle: !root.colorFormat ? "" : (root.colorFormat.source === "server"
            ? qsTr("The server negotiated %1 instead of %2.")
            : qsTr("Video output is %1 instead of %2."))
                .arg(root.colorLabel(root.colorFormat.actualColorQuality))
                .arg(root.colorLabel(root.colorFormat.requestedColorQuality))
        lifetimeFraction: root.colorLifetime
    }

    DesktopStreamToast {
        id: hdrToast
        objectName: "streamHdrDowngradeToast"
        width: root.width
        visible: root.active && root.displayedStack.indexOf(hdrToast) >= 0 && !ShellStore.streamerStopExpected
        onVisibleChanged: {
            root.syncLifetimeRunning()
            Qt.callLater(root.presentHdrNotice)
        }
        formatNotice: true
        title: qsTr("Streaming in SDR")
        subtitle: qsTr("%1 doesn't support HDR. Your HDR setting stays on.")
            .arg(root.hdrDowngrade && root.hdrDowngrade.title ? root.hdrDowngrade.title : qsTr("This game"))
        lifetimeFraction: root.hdrLifetime
    }

    DesktopStreamToast {
        id: bugReportToast
        objectName: "streamBugReportToast"
        width: root.width
        visible: root.active && root.displayedStack.indexOf(bugReportToast) >= 0
        onVisibleChanged: root.syncLifetimeRunning()
        formatNotice: true
        title: !root.bugReport ? "" : root.bugReport.kind === "frame_drops"
            ? qsTr("Frame drops reported to the developer")
            : qsTr("Problem reported to the developer")
        subtitle: qsTr("A report with logs was sent so it can be fixed.")
        lifetimeFraction: root.bugReportLifetime
    }

    DesktopStreamToast {
        id: controllerToast
        objectName: "streamControllerToast"
        width: root.width
        visible: root.active && root.displayedStack.indexOf(controllerToast) >= 0
        onVisibleChanged: root.syncLifetimeRunning()
        title: qsTr("Controller connected")
        subtitle: root.controllerNotice
            ? root.controllerNotice.name + " · " + qsTr("Player %1").arg(root.controllerNotice.slot) : ""
        controllerFamily: root.controllerNotice && (root.controllerNotice.family === "playstation"
            || root.controllerNotice.family === "xbox") ? root.controllerNotice.family : "controller"
        batteryPercent: {
            const controller = root.controllerNotice
            if (!controller || ["onBattery", "charging", "charged"].indexOf(controller.powerState) < 0
                    || controller.batteryPercent === null || controller.batteryPercent === undefined) return -1
            const percent = Number(controller.batteryPercent)
            return Number.isInteger(percent) && percent >= 0 && percent <= 100 ? percent : -1
        }
        lifetimeFraction: root.controllerLifetime
    }
    DesktopStreamToast {
        id: lossToast
        objectName: "streamPacketLossToast"
        width: root.width
        visible: root.active && root.displayedStack.indexOf(lossToast) >= 0
        onVisibleChanged: {
            root.syncLifetimeRunning()
            Qt.callLater(root.presentLossNotice)
        }
        warning: true
        title: qsTr("Connection unstable")
        subtitle: root.lastLoss === null ? "" : qsTr("Packet loss · %1%").arg(Number(root.lastLoss).toFixed(1))
        history: root.lossHistory
        lifetimeFraction: root.lossLifetime
    }
}

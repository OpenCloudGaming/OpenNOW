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
    property var lossHistory: []
    property var lastLoss: null
    property bool lossEpisode: false
    property bool lossNotice: false
    property real controllerLifetime: 0
    property real lossLifetime: 0
    property var colorFormat: ShellStore.streamColorNotice
    property bool colorNotice: false
    property real colorLifetime: 0
    width: Math.min(384, parent ? Math.max(0, parent.width - 48) : 384)
    spacing: 12

    function reset() {
        controllerAnimation.stop()
        lossAnimation.stop()
        colorAnimation.stop()
        colorNotice = false
        lossCooldown.stop()
        controllerNotice = null
        lossNotice = false
        lossEpisode = false
        lossHistory = []
        lastLoss = null
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
        const value = telemetry.packetLossPercent
        if (value === null || value === undefined || value === ""
                || !Number.isFinite(Number(value)) || Number(value) < 0 || Number(value) > 100) {
            lossHistory = []
            lastLoss = null
            lossAnimation.stop()
            lossNotice = false
            return
        }
        const loss = Number(value)
        if (loss !== lastLoss) {
            lossHistory = lossHistory.concat([loss]).slice(-12)
            lastLoss = loss
        }
        if (loss === 0) {
            lossEpisode = false
            lossAnimation.stop()
            lossNotice = false
            return
        }
        if (lossEpisode || lossCooldown.running) return
        lossEpisode = true
        lossNotice = true
        lossAnimation.restart()
        lossCooldown.start()
    }

    onControllersChanged: observeControllers()
    function observeColorFormat() {
        if (!active || !colorFormat || colorFormat.sessionId !== sessionId
                || ShellStore.streamColorNoticeShown || ShellStore.streamerStopExpected) return
        ShellStore.streamColorNoticeShown = true
        colorNotice = true
        colorAnimation.restart()
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
    onTelemetryChanged: observeTelemetry()
    onActiveChanged: {
        reset()
        if (active) {
            observeTelemetry()
            observeColorFormat()
        }
    }
    onSessionIdChanged: reset()
    Component.onCompleted: {
        reset()
        if (active) {
            observeTelemetry()
            observeColorFormat()
        }
    }

    NumberAnimation {
        id: controllerAnimation
        target: root; property: "controllerLifetime"
        from: 1; to: 0; duration: 4000
        onFinished: root.controllerNotice = null
    }
    NumberAnimation {
        id: lossAnimation
        target: root; property: "lossLifetime"
        from: 1; to: 0; duration: 4000
        onFinished: root.lossNotice = false
    }
    Timer { id: lossCooldown; interval: 30000 }
    NumberAnimation {
        id: colorAnimation
        target: root; property: "colorLifetime"
        from: 1; to: 0; duration: 4000
        onFinished: root.colorNotice = false
    }

    DesktopStreamToast {
        objectName: "streamColorFormatToast"
        width: root.width
        visible: root.active && root.colorNotice && !ShellStore.streamerStopExpected
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
        objectName: "streamControllerToast"
        width: root.width
        visible: root.active && root.connectionNotificationsEnabled && root.controllerNotice !== null
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
        objectName: "streamPacketLossToast"
        width: root.width
        visible: root.active && root.connectionNotificationsEnabled && root.lossNotice
        warning: true
        title: qsTr("Connection unstable")
        subtitle: root.lastLoss === null ? "" : qsTr("Packet loss · %1%").arg(Number(root.lastLoss).toFixed(1))
        history: root.lossHistory
        lifetimeFraction: root.lossLifetime
    }
}

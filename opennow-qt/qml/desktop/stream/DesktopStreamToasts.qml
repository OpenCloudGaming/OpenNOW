import QtQuick
import OpenNOW

Column {
    id: root
    objectName: "desktopStreamToasts"
    property bool active: visible && telemetry.status === "streaming"
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
    width: Math.min(384, parent ? Math.max(0, parent.width - 48) : 384)
    spacing: 12

    function reset() {
        controllerAnimation.stop()
        lossAnimation.stop()
        lossCooldown.stop()
        controllerNotice = null
        lossNotice = false
        lossEpisode = false
        lossHistory = []
        lastLoss = null
        knownControllerIds = controllers.map(controller => controller.instanceId)
    }

    function observeControllers() {
        if (!active) {
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
        if (!active) return
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
    onTelemetryChanged: observeTelemetry()
    onActiveChanged: {
        reset()
        if (active) observeTelemetry()
    }
    onSessionIdChanged: reset()
    Component.onCompleted: {
        reset()
        if (active) observeTelemetry()
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

    DesktopStreamToast {
        objectName: "streamControllerToast"
        width: root.width
        visible: root.active && root.controllerNotice !== null
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
        visible: root.active && root.lossNotice
        warning: true
        title: qsTr("Connection unstable")
        subtitle: root.lastLoss === null ? "" : qsTr("Packet loss · %1%").arg(Number(root.lastLoss).toFixed(1))
        history: root.lossHistory
        lifetimeFraction: root.lossLifetime
    }
}

import QtQuick
import QtQuick.Controls
import OpenNOW

FocusScope {
    id: root
    readonly property bool streamPointerLocked: streamVideo.enabled && streamVideo.inputEnabled && streamVideo.relativeMouse
    focus: true
    Accessible.role: Accessible.Pane
    Accessible.name: qsTr("Live session")

    signal stopRequested()
    property bool launchCovered: false

    Rectangle { anchors.fill: parent; color: "black"; z: -1 }

    readonly property var session: ShellStore.activeSession || ({})
    readonly property var profile: session.negotiatedStreamProfile || session.streamProfile || ({})
    readonly property var streamer: ShellStore.streamer || ({})
    readonly property string status: {
        if (ShellStore.streamState === "error")
            return "error"
        if (ShellStore.streamState === "reconnecting"
                || (ShellStore.streamerRestartAttempts > 0 && root.streamer.status !== "streaming"))
            return "reconnecting"
        return String(root.streamer.status || ShellStore.streamState || "starting")
    }
    readonly property bool streaming: root.status === "streaming"
    readonly property bool videoReady: streaming
        && root.streamer.firstFrameLatencyMs !== undefined
        && root.streamer.firstFrameLatencyMs !== null
    property var frameGenerationStats: streamVideo.frameGenerationStats || ({})
    property double clockNowMs: Date.now()
    readonly property int clockSeconds: ShellStore.streamStartedAtMs > 0
        ? Math.max(0, Math.floor((clockNowMs - ShellStore.streamStartedAtMs) / 1000)) : 0
    Timer {
        interval: 1000; repeat: true
        running: root.visible && root.streaming && ShellStore.settings.sessionCounterEnabled === true
        onTriggered: root.clockNowMs = Date.now()
    }
    Rectangle {
        anchors.right: parent.right; anchors.top: parent.top; anchors.margins: 24
        layer.enabled: HdrOutput.chromeRequired
        layer.effect: HdrChromeEffect {}
        width: sessionClock.implicitWidth + 24; height: 36; radius: 18; z: 4
        color: Qt.rgba(Theme.shell.r,Theme.shell.g,Theme.shell.b,0.85)
        visible: root.streaming && ShellStore.settings.sessionCounterEnabled === true
            && AppController.overlay.indexOf("stats") < 0
        Text {
            id: sessionClock; anchors.centerIn: parent
            text: Math.floor(root.clockSeconds / 3600) + ":" + String(Math.floor(root.clockSeconds / 60) % 60).padStart(2,"0") + ":" + String(root.clockSeconds % 60).padStart(2,"0")
            color: Theme.label; font.family: Theme.monoFont; font.pixelSize: 12
        }
    }
    function publishCaptureRect() {
        const window = root.Window.window
        if (!window)
            return
        const point = root.mapToItem(null, 0, 0)
        ShellStore.streamCaptureRect = Qt.rect(
            window.x + point.x, window.y + point.y, root.width, root.height)
    }
    function resynchronizeStreamInput() {
        root.publishCaptureRect()
        if (!root.visible || !root.streaming || root.launchCovered
                || ShellStore.streamOverlayBlocksGameplayInput(AppController.overlay))
            return
        streamVideo.forceActiveFocus()
        streamVideo.resynchronizeInput()
    }

    onXChanged: publishCaptureRect()
    onYChanged: publishCaptureRect()
    onWidthChanged: publishCaptureRect()
    onHeightChanged: publishCaptureRect()
    Component.onCompleted: publishCaptureRect()

    Connections {
        target: root.Window.window
        function onXChanged() { root.resynchronizeStreamInput() }
        function onYChanged() { root.resynchronizeStreamInput() }
        function onWidthChanged() { Qt.callLater(root.resynchronizeStreamInput) }
        function onHeightChanged() { Qt.callLater(root.resynchronizeStreamInput) }
        function onVisibilityChanged() { Qt.callLater(root.resynchronizeStreamInput) }
        function onActiveChanged() { Qt.callLater(root.resynchronizeStreamInput) }
    }

    StreamVideoItem {
        id: streamVideo
        objectName: "streamSurfaceHost"
        anchors.fill: parent
        visible: root.visible && root.streaming
        enabled: !root.launchCovered
        focus: visible && !root.launchCovered
        inputEnabled: visible
            && !ShellStore.streamOverlayBlocksGameplayInput(AppController.overlay)
        shortcutBindings: ShellStore.streamShortcutBindings()
        videoSize: Qt.size(Number(root.profile.width || 0), Number(root.profile.height || 0))
        frameGeneration: String(ShellStore.settings.frameGeneration || 'off') === '2x'
        z: 0
        onLocalShortcutRequested: action => ShellStore.applyStreamShortcutAction(action)
    }

    StreamInputNotice {
        layer.enabled: HdrOutput.chromeRequired
        layer.effect: HdrChromeEffect {}
        message: root.streaming && streamVideo.relativeMouse ? streamVideo.inputCaptureError : ""
        z: 3
    }

    StreamCaptureStatus {
        layer.enabled: HdrOutput.chromeRequired
        layer.effect: HdrChromeEffect {}
        z: 4
    }

    Connections {
        target: ShellStore
        function onPointerLockToggleRequested() {
            streamVideo.relativeMouse = !streamVideo.relativeMouse
        }
    }

    Rectangle {
        anchors.horizontalCenter: parent.horizontalCenter
        layer.enabled: HdrOutput.chromeRequired
        layer.effect: HdrChromeEffect {}
        anchors.bottom: parent.bottom
        anchors.bottomMargin: 24
        width: liveHints.implicitWidth + 28
        height: 32
        radius: 16
        visible: root.streaming && AppController.overlay === ""
        color: "#9904060A"
        border.width: 1
        border.color: "#24FFFFFF"
        z: 2

        Row {
            id: liveHints
            anchors.centerIn: parent
            spacing: 18
            DesktopKeyHint { keyText: "Ctrl G"; label: qsTr("Session") }
            DesktopKeyHint { keyText: "F3"; label: qsTr("Stats") }
            DesktopKeyHint { keyText: "F11"; label: qsTr("Fullscreen") }
        }
    }

    function restoreStreamFocus() {
        if (!root.visible || root.launchCovered
                || ShellStore.streamOverlayBlocksGameplayInput(AppController.overlay))
            return
        streamVideo.forceActiveFocus()
    }

    onVisibleChanged: {
        publishCaptureRect()
        if (visible)
            Qt.callLater(root.restoreStreamFocus)
    }
    onLaunchCoveredChanged: if (!launchCovered) Qt.callLater(root.resynchronizeStreamInput)

    Keys.onPressed: event => {
        if (event.isAutoRepeat)
            return
        if (!root.streaming
                && (event.key === Qt.Key_Escape || event.key === Qt.Key_Back)) {
            root.stopRequested()
            event.accepted = true
        }
    }
}

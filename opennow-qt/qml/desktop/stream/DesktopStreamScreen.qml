import QtQuick
import QtQuick.Controls
import OpenNOW

FocusScope {
    id: root
    readonly property bool streamPointerLocked: streamVideo.captureActive && streamVideo.relativeMouse
    focus: true
    Accessible.role: Accessible.Pane
    Accessible.name: qsTr("Live session")

    signal stopRequested()
    property bool launchCovered: false

    Rectangle { anchors.fill: parent; color: "black"; z: -1 }

    readonly property var session: ShellStore.activeSession || ({})
    readonly property string sessionId: String(session.sessionId || "")
    property string clipboardPasteMessage: ""
    onSessionIdChanged: {
        clipboardPasteMessage = ""
        if (clipboardPasteNotice) clipboardPasteNotice.stop()
    }
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
    readonly property bool videoSurfaceActive: root.streaming || (root.streamer.status === "connecting" && root.status !== "error")
    readonly property bool videoReady: streaming
        && root.streamer.firstFrameLatencyMs !== undefined
        && root.streamer.firstFrameLatencyMs !== null
    property var frameGenerationStats: streamVideo.frameGenerationStats || ({})
    property var swapStats: streamVideo.swapStats || ({})
    property double clockNowMs: Date.now()
    readonly property int clockSeconds: ShellStore.streamStartedAtMs > 0
        ? Math.max(0, Math.floor((clockNowMs - ShellStore.streamStartedAtMs) / 1000)) : 0
    Timer {
        interval: 1000; repeat: true
        running: root.visible && root.streaming
            && (ShellStore.settings.sessionCounterEnabled === true || ShellStore.antiAfkEnabled)
        onTriggered: root.clockNowMs = Date.now()
    }
    MotionProgress {
        id: antiAfkReveal
        shown: root.videoReady && ShellStore.antiAfkIndicatorShown(root.clockSeconds)
            && AppController.overlay.indexOf("stats") < 0
    }
    Rectangle {
        objectName: "desktopAntiAfkIndicator"
        anchors.left: parent.left; anchors.top: parent.top; anchors.margins: 24
        layer.enabled: HdrOutput.chromeRequired
        layer.effect: HdrChromeEffect {}
        width: antiAfkLabel.implicitWidth + 28; height: 36; radius: 18; z: 4
        color: Qt.rgba(Theme.shell.r,Theme.shell.g,Theme.shell.b,0.85)
        visible: antiAfkReveal.present
        opacity: antiAfkReveal.progress
        Accessible.role: Accessible.StaticText
        Accessible.name: qsTr("Anti-AFK is enabled")
        Row {
            id: antiAfkLabel; anchors.centerIn: parent; spacing: 8
            Rectangle { width: 8; height: 8; radius: 4; color: DesktopTokens.green; anchors.verticalCenter: parent.verticalCenter }
            Text { text: qsTr("ANTI-AFK ON"); color: Theme.label; font.family: Theme.monoFont; font.pixelSize: 12 }
        }
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
        const rect = root.mapToItem(null, 0, 0, root.width, root.height)
        ShellStore.streamCaptureRect = Qt.rect(
            Math.round(window.x + rect.x), Math.round(window.y + rect.y),
            Math.round(rect.width), Math.round(rect.height))
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
        visible: root.visible && root.videoSurfaceActive
        enabled: !root.launchCovered
        focus: root.streaming && visible && !root.launchCovered
        inputEnabled: root.streaming && visible
            && !ShellStore.streamOverlayBlocksGameplayInput(AppController.overlay)
        shortcutBindings: ShellStore.streamShortcutBindings()
        clipboardPaste: ShellStore.settings.clipboardPaste === true
        macPointerLockEnabled: ShellStore.settings.macPointerLockEnabled !== false
        onMacPointerLockEnabledChanged: {
            if (Qt.platform.os === "osx" && macPointerLockEnabled !== (ShellStore.settings.macPointerLockEnabled !== false))
                ShellStore.setSetting("macPointerLockEnabled", macPointerLockEnabled)
        }
        keyboardLayout: String((ShellStore.activeSession || {}).keyboardLayout || "en-US")
        videoSize: Qt.size(Number(root.profile.width || 0), Number(root.profile.height || 0))
        frameGeneration: String(ShellStore.settings.frameGeneration || 'off') === '2x'
        metalFxUpscaling: Qt.platform.os === "osx" && ShellStore.settings.upscaling === "metalfx"
        fsrUpscaling: Qt.platform.os !== "osx" && ShellStore.settings.upscaling === "fsr1"
        upscalingSharpness: Number(ShellStore.settings.upscalingSharpness ?? 10)
        upscalingDenoise: Number(ShellStore.settings.upscalingDenoise ?? 0)
        z: 0
        onLocalShortcutRequested: action => ShellStore.applyStreamShortcutAction(action)
        onClipboardPasteFailed: {
            root.clipboardPasteMessage = ""
            clipboardPasteNotice.restart()
        }
    }

    Timer {
        id: clipboardPasteNotice
        objectName: "streamClipboardPasteNoticeTimer"
        interval: 5000
        onTriggered: root.clipboardPasteMessage = ""
    }

    StreamIdleWarning {
        layer.enabled: HdrOutput.chromeRequired
        layer.effect: HdrChromeEffect {}
        z: 5
    }

    StreamInputNotice {
        layer.enabled: HdrOutput.chromeRequired
        layer.effect: HdrChromeEffect {}
        title: clipboardPasteNotice.running ? qsTr("Text input unavailable") : qsTr("Relative mouse input unavailable")
        message: !root.streaming ? "" : clipboardPasteNotice.running
            ? root.clipboardPasteMessage || qsTr("Clipboard paste failed. Use plain text up to 64 KiB and try again.")
            : streamVideo.relativeMouse ? streamVideo.inputCaptureError : ""
        z: 3
    }

    StreamPointerLockHint {
        layer.enabled: HdrOutput.chromeRequired
        layer.effect: HdrChromeEffect {}
        sessionId: root.sessionId
        streaming: root.streaming
        active: Qt.platform.os === "osx" && root.streaming && streamVideo.inputEnabled
            && !root.launchCovered && !clipboardPasteNotice.running
        pointerLockEnabled: streamVideo.macPointerLockEnabled
        onEnableRequested: {
            streamVideo.togglePointerLock()
            Qt.callLater(root.resynchronizeStreamInput)
        }
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
            if (root.visible && root.streaming)
                streamVideo.togglePointerLock()
        }
        function onTextInputErrorAnnounced(message) {
            root.clipboardPasteMessage = message
            clipboardPasteNotice.restart()
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

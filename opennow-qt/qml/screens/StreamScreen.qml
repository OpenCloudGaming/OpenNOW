import QtQuick
import QtQuick.Controls
import OpenNOW

FocusScope {
    id: root
    focus: true
    Accessible.role: Accessible.Pane
    Accessible.name: qsTr("Live session")
    readonly property var game: ShellStore.selectedGame || ({})
    readonly property var session: ShellStore.activeSession || ({})
    readonly property var profile: session.negotiatedStreamProfile || ({})
    readonly property var streamer: ShellStore.streamer || ({})
    readonly property string status: {
        if (ShellStore.streamState === "error")
            return "error"
        if (ShellStore.streamState === "reconnecting"
                || (ShellStore.streamerRestartAttempts > 0 && root.streamer.status !== "streaming"))
            return "reconnecting"
        return String(streamer.status || ShellStore.streamState || "starting")
    }
    readonly property bool streaming: status === "streaming"
    readonly property bool failed: status === "error"
    property double nowMs: Date.now()
    readonly property int elapsedSeconds: ShellStore.streamStartedAtMs > 0
        ? Math.max(0, Math.floor((nowMs - ShellStore.streamStartedAtMs) / 1000)) : 0
    readonly property int clockDuration: Math.max(1, Number(ShellStore.settings.sessionClockShowDurationSeconds || 30))
    readonly property int clockInterval: Math.max(0, Number(ShellStore.settings.sessionClockShowEveryMinutes || 0) * 60)
    readonly property bool sessionClockVisible: Boolean(ShellStore.settings.sessionCounterEnabled) && streaming
        && (elapsedSeconds < clockDuration
            || (clockInterval > 0 && elapsedSeconds % clockInterval < clockDuration))
    readonly property int antiAfkReminderDuration: Math.max(1, Number(ShellStore.settings.antiAfkReminderDurationSeconds || 5))
    readonly property int antiAfkReminderInterval: Math.max(0, Number(ShellStore.settings.antiAfkReminderEveryMinutes || 0) * 60)
    readonly property bool antiAfkReminderVisible: ShellStore.antiAfkEnabled && streaming
        && !Boolean(ShellStore.settings.showAntiAfkIndicator) && antiAfkReminderInterval > 0
        && elapsedSeconds % antiAfkReminderInterval < antiAfkReminderDuration

    function elapsed(value) {
        const hours = Math.floor(value / 3600)
        const minutes = Math.floor((value % 3600) / 60)
        const seconds = value % 60
        return (hours > 0 ? String(hours).padStart(2, "0") + ":" : "")
            + String(minutes).padStart(2, "0") + ":" + String(seconds).padStart(2, "0")
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
        if (!root.visible || !root.streaming
                || ShellStore.streamOverlayBlocksGameplayInput(AppController.overlay))
            return
        streamVideo.forceActiveFocus()
        streamVideo.resynchronizeInput()
    }

    onXChanged: publishCaptureRect()
    onYChanged: publishCaptureRect()
    onWidthChanged: publishCaptureRect()
    onHeightChanged: publishCaptureRect()
    onVisibleChanged: publishCaptureRect()
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

    Timer { interval: 1000; repeat: true; running: root.streaming; onTriggered: root.nowMs = Date.now() }

    StreamVideoItem {
        id: streamVideo
        objectName: "streamSurfaceHost"
        anchors.fill: parent
        visible: root.visible && root.streaming
        focus: visible
        inputEnabled: visible
            && !ShellStore.streamOverlayBlocksGameplayInput(AppController.overlay)
        shortcutBindings: ShellStore.streamShortcutBindings()
        videoSize: Qt.size(Number(root.profile.width || 0), Number(root.profile.height || 0))
        z: 0
        onLocalShortcutRequested: action => ShellStore.applyStreamShortcutAction(action)
    }

    Connections {
        target: ShellStore
        function onPointerLockToggleRequested() {
            streamVideo.relativeMouse = !streamVideo.relativeMouse
        }
    }

    ScreenBackground {
        visible: !root.streaming
        artwork: root.game.heroImageUrl || root.game.imageUrl || ""
        tint: "#101B2A"
        z: 1
    }

    Rectangle {
        visible: !root.streaming
        anchors.fill: parent
        color: Qt.rgba(0.02, 0.04, 0.08, 0.38)
        z: 2
    }

    GlassPanel {
        visible: !root.streaming
        anchors.centerIn: parent
        width: Math.min(980, parent.width - 160)
        height: 500
        panelRadius: 44
        strong: true
        z: 3

        Column {
            anchors.fill: parent
            anchors.margins: 44
            spacing: 22

            Row {
                width: parent.width
                spacing: 18
                Rectangle {
                    width: 62; height: 62; radius: 31
                    color: root.failed ? Theme.coral : (root.streaming ? Theme.mint : Theme.focus)
                    Rectangle { anchors.centerIn: parent; width: 22; height: 22; radius: 11; color: Theme.shell }
                    SequentialAnimation on scale {
                        running: !AppController.reducedMotion
                        loops: Animation.Infinite
                        NumberAnimation { to: 1.08; duration: 700; easing.type: Easing.InOutSine }
                        NumberAnimation { to: 1.0; duration: 700; easing.type: Easing.InOutSine }
                    }
                }
                Column {
                    anchors.verticalCenter: parent.verticalCenter
                    spacing: 3
                    Text { text: root.failed ? qsTr("MEDIA STARTUP FAILED") : (root.status === "reconnecting" ? qsTr("RECONNECTING") : qsTr("CLOUD SEAT READY")); color: root.failed ? Theme.coral : Theme.focus; font.family: Theme.bodyFont; font.pixelSize: 14; font.weight: Font.Black; font.letterSpacing: 1.4 }
                    Text { text: root.game.title || qsTr("GeForce NOW"); color: Theme.label; font.family: Theme.displayFont; font.pixelSize: 34; font.weight: Font.Black }
                }
            }

            Text {
                width: parent.width
                text: root.streamer.message || ShellStore.streamMessage || qsTr("GeForce NOW prepared the remote machine. OpenNOW is validating the negotiated media transport.")
                wrapMode: Text.WordWrap
                color: Theme.textMuted
                font.family: Theme.bodyFont
                font.pixelSize: 18
                lineHeight: 1.25
            }

            Row {
                spacing: 12
                Repeater {
                    model: [
                        root.profile.resolution || ShellStore.settings.resolution || "—",
                        root.profile.fps || ShellStore.settings.fps
                            ? String(root.profile.fps || ShellStore.settings.fps) + " FPS" : "—",
                        root.profile.codec || ShellStore.settings.codec
                            ? String(root.profile.codec || ShellStore.settings.codec).toUpperCase() : "—",
                        root.session.gpuType || "—"
                    ]
                    GlassPanel {
                        required property string modelData
                        width: 190; height: 58; panelRadius: 20; strong: true
                        Text { anchors.centerIn: parent; text: modelData; color: Theme.label; font.family: Theme.bodyFont; font.pixelSize: 15; font.weight: Font.Bold }
                    }
                }
            }

            GlassPanel {
                width: parent.width; height: 82; panelRadius: 22
                Row {
                    anchors.fill: parent; anchors.margins: 18; spacing: 22
                    Column {
                        width: parent.width * 0.42
                        Text { text: qsTr("SESSION"); color: Theme.textMuted; font.family: Theme.bodyFont; font.pixelSize: 11; font.weight: Font.Black; font.letterSpacing: 1.2 }
                        Text { width: parent.width; elide: Text.ElideMiddle; text: root.session.sessionId || "—"; color: Theme.label; font.family: Theme.bodyFont; font.pixelSize: 14 }
                    }
                    Column {
                        width: parent.width * 0.42
                        Text { text: qsTr("SERVER"); color: Theme.textMuted; font.family: Theme.bodyFont; font.pixelSize: 11; font.weight: Font.Black; font.letterSpacing: 1.2 }
                        Text { width: parent.width; elide: Text.ElideMiddle; text: root.session.serverLocation || root.session.zone || "—"; color: Theme.label; font.family: Theme.bodyFont; font.pixelSize: 14 }
                    }
                }
            }

            Row {
                anchors.horizontalCenter: parent.horizontalCenter
                spacing: 12
                GlassButton { width: 260; text: qsTr("Stop session"); glyph: "B"; onClicked: ShellStore.stopStreamingSession(); Component.onCompleted: forceActiveFocus() }
                GlassButton { width: 260; text: root.failed ? qsTr("Retry media") : qsTr("Session guide"); glyph: root.failed ? "↻" : "≡"; primary: true; onClicked: root.failed ? ShellStore.retryNativeStreamer() : AppController.showOverlay("guide-session") }
            }
        }
    }

    GlassPanel {
        visible: root.sessionClockVisible
        z: 12
        x: 34; y: 34; width: 190; height: 58; panelRadius: 22; strong: true
        Row {
            anchors.centerIn: parent; spacing: 10
            Text { text: "◷"; color: Theme.focus; font.pixelSize: 19; font.weight: Font.Black }
            Text { text: root.elapsed(root.elapsedSeconds); color: Theme.label; font.family: Theme.monoFont; font.pixelSize: 17; font.weight: Font.Bold }
        }
        opacity: visible ? 1 : 0
        Behavior on opacity { NumberAnimation { duration: Theme.overlayDuration; easing.type: Easing.OutCubic } }
    }

    GlassPanel {
        visible: root.streaming && AppController.overlay === ""
        z: 12
        anchors.horizontalCenter: parent.horizontalCenter
        anchors.bottom: parent.bottom
        anchors.bottomMargin: 34
        width: streamHints.implicitWidth + 34
        height: 52
        panelRadius: 26
        strong: true

        Row {
            id: streamHints
            anchors.centerIn: parent
            spacing: 22
            ControllerGlyph { glyph: "GUIDE"; label: qsTr("Session") }
            ControllerGlyph { glyph: "F3"; label: qsTr("Stats") }
            ControllerGlyph { glyph: "F11"; label: qsTr("Fullscreen") }
        }
    }

    GlassPanel {
        visible: root.streaming && ShellStore.antiAfkEnabled
            && (Boolean(ShellStore.settings.showAntiAfkIndicator) || root.antiAfkReminderVisible)
        z: 12
        x: root.width - width - 34; y: 34; width: 180; height: 58; panelRadius: 22; strong: true
        Row {
            anchors.centerIn: parent; spacing: 10
            Rectangle { width: 10; height: 10; radius: 5; color: Theme.mint }
            Text { text: qsTr("ANTI-AFK ON"); color: Theme.label; font.family: Theme.monoFont; font.pixelSize: 13; font.weight: Font.Black; font.letterSpacing: 0.8 }
        }
        opacity: visible ? 1 : 0
        Behavior on opacity { NumberAnimation { duration: Theme.overlayDuration; easing.type: Easing.OutCubic } }
    }

    Keys.onPressed: event => {
        if (event.isAutoRepeat)
            return
        if (!root.streaming
                && (event.key === Qt.Key_Escape || event.key === Qt.Key_Back)) {
            event.accepted = true
            ShellStore.requestStreamExitConfirmation()
        }
    }

    onStreamingChanged: if (streaming) Qt.callLater(root.forceActiveFocus)

    AppChrome { visible: !root.streaming; anchors.fill: parent; title: qsTr("Live session"); currentRoute: "home"; bottomVisible: false; z: 4 }
}

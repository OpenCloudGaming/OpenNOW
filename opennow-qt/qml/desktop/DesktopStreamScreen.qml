import QtQuick
import QtQuick.Controls
import OpenNOW

FocusScope {
    id: root
    focus: true
    Accessible.role: Accessible.Pane
    Accessible.name: qsTr("Live session")

    signal stopRequested()
    signal retryRequested()
    signal menuRequested()

    readonly property var game: ShellStore.selectedGame || ({})
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
    readonly property bool failed: root.status === "error"
    readonly property bool statusVisible: !root.streaming
    readonly property string artwork: DesktopTokens.decodeArtworkUrl(
        String(root.game.heroImageUrl || root.game.imageUrl || ""))

    function statusLabel() {
        if (root.failed) return qsTr("MEDIA RUNTIME FAILED")
        if (root.status === "negotiating") return qsTr("NEGOTIATING TRANSPORT")
        if (root.status === "connecting") return qsTr("CONNECTING TO RIG")
        if (root.status === "reconnecting") return qsTr("RECONNECTING")
        if (root.status === "stopped") return qsTr("SESSION CLOSED")
        return qsTr("STARTING VIDEO")
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

    ArtworkSource {
        id: streamArtwork
        sourceUrl: root.artwork
        active: root.statusVisible && root.visible
    }

    Rectangle {
        anchors.fill: parent
        visible: root.statusVisible
        color: "#04060A"
        z: 1

        Image {
            anchors.fill: parent
            source: streamArtwork.resolvedUrl
            fillMode: Image.PreserveAspectCrop
            asynchronous: true
            cache: true
            opacity: status === Image.Ready ? 0.28 : 0
            Behavior on opacity { NumberAnimation { duration: DesktopTokens.revealDuration; easing.type: Easing.OutCubic } }
        }
        Rectangle {
            anchors.fill: parent
            gradient: Gradient {
                GradientStop { position: 0; color: "#D604060A" }
                GradientStop { position: 0.5; color: "#BC04060A" }
                GradientStop { position: 1; color: "#F204060A" }
            }
        }
    }

    Column {
        id: statusContent
        visible: root.statusVisible
        width: Math.min(560, root.width - 64)
        x: Math.round((root.width - width) / 2)
        y: Math.max(72, Math.round((root.height - height) / 2))
        spacing: 0
        z: 2

        Text {
            text: root.statusLabel()
            color: root.failed ? DesktopTokens.danger : DesktopTokens.focus
            font.family: DesktopTokens.monoFont
            font.pixelSize: 11
            font.weight: Font.Bold
            font.letterSpacing: 1.8
        }
        Text {
            width: parent.width
            topPadding: 5
            text: String(root.game.title || qsTr("GeForce NOW"))
            color: DesktopTokens.text
            font.family: DesktopTokens.displayFont
            font.pixelSize: 40
            font.weight: Font.Black
            font.letterSpacing: -1.1
            elide: Text.ElideRight
        }
        Text {
            width: parent.width
            topPadding: 6
            text: String(root.streamer.message || ShellStore.streamMessage
                || qsTr("Your rig is live. OpenNOW is starting the native media runtime."))
            color: DesktopTokens.textBody
            font.family: DesktopTokens.bodyFont
            font.pixelSize: 14
            font.weight: Font.Medium
            wrapMode: Text.WordWrap
        }

        Item { width: 1; height: 26 }
        Row {
            spacing: 10
            DesktopButton {
                id: primaryAction
                text: root.failed ? qsTr("Retry media") : qsTr("Session menu")
                shortcutText: root.failed ? "" : "Ctrl G"
                primary: true
                onClicked: root.failed ? root.retryRequested() : root.menuRequested()
            }
            DesktopButton {
                text: qsTr("End session")
                shortcutText: "Esc"
                danger: true
                onClicked: root.stopRequested()
            }
        }

        Item { width: 1; height: 24 }
        Row {
            spacing: 22
            DesktopKeyHint { keyText: "Ctrl G"; label: qsTr("Session menu") }
            DesktopKeyHint { keyText: "F3"; label: qsTr("Stream stats") }
            DesktopKeyHint { keyText: "F10"; label: qsTr("Console mode") }
        }
    }

    Text {
        visible: root.statusVisible
        anchors.left: parent.left
        anchors.bottom: parent.bottom
        anchors.margins: 32
        text: root.session.sessionId
            ? qsTr("SESSION %1").arg(String(root.session.sessionId).slice(0, 9).toUpperCase())
            : qsTr("SESSION PENDING")
        color: DesktopTokens.textFaint
        font.family: DesktopTokens.monoFont
        font.pixelSize: 9
        font.weight: Font.Bold
        font.letterSpacing: 0.8
        z: 2
    }

    Rectangle {
        anchors.horizontalCenter: parent.horizontalCenter
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
        if (!root.visible)
            return
        if (root.statusVisible)
            primaryAction.forceActiveFocus()
        else
            streamVideo.forceActiveFocus()
    }

    onVisibleChanged: {
        publishCaptureRect()
        if (visible)
            Qt.callLater(root.restoreStreamFocus)
    }
    onStatusVisibleChanged: Qt.callLater(root.restoreStreamFocus)

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

pragma ComponentBehavior: Bound

import QtQuick
import QtQuick.Controls
import OpenNOW

FocusScope {
    id: root
    objectName: "desktopSessionStarting"
    width: 1440
    height: 900
    Accessible.role: Accessible.Pane
    Accessible.name: qsTr("Starting session")

    signal cancelRequested()
    signal retryRequested()

    readonly property var game: ShellStore.selectedGame || ({})
    readonly property var session: ShellStore.activeSession || ({})
    readonly property var streamer: ShellStore.streamer || ({})
    readonly property bool connecting: AppController.route === "stream"
    readonly property string phase: String(ShellStore.streamState || "preparing")
    readonly property bool stopping: phase === "stopping"
    // A terminal phase stays a failure after reconnect attempts are exhausted.
    // Those counters stay non-zero, and treating them as an active reconnect
    // hid the interrupted state and its retry action. A stopped runtime during
    // an active reconnect is not itself a failure.
    readonly property bool failed: !stopping && phase !== "idle"
        && (phase === "failed" || phase === "error"
            || (connecting && !reconnecting && streamer.status === "error"))
    readonly property bool reconnecting: phase === "reconnecting"
        || ShellStore.streamerRestartAttempts > 0 || ShellStore.sessionReconnectAttempts > 0
    SessionSetupProgress {
        id: setupProgress
        session: root.session
    }
    readonly property string statusText: {
        if (stopping) return qsTr("Closing your session")
        if (failed) return ShellStore.launchConflictDetected
            ? qsTr("Your game is still running") : qsTr("Session could not start")
        if (reconnecting) return qsTr("Reconnecting to your session")
        if (connecting) return qsTr("Connecting to your game")
        if (setupProgress.queued) return setupProgress.title
        if (phase === "checking") return qsTr("Checking session availability")
        if (phase === "requesting") return qsTr("Requesting your session")
        if (phase === "resuming") return qsTr("Reconnecting to your game")
        return setupProgress.title
    }
    readonly property string detailText: {
        if (stopping) return qsTr("Waiting for your session to close.")
        if (failed) return String((connecting && streamer.message) || ShellStore.streamMessage
            || qsTr("Please try again or return to your library."))
        if (reconnecting) return qsTr("Your stream will return when the connection is restored.")
        if (phase === "resuming" || phase === "checking" || phase === "conflict")
            return ShellStore.streamMessage
        if (connecting) {
            if (streamer.status === "starting") return qsTr("Initializing the native streaming connection.")
            if (streamer.status === "streaming") return qsTr("Connected. Waiting for the first video frame.")
            return qsTr("Your game will appear here as soon as the video is ready.")
        }
        return setupProgress.detail
    }

    function restoreFocus() {
        if (root.visible && root.enabled
                && !ShellStore.streamOverlayBlocksGameplayInput(AppController.overlay))
            cancelButton.forceActiveFocus()
    }

    ArtworkSource {
        id: artwork
        sourceUrl: DesktopTokens.decodeArtworkUrl(String(root.game.heroImageUrl || root.game.imageUrl || ""))
        active: root.visible
    }

    Rectangle { anchors.fill: parent; color: "#04060A" }
    Image {
        anchors.fill: parent
        source: artwork.resolvedUrl
        fillMode: Image.PreserveAspectCrop
        asynchronous: true
        cache: true
        opacity: status === Image.Ready ? 0.36 : 0
        Behavior on opacity { NumberAnimation { duration: DesktopTokens.revealDuration; easing.type: Easing.OutCubic } }
    }
    Rectangle {
        anchors.fill: parent
        gradient: Gradient {
            GradientStop { position: 0; color: "#C004060A" }
            GradientStop { position: 0.5; color: "#AC04060A" }
            GradientStop { position: 1; color: "#F204060A" }
        }
    }

    Row {
        anchors.left: parent.left
        anchors.top: parent.top
        anchors.margins: 32
        spacing: 10
        DesktopBrandLockup {
            anchors.verticalCenter: parent.verticalCenter
            markHeight: 12
            fontPixelSize: 16
            spacing: 10
            ink: Theme.mediaForeground
        }
    }

    Column {
        anchors.centerIn: parent
        width: Math.min(560, root.width - 64)
        spacing: 0

        Text {
            text: root.stopping ? qsTr("ENDING SESSION")
                : root.failed ? qsTr("SESSION INTERRUPTED") : qsTr("STARTING SESSION")
            color: root.failed ? DesktopTokens.danger : Theme.mediaAccent
            font.family: DesktopTokens.monoFont
            font.pixelSize: 11
            font.weight: Font.Bold
            font.letterSpacing: 1.8
        }
        Text {
            width: parent.width
            topPadding: 10
            text: String(root.game.title || qsTr("GeForce NOW"))
            color: Theme.mediaForeground
            font.family: DesktopTokens.displayFont
            font.pixelSize: root.width < 800 ? 34 : 44
            font.weight: Font.Black
            font.letterSpacing: -1.2
            maximumLineCount: 2
            wrapMode: Text.WordWrap
            elide: Text.ElideRight
        }
        Item { width: 1; height: 32 }
        Row {
            width: parent.width
            spacing: 12
            Item {
                width: 20; height: 26
                visible: !root.failed
                Rectangle {
                    anchors.centerIn: parent
                    width: 18; height: 18; radius: 9
                    color: "transparent"
                    border.width: 2
                    border.color: "#35FFFFFF"
                    Rectangle {
                        anchors.horizontalCenter: parent.horizontalCenter
                        y: -1
                        width: 6; height: 6; radius: 3
                        color: Theme.mediaAccent
                    }
                    RotationAnimation on rotation {
                        from: 0; to: 360; duration: 1400; loops: Animation.Infinite
                        running: root.visible && !root.failed && !AppController.reducedMotion
                    }
                }
            }
            Text {
                objectName: "sessionLaunchStatus"
                width: parent.width - (root.failed ? 0 : 32)
                text: root.statusText
                color: root.failed ? DesktopTokens.danger : Theme.mediaForeground
                font.family: DesktopTokens.bodyFont
                font.pixelSize: 19
                font.weight: Font.DemiBold
                wrapMode: Text.WordWrap
            }
        }
        Text {
            width: parent.width
            topPadding: 10
            text: root.detailText
            color: Theme.mediaMuted
            font.family: DesktopTokens.bodyFont
            font.pixelSize: 14
            lineHeight: 1.4
            wrapMode: Text.WordWrap
            maximumLineCount: 5
            elide: Text.ElideRight
        }
        Item { width: 1; height: 32 }
        Flow {
            width: parent.width
            spacing: 12
            DesktopButton {
                onMediaBackground: true
                visible: root.failed && (root.connecting || ShellStore.activeSession !== null
                    || ShellStore.pendingLaunchParams !== null || ShellStore.conflictSession !== null)
                enabled: !ShellStore.streamBusy
                text: root.connecting ? qsTr("Retry connection") : qsTr("Try again")
                primary: true
                onClicked: root.retryRequested()
            }
            DesktopButton {
                id: cancelButton
                onMediaBackground: true
                enabled: !root.stopping
                text: root.failed && !ShellStore.activeSession ? qsTr("Back") : qsTr("Cancel session")
                shortcutText: qsTr("Esc")
                onClicked: root.cancelRequested()
            }
        }
    }

    onVisibleChanged: if (visible) Qt.callLater(root.restoreFocus)
    onEnabledChanged: if (enabled) Qt.callLater(root.restoreFocus)
    Connections {
        target: AppController
        function onOverlayChanged() { Qt.callLater(root.restoreFocus) }
    }
    Keys.onPressed: event => {
        if (event.isAutoRepeat || root.stopping)
            return
        if (event.key === Qt.Key_Escape || event.key === Qt.Key_Back) {
            root.cancelRequested()
            event.accepted = true
        }
    }
}

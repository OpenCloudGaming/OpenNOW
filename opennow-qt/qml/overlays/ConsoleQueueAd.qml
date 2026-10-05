import QtQuick
import QtMultimedia
import OpenNOW

FocusScope {
    id: root
    objectName: "consoleQueueAd"
    anchors.fill: parent
    focus: true
    Accessible.role: Accessible.Dialog
    Accessible.name: qsTr("Queue message")

    readonly property var progress: ShellStore.sessionSetupProgress
    readonly property bool queued: ShellStore.activeSession !== null && progress.queued
    readonly property int step: progress.queued || progress.setupStep === 5 || progress.setupStep === 6 ? 2
        : progress.setupStep >= 2 && progress.setupStep <= 4 ? 3 : 1

    Component.onCompleted: playbackButton.forceActiveFocus()

    Keys.onPressed: event => {
        if (event.key !== Qt.Key_Escape && event.key !== Qt.Key_Back)
            return
        event.accepted = true
        if (!event.isAutoRepeat)
            ShellStore.requestStreamExitConfirmation()
    }

    SessionGlyphs { id: glyphs }

    QueueAdPlayback {
        id: player
        videoOutput: adVideo
    }

    LaunchStage {
        anchors.fill: parent
        game: ShellStore.selectedGame || ({})
        tone: root.queued || root.step === 2 ? Theme.yellow : Theme.focus
        statusText: root.queued ? qsTr("In queue") : qsTr("Starting")
        eyebrow: qsTr("Starting your session")
        headline: root.queued
            ? (root.progress.queuePosition > 0 ? qsTr("You're in line") : qsTr("Waiting for an available rig"))
            : root.progress.title
        detail: root.queued
            ? qsTr("Waiting for an available rig. Your session starts by itself, so you can put the controller down.")
            : root.progress.detail
        railVisible: true
        activeStep: root.step
        activeStepDetail: root.queued ? qsTr("Position comes from GeForce NOW") : ""
        footerText: root.queued ? qsTr("Leave queue…") : qsTr("Cancel session…")
        onFooterRequested: ShellStore.requestStreamExitConfirmation()

        aside: [
            Item {
                anchors.fill: parent

                Text {
                    text: qsTr("Required by GeForce NOW while you wait").toUpperCase()
                    color: Theme.textMuted
                    font.family: Theme.monoFont; font.pixelSize: 15; font.weight: Font.Bold; font.letterSpacing: 3
                }
                Text {
                    anchors.right: parent.right
                    visible: root.queued && root.progress.queuePosition > 0
                    text: qsTr("Queue position %1").arg(root.progress.queuePosition).toUpperCase()
                    color: Theme.yellow
                    font.family: Theme.monoFont; font.pixelSize: 15; font.weight: Font.Bold; font.letterSpacing: 3
                }

                Rectangle {
                    id: adCard
                    y: 38
                    width: parent.width
                    height: Math.round(width * 9 / 16)
                    radius: 32
                    clip: true
                    color: "#05070D"
                    border.width: 1
                    border.color: Theme.seam

                    VideoOutput {
                        id: adVideo
                        anchors.fill: parent
                        fillMode: VideoOutput.PreserveAspectCrop
                    }
                    Rectangle {
                        anchors.left: parent.left; anchors.right: parent.right; anchors.bottom: parent.bottom
                        height: 150
                        gradient: Gradient {
                            GradientStop { position: 0; color: "transparent" }
                            GradientStop { position: 1; color: Qt.rgba(0, 0, 0, 0.82) }
                        }
                    }
                    Rectangle {
                        x: 28; y: 24
                        width: adBadge.implicitWidth + 24; height: 32; radius: 10
                        color: Qt.rgba(0, 0, 0, 0.55)
                        Text {
                            id: adBadge
                            anchors.centerIn: parent
                            text: (player.playing ? qsTr("Ad · playing") : qsTr("Ad · paused")).toUpperCase()
                            color: "#FFFFFF"
                            font.family: Theme.monoFont; font.pixelSize: 13; font.weight: Font.Bold; font.letterSpacing: 2
                        }
                    }
                    Rectangle {
                        anchors.centerIn: parent
                        visible: !player.playing
                        width: 96; height: 96; radius: 48
                        color: Qt.rgba(0, 0, 0, 0.5)
                        border.width: 2
                        border.color: Qt.rgba(1, 1, 1, 0.6)
                        Text { anchors.centerIn: parent; anchors.horizontalCenterOffset: 3; text: "▶"; color: "#FFFFFF"; font.pixelSize: 34 }
                    }
                    Column {
                        x: 28
                        anchors.bottom: parent.bottom
                        anchors.bottomMargin: 26
                        width: parent.width - 56
                        spacing: 14
                        Text {
                            width: parent.width
                            text: player.ad ? (player.ad.title || player.adState.message || qsTr("A short message while you wait")) : qsTr("Preparing your session")
                            elide: Text.ElideRight
                            color: "#FFFFFF"
                            font.family: Theme.displayFont; font.pixelSize: 28; font.weight: Font.Black
                        }
                        Rectangle {
                            visible: player.duration > 0
                            width: parent.width; height: 6; radius: 3
                            color: Qt.rgba(1, 1, 1, 0.22)
                            Rectangle {
                                width: parent.width * Math.max(0, Math.min(1, player.position / Math.max(1, player.duration)))
                                height: parent.height; radius: 3
                                color: "#FFFFFF"
                            }
                        }
                    }
                }

                Row {
                    anchors.top: adCard.bottom
                    anchors.topMargin: 24
                    spacing: 22
                    ConsoleActionButton {
                        id: playbackButton
                        objectName: "queueAdPlaybackButton"
                        width: 260
                        enabled: player.ad !== null && player.mediaUrl !== ""
                        text: player.playing ? qsTr("Pause ad") : qsTr("Resume ad")
                        glyph: glyphs.keyboard ? "" : glyphs.button("A")
                        onClicked: player.toggle()
                    }
                    Text {
                        anchors.verticalCenter: playbackButton.verticalCenter
                        width: adCard.width - playbackButton.width - 22
                        wrapMode: Text.WordWrap
                        text: qsTr("GeForce NOW requires this ad. You can pause it, not skip it.")
                        color: Theme.textMuted
                        font.family: Theme.bodyFont; font.pixelSize: 17
                    }
                }
            }
        ]
    }
}

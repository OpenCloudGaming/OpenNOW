pragma ComponentBehavior: Bound

import QtQuick
import QtQuick.Controls
import QtQuick.Effects
import OpenNOW

FocusScope {
    id: root
    width: 1440
    height: 900
    focus: true
    Accessible.role: Accessible.Pane
    Accessible.name: qsTr("Starting session")

    signal cancelRequested()

    readonly property var game: ShellStore.selectedGame || ({})
    readonly property var session: ShellStore.activeSession || ({})
    readonly property var profile: session.negotiatedStreamProfile || session.streamProfile || ({})
    readonly property string phase: String(ShellStore.streamState || "preparing")
    readonly property bool failed: phase === "failed" || phase === "error"
    readonly property int activeStep: phase === "checking" ? 0
        : phase === "requesting" || phase === "resuming" ? 1
        : phase === "ready" || phase === "streaming" ? 3 : 2
    readonly property real progressValue: phase === "checking" ? 0.16
        : phase === "requesting" ? 0.34
        : phase === "resuming" ? 0.48
        : phase === "ready" || phase === "streaming" ? 1.0 : 0.665
    readonly property string region: String(session.regionName || session.region || session.serverRegionId || ShellStore.settings.region || "").toUpperCase()
    readonly property int latency: Math.max(0, Number(session.latencyMs || session.pingMs || 0))
    readonly property int widthPx: Number(profile.width || ShellStore.settings.resolutionWidth || 0)
    readonly property int heightPx: Number(profile.height || ShellStore.settings.resolutionHeight || 0)
    readonly property int fps: Number(profile.fps || profile.frameRate || ShellStore.settings.fps || ShellStore.settings.frameRate || 0)
    readonly property string codec: String(profile.codec || ShellStore.settings.codec || "").toUpperCase()
    readonly property int bitrate: Math.round(Number(profile.bitrateMbps || ShellStore.settings.maxBitrateMbps || 0))
    readonly property string membershipTier: String(ShellStore.subscription
        && ShellStore.subscription.membershipTier || "").toUpperCase()
    readonly property string rigName: String(session.rigName || session.gpuName || "").toUpperCase()
    readonly property string sessionArtwork: DesktopTokens.decodeArtworkUrl(
        String(root.game.heroImageUrl || root.game.imageUrl || ""))
    // The launch card is centred, so the corner footers only fit once the window
    // leaves them clear room below it.
    readonly property bool footerVisible: root.height - launchContent.y - launchContent.height > 104

    ArtworkSource {
        id: sessionArtworkSource
        sourceUrl: root.sessionArtwork
        active: root.visible
    }

    function resolutionLabel() {
        if (heightPx >= 2160) return "2160P"
        if (heightPx >= 1440) return "1440P"
        if (heightPx >= 1080) return "1080P"
        if (heightPx > 0) return heightPx + "P"
        return ""
    }

    function streamChips() {
        const chips = []
        if (root.region)
            chips.push(root.latency > 0 ? root.region + " · " + root.latency + " MS" : root.region)
        const res = root.resolutionLabel()
        if (res && root.fps > 0)
            chips.push(res + " · " + root.fps + " FPS")
        else if (res)
            chips.push(res)
        else if (root.fps > 0)
            chips.push(root.fps + " FPS")
        if (root.codec && root.bitrate > 0)
            chips.push(root.codec + " · " + root.bitrate + " MBPS")
        else if (root.codec)
            chips.push(root.codec)
        return chips
    }

    Rectangle { anchors.fill: parent; color: "#04060A" }

    Image {
        anchors.fill: parent
        source: sessionArtworkSource.resolvedUrl
        fillMode: Image.PreserveAspectCrop
        asynchronous: true
        cache: true
        opacity: status === Image.Ready ? 0.32 : 0
        Behavior on opacity { NumberAnimation { duration: DesktopTokens.revealDuration; easing.type: Easing.OutCubic } }
    }
    Rectangle {
        anchors.fill: parent
        gradient: Gradient {
            GradientStop { position: 0; color: "#CF04060A" }
            GradientStop { position: 0.48; color: "#B804060A" }
            GradientStop { position: 1; color: "#F204060A" }
        }
    }
    Rectangle {
        anchors.fill: parent
        color: "transparent"
        border.width: 1
        border.color: "#0AFFFFFF"
    }

    Row {
        anchors.left: parent.left
        anchors.top: parent.top
        anchors.leftMargin: 32
        anchors.topMargin: 28
        spacing: 10
        Image {
            anchors.verticalCenter: parent.verticalCenter
            width: 22
            height: 12
            source: "qrc:/qt/qml/OpenNOW/res/brand/opennow-mark.png"
            fillMode: Image.PreserveAspectFit
            smooth: true
        }
        Text {
            text: "OpenNOW"
            color: DesktopTokens.text
            font.family: DesktopTokens.displayFont
            font.pixelSize: 16
            font.weight: Font.Black
            font.letterSpacing: -0.3
        }
    }

    Column {
        id: launchContent
        x: Math.round((root.width - width) / 2)
        y: Math.max(76, Math.round((root.height - height) / 2))
        width: Math.min(620, root.width - 64)
        spacing: 0
        opacity: 1
        transform: Translate { id: revealTranslate; y: 0 }

        Text {
            text: root.failed ? qsTr("SESSION INTERRUPTED") : qsTr("STARTING SESSION")
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
            font.pixelSize: 44
            font.weight: Font.Black
            font.letterSpacing: -1.2
            elide: Text.ElideRight
        }
        Text {
            width: parent.width
            topPadding: 4
            text: root.failed
                ? (ShellStore.streamMessage || qsTr("The cloud rig could not finish preparing. Your library is safe."))
                : qsTr("No queue. Your rig is being handed over now.")
            color: DesktopTokens.textBody
            font.family: DesktopTokens.bodyFont
            font.pixelSize: 14
            font.weight: Font.Medium
            wrapMode: Text.WordWrap
        }

        Item { width: 1; height: 29 }
        Rectangle {
            width: parent.width
            height: 5
            radius: 2.5
            color: "#21FFFFFF"
            clip: true
            Rectangle {
                width: parent.width * root.progressValue
                height: parent.height
                radius: parent.radius
                color: root.failed ? DesktopTokens.danger : DesktopTokens.focus
                Behavior on width { NumberAnimation { duration: DesktopTokens.motionDuration; easing.type: Easing.OutCubic } }
            }
        }
        Item {
            width: parent.width
            height: 31
            Text {
                anchors.left: parent.left
                anchors.top: parent.top
                anchors.topMargin: 8
                text: root.failed ? qsTr("LAUNCH PAUSED")
                    : root.activeStep < 2 ? qsTr("RESERVING YOUR RIG")
                    : root.activeStep === 2 ? qsTr("WARMING UP SHADERS") : qsTr("OPENING STREAM")
                color: DesktopTokens.textMuted
                font.family: DesktopTokens.monoFont
                font.pixelSize: 9
                font.weight: Font.Bold
                font.letterSpacing: 1.2
            }
            Text {
                anchors.right: parent.right
                anchors.top: parent.top
                anchors.topMargin: 8
                text: root.failed ? qsTr("ACTION NEEDED")
                    : root.activeStep >= 3 ? qsTr("OPENING STREAM") : qsTr("IN PROGRESS")
                color: DesktopTokens.textFaint
                font.family: DesktopTokens.monoFont
                font.pixelSize: 9
                font.weight: Font.DemiBold
                font.letterSpacing: 0.9
            }
        }

        Rectangle {
            width: parent.width
            height: 174
            radius: 16
            color: "#B804060A"
            border.width: 1
            border.color: "#17FFFFFF"

            Column {
                x: 16
                y: 14
                width: parent.width - 32
                spacing: 0
                Repeater {
                    model: [
                        { label: root.region ? qsTr("Requesting a machine in %1").arg(root.region) : qsTr("Requesting a machine"), meta: qsTr("LIVE") },
                        { label: qsTr("Preparing your library session"), meta: qsTr("LIVE") },
                        { label: qsTr("Applying stream settings"), meta: qsTr("RUNNING") },
                        { label: qsTr("Opening the stream transport"), meta: qsTr("QUEUED") }
                    ]
                    delegate: Item {
                        id: stepRow
                        required property var modelData
                        required property int index
                        width: parent.width
                        height: 36
                        readonly property bool done: index < root.activeStep
                        readonly property bool active: index === root.activeStep && !root.failed

                        Rectangle {
                            x: 0
                            anchors.verticalCenter: parent.verticalCenter
                            width: 18
                            height: 18
                            radius: 9
                            color: stepRow.done ? "#2856E6A5" : stepRow.active ? "#237FD4FF" : "#0FFFFFFF"
                            border.width: 1
                            border.color: stepRow.done ? DesktopTokens.green : stepRow.active ? DesktopTokens.focus : "#29FFFFFF"
                            DesktopGlyph {
                                visible: stepRow.done
                                anchors.centerIn: parent
                                width: 10
                                height: 8
                                icon: "desktop-check-mint.svg"
                            }
                            Rectangle {
                                visible: stepRow.active && !stepRow.done
                                anchors.centerIn: parent
                                width: 6
                                height: 6
                                radius: 3
                                color: DesktopTokens.focus
                            }
                        }
                        Text {
                            x: 30
                            anchors.verticalCenter: parent.verticalCenter
                            width: parent.width - 118
                            text: stepRow.modelData.label
                            color: stepRow.done || stepRow.active ? DesktopTokens.textHigh : DesktopTokens.textFaint
                            font.family: DesktopTokens.bodyFont
                            font.pixelSize: 12
                            font.weight: stepRow.active ? Font.Bold : Font.DemiBold
                            elide: Text.ElideRight
                        }
                        Text {
                            anchors.right: parent.right
                            anchors.verticalCenter: parent.verticalCenter
                            text: stepRow.index === 2 && stepRow.active ? qsTr("RUNNING")
                                : stepRow.index === 3 && stepRow.active ? qsTr("RUNNING")
                                : stepRow.modelData.meta
                            color: stepRow.active ? DesktopTokens.focus : stepRow.done ? DesktopTokens.textMuted : DesktopTokens.textFaint
                            font.family: DesktopTokens.monoFont
                            font.pixelSize: 9
                            font.weight: Font.Bold
                            font.letterSpacing: 0.7
                        }
                    }
                }
            }
        }

        Item { width: 1; height: 14 }
        Row {
            width: parent.width
            spacing: 8
            Repeater {
                model: root.streamChips()
                delegate: Rectangle {
                    id: metaChip
                    required property string modelData
                    required property int index
                    width: metaText.implicitWidth + 18
                    height: 25
                    radius: 12.5
                    color: index === 3 ? "#279F7AEA" : "#10FFFFFF"
                    border.width: 1
                    border.color: index === 3 ? "#669F7AEA" : "#20FFFFFF"
                    Text {
                        id: metaText
                        anchors.centerIn: parent
                        text: metaChip.modelData
                        color: metaChip.index === 3 ? "#CDBDFF" : DesktopTokens.textMuted
                        font.family: DesktopTokens.monoFont
                        font.pixelSize: 9
                        font.weight: Font.Bold
                        font.letterSpacing: 0.6
                    }
                }
            }
        }

        Item { width: 1; height: 16 }
        DesktopButton {
            id: cancelButton
            width: parent.width
            height: 36
            text: root.failed ? qsTr("Back to game details") : qsTr("Cancel launch")
            shortcutText: "Esc"
            danger: root.failed
            onClicked: root.cancelRequested()
        }
    }

    Item {
        anchors.left: parent.left
        anchors.bottom: parent.bottom
        anchors.leftMargin: 32
        anchors.bottomMargin: 32
        width: Math.min(340, Math.round(root.width / 3))
        height: 64
        visible: root.footerVisible
        Text {
            text: qsTr("WHILE YOU WAIT")
            color: DesktopTokens.textFaint
            font.family: DesktopTokens.monoFont
            font.pixelSize: 9
            font.weight: Font.Bold
            font.letterSpacing: 1.2
        }
        Text {
            y: 22
            width: parent.width
            text: qsTr("Press F3 any time to cycle compact and detailed stream stats.")
            color: DesktopTokens.textMuted
            font.family: DesktopTokens.bodyFont
            font.pixelSize: 11
            wrapMode: Text.WordWrap
        }
    }

    Column {
        anchors.right: parent.right
        anchors.bottom: parent.bottom
        anchors.rightMargin: 32
        anchors.bottomMargin: 32
        spacing: 5
        visible: root.footerVisible
        Text {
            anchors.right: parent.right
            text: root.session.sessionId
                ? qsTr("SESSION %1").arg(String(root.session.sessionId).slice(0, 9).toUpperCase())
                : qsTr("SESSION PENDING")
            color: DesktopTokens.textFaint
            font.family: DesktopTokens.monoFont
            font.pixelSize: 9
            font.weight: Font.Bold
            font.letterSpacing: 0.8
        }
        Text {
            anchors.right: parent.right
            text: root.membershipTier && root.rigName
                ? root.membershipTier + " · " + root.rigName
                : root.membershipTier || root.rigName || qsTr("WAITING FOR RIG ASSIGNMENT")
            color: DesktopTokens.textMuted
            font.family: DesktopTokens.monoFont
            font.pixelSize: 9
            font.weight: Font.DemiBold
            font.letterSpacing: 0.6
        }
    }

    SequentialAnimation {
        running: true
        NumberAnimation { target: launchContent; property: "opacity"; from: 0; to: 1; duration: AppController.reducedMotion ? 0 : 200; easing.type: Easing.OutCubic }
        NumberAnimation { target: revealTranslate; property: "y"; from: AppController.reducedMotion ? 0 : 10; to: 0; duration: AppController.reducedMotion ? 0 : 180; easing.type: Easing.OutCubic }
    }

    Keys.onPressed: event => {
        if (event.isAutoRepeat)
            return
        if (event.key === Qt.Key_Escape || event.key === Qt.Key_Back) {
            root.cancelRequested()
            event.accepted = true
        }
    }
}

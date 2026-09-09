pragma ComponentBehavior: Bound

import QtQuick
import QtQuick.Controls
import QtQuick.Effects
import QtQuick.Window
import OpenNOW

FocusScope {
    id: root
    width: 1440
    height: 900
    property bool opened: false
    readonly property bool present: reveal.present
    visible: present
    enabled: opened
    focus: opened

    signal resumeRequested()
    signal inviteRequested()
    signal consoleModeRequested(bool enabled)
    signal fullscreenRequested()
    signal endSessionRequested()
    signal statsRequested()

    property int selectedIndex: 0
    property int pendingAction: -1
    property bool closing: false
    property double nowMs: Date.now()
    readonly property var game: ShellStore.selectedGame || ({})
    readonly property var session: ShellStore.activeSession || ({})
    readonly property var profile: session.negotiatedStreamProfile || session.streamProfile || ({})
    readonly property var live: ShellStore.streamer || ({})
    readonly property bool liveTelemetryAvailable: ["starting", "negotiating", "connecting", "streaming", "error"]
        .indexOf(String(live.status || "")) >= 0
    readonly property bool modePending: DesktopTokens.consoleModePending(Window.window)
    readonly property bool modeOn: DesktopTokens.consoleModeOn(Window.window)
    readonly property bool fullscreen: Window.window
        && Window.window.visibility === Window.FullScreen
    readonly property int outputHeight: Number(profile.height || 0)
    readonly property bool invitesAvailable: Boolean(ShellStore.socialCapabilities && ShellStore.socialCapabilities.invitesAvailable)

    function liveNumber(value) {
        return value === undefined || value === null || isNaN(Number(value)) ? null : Number(value)
    }
    function firstAvailable(primary, fallback) {
        return primary === undefined || primary === null ? fallback : primary
    }
    function liveValue(value) {
        return root.liveTelemetryAvailable ? value : undefined
    }
    function liveText(value, suffix, fallback) {
        const number = root.liveNumber(value)
        return number === null ? (fallback || qsTr("Measuring"))
            : String(Math.round(number)) + (suffix || "")
    }
    readonly property string fpsText: root.liveText(root.liveValue(root.firstAvailable(live.framesPerSecond, live.fps)), "", qsTr("Measuring"))
    readonly property string bitrateText: root.liveText(root.liveValue(root.firstAvailable(live.bitrateMbps, live.receiveBitrateMbps)), " Mb", qsTr("Measuring"))
    readonly property string codecText: String(root.liveValue(live.codec) || profile.codec
        || ShellStore.runtimeStreamProfile.codec || ShellStore.settings.codec || qsTr("Automatic")).toUpperCase()
    readonly property string transportText: String(root.liveValue(live.transport) || "NVST").toUpperCase()
    readonly property string backendText: String(root.liveValue(live.mediaBackend) || qsTr("Pending")).toUpperCase()
    readonly property string streamerStatus: String(live.status || qsTr("Starting")).toUpperCase()
    readonly property string resolution: outputHeight >= 2160 ? "2160P"
        : outputHeight >= 1440 ? "1440P"
        : outputHeight >= 1080 ? "1080P"
        : outputHeight > 0 ? outputHeight + "P" : qsTr("Pending")

    function runAction(index) {
        if (closing) return
        if (index === 6) {
            ShellStore.toggleMicrophone()
            return
        }
        pendingAction = index
        closing = true
        if (reveal.progress === 0) Qt.callLater(root.finishAction)
    }
    function finishAction() {
        // A hidden callback or rapid toggle must never dispatch the action twice.
        const action = pendingAction
        pendingAction = -1
        if (action === 0) resumeRequested()
        else if (action === 1) inviteRequested()
        else if (action === 2) consoleModeRequested(!root.modeOn)
        else if (action === 3) {
            fullscreenRequested()
            resumeRequested()
        }
        else if (action === 4) endSessionRequested()
        else if (action === 5) statsRequested()
    }
    function clockText() {
        if (ShellStore.streamStartedAtMs <= 0)
            return "—"
        const elapsed = Math.max(0, nowMs - ShellStore.streamStartedAtMs)
        const total = Math.floor(elapsed / 1000)
        const hours = Math.floor(total / 3600)
        const minutes = Math.floor((total % 3600) / 60)
        const seconds = total % 60
        return (hours > 0 ? hours + ":" : "") + String(minutes).padStart(2, "0") + ":" + String(seconds).padStart(2, "0")
    }

    MotionProgress {
        id: reveal
        objectName: "streamMenuMotion"
        shown: root.opened && !root.closing
        exitDuration: 140
        onHidden: if (root.closing && root.pendingAction >= 0) root.finishAction()
    }
    Rectangle {
        anchors.fill: parent
        color: "#B804060A"
        opacity: reveal.progress
        TapHandler { onTapped: root.runAction(0) }
    }

    MultiEffect {
        anchors.fill: panel
        source: panel
        visible: false
        shadowEnabled: true
        shadowColor: "#D9000000"
        shadowOpacity: 0.86
        shadowBlur: 1.0
        shadowVerticalOffset: 22
        shadowHorizontalOffset: 0
    }

    Rectangle {
        id: panel
        x: Math.round((root.width - width) / 2)
        y: Math.round((root.height - height) / 2) - 2
        width: Math.min(760, root.width - 32)
        height: Math.min(460, root.height - 32)
        radius: 20
        color: Theme.lightMode ? Theme.shell : "#F00A0E15"
        border.width: 1
        border.color: "#29FFFFFF"
        opacity: reveal.progress
        scale: reveal.zoom
        transformOrigin: Item.Center

        Item {
            id: header
            x: 20
            y: 18
            width: parent.width - 40
            height: 58

            RoundedArtwork {
                x: 0
                y: 0
                width: 42
                height: 56
                artwork: String(root.game.imageUrl || root.game.heroImageUrl || "")
                cornerRadius: 8
                scrimStart: 1
                fallbackColor: "#1A2030"
            }
            Text {
                x: 56
                y: 4
                width: 300
                text: String(root.game.title || qsTr("GeForce NOW"))
                color: DesktopTokens.text
                font.family: DesktopTokens.displayFont
                font.pixelSize: 20
                font.weight: Font.Black
                elide: Text.ElideRight
            }
            Row {
                x: 56
                y: 35
                spacing: 7
                Rectangle { width: 6; height: 6; radius: 3; anchors.verticalCenter: parent.verticalCenter; color: DesktopTokens.green }
                Text {
                    text: qsTr("LIVE · %1").arg(root.clockText())
                    color: DesktopTokens.textMuted
                    font.family: DesktopTokens.monoFont
                    font.pixelSize: 9
                    font.weight: Font.Bold
                    font.letterSpacing: 0.7
                }
            }
            Row {
                anchors.right: parent.right
                y: 6
                spacing: 26
                Repeater {
                    model: [
                        { label: "FPS", value: root.fpsText },
                        { label: qsTr("OUTPUT"), value: root.resolution },
                        { label: qsTr("BITRATE"), value: root.bitrateText }
                    ]
                    delegate: Column {
                        id: headerMetric
                        required property var modelData
                        spacing: 4
                        Text {
                            anchors.horizontalCenter: parent.horizontalCenter
                            text: headerMetric.modelData.label
                            color: DesktopTokens.textFaint
                            font.family: DesktopTokens.monoFont
                            font.pixelSize: 8
                            font.weight: Font.Bold
                            font.letterSpacing: 1
                        }
                        Text {
                            anchors.horizontalCenter: parent.horizontalCenter
                            text: headerMetric.modelData.value
                            color: DesktopTokens.textHigh
                            font.family: DesktopTokens.monoFont
                            font.pixelSize: 12
                            font.weight: Font.Bold
                        }
                    }
                }
            }
        }

        Rectangle { x: 20; y: 90; width: parent.width - 40; height: 1; color: "#14FFFFFF" }

        Row {
            x: 20
            y: 109
            spacing: 18
            Column {
                id: actions
                width: 352
                spacing: 8
                Repeater {
                    model: [
                        { title: qsTr("Back to game"), detail: qsTr("Esc"), icon: "desktop-play.svg", primary: true, danger: false },
                        { title: qsTr("Invite a friend"), detail: root.invitesAvailable ? qsTr("AVAILABLE") : qsTr("UNAVAILABLE"), icon: "desktop-user-plus.svg", primary: false, danger: false },
                        { title: root.modeOn ? qsTr("Switch to desktop mode") : qsTr("Switch to console mode"), detail: root.modePending ? qsTr("SWITCHING…") : root.modeOn ? qsTr("DESKTOP READY") : qsTr("GAMEPAD READY"), icon: "desktop-gamepad.svg", primary: false, danger: false },
                        { title: root.fullscreen ? qsTr("Exit fullscreen") : qsTr("Go fullscreen"), detail: "F11", icon: "desktop-expand.svg", primary: false, danger: false },
                        { title: qsTr("End session"), detail: "Ctrl Shift Q", icon: "desktop-logout.svg", primary: false, danger: true }
                    ]
                    delegate: Rectangle {
                        id: actionButton
                        required property var modelData
                        required property int index
                        width: actions.width
                        height: index === 0 ? 52 : 48
                        radius: 10
                        color: modelData.primary ? "#F2FFFFFF"
                            : modelData.danger && (actionHover.hovered || root.selectedIndex === index) ? "#29FF8A80"
                            : (actionHover.hovered || root.selectedIndex === index) ? "#1FFFFFFF" : "#0FFFFFFF"
                        border.width: modelData.primary ? 0 : 1
                        border.color: modelData.danger ? "#52FF8A80" : "#1FFFFFFF"
                        scale: actionTap.pressed && !AppController.reducedMotion ? 0.985 : 1
                        Behavior on color { ColorAnimation { duration: DesktopTokens.quickDuration } }
                        Behavior on scale { NumberAnimation { duration: DesktopTokens.quickDuration; easing.type: Easing.OutCubic } }
                        Rectangle {
                            anchors.fill: parent
                            anchors.margins: -2
                            radius: parent.radius + 2
                            color: "transparent"
                            border.width: 2
                            border.color: DesktopTokens.focus
                            visible: root.selectedIndex === actionButton.index
                        }
                        Rectangle {
                            x: 12
                            anchors.verticalCenter: parent.verticalCenter
                            width: 25
                            height: 25
                            radius: 7
                            color: actionButton.modelData.primary ? "#160B0F1A"
                                : actionButton.modelData.danger ? "#1FFF8A80" : "#12FFFFFF"
                            DesktopGlyph {
                                anchors.centerIn: parent
                                width: 16
                                height: 16
                                icon: actionButton.modelData.icon
                            }
                        }
                        Text {
                            x: 48
                            width: parent.width - 164
                            anchors.verticalCenter: parent.verticalCenter
                            text: actionButton.modelData.title
                            color: actionButton.modelData.primary ? "#0B0F1A"
                                : actionButton.modelData.danger ? (Theme.lightMode ? "#9F1239" : "#FFB4AE") : DesktopTokens.textHigh
                            font.family: DesktopTokens.bodyFont
                            font.pixelSize: 12
                            font.weight: Font.Bold
                            elide: Text.ElideRight
                        }
                        Text {
                            anchors.right: parent.right
                            anchors.rightMargin: 14
                            width: 100
                            visible: actionButton.index === 1 || actionButton.index === 2
                            horizontalAlignment: Text.AlignRight
                            anchors.verticalCenter: parent.verticalCenter
                            text: actionButton.modelData.detail
                            color: actionButton.modelData.primary ? "#990B0F1A"
                                : actionButton.modelData.danger ? "#B3FFB4AE" : DesktopTokens.textFaint
                            font.family: DesktopTokens.monoFont
                            font.pixelSize: 8
                            font.weight: Font.Bold
                            font.letterSpacing: 0.7
                        }
                        KeyboardGlyph {
                            visible: actionButton.index === 0 || actionButton.index >= 3
                            anchors.right: parent.right; anchors.rightMargin: 14
                            anchors.verticalCenter: parent.verticalCenter
                            shortcut: visible ? actionButton.modelData.detail : ""
                            keySize: 18
                            ink: actionButton.modelData.primary ? "#0B0F1A" : DesktopTokens.textMuted
                        }
                        HoverHandler { id: actionHover; onHoveredChanged: if (hovered) root.selectedIndex = actionButton.index }
                        TapHandler { id: actionTap; onTapped: root.runAction(actionButton.index) }
                    }
                }
            }

            Column {
                width: 350
                spacing: 12
                Rectangle {
                    width: parent.width
                    height: 130
                    radius: 12
                    color: "#0FFFFFFF"
                    border.width: 1
                    border.color: "#17FFFFFF"
                    Text { x: 14; y: 13; text: qsTr("CONNECTION"); color: DesktopTokens.textFaint; font.family: DesktopTokens.monoFont; font.pixelSize: 8; font.weight: Font.Bold; font.letterSpacing: 1 }
                    Rectangle {
                        anchors.right: parent.right
                        anchors.rightMargin: 13
                        y: 10
                        width: statusText.implicitWidth + 20
                        height: 20
                        radius: 10
                        color: root.streamerStatus === "STREAMING" ? "#1756E6A5" : "#17FFFFFF"
                        border.width: 1
                        border.color: root.streamerStatus === "STREAMING" ? "#3856E6A5" : "#29FFFFFF"
                        Text {
                            id: statusText
                            anchors.centerIn: parent
                            text: root.streamerStatus
                            color: root.streamerStatus === "STREAMING" ? DesktopTokens.green : DesktopTokens.textMuted
                            font.family: DesktopTokens.monoFont
                            font.pixelSize: 8
                            font.weight: Font.Bold
                            font.letterSpacing: 0.8
                        }
                    }
                    Column {
                        x: 14
                        y: 50
                        width: parent.width - 28
                        spacing: 6
                        Repeater {
                            model: [
                                { label: qsTr("TRANSPORT"), value: root.transportText },
                                { label: qsTr("CODEC"), value: root.codecText },
                                { label: qsTr("DECODER"), value: root.backendText }
                            ]
                            delegate: Item {
                                id: connectionRow
                                required property var modelData
                                width: parent.width
                                height: 18
                                Text {
                                    text: connectionRow.modelData.label
                                    color: DesktopTokens.textFaint
                                    font.family: DesktopTokens.monoFont
                                    font.pixelSize: 8
                                    font.weight: Font.Bold
                                    font.letterSpacing: 0.7
                                }
                                Text {
                                    anchors.right: parent.right
                                    anchors.verticalCenter: parent.verticalCenter
                                    text: connectionRow.modelData.value
                                    color: DesktopTokens.textHigh
                                    font.family: DesktopTokens.monoFont
                                    font.pixelSize: 10
                                    font.weight: Font.Bold
                                }
                            }
                        }
                    }
                }
                Rectangle {
                    id: statsCard
                    width: parent.width
                    height: 118
                    radius: 12
                    color: root.selectedIndex === 5 ? "#1FFFFFFF" : "#0FFFFFFF"
                    border.width: root.selectedIndex === 5 ? 2 : 1
                    border.color: root.selectedIndex === 5 ? DesktopTokens.focus : "#17FFFFFF"
                    Text { x: 14; y: 13; text: qsTr("STREAM STATS OVERLAY"); color: DesktopTokens.textFaint; font.family: DesktopTokens.monoFont; font.pixelSize: 8; font.weight: Font.Bold; font.letterSpacing: 1 }
                    Text { x: 14; y: 39; text: qsTr("See frame rate, latency and bitrate\nwithout leaving the game."); color: DesktopTokens.textBody; font.family: DesktopTokens.bodyFont; font.pixelSize: 11; font.weight: Font.DemiBold; lineHeight: 1.25 }
                    KeyboardGlyph { x: 14; y: 83; shortcut: "F3"; keySize: 21; ink: DesktopTokens.textHigh }
                    Text { x: 49; y: 87; text: qsTr("Cycle"); color: DesktopTokens.textMuted; font.family: DesktopTokens.bodyFont; font.pixelSize: 10; font.weight: Font.DemiBold }
                    Text { anchors.right: parent.right; anchors.rightMargin: 14; y: 87; text: qsTr("Hold F3 for details"); color: DesktopTokens.textFaint; font.family: DesktopTokens.bodyFont; font.pixelSize: 10 }
                    HoverHandler { id: statsHover; onHoveredChanged: if (hovered) root.selectedIndex = 5 }
                    TapHandler { onTapped: root.runAction(5) }
                }
                Rectangle {
                    objectName: "streamMicrophoneControl"
                    width: parent.width
                    height: 40
                    radius: 10
                    enabled: ShellStore.microphoneCanToggle
                    opacity: enabled ? 1 : 0.5
                    color: root.selectedIndex === 6 ? "#1FFFFFFF" : "#0FFFFFFF"
                    border.width: root.selectedIndex === 6 ? 2 : 1
                    border.color: root.selectedIndex === 6 ? DesktopTokens.focus : "#17FFFFFF"
                    Accessible.role: Accessible.Button
                    Accessible.name: ShellStore.microphoneActionLabel
                    Accessible.description: ShellStore.microphoneDescription
                    Accessible.onPressAction: ShellStore.toggleMicrophone()
                    Text {
                        x: 14; anchors.verticalCenter: parent.verticalCenter
                        text: ShellStore.microphoneToggleAvailable ? ShellStore.microphoneActionLabel : qsTr("Microphone")
                        color: DesktopTokens.textHigh; font.family: DesktopTokens.bodyFont; font.pixelSize: 11
                    }
                    Text {
                        anchors.right: parent.right; anchors.rightMargin: 14; anchors.verticalCenter: parent.verticalCenter
                        text: ShellStore.microphoneLabel
                        color: DesktopTokens.textMuted; font.family: DesktopTokens.monoFont; font.pixelSize: 9
                    }
                    ToolTip.visible: microphoneHover.hovered || (root.activeFocus && root.selectedIndex === 6)
                    ToolTip.text: ShellStore.microphoneDescription
                    ToolTip.delay: 600
                    HoverHandler { id: microphoneHover; onHoveredChanged: if (hovered) root.selectedIndex = 6 }
                    TapHandler { onTapped: root.runAction(6) }
                }
            }
        }
    }

    Row {
        anchors.horizontalCenter: parent.horizontalCenter
        y: panel.y + panel.height - 27
        spacing: 22
        Repeater {
            model: [{key:"Esc",label:qsTr("Resume")}, {key:"Ctrl K",label:qsTr("Commands")}, {key:"Ctrl Shift Q",label:qsTr("End session")}]
            delegate: DesktopKeyHint {
                required property var modelData
                keyText: modelData.key
                label: modelData.label
                compact: true
            }
        }
    }

    Timer { interval: 1000; repeat: true; running: root.visible; onTriggered: root.nowMs = Date.now() }
    onOpenedChanged: if (opened) {
        closing = false
        pendingAction = -1
        selectedIndex = 0
        forceActiveFocus()
    }
    Keys.onPressed: event => {
        if (event.key === Qt.Key_Escape || event.key === Qt.Key_Back) {
            root.runAction(0)
            event.accepted = true
        } else if (event.key === Qt.Key_Down) {
            root.selectedIndex = Math.min(ShellStore.microphoneCanToggle ? 6 : 5, root.selectedIndex + 1)
            event.accepted = true
        } else if (event.key === Qt.Key_Up) {
            root.selectedIndex = Math.max(0, root.selectedIndex - 1)
            event.accepted = true
        } else if (event.key === Qt.Key_Return || event.key === Qt.Key_Enter || event.key === Qt.Key_Space) {
            root.runAction(root.selectedIndex)
            event.accepted = true
        } else if (event.key === Qt.Key_F3) {
            root.runAction(5)
            event.accepted = true
        } else if (event.key === Qt.Key_Q
                && (event.modifiers & (Qt.ControlModifier | Qt.ShiftModifier
                    | Qt.AltModifier | Qt.MetaModifier))
                    === (Qt.ControlModifier | Qt.ShiftModifier)) {
            root.runAction(4)
            event.accepted = true
        }
    }
}

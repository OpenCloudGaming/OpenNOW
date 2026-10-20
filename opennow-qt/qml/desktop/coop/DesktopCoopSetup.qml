pragma ComponentBehavior: Bound

import QtQuick
import QtQuick.Controls
import OpenNOW

// Host co-op setup sheet over Home (overlay coop-setup), artboard 56C-0.
FocusScope {
    id: root
    objectName: "coopSetupHost"
    property bool opened: false
    readonly property var coop: ShellStore.coop
    readonly property var game: coop.setupGame
    readonly property string gameTitle: game && game.title ? String(game.title) : qsTr("this game")
    readonly property var pads: ControllerInput.availableControllers || []
    readonly property var presetParts: String(coop.preferences.preset || "same").split("-")
    readonly property string presetCodec: presetParts[1] || "same"
    readonly property var codecChoices: [
        {codec: "same", preset: "same", label: qsTr("Same as mine")},
        {codec: "h264", preset: "720p60-h264", label: qsTr("H.264")},
        {codec: "h265", preset: "1080p60-h265", label: qsTr("H.265")},
        {codec: "av1", preset: "1080p60-av1", label: qsTr("AV1")}
    ]
    readonly property int mineMbps: Math.round(Number(ShellStore.settings.maxBitrateMbps || 0))
    readonly property var presets: [
        {mbps: 15, guests: 2, label: qsTr("15 Mbps · 2 guests")},
        {mbps: 12, guests: 3, label: qsTr("12 Mbps · 3 guests")},
        {mbps: mineMbps, guests: Number(coop.preferences.maxGuests || coop.maxGuestsLimit), label: qsTr("Mine")}
    ]
    property int presetIndex: 0
    readonly property bool present: reveal.present
    visible: present
    enabled: opened
    focus: opened
    MotionProgress { id: reveal; shown: root.opened }
    Accessible.role: Accessible.Dialog
    Accessible.name: qsTr("Play co-op")

    function cancel() {
        AppController.showOverlay("")
    }
    function start() {
        const preset = presets[presetIndex]
        coop.startHosting(game, preset.guests, preset.mbps)
    }

    Shortcut {
        sequences: ["Return", "Enter"]
        enabled: root.opened
        context: Qt.WindowShortcut
        autoRepeat: false
        onActivated: root.start()
    }

    Rectangle {
        anchors.fill: parent
        color: "#A6000000"
        opacity: reveal.progress
        TapHandler { onTapped: root.cancel() }
    }

    Rectangle {
        id: sheet
        objectName: "coopSetup"
        opacity: reveal.progress
        scale: reveal.zoom
        x: Math.round((root.width - width) / 2)
        y: Math.max(24, Math.round((root.height - height) * 0.31))
        width: Math.min(640, root.width - 48)
        height: content.implicitHeight + 56
        radius: 28
        color: "#F50E1018"
        border.width: 1
        border.color: "#24FFFFFF"
        MouseArea { anchors.fill: parent }

        component CheckRow: Item {
            id: check
            property bool ok: true
            property string title: ""
            property string body: ""
            property bool last: false
            width: parent ? parent.width : 0
            height: checkText.implicitHeight + 24
            Rectangle {
                y: 12
                width: 22
                height: 22
                radius: 11
                color: check.ok ? "#291DB954" : "#29F5A623"
                DesktopCoopHostIcon {
                    anchors.centerIn: parent
                    width: check.ok ? 13 : 12
                    height: width
                    glyph: check.ok ? "check" : "alert"
                    strokeWidth: check.ok ? 3 : 2
                    ink: check.ok ? "#1DB954" : "#F5A623"
                }
            }
            Column {
                id: checkText
                x: 36
                y: 12
                width: parent.width - x
                spacing: 3
                Text {
                    width: parent.width
                    text: check.title
                    color: "#FFFFFF"
                    font.family: DesktopTokens.bodyFont
                    font.pixelSize: 15
                    font.weight: Font.ExtraBold
                    lineHeight: 18
                    lineHeightMode: Text.FixedHeight
                }
                Text {
                    width: parent.width
                    text: check.body
                    color: "#A3FFFFFF"
                    font.family: DesktopTokens.bodyFont
                    font.pixelSize: 13
                    font.weight: Font.Medium
                    wrapMode: Text.WordWrap
                    lineHeight: 19
                    lineHeightMode: Text.FixedHeight
                }
            }
            Rectangle {
                visible: !check.last
                anchors.bottom: parent.bottom
                width: parent.width
                height: 1
                color: "#0FFFFFFF"
            }
        }

        Column {
            id: content
            x: 30
            y: 28
            width: parent.width - 60
            spacing: 10

            Text {
                text: qsTr("PLAY CO-OP")
                color: "#7FD4FF"
                font.family: DesktopTokens.monoFont
                font.pixelSize: 11
                font.weight: Font.Bold
                font.letterSpacing: 11 * 0.18
            }
            Text {
                width: parent.width
                text: qsTr("Play %1 together").arg(root.gameTitle)
                color: "#FFFFFF"
                font.family: DesktopTokens.displayFont
                font.pixelSize: 28
                font.weight: Font.Black
                font.letterSpacing: -0.28
                lineHeight: 34
                lineHeightMode: Text.FixedHeight
                elide: Text.ElideRight
            }
            Text {
                width: parent.width
                text: qsTr("You stream the game. Up to %1 friends join with a code and a controller.").arg(root.coop.maxGuestsLimit)
                color: "#A3FFFFFF"
                font.family: DesktopTokens.bodyFont
                font.pixelSize: 14
                font.weight: Font.Medium
                wrapMode: Text.WordWrap
            }
            Column {
                width: parent.width
                topPadding: 6
                CheckRow {
                    title: qsTr("Supports local co-op")
                    body: qsTr("Up to 4 players on one screen.")
                }
                CheckRow {
                    ok: root.pads.length > 0
                    title: root.pads.length > 0 ? qsTr("Controller ready") : qsTr("No controller yet")
                    body: root.pads.length > 0
                        ? qsTr("%1 · you're P1.").arg(String(root.pads[0].name || qsTr("Controller")))
                        : qsTr("Connect one to play as P1. Keyboard and mouse stay yours either way.")
                }
                CheckRow {
                    ok: false
                    title: qsTr("Each guest uses your upload")
                    body: qsTr("2 guests at 15 Mbps need 30 Mbps. For 3, start at 12 Mbps: bitrate can't change once it runs.")
                }
            }
            Item {
                width: parent.width
                height: 64
                Column {
                    anchors.verticalCenter: parent.verticalCenter
                    spacing: 3
                    Text {
                        text: root.presetCodec === "same" ? qsTr("Co-op at your stream settings") : qsTr("Co-op %1").arg(String(root.presetParts[0] || ""))
                        color: "#FFFFFF"
                        font.family: DesktopTokens.bodyFont
                        font.pixelSize: 15
                        font.weight: Font.ExtraBold
                    }
                    Text {
                        text: root.presetCodec === "same" ? qsTr("YOUR CODEC · SAME STREAM TO EVERY GUEST")
                            : qsTr("%1 · SAME STREAM TO EVERY GUEST").arg(root.coop.codecName(root.presetCodec).toUpperCase())
                        color: "#52FFFFFF"
                        font.family: DesktopTokens.monoFont
                        font.pixelSize: 9
                        font.weight: Font.DemiBold
                        font.letterSpacing: 0.72
                    }
                }
                Rectangle {
                    anchors.right: parent.right
                    anchors.verticalCenter: parent.verticalCenter
                    width: presetRow.implicitWidth + 10
                    height: 40
                    radius: 20
                    color: "#38000000"
                    border.width: 1
                    border.color: "#24FFFFFF"
                    Row {
                        id: presetRow
                        x: 5
                        anchors.verticalCenter: parent.verticalCenter
                        spacing: 2
                        Repeater {
                            model: root.presets
                            delegate: AbstractButton {
                                id: chip
                                required property var modelData
                                required property int index
                                readonly property bool selected: root.presetIndex === index
                                width: chipLabel.implicitWidth + 28
                                height: 32
                                hoverEnabled: true
                                focusPolicy: Qt.NoFocus
                                enabled: modelData.mbps > 0
                                Accessible.role: Accessible.RadioButton
                                Accessible.name: modelData.label
                                Accessible.checked: selected
                                onClicked: root.presetIndex = index
                                background: Rectangle {
                                    radius: 16
                                    color: chip.selected ? "#7FD4FF" : chip.hovered ? "#14FFFFFF" : "transparent"
                                }
                                contentItem: Text {
                                    id: chipLabel
                                    text: chip.modelData.label
                                    horizontalAlignment: Text.AlignHCenter
                                    verticalAlignment: Text.AlignVCenter
                                    color: chip.selected ? "#0B0F1A" : "#D1FFFFFF"
                                    font.family: DesktopTokens.bodyFont
                                    font.pixelSize: 13
                                    font.weight: chip.selected ? Font.ExtraBold : Font.Bold
                                }
                            }
                        }
                    }
                }
            }
            Item {
                width: parent.width
                height: 56
                Column {
                    anchors.verticalCenter: parent.verticalCenter
                    width: parent.width - codecChips.width - 16
                    spacing: 3
                    Text {
                        text: qsTr("Video format")
                        color: "#FFFFFF"
                        font.family: DesktopTokens.bodyFont
                        font.pixelSize: 15
                        font.weight: Font.ExtraBold
                    }
                    Text {
                        width: parent.width
                        wrapMode: Text.WordWrap
                        text: qsTr("Every guest's device must be able to play it.")
                        color: "#A3FFFFFF"
                        font.family: DesktopTokens.bodyFont
                        font.pixelSize: 12
                        font.weight: Font.Medium
                    }
                }
                Rectangle {
                    id: codecChips
                    anchors.right: parent.right
                    anchors.verticalCenter: parent.verticalCenter
                    width: codecRow.implicitWidth + 10
                    height: 40
                    radius: 20
                    color: "#38000000"
                    border.width: 1
                    border.color: "#24FFFFFF"
                    Row {
                        id: codecRow
                        x: 5
                        anchors.verticalCenter: parent.verticalCenter
                        spacing: 2
                        Repeater {
                            model: root.codecChoices
                            delegate: AbstractButton {
                                id: codecChip
                                required property var modelData
                                readonly property bool selected: root.presetCodec === modelData.codec
                                objectName: "coopSetupCodec-" + modelData.codec
                                width: codecLabel.implicitWidth + 24
                                height: 32
                                hoverEnabled: true
                                focusPolicy: Qt.NoFocus
                                Accessible.role: Accessible.RadioButton
                                Accessible.name: modelData.label
                                Accessible.checked: selected
                                onClicked: if (!selected) root.coop.setPreference("preset", modelData.preset)
                                background: Rectangle {
                                    radius: 16
                                    color: codecChip.selected ? "#7FD4FF" : codecChip.hovered ? "#14FFFFFF" : "transparent"
                                }
                                contentItem: Text {
                                    id: codecLabel
                                    text: codecChip.modelData.label
                                    horizontalAlignment: Text.AlignHCenter
                                    verticalAlignment: Text.AlignVCenter
                                    color: codecChip.selected ? "#0B0F1A" : "#D1FFFFFF"
                                    font.family: DesktopTokens.bodyFont
                                    font.pixelSize: 13
                                    font.weight: codecChip.selected ? Font.ExtraBold : Font.Bold
                                }
                            }
                        }
                    }
                }
            }
            Row {
                anchors.right: parent.right
                topPadding: 6
                spacing: 8
                DesktopCoopHostButton {
                    id: cancelButton
                    objectName: "coopSetupCancel"
                    text: qsTr("Cancel")
                    keyText: "Esc"
                    onClicked: root.cancel()
                }
                DesktopCoopHostButton {
                    id: startButton
                    objectName: "coopSetupStart"
                    text: qsTr("Start co-op")
                    fill: "#FFFFFF"
                    hoverFill: "#14000000"
                    edge: "#7FD4FF"
                    edgeWidth: 2
                    ink: "#0A0D14"
                    glyph: "play"
                    glyphSize: 12
                    keyText: "Enter"
                    onClicked: root.start()
                }
            }
            Text {
                width: parent.width
                horizontalAlignment: Text.AlignRight
                text: qsTr("Your lobby opens over the game. Friends can join while it loads.")
                color: "#52FFFFFF"
                font.family: DesktopTokens.bodyFont
                font.pixelSize: 12
                font.weight: Font.Medium
                wrapMode: Text.WordWrap
            }
        }
    }

    onOpenedChanged: if (opened) {
        presetIndex = 0
        Qt.callLater(() => { if (root.opened) startButton.forceActiveFocus() })
    }
    Keys.onPressed: event => {
        if (event.key === Qt.Key_Escape || event.key === Qt.Key_Back) {
            root.cancel()
        } else if (event.key === Qt.Key_Left || event.key === Qt.Key_Right) {
            const step = event.key === Qt.Key_Left ? -1 : 1
            let next = presetIndex + step
            while (next >= 0 && next < presets.length && presets[next].mbps <= 0)
                next += step
            if (next >= 0 && next < presets.length)
                presetIndex = next
        } else {
            return
        }
        event.accepted = true
    }
}

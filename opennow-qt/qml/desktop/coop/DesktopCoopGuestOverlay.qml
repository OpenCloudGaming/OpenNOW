import QtQuick
import QtQuick.Layouts
import QtQuick.Window
import OpenNOW

// Guest in-stream panel (artboard 5ON-0): relay chip, ping breakdown and the player bar. It stays
// passive over the stream; open(true) hands it keyboard focus for Leave and Close.
FocusScope {
    id: root
    objectName: "coopGuestOverlay"
    readonly property var coop: ShellStore.coop
    readonly property var stats: coop.guestStats
    readonly property bool relay: coop.guestPath === "relay"
    property bool revealed: false
    readonly property bool hovered: pingHover.hovered || barHover.hovered
    readonly property var pads: ControllerInput.availableControllers || []
    signal closeRequested()
    signal controllersRequested()
    Accessible.role: Accessible.Pane
    Accessible.name: qsTr("Co-op")

    function focusPanel() {
        leaveButton.forceActiveFocus()
    }
    function videoLabel(stats) {
        const codec = {h264: qsTr("H.264"), h265: qsTr("H.265"), hevc: qsTr("H.265"), av1: qsTr("AV1")}[String(stats.codec || "").toLowerCase()]
            || String(stats.codec || "").toUpperCase()
        const decoder = stats.decoder === "software" ? qsTr("software decode") : stats.decoder === "hardware" ? qsTr("hardware decode") : ""
        return decoder !== "" ? qsTr("%1 · %2").arg(codec).arg(decoder) : codec
    }
    function qualityLabel(value) {
        return value === "great" ? qsTr("Great") : value === "poor" ? qsTr("Poor") : value === "good" ? qsTr("Good") : qsTr("Checking")
    }

    Keys.onPressed: event => {
        if (event.key === Qt.Key_Escape || event.key === Qt.Key_Back || event.key === Qt.Key_F1) {
            event.accepted = true
            if (!event.isAutoRepeat)
                root.closeRequested()
        }
    }

    DesktopCoopGuestRelayChip {
        x: 24; y: 24
        visible: root.relay
    }

    Rectangle {
        id: pingCard
        x: 24; y: 62
        width: 300
        height: pingColumn.implicitHeight + 28
        radius: 18
        color: "#F00E1018"
        border.width: 1
        border.color: "#24FFFFFF"
        Accessible.role: Accessible.StaticText
        Accessible.name: qsTr("Ping %1 ms").arg(root.coop.msText(root.stats ? root.stats.totalMs : null))
        HoverHandler { id: pingHover }

        Column {
            id: pingColumn
            x: 16; y: 14
            width: parent.width - 32
            spacing: 10
            Item {
                width: parent.width
                height: 24
                DesktopCoopGuestText {
                    anchors.verticalCenter: parent.verticalCenter
                    text: qsTr("Ping")
                    size: 14
                    lineHeightPx: 18
                    font.weight: Font.ExtraBold
                }
                Row {
                    anchors.right: parent.right
                    spacing: 4
                    DesktopCoopGuestText {
                        id: pingValue
                        text: root.coop.msText(root.stats ? root.stats.totalMs : null)
                        color: root.stats && root.stats.quality === "poor" ? "#FF8A80" : "#FFD166"
                        mono: true
                        size: 20
                        lineHeightPx: 24
                        font.weight: Font.Bold
                    }
                    DesktopCoopGuestText {
                        y: pingValue.baselineOffset - baselineOffset
                        text: qsTr("ms")
                        color: "#52FFFFFF"
                        mono: true
                        size: 10
                        lineHeightPx: 12
                    }
                }
            }
            Repeater {
                model: [
                    {label: qsTr("Host → GeForce NOW"), value: qsTr("%1 ms").arg(root.coop.msText(root.stats ? root.stats.hostToGfnPingMs : null))},
                    {label: root.relay ? qsTr("You → host (relay)") : qsTr("You → host (direct)"),
                        value: qsTr("%1 ms").arg(root.coop.msText(root.stats ? root.stats.guestToHostPingMs : null))},
                    {label: qsTr("Connection"), value: root.stats
                        ? qsTr("%1 · %2% loss").arg(root.qualityLabel(root.stats.quality)).arg(Number(root.stats.lossPct || 0).toFixed(1))
                        : root.qualityLabel("")}
                ].concat(root.stats && root.stats.codec ? [{label: qsTr("Video"), value: root.videoLabel(root.stats),
                    warn: root.stats.decoder === "software"}] : [])
                delegate: Item {
                    required property var modelData
                    width: pingColumn.width
                    height: 25
                    Rectangle { width: parent.width; height: 1; color: "#0FFFFFFF" }
                    DesktopCoopGuestText {
                        y: 9
                        text: parent.modelData.label
                        color: "#A3FFFFFF"
                        size: 12
                        lineHeightPx: 16
                    }
                    DesktopCoopGuestText {
                        y: 9
                        anchors.right: parent.right
                        objectName: parent.modelData.warn !== undefined ? "coopGuestVideoValue" : ""
                        text: parent.modelData.value
                        color: parent.modelData.warn ? "#FFD166" : "#FFFFFF"
                        mono: true
                        size: 12
                        lineHeightPx: 16
                    }
                }
            }
        }
    }

    Rectangle {
        id: playerBar
        anchors.horizontalCenter: parent.horizontalCenter
        anchors.bottom: parent.bottom
        anchors.bottomMargin: 36
        width: Math.min(940, parent.width - 48)
        height: 76
        radius: 24
        color: "#F00E1018"
        border.width: 1
        border.color: "#24FFFFFF"
        HoverHandler { id: barHover }

        RowLayout {
            anchors.fill: parent
            anchors.leftMargin: 12
            anchors.rightMargin: 12
            spacing: 8
            Repeater {
                model: Math.max(0, Math.min(4, root.coop.maxPlayers))
                delegate: Rectangle {
                    id: cell
                    required property int index
                    readonly property var member: root.coop.memberAt(index)
                    readonly property bool self: member !== null && index === root.coop.slot
                    Layout.fillWidth: true
                    Layout.preferredWidth: 1
                    Layout.preferredHeight: 52
                    radius: 14
                    color: self ? Qt.rgba(chip.tint.r, chip.tint.g, chip.tint.b, 0.12) : "#08FFFFFF"
                    border.width: member === null ? 0 : 1
                    border.color: self ? Qt.rgba(chip.tint.r, chip.tint.g, chip.tint.b, 0.5) : "#0FFFFFFF"
                    Accessible.role: Accessible.StaticText
                    Accessible.name: member === null ? qsTr("Player %1: open slot").arg(index + 1)
                        : qsTr("Player %1: %2").arg(index + 1).arg(cellName.text)
                    DesktopDashedOutline {
                        anchors.fill: parent
                        visible: cell.member === null
                        stroke: "#0FFFFFFF"
                        radius: cell.radius
                    }
                    Row {
                        x: 12
                        anchors.verticalCenter: parent.verticalCenter
                        spacing: 10
                        DesktopCoopGuestSlotChip {
                            id: chip
                            anchors.verticalCenter: parent.verticalCenter
                            slot: cell.index
                            open: cell.member === null
                        }
                        DesktopCoopGuestText {
                            id: cellName
                            anchors.verticalCenter: parent.verticalCenter
                            width: Math.max(0, cell.width - chip.width - 34)
                            elide: Text.ElideRight
                            text: cell.member === null ? qsTr("Open") : cell.self ? qsTr("You") : String(cell.member.displayName || "")
                            color: cell.member === null ? "#52FFFFFF" : "#FFFFFF"
                            size: 14
                            lineHeightPx: 18
                            font.weight: Font.ExtraBold
                        }
                    }
                }
            }
            Rectangle {
                Layout.leftMargin: 4
                Layout.rightMargin: 4
                Layout.preferredWidth: 1
                Layout.preferredHeight: 40
                color: "#0FFFFFFF"
            }
            DesktopCoopGuestButton {
                id: controllersButton
                objectName: "coopGuestOverlayControllers"
                Layout.preferredHeight: 40
                Layout.preferredWidth: implicitWidth
                text: root.pads.length === 0 ? qsTr("No controller") : root.pads.length === 1 ? qsTr("1 controller")
                    : qsTr("%1 controllers").arg(root.pads.length)
                Accessible.description: qsTr("Dead zones and vibration")
                KeyNavigation.right: leaveButton
                onClicked: root.controllersRequested()
            }
            DesktopCoopGuestButton {
                id: leaveButton
                objectName: "coopGuestOverlayLeave"
                Layout.preferredHeight: 40
                Layout.preferredWidth: implicitWidth
                danger: true
                glyph: "A"
                glyphSize: 18
                text: qsTr("Leave")
                KeyNavigation.left: controllersButton
                KeyNavigation.right: closeHint
                onClicked: root.coop.requestLeave()
            }
            Item {
                id: closeHint
                Layout.preferredWidth: closeRow.implicitWidth
                Layout.preferredHeight: 40
                activeFocusOnTab: true
                Accessible.role: Accessible.Button
                Accessible.name: qsTr("Close")
                KeyNavigation.left: leaveButton
                Keys.onReturnPressed: root.closeRequested()
                Keys.onEnterPressed: root.closeRequested()
                Rectangle {
                    anchors.fill: parent
                    anchors.margins: -4
                    radius: 8
                    color: "transparent"
                    border.width: 2
                    border.color: "#7FD4FF"
                    visible: closeHint.activeFocus
                }
                Row {
                    id: closeRow
                    anchors.verticalCenter: parent.verticalCenter
                    spacing: 6
                    Image {
                        anchors.verticalCenter: parent.verticalCenter
                        width: 18
                        height: 18
                        source: InputPromptIcons.sourceFor("GUIDE", "#FFFFFF")
                        sourceSize: Qt.size(Math.ceil(width * Screen.devicePixelRatio), Math.ceil(height * Screen.devicePixelRatio))
                        fillMode: Image.PreserveAspectFit
                    }
                    DesktopCoopGuestText {
                        anchors.verticalCenter: parent.verticalCenter
                        text: qsTr("Close")
                        color: "#A3FFFFFF"
                        size: 11
                        lineHeightPx: 14
                    }
                }
                TapHandler { onTapped: root.closeRequested() }
            }
        }
    }
}

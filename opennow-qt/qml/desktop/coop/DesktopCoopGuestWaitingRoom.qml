import QtQuick
import QtQuick.Window
import OpenNOW

FocusScope {
    id: root
    objectName: "coopWaitingRoom"
    required property var coop
    readonly property string host: String(coop.hostName || "")
    readonly property var stats: coop.guestStats
    readonly property bool relay: coop.guestPath === "relay"
    readonly property string gameTitle: coop.game && coop.game.title ? String(coop.game.title) : ""
    property string litButton: ""
    focus: true
    Accessible.role: Accessible.Pane
    Accessible.name: titleText.text

    function testRumble() {
        const pads = ControllerInput.availableControllers || []
        const chosen = Number(ControllerInput.inputControllerId || 0)
        const pad = pads.find(item => Number(item.instanceId) === chosen) || pads[0]
        if (pad)
            ControllerInput.testRumble(Number(pad.instanceId))
    }

    Keys.onPressed: event => {
        if (event.key === Qt.Key_Escape || event.key === Qt.Key_Back) {
            event.accepted = true
            if (!event.isAutoRepeat)
                coop.requestLeave()
            return
        }
        const button = event.key === Qt.Key_Return || event.key === Qt.Key_Enter ? "A"
            : event.text === "" && event.key === Qt.Key_X ? "X" : event.text === "" && event.key === Qt.Key_Y ? "Y" : ""
        if (button === "")
            return
        event.accepted = true
        litButton = button
        if (button === "A" && !event.isAutoRepeat)
            testRumble()
    }
    Keys.onReleased: event => {
        if (!event.isAutoRepeat)
            litButton = ""
    }

    DesktopCoopGuestRelayChip {
        x: 718; y: 26
        visible: root.relay
    }

    Rectangle {
        id: slotTile
        x: 200; y: 150
        width: 220
        height: 220
        radius: 32
        color: selfChip.tint
        Accessible.role: Accessible.StaticText
        Accessible.name: qsTr("You are player %1").arg(root.coop.slot + 1)
        DesktopCoopGuestSlotChip { id: selfChip; visible: false; slot: root.coop.slot }
        Column {
            anchors.centerIn: parent
            spacing: 2
            DesktopCoopGuestText {
                anchors.horizontalCenter: parent.horizontalCenter
                text: qsTr("P%1").arg(root.coop.slot + 1)
                color: "#0B0F1A"
                mono: true
                size: 92
                lineHeightPx: 100
                font.weight: Font.Bold
            }
            DesktopCoopGuestText {
                anchors.horizontalCenter: parent.horizontalCenter
                text: qsTr("That's you")
                color: "#0B0F1A"
                size: 16
                lineHeightPx: 20
                font.weight: Font.Black
            }
        }
    }

    Column {
        x: 460
        anchors.verticalCenter: slotTile.verticalCenter
        width: 560
        spacing: 10
        DesktopCoopGuestText {
            text: root.host !== "" ? qsTr("YOU'RE IN %1'S LOBBY").arg(root.host.toUpperCase()) : qsTr("YOU'RE IN THE LOBBY")
            color: "#7FD4FF"
            mono: true
            size: 11
            lineHeightPx: 14
            tracking: 0.18
        }
        DesktopCoopGuestText {
            id: titleText
            width: parent.width
            wrapMode: Text.WordWrap
            objectName: "coopWaitingTitle"
            text: root.coop.admissionPending
                ? (root.host !== "" ? qsTr("Waiting for %1 to let you in").arg(root.host) : qsTr("Waiting for the host to let you in"))
                : root.coop.guestPhase === "starting"
                ? (root.host !== "" ? qsTr("Waiting for %1's stream…").arg(root.host) : qsTr("Waiting for the host's stream…"))
                : root.host !== "" ? qsTr("Waiting for %1 to start").arg(root.host) : qsTr("Waiting for the host to start")
            size: 40
            lineHeightPx: 46
            tracking: -0.01
            font.weight: Font.Black
            Accessible.role: Accessible.Heading
        }
        DesktopCoopGuestText {
            width: parent.width
            elide: Text.ElideRight
            visible: root.gameTitle !== ""
            text: root.host !== "" ? qsTr("%1 · the game is already running on %2's PC").arg(root.gameTitle).arg(root.host)
                : qsTr("%1 · the game is already running on the host's PC").arg(root.gameTitle)
            color: "#A3FFFFFF"
            size: 15
            lineHeightPx: 18
        }
        Item {
            width: parent.width
            height: 26
            Row {
                y: 8
                spacing: 10
            DesktopCoopGuestSpinner { anchors.verticalCenter: parent.verticalCenter; size: 16 }
            DesktopCoopGuestText {
                anchors.verticalCenter: parent.verticalCenter
                text: root.coop.connectStage === "trying_relay" ? qsTr("Trying a relay…")
                    : root.coop.guestPhase === "reconnecting" ? qsTr("Reconnecting to the host…")
                    : root.coop.guestPhase === "starting" ? qsTr("Connected. The video starts in a moment.")
                    : root.coop.admissionPending ? qsTr("Your slot is held. The stream starts once you're let in.")
                    : root.host !== "" ? qsTr("You join the moment %1 presses Start").arg(root.host)
                    : qsTr("You join the moment the host presses Start")
                color: "#7FD4FF"
                size: 14
                lineHeightPx: 18
                font.weight: Font.ExtraBold
            }
            }
        }
    }

    Rectangle {
        x: 200; y: 430
        width: 700
        height: testRow.implicitHeight + 52
        radius: 24
        color: "#08FFFFFF"
        border.width: 1
        border.color: "#0FFFFFFF"
        Row {
            id: testRow
            x: 26
            anchors.verticalCenter: parent.verticalCenter
            spacing: 26
            Image {
                anchors.verticalCenter: parent.verticalCenter
                width: 76
                height: 76
                source: InputPromptIcons.sourceFor("controller", "#FFFFFF")
                sourceSize: Qt.size(Math.ceil(width * Screen.devicePixelRatio), Math.ceil(height * Screen.devicePixelRatio))
                fillMode: Image.PreserveAspectFit
            }
            Column {
                spacing: 8
                DesktopCoopGuestText {
                    text: qsTr("Press any button")
                    size: 24
                    lineHeightPx: 30
                    font.weight: Font.Black
                }
                DesktopCoopGuestText {
                    text: qsTr("Your controller lights up here and rumbles once.")
                    color: "#A3FFFFFF"
                    size: 14
                    lineHeightPx: 18
                    font.weight: Font.Medium
                }
                Item {
                    width: 1
                    height: 54
                Row {
                    y: 6
                    spacing: 10
                    Accessible.role: Accessible.StaticText
                    Accessible.name: qsTr("Controller test")
                    Repeater {
                        model: ["A", "B", "X", "Y"]
                        delegate: Rectangle {
                            id: padButton
                            required property string modelData
                            readonly property bool lit: root.litButton === modelData
                            width: 48
                            height: 48
                            radius: 24
                            color: lit ? "#2E1DB954" : "#0DFFFFFF"
                            border.width: lit ? 2 : 1
                            border.color: lit ? "#1DB954" : "#0FFFFFFF"
                            Image {
                                anchors.centerIn: parent
                                width: 26
                                height: 26
                                source: InputPromptIcons.sourceFor(padButton.modelData, "#FFFFFF")
                                sourceSize: Qt.size(Math.ceil(width * Screen.devicePixelRatio), Math.ceil(height * Screen.devicePixelRatio))
                                fillMode: Image.PreserveAspectFit
                            }
                        }
                    }
                }
                }
            }
        }
    }

    Row {
        x: 200; y: 640
        spacing: 40
        Row {
            spacing: 16
            Repeater {
                model: [
                    {value: root.stats ? root.stats.hostToGfnPingMs : null, label: qsTr("HOST → GEFORCE NOW"), operator: "+"},
                    {value: root.stats ? root.stats.guestToHostPingMs : null, label: qsTr("YOU → HOST"), operator: "="},
                    {value: root.stats ? root.stats.totalMs : null, label: qsTr("TOTAL"), operator: ""}
                ]
                delegate: Row {
                    id: stat
                    required property var modelData
                    readonly property bool total: modelData.operator === ""
                    spacing: 16
                    Column {
                        spacing: 4
                        Row {
                            spacing: 4
                            DesktopCoopGuestText {
                                id: statValue
                                text: root.coop.msText(stat.modelData.value)
                                color: stat.total ? (root.stats && root.stats.quality === "poor" ? "#FF8A80" : "#FFD166") : "#FFFFFF"
                                mono: true
                                size: 34
                                lineHeightPx: 42
                                font.weight: stat.total ? Font.Bold : Font.DemiBold
                            }
                            DesktopCoopGuestText {
                                y: statValue.baselineOffset - baselineOffset
                                text: qsTr("ms")
                                color: "#52FFFFFF"
                                mono: true
                                size: 14
                                lineHeightPx: 18
                            }
                        }
                        DesktopCoopGuestText {
                            text: stat.modelData.label
                            color: "#A3FFFFFF"
                            mono: true
                            size: 10
                            lineHeightPx: 12
                            tracking: 0.08
                        }
                    }
                    DesktopCoopGuestText {
                        visible: !stat.total
                        y: 42 + 4 + 12 - 18 - 30
                        text: stat.modelData.operator
                        color: "#52FFFFFF"
                        mono: true
                        size: 24
                        lineHeightPx: 30
                        font.weight: Font.Medium
                    }
                }
            }
        }
        Column {
            anchors.verticalCenter: parent.verticalCenter
            visible: root.stats !== null
            DesktopCoopGuestText {
                text: root.relay ? qsTr("Relay adds a few ms.") : qsTr("Direct link, no relay.")
                color: "#A3FFFFFF"
                size: 14
                lineHeightPx: 21
            }
            DesktopCoopGuestText {
                text: root.stats && root.stats.quality === "poor" ? qsTr("%1 ms may feel slow.").arg(root.coop.msText(root.stats.totalMs))
                    : qsTr("%1 ms is fine for co-op.").arg(root.coop.msText(root.stats ? root.stats.totalMs : null))
                color: "#A3FFFFFF"
                size: 14
                lineHeightPx: 21
            }
        }
    }

    Column {
        x: 960; y: 430
        width: 440
        spacing: 8
        Repeater {
            model: Math.max(0, Math.min(4, root.coop.maxPlayers))
            delegate: Rectangle {
                id: row
                required property int index
                readonly property var member: root.coop.memberAt(index)
                readonly property bool self: member !== null && index === root.coop.slot
                width: parent.width
                height: 52
                radius: 14
                color: self ? Qt.rgba(chip.tint.r, chip.tint.g, chip.tint.b, 0.10) : "#08FFFFFF"
                border.width: member === null ? 0 : 1
                border.color: self ? Qt.rgba(chip.tint.r, chip.tint.g, chip.tint.b, 0.45) : "#0FFFFFFF"
                Accessible.role: Accessible.StaticText
                Accessible.name: member === null ? qsTr("Player %1: open slot").arg(index + 1)
                    : qsTr("Player %1: %2").arg(index + 1).arg(nameText.text)
                DesktopDashedOutline {
                    anchors.fill: parent
                    visible: row.member === null
                    stroke: "#0FFFFFFF"
                    radius: row.radius
                }
                Row {
                    x: 16
                    anchors.verticalCenter: parent.verticalCenter
                    spacing: 12
                    DesktopCoopGuestSlotChip {
                        id: chip
                        anchors.verticalCenter: parent.verticalCenter
                        slot: row.index
                        open: row.member === null
                    }
                    DesktopCoopGuestText {
                        id: nameText
                        anchors.verticalCenter: parent.verticalCenter
                        width: 260
                        elide: Text.ElideRight
                        text: row.member === null ? qsTr("Open slot") : row.self ? qsTr("You") : String(row.member.displayName || "")
                        color: row.member === null ? "#52FFFFFF" : "#FFFFFF"
                        size: 16
                        lineHeightPx: 20
                        font.weight: Font.ExtraBold
                    }
                }
                Rectangle {
                    id: badgeBox
                    readonly property color tint: row.index === 0 ? "#7FD4FF" : root.relay ? "#F5A623" : "#1DB954"
                    visible: row.member !== null && (row.index === 0 || row.self) && (row.index === 0 || root.coop.guestPath !== "")
                    anchors.right: parent.right
                    anchors.rightMargin: 16
                    anchors.verticalCenter: parent.verticalCenter
                    width: badge.implicitWidth + 20
                    height: 22
                    radius: 11
                    color: Qt.rgba(tint.r, tint.g, tint.b, 0.10)
                    border.width: 1
                    border.color: Qt.rgba(tint.r, tint.g, tint.b, 0.45)
                    Row {
                        id: badge
                        anchors.centerIn: parent
                        spacing: 6
                        Rectangle { anchors.verticalCenter: parent.verticalCenter; width: 6; height: 6; radius: 3; color: badgeBox.tint }
                        DesktopCoopGuestText {
                            anchors.verticalCenter: parent.verticalCenter
                            text: row.index === 0 ? qsTr("HOST") : root.relay ? qsTr("RELAY") : qsTr("DIRECT")
                            color: badgeBox.tint
                            mono: true
                            size: 9
                            lineHeightPx: 12
                            tracking: 0.08
                            font.weight: Font.Bold
                        }
                    }
                }
            }
        }
    }

    DesktopCoopGuestHints {
        x: 200; y: 836
        hints: [{glyph: "A", label: qsTr("Test rumble")}, {glyph: "B", label: qsTr("Leave lobby")}]
        onActivated: index => index === 0 ? root.testRumble() : root.coop.requestLeave()
    }
}

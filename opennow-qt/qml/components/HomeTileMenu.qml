import QtQuick
import QtQuick.Controls
import OpenNOW

FocusScope {
    id: root
    property var game: null
    property string tileSize: "square"
    property int homePosition: 0
    property int selectedAction: 0
    signal moveRequested()
    signal sizeRequested(string size)
    signal detailsRequested()
    signal removeRequested()
    signal closeRequested()

    focus: visible
    z: 80

    readonly property var actions: [
        { title: qsTr("Move tile"), description: qsTr("Pick it up, then use the D-pad to place it"), icon: "↕", color: Theme.focus, height: 68 },
        { title: qsTr("Tile size"), description: qsTr("Choose how much room this game gets"), icon: "", color: Theme.face, height: 68, sizes: true },
        { title: qsTr("Game details"), description: "", icon: "i", color: Theme.violet, height: 58 },
        { title: qsTr("Remove from Home"), description: qsTr("Still available in Library"), icon: "−", color: Theme.coral, height: 58, danger: true }
    ]

    function activate(index) {
        if (index === 0)
            root.moveRequested()
        else if (index === 1)
            root.sizeRequested(root.tileSize === "wide" ? "square" : "wide")
        else if (index === 2)
            root.detailsRequested()
        else if (index === 3)
            root.removeRequested()
    }

    onVisibleChanged: if (visible) {
        selectedAction = 0
        Qt.callLater(actionList.forceActiveFocus)
    }

    Keys.onEscapePressed: {
        root.closeRequested()
        event.accepted = true
    }

    Rectangle {
        anchors.fill: parent
        color: Qt.rgba(0.024, 0.035, 0.071, 0.56)
        MouseArea {
            anchors.fill: parent
            onClicked: root.closeRequested()
        }
    }

    GlassPanel {
        id: menuPanel
        x: 1190
        y: 250
        width: 520
        height: 570
        panelRadius: 36
        strong: true
        color: "#10131C"

        Column {
            x: 24
            y: 24
            width: parent.width - 48
            spacing: 4
            Text {
                text: qsTr("Edit Home tile")
                color: Theme.label
                font.family: Theme.displayFont
                font.pixelSize: 28
                font.weight: Font.Black
                font.letterSpacing: -0.56
            }
            Text {
                width: 330
                text: root.game
                    ? String(root.game.title || qsTr("Game")).toUpperCase()
                        + qsTr(" · Home position %1").arg(root.homePosition + 1)
                    : qsTr("Home tile")
                color: Theme.textMuted
                font.family: Theme.bodyFont
                font.pixelSize: 14
                font.weight: Font.Bold
                elide: Text.ElideRight
            }
        }

        Rectangle {
            x: parent.width - width - 28
            y: 40
            width: rightClickLabel.implicitWidth + 24
            height: 30
            radius: 15
            color: Qt.rgba(1, 1, 1, 0.10)
            Text {
                id: rightClickLabel
                anchors.centerIn: parent
                text: qsTr("Right-click")
                color: Theme.textMuted
                font.family: Theme.bodyFont
                font.pixelSize: 12
                font.weight: Font.ExtraBold
            }
        }

        Rectangle {
            x: 28
            y: 96
            width: parent.width - 56
            height: 1
            color: Theme.seam
        }

        ListView {
            id: actionList
            x: 24
            y: 112
            width: parent.width - 48
            height: 288
            spacing: 12
            clip: false
            focus: true
            currentIndex: root.selectedAction
            keyNavigationWraps: true
            model: root.actions
            onCurrentIndexChanged: root.selectedAction = currentIndex
            Keys.onReturnPressed: root.activate(currentIndex)
            Keys.onEnterPressed: root.activate(currentIndex)
            Keys.onSpacePressed: root.activate(currentIndex)
            Keys.onEscapePressed: root.closeRequested()

            delegate: ItemDelegate {
                id: action
                required property var modelData
                required property int index
                width: ListView.view.width
                height: modelData.height
                padding: 0
                focusPolicy: Qt.StrongFocus
                highlighted: ListView.isCurrentItem
                Accessible.name: modelData.title
                Accessible.description: modelData.description
                onClicked: root.activate(index)

                background: Rectangle {
                    radius: index < 2 ? 22 : 20
                    color: action.highlighted ? Theme.face
                         : modelData.danger ? Qt.rgba(1, 0.541, 0.502, 0.08)
                         : Qt.rgba(1, 1, 1, 0.04)
                    border.color: action.highlighted ? Theme.focus
                                : modelData.danger ? Qt.rgba(1, 0.541, 0.502, 0.44)
                                : Theme.seam
                    border.width: action.highlighted ? 4 : 1
                    Behavior on color { ColorAnimation { duration: Theme.focusDuration } }
                }

                contentItem: Item {
                    Rectangle {
                        visible: !modelData.sizes
                        x: 16
                        anchors.verticalCenter: parent.verticalCenter
                        width: index < 2 ? 32 : 30
                        height: width
                        radius: index < 2 ? 10 : 9
                        color: modelData.color
                        Text {
                            anchors.centerIn: parent
                            text: modelData.icon
                            color: Theme.faceText
                            font.family: Theme.bodyFont
                            font.pixelSize: index === 2 ? 17 : 20
                            font.weight: Font.Black
                        }
                    }

                    Column {
                        x: modelData.sizes ? 16 : 60
                        anchors.verticalCenter: parent.verticalCenter
                        width: modelData.sizes ? 220 : parent.width - x - 70
                        spacing: 2
                        Text {
                            width: parent.width
                            text: modelData.title
                            color: action.highlighted ? Theme.faceText
                                 : modelData.danger ? Theme.coral : Theme.label
                            font.family: Theme.bodyFont
                            font.pixelSize: index < 2 ? 17 : 16
                            font.weight: Font.Black
                            elide: Text.ElideRight
                        }
                        Text {
                            width: parent.width
                            visible: modelData.description.length > 0
                            text: modelData.description
                            color: action.highlighted ? Qt.rgba(0.043, 0.059, 0.102, 0.62) : Theme.textMuted
                            font.family: Theme.bodyFont
                            font.pixelSize: 13
                            font.weight: Font.Bold
                            elide: Text.ElideRight
                        }
                    }

                    Row {
                        visible: Boolean(modelData.sizes)
                        anchors.right: parent.right
                        anchors.rightMargin: 12
                        anchors.verticalCenter: parent.verticalCenter
                        spacing: 6
                        Repeater {
                            model: ["square", "wide"]
                            Rectangle {
                                required property string modelData
                                width: sizeLabel.implicitWidth + 28
                                height: 36
                                radius: 18
                                color: root.tileSize === modelData
                                    ? (action.highlighted ? Theme.faceText : Theme.face)
                                    : "transparent"
                                border.color: root.tileSize === modelData ? "transparent"
                                    : action.highlighted ? Qt.rgba(0.043, 0.059, 0.102, 0.20) : Theme.seam
                                border.width: 1
                                Text {
                                    id: sizeLabel
                                    anchors.centerIn: parent
                                    text: modelData === "wide" ? qsTr("Wide") : qsTr("Square")
                                    color: root.tileSize === modelData
                                        ? (action.highlighted ? Theme.face : Theme.faceText)
                                        : action.highlighted ? Theme.faceText : Theme.textMuted
                                    font.family: Theme.bodyFont
                                    font.pixelSize: 13
                                    font.weight: Font.Black
                                }
                            }
                        }
                    }

                    ControllerGlyph {
                        visible: index === 0
                        anchors.right: parent.right
                        anchors.rightMargin: 16
                        anchors.verticalCenter: parent.verticalCenter
                        glyph: "X"
                        label: ""
                        glyphColor: action.highlighted ? Theme.faceText : Theme.face
                        glyphSize: 28
                    }

                    Text {
                        visible: index === 2
                        anchors.right: parent.right
                        anchors.rightMargin: 18
                        anchors.verticalCenter: parent.verticalCenter
                        text: "›"
                        color: action.highlighted ? Theme.faceText : Theme.label
                        font.pixelSize: 24
                        font.weight: Font.Black
                    }

                    Text {
                        visible: index === 3
                        anchors.right: parent.right
                        anchors.rightMargin: 16
                        anchors.verticalCenter: parent.verticalCenter
                        text: qsTr("Still available in Library")
                        color: action.highlighted ? Qt.rgba(0.043, 0.059, 0.102, 0.62) : Theme.textMuted
                        font.family: Theme.bodyFont
                        font.pixelSize: 13
                        font.weight: Font.Bold
                    }
                }
            }
        }

        Rectangle {
            x: 30
            y: 468
            width: parent.width - 60
            height: 1
            color: Theme.seam
        }

        Row {
            x: 30
            y: 486
            spacing: 14
            ControllerGlyph { glyph: "A"; label: qsTr("Choose"); glyphSize: 24 }
            ControllerGlyph { glyph: "B"; label: qsTr("Close"); glyphSize: 24 }
        }

        Text {
            anchors.right: parent.right
            anchors.rightMargin: 30
            y: 491
            text: qsTr("Changes save automatically")
            color: Theme.textMuted
            font.family: Theme.bodyFont
            font.pixelSize: 12
            font.weight: Font.Bold
        }
    }

    opacity: visible ? 1 : 0
    Behavior on opacity {
        NumberAnimation { duration: Theme.overlayDuration; easing.type: Easing.OutCubic }
    }
}

import QtQuick
import QtQuick.Controls
import OpenNOW

FocusScope {
    id: root
    property bool opened: false
    property bool presented: false
    property alias text: queryField.text
    property var keys: [
        "1", "2", "3", "4", "5", "6", "7", "8", "9", "0",
        "Q", "W", "E", "R", "T", "Y", "U", "I", "O", "P",
        "A", "S", "D", "F", "G", "H", "J", "K", "L", "⌫",
        "Z", "X", "C", "V", "B", "N", "M", "SPACE", "CLEAR", "DONE"
    ]
    signal accepted(string value)
    signal canceled()

    visible: presented
    opacity: opened ? 1 : 0
    focus: opened
    z: 1000

    function openKeyboard(initialText) {
        closeTimer.stop()
        queryField.text = String(initialText || "")
        presented = true
        opened = true
        keyGrid.currentIndex = 10
        Qt.callLater(() => root.forceActiveFocus())
    }

    function closeKeyboard(commit) {
        if (!presented)
            return
        opened = false
        closeTimer.restart()
        if (commit)
            accepted(queryField.text)
        else
            canceled()
    }

    function typeKey(label) {
        if (label === "DONE") {
            closeKeyboard(true)
        } else if (label === "CLEAR") {
            queryField.text = ""
        } else if (label === "⌫") {
            queryField.text = queryField.text.slice(0, -1)
        } else if (label === "SPACE") {
            queryField.text += " "
        } else {
            queryField.text += String(label).toLowerCase()
        }
    }

    Keys.onPressed: event => {
        const columns = 10
        if (event.key === Qt.Key_Escape || event.key === Qt.Key_Back) {
            root.closeKeyboard(false)
        } else if (event.key === Qt.Key_Return || event.key === Qt.Key_Enter || event.key === Qt.Key_Space) {
            root.typeKey(root.keys[keyGrid.currentIndex])
        } else if (event.key === Qt.Key_Left) {
            keyGrid.currentIndex = (keyGrid.currentIndex + root.keys.length - 1) % root.keys.length
        } else if (event.key === Qt.Key_Right) {
            keyGrid.currentIndex = (keyGrid.currentIndex + 1) % root.keys.length
        } else if (event.key === Qt.Key_Up) {
            keyGrid.currentIndex = (keyGrid.currentIndex + root.keys.length - columns) % root.keys.length
        } else if (event.key === Qt.Key_Down) {
            keyGrid.currentIndex = (keyGrid.currentIndex + columns) % root.keys.length
        } else if (event.key === Qt.Key_Backspace) {
            root.typeKey("⌫")
        } else if (event.text && event.text.length === 1 && event.modifiers === Qt.NoModifier) {
            queryField.text += event.text
        } else {
            return
        }
        event.accepted = true
    }

    Timer {
        id: closeTimer
        interval: Theme.overlayDuration
        repeat: false
        onTriggered: root.presented = false
    }

    Rectangle {
        anchors.fill: parent
        color: Qt.rgba(0, 0, 0, 0.46)
        TapHandler { onTapped: root.closeKeyboard(false) }
    }

    GlassPanel {
        anchors.horizontalCenter: parent.horizontalCenter
        y: 420
        width: 1160
        height: 520
        panelRadius: 40
        strong: true
        color: "#10131C"
        scale: root.opened ? 1 : 0.96
        transformOrigin: Item.Bottom
        Behavior on scale { NumberAnimation { duration: Theme.overlayDuration; easing.type: Easing.OutCubic } }

        Text {
            x: 32; y: 24
            text: qsTr("Search the GeForce NOW library")
            color: Theme.label
            font.family: Theme.displayFont
            font.pixelSize: 24
            font.weight: Font.Black
        }
        TextField {
            id: queryField
            x: 32; y: 66
            width: parent.width - 64
            height: 58
            readOnly: true
            color: Theme.label
            placeholderText: qsTr("Start typing…")
            placeholderTextColor: Theme.textMuted
            font.family: Theme.bodyFont
            font.pixelSize: 19
            leftPadding: 22
            background: Rectangle {
                radius: 29
                color: Theme.glassStrong
                border.color: Theme.seam
                border.width: 1
            }
        }

        GridView {
            id: keyGrid
            x: 32; y: 142
            width: parent.width - 64
            height: 324
            cellWidth: width / 10
            cellHeight: 81
            interactive: false
            model: root.keys
            currentIndex: 10
            delegate: ItemDelegate {
                id: keyDelegate
                required property string modelData
                required property int index
                width: keyGrid.cellWidth - 8
                height: keyGrid.cellHeight - 8
                x: 4; y: 4
                padding: 0
                highlighted: GridView.isCurrentItem
                onClicked: {
                    keyGrid.currentIndex = index
                    root.typeKey(modelData)
                }
                background: Rectangle {
                    radius: 20
                    color: keyDelegate.highlighted ? Theme.face : Theme.glass
                    border.color: keyDelegate.highlighted ? Theme.focus : Theme.seam
                    border.width: keyDelegate.highlighted ? 3 : 1
                }
                contentItem: Text {
                    text: modelData
                    horizontalAlignment: Text.AlignHCenter
                    verticalAlignment: Text.AlignVCenter
                    color: keyDelegate.highlighted ? Theme.faceText : Theme.label
                    font.family: Theme.bodyFont
                    font.pixelSize: modelData.length > 1 ? 13 : 19
                    font.weight: Font.Black
                }
            }
        }

        Row {
            x: 36
            y: parent.height - 38
            spacing: 18
            ControllerGlyph { glyph: "A"; label: qsTr("Type"); glyphSize: 22 }
            ControllerGlyph { glyph: "B"; label: qsTr("Cancel"); glyphSize: 22 }
        }
    }

    Behavior on opacity { NumberAnimation { duration: Theme.overlayDuration; easing.type: Easing.OutCubic } }
}

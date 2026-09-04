import QtQuick
import QtQuick.Controls
import OpenNOW

Item {
    id: root
    property string title: ""
    property string description: ""
    property string value: ""
    property int rowHeight: DesktopTokens.rowHeight
    property bool showDivider: true
    property string leadingLetter: ""
    property url leadingIcon: ""
    property color leadingColor: Qt.rgba(1, 1, 1, 0.06)
    readonly property bool hasLeading: leadingLetter !== "" || leadingIcon.toString() !== ""
    default property alias trailing: trailingSlot.data

    implicitHeight: rowHeight

    Rectangle {
        id: leadingTile
        visible: root.hasLeading
        x: 0
        anchors.verticalCenter: parent.verticalCenter
        width: 36
        height: 36
        radius: 10
        color: root.leadingColor
        border.width: 1
        border.color: Qt.rgba(1, 1, 1, 0.14)
        Image {
            anchors.centerIn: parent
            width: 19
            height: 19
            source: root.leadingIcon
            sourceSize: Qt.size(width, height)
            fillMode: Image.PreserveAspectFit
            visible: root.leadingIcon.toString() !== ""
        }
        Text {
            anchors.centerIn: parent
            text: root.leadingLetter
            visible: root.leadingIcon.toString() === ""
            color: Theme.label
            font.family: DesktopTokens.bodyFont
            font.pixelSize: 16
            font.weight: Font.Black
        }
    }

    Column {
        anchors.left: parent.left
        anchors.leftMargin: root.hasLeading ? 50 : 0
        anchors.right: trailingSlot.left
        anchors.rightMargin: 20
        anchors.verticalCenter: parent.verticalCenter
        spacing: 2
        Text {
            width: parent.width
            text: root.title
            color: Theme.label
            font.family: Theme.bodyFont
            font.pixelSize: DesktopTokens.bodySize
            font.weight: Font.Bold
            elide: Text.ElideRight
        }
        Text {
            width: parent.width
            visible: root.description !== ""
            text: root.description
            color: Theme.textMuted
            font.family: Theme.bodyFont
            font.pixelSize: DesktopTokens.captionSize
            font.weight: Font.Medium
            elide: Text.ElideRight
        }
    }

    Row {
        id: trailingSlot
        anchors.right: parent.right
        anchors.verticalCenter: parent.verticalCenter
        spacing: 10
        height: DesktopTokens.controlHeight

        add: Transition {
            ScriptAction { script: root.centerTrailing() }
        }

        Text {
            visible: root.value !== ""
            text: root.value
            color: Qt.rgba(1, 1, 1, 0.80)
            font.family: Theme.monoFont
            font.pixelSize: DesktopTokens.monoSize
            font.weight: Font.Bold
            anchors.verticalCenter: parent.verticalCenter
        }
    }

    function centerTrailing() {
        for (let i = 0; i < trailingSlot.children.length; ++i) {
            const item = trailingSlot.children[i]
            if (item)
                item.anchors.verticalCenter = trailingSlot.verticalCenter
        }
    }

    Component.onCompleted: centerTrailing()

    Rectangle {
        visible: root.showDivider
        anchors.left: parent.left
        anchors.right: parent.right
        anchors.bottom: parent.bottom
        height: 1
        color: Qt.rgba(1, 1, 1, 0.06)
    }
}

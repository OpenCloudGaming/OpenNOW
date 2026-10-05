import QtQuick
import OpenNOW

Item {
    id: root
    property bool focused: false
    property bool parked: false
    property real frameRadius: 22

    anchors.fill: parent

    Rectangle {
        anchors.fill: parent
        anchors.margins: -12
        visible: root.focused && !root.parked
        radius: root.frameRadius + 12
        color: "transparent"
        border.width: 5
        border.color: Qt.rgba(Theme.focus.r, Theme.focus.g, Theme.focus.b, 0.5)
    }

    Rectangle {
        anchors.fill: parent
        anchors.margins: -7
        visible: root.focused && !root.parked
        radius: root.frameRadius + 7
        color: "transparent"
        border.width: 4
        border.color: Theme.shell
    }

    Rectangle {
        anchors.fill: parent
        anchors.margins: root.focused ? -3 : 0
        radius: root.frameRadius + (root.focused ? 3 : 0)
        color: "transparent"
        border.width: root.focused ? (root.parked ? 2 : 3) : 1
        border.color: root.focused ? Theme.face : Theme.seam
    }
}

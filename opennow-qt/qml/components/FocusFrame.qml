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
        anchors.margins: -5
        visible: root.focused && !root.parked
        radius: root.frameRadius + 5
        color: "transparent"
        border.width: 5
        border.color: Qt.rgba(Theme.focus.r, Theme.focus.g, Theme.focus.b, 0.45)
    }

    Rectangle {
        anchors.fill: parent
        radius: root.frameRadius
        color: "transparent"
        border.width: root.focused ? (root.parked ? 2 : 3) : 1
        border.color: !root.focused ? Theme.seam
            : root.parked ? Qt.rgba(Theme.face.r, Theme.face.g, Theme.face.b, 0.7) : Theme.face
    }
}

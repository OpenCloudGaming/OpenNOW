import QtQuick
import QtQuick.Controls
import OpenNOW

Switch {
    id: root
    implicitWidth: 60
    implicitHeight: 34
    focusPolicy: Qt.StrongFocus
    indicator: Rectangle {
        anchors.fill: parent
        radius: height / 2
        color: root.checked ? Theme.mint : Qt.rgba(1, 1, 1, 0.18)
        border.color: root.activeFocus ? Theme.focus : Theme.seam
        border.width: root.activeFocus ? 3 : 1
        Rectangle {
            width: 26; height: 26; radius: 13
            x: root.checked ? parent.width - width - 4 : 4
            anchors.verticalCenter: parent.verticalCenter
            color: Theme.face
            Behavior on x { NumberAnimation { duration: Theme.focusDuration; easing.type: Easing.OutCubic } }
        }
        Behavior on color { ColorAnimation { duration: Theme.focusDuration } }
    }
    contentItem: Item {}
}

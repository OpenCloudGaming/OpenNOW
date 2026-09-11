import QtQuick
import QtQuick.Effects
import OpenNOW

Item {
    id: root
    property bool focused: false
    property real frameRadius: 34

    anchors.fill: parent
    anchors.margins: focused ? -7 : 0

    Rectangle {
        anchors.fill: parent
        visible: root.focused
        radius: root.frameRadius + 7
        color: "transparent"
        border.width: 5
        border.color: Theme.focus
        layer.enabled: root.focused
        layer.effect: MultiEffect {
            shadowEnabled: true
            shadowColor: Qt.rgba(0.5, 0.83, 1, 0.35)
            shadowBlur: 0.7
            shadowHorizontalOffset: 0
            shadowVerticalOffset: 10
        }
    }

    Rectangle {
        anchors.fill: parent
        anchors.margins: root.focused ? 7 : 0
        radius: root.frameRadius
        color: "transparent"
        border.width: 3
        border.color: Qt.rgba(1, 1, 1, 0.92)
    }

    Behavior on anchors.margins {
        NumberAnimation { duration: Theme.focusDuration; easing.type: Easing.OutCubic }
    }
}

import QtQuick
import OpenNOW

Rectangle {
    id: root
    property int slot: 0
    property bool open: false
    readonly property color tint: ["#7FD4FF", "#6EE7B7", "#FFD166", "#A78BFA"][Math.max(0, Math.min(3, slot))]
    width: Math.max(30, label.implicitWidth + 14)
    height: 22
    radius: 7
    color: open ? "transparent" : tint
    border.width: open ? 0 : 1
    border.color: tint
    Accessible.ignored: true

    DesktopDashedOutline {
        anchors.fill: parent
        visible: root.open
        stroke: root.tint
        radius: root.radius
    }
    DesktopCoopGuestText {
        id: label
        anchors.centerIn: parent
        text: qsTr("P%1").arg(root.slot + 1)
        color: root.open ? root.tint : "#0B0F1A"
        mono: true
        size: 11
        lineHeightPx: 14
        font.weight: Font.Bold
    }
}

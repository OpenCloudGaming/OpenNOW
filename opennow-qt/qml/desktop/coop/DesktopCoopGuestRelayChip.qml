import QtQuick
import OpenNOW

Rectangle {
    id: root
    objectName: "coopGuestRelayChip"
    width: content.implicitWidth + 26
    height: 28
    radius: 14
    color: "#DB0E1018"
    border.width: 1
    border.color: "#66F5A623"
    Accessible.role: Accessible.StaticText
    Accessible.name: chipText.text

    Row {
        id: content
        x: 12
        anchors.verticalCenter: parent.verticalCenter
        spacing: 8
        DesktopCoopGuestIcon {
            anchors.verticalCenter: parent.verticalCenter
            size: 13
            stroke: "#F5A623"
            path: "M3 19a3 3 0 1 0 6 0a3 3 0 1 0 -6 0M9 19h8.5a3.5 3.5 0 0 0 0-7h-11a3.5 3.5 0 0 1 0-7H15M15 5a3 3 0 1 0 6 0a3 3 0 1 0 -6 0"
        }
        DesktopCoopGuestText {
            id: chipText
            anchors.verticalCenter: parent.verticalCenter
            text: qsTr("You're using relay")
            color: "#FFD166"
            size: 12
            lineHeightPx: 16
            font.weight: Font.ExtraBold
        }
    }
}

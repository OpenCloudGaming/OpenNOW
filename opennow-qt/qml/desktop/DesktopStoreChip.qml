import QtQuick
import QtQuick.Controls
import OpenNOW

Button {
    id: root

    property bool selected: false
    property bool hasMenu: false

    implicitWidth: contentRow.implicitWidth + 26
    implicitHeight: 30
    padding: 0
    focusPolicy: Qt.NoFocus
    hoverEnabled: true
    Accessible.role: Accessible.Button
    Accessible.name: text

    background: Rectangle {
        radius: 9
        color: root.selected || root.down ? Qt.rgba(1, 1, 1, 0.12)
                                          : root.hovered ? Qt.rgba(1, 1, 1, 0.07)
                                                         : Qt.rgba(1, 1, 1, 0.04)
        border.width: 1
        border.color: root.selected ? Qt.rgba(1, 1, 1, 0.18)
                                    : root.hovered ? Qt.rgba(1, 1, 1, 0.13)
                                                   : Qt.rgba(1, 1, 1, 0.08)

        Behavior on color {
            ColorAnimation { duration: Theme.focusDuration }
        }
        Behavior on border.color {
            ColorAnimation { duration: Theme.focusDuration }
        }
    }

    contentItem: Row {
        id: contentRow
        anchors.centerIn: parent
        spacing: root.hasMenu ? 7 : 0

        Text {
            anchors.verticalCenter: parent.verticalCenter
            text: root.text
            color: root.selected ? "#FFFFFF" : Qt.rgba(1, 1, 1, 0.64)
            font.family: Theme.bodyFont
            font.pixelSize: 12
            font.weight: root.selected ? Font.ExtraBold : Font.DemiBold
        }

        DesktopGlyph {
            anchors.verticalCenter: parent.verticalCenter
            visible: root.hasMenu
            width: 10
            height: 6
            icon: "desktop-chevron-down.svg"
        }
    }
}

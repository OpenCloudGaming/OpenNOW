import QtQuick
import OpenNOW

Rectangle {
    id: root
    property string store: ""

    implicitWidth: chipRow.implicitWidth + 30
    implicitHeight: 44
    radius: height / 2
    color: Qt.rgba(Theme.face.r, Theme.face.g, Theme.face.b, 0.07)
    border.color: Theme.seam
    border.width: 1
    Accessible.role: Accessible.StaticText
    Accessible.name: ConsoleStores.label(store)

    Row {
        id: chipRow
        x: 8
        anchors.verticalCenter: parent.verticalCenter
        spacing: 12
        ConsoleStoreMark {
            anchors.verticalCenter: parent.verticalCenter
            store: root.store
            markSize: 30
        }
        Text {
            anchors.verticalCenter: parent.verticalCenter
            text: ConsoleStores.label(root.store)
            color: Theme.label
            font.family: Theme.bodyFont
            font.pixelSize: 17
            font.weight: Font.Bold
        }
    }
}

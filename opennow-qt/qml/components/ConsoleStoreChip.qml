import QtQuick
import OpenNOW

Rectangle {
    id: root
    property string store: ""
    property real markSize: 30
    property real inset: 5

    implicitWidth: chipRow.implicitWidth + inset + 16
    implicitHeight: markSize + inset * 2
    radius: height / 2
    color: Qt.rgba(Theme.face.r, Theme.face.g, Theme.face.b, 0.07)
    border.color: Theme.seam
    border.width: 1
    Accessible.role: Accessible.StaticText
    Accessible.name: ConsoleStores.label(store)

    Row {
        id: chipRow
        x: root.inset
        anchors.verticalCenter: parent.verticalCenter
        spacing: 10
        ConsoleStoreMark {
            anchors.verticalCenter: parent.verticalCenter
            store: root.store
            markSize: root.markSize
        }
        Text {
            anchors.verticalCenter: parent.verticalCenter
            text: ConsoleStores.label(root.store)
            color: Theme.label
            font.family: Theme.bodyFont
            font.pixelSize: 16
            font.weight: Font.ExtraBold
        }
    }
}

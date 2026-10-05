import QtQuick
import QtQuick.Window
import OpenNOW

Rectangle {
    id: root
    property string store: ""
    property real markSize: 32
    readonly property url iconSource: ConsoleStores.iconUrl(store)

    width: markSize
    height: markSize
    radius: markSize / 2
    color: ConsoleStores.color(store)
    border.color: Qt.rgba(1, 1, 1, 0.18)
    border.width: 1
    Accessible.role: Accessible.Graphic
    Accessible.name: ConsoleStores.label(store)

    Image {
        anchors.centerIn: parent
        width: Math.round(root.markSize * 0.56)
        height: width
        visible: root.iconSource.toString() !== ""
        source: root.iconSource
        sourceSize: Qt.size(width * Screen.devicePixelRatio, height * Screen.devicePixelRatio)
        fillMode: Image.PreserveAspectFit
    }
    Text {
        anchors.centerIn: parent
        visible: root.iconSource.toString() === ""
        text: ConsoleStores.label(root.store).slice(0, 1).toUpperCase()
        color: Theme.mediaForeground
        font.family: Theme.displayFont
        font.pixelSize: Math.round(root.markSize * 0.42)
        font.weight: Font.Black
    }
}

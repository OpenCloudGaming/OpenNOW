import QtQuick
import OpenNOW

Rectangle {
    id: root
    property string storeGlyph: "S"
    property color storeColor: Theme.cartSteam
    property int badgeSize: 30

    width: badgeSize
    height: badgeSize
    radius: badgeSize === 30 ? 9 : 8
    color: storeColor
    border.color: Qt.rgba(1, 1, 1, 0.85)
    border.width: 2

    function storeIcon() {
        const key = root.storeGlyph.toUpperCase()
        if (key === "S") return "qrc:/qt/qml/OpenNOW/res/icons/store-steam.svg"
        if (key === "E") return "qrc:/qt/qml/OpenNOW/res/icons/store-epic.svg"
        if (key === "U") return "qrc:/qt/qml/OpenNOW/res/icons/store-ubisoft.svg"
        if (key === "B") return "qrc:/qt/qml/OpenNOW/res/icons/store-battlenet.svg"
        return ""
    }

    Image {
        anchors.centerIn: parent
        width: root.badgeSize === 30 ? 16 : 15
        height: width
        source: root.storeIcon()
        sourceSize: Qt.size(width, height)
        fillMode: Image.PreserveAspectFit
        visible: source.toString().length > 0
    }
    Text {
        anchors.centerIn: parent
        text: root.storeGlyph
        visible: root.storeIcon().length === 0
        color: Theme.mediaForeground
        font.family: Theme.displayFont
        font.pixelSize: root.badgeSize === 30 ? 12 : 11
        font.weight: Font.Black
    }
}

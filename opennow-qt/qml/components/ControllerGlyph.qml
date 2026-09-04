import QtQuick
import OpenNOW

Row {
    id: root
    Accessible.ignored: true
    property string glyph: "A"
    property string label: qsTr("Select")
    property color glyphColor: Theme.face
    property real glyphSize: 26
    spacing: 8

    Rectangle {
        width: root.glyphSize
        height: root.glyphSize
        radius: root.glyph.length > 2 ? 7 : root.glyphSize / 2
        color: root.glyphColor

        Text {
            anchors.centerIn: parent
            text: root.glyph
            color: Theme.contrastText(root.glyphColor)
            font.family: Theme.displayFont
            font.pixelSize: root.glyph.length > 2 ? 11 : 12
            font.weight: Font.Black
        }
    }

    Text {
        anchors.verticalCenter: parent.verticalCenter
        text: I18n.source(root.label, I18n.revision)
        color: Theme.label
        font.family: Theme.bodyFont
        font.pixelSize: 16
        font.weight: Font.Bold
    }
}

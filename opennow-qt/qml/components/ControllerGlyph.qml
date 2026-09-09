import QtQuick
import QtQuick.Window
import OpenNOW

Row {
    id: root
    Accessible.ignored: true
    property string glyph: "A"
    property string label: qsTr("Select")
    property color glyphColor: Theme.face
    property real glyphSize: 26
    property bool keyboard: false
    spacing: 8

    Item {
        width: root.glyphSize
        height: root.glyphSize

        Image {
            id: icon
            anchors.fill: parent
            source: root.keyboard ? InputPromptIcons.keyboardSourceFor(root.glyph, root.glyphColor)
                : InputPromptIcons.sourceFor(root.glyph, root.glyphColor)
            sourceSize: Qt.size(Math.max(1, width * Screen.devicePixelRatio), Math.max(1, height * Screen.devicePixelRatio))
            fillMode: Image.PreserveAspectFit
            opacity: root.glyphColor.a
        }

        Rectangle {
            anchors.fill: parent
            visible: icon.source.toString() === "" && root.glyph !== ""
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
    }

    Text {
        visible: root.label !== ""
        anchors.verticalCenter: parent.verticalCenter
        text: I18n.source(root.label, I18n.revision)
        color: Theme.label
        font.family: Theme.bodyFont
        font.pixelSize: 16
        font.weight: Font.Bold
    }
}

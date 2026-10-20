import QtQuick
import QtQuick.Window
import OpenNOW

Row {
    id: root
    property var hints: []
    signal activated(int index)
    spacing: 14

    Repeater {
        model: root.hints
        delegate: Row {
            id: hint
            required property var modelData
            required property int index
            spacing: 6
            Accessible.role: Accessible.Button
            Accessible.name: String(modelData.label)
            Image {
                anchors.verticalCenter: parent.verticalCenter
                width: 19
                height: 19
                source: InputPromptIcons.sourceFor(String(hint.modelData.glyph), "#FFFFFF")
                sourceSize: Qt.size(Math.ceil(width * Screen.devicePixelRatio), Math.ceil(height * Screen.devicePixelRatio))
                fillMode: Image.PreserveAspectFit
            }
            DesktopCoopGuestText {
                anchors.verticalCenter: parent.verticalCenter
                text: String(hint.modelData.label)
                color: "#A3FFFFFF"
                size: 11
                lineHeightPx: 14
            }
            TapHandler { onTapped: root.activated(hint.index) }
        }
    }
}

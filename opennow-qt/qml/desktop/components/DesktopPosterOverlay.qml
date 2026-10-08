import QtQuick
import OpenNOW

Column {
    id: root
    required property var game
    property bool showPlay: true
    property string note: ""
    spacing: 7

    Text {
        width: parent.width
        text: root.game ? String(root.game.title || qsTr("Game")) : qsTr("Game")
        textFormat: Text.PlainText
        color: Theme.mediaForeground
        elide: Text.ElideRight
        font.family: DesktopTokens.bodyFont
        font.pixelSize: 12
        font.weight: Font.Bold
    }

    Text {
        width: parent.width
        visible: root.note !== ""
        text: root.note
        textFormat: Text.PlainText
        color: Theme.mediaForeground
        opacity: 0.78
        elide: Text.ElideRight
        font.family: DesktopTokens.bodyFont
        font.pixelSize: 11
        font.weight: Font.DemiBold
    }

    Rectangle {
        width: parent.width
        visible: root.showPlay && root.note === ""
        height: 32
        radius: 8
        color: "#F2FFFFFF"

        Row {
            anchors.centerIn: parent
            spacing: 6

            DesktopGlyph {
                anchors.verticalCenter: parent.verticalCenter
                width: 18
                height: 18
                icon: "desktop-play-filled.svg"
                sourceSize: Qt.size(Math.ceil(width * dpr * DesktopTokens.cardHoverScale),
                                    Math.ceil(height * dpr * DesktopTokens.cardHoverScale))
                smooth: true
            }
            Text {
                anchors.verticalCenter: parent.verticalCenter
                text: qsTr("Play")
                color: "#0B0F1A"
                font.family: DesktopTokens.bodyFont
                font.pixelSize: 12
                font.weight: Font.Bold
            }
        }
    }
}

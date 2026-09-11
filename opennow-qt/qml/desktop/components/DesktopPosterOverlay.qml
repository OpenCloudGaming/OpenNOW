import QtQuick
import OpenNOW

Column {
    id: root
    required property var game
    spacing: 7

    Text {
        width: parent.width
        text: root.game ? String(root.game.title || qsTr("Game")) : qsTr("Game")
        color: Theme.mediaForeground
        elide: Text.ElideRight
        font.family: DesktopTokens.bodyFont
        font.pixelSize: 12
        font.weight: Font.Bold
    }

    Rectangle {
        width: parent.width
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

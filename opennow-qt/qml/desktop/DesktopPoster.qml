import QtQuick
import QtQuick.Controls
import OpenNOW

ItemDelegate {
    id: root
    property var game: null
    property bool showTitle: false
    property bool showPlay: false
    property bool selected: activeFocus
    property int tileWidth: DesktopTokens.libraryCellWidth
    property int tileHeight: showTitle ? DesktopTokens.px(248) : DesktopTokens.libraryCellHeight
    readonly property int artGutter: 6
    readonly property int artWidth: Math.max(1, tileWidth - artGutter * 2)
    readonly property int artHeight: Math.round(artWidth * 198 / 132)
    readonly property bool cardLifted: hovered || activeFocus
    signal contextRequested(real sceneX, real sceneY)
    // Keep focus geometry inside the delegate so GridView clipping never cuts
    // off the top/left ring while the tile scales up.
    width: tileWidth
    height: tileHeight
    padding: 0
    hoverEnabled: true
    focusPolicy: Qt.StrongFocus
    scale: down ? 0.985 : (cardLifted ? DesktopTokens.cardHoverScale : 1)
    transformOrigin: Item.Center
    z: cardLifted ? 20 : 1
    Behavior on scale {
        NumberAnimation { duration: Theme.focusDuration; easing.type: Easing.OutCubic }
    }
    background: Item {}
    contentItem: Item {
        RoundedArtwork {
            id: art
            x: root.artGutter; y: root.artGutter; width: root.artWidth; height: root.artHeight
            artwork: DesktopTokens.artworkUrl(root.game, false)
            cornerRadius: 12
            scrimStart: root.cardLifted || root.showPlay ? 0.48 : 1
            fallbackColor: "#1A2232"
        }
        Rectangle {
            x: art.x - DesktopTokens.cardOutlinePad
            y: art.y - DesktopTokens.cardOutlinePad
            width: art.width + DesktopTokens.cardOutlinePad * 2
            height: art.height + DesktopTokens.cardOutlinePad * 2
            radius: 14
            color: "transparent"
            border.width: root.cardLifted ? 2 : 1
            border.color: root.cardLifted ? DesktopTokens.focus : DesktopTokens.cardOutlineIdle
            Behavior on border.color {
                ColorAnimation { duration: Theme.focusDuration }
            }
        }
        Column {
            x: root.artGutter + 9
            anchors.bottom: art.bottom
            anchors.bottomMargin: 12
            width: root.artWidth - 18
            spacing: 7
            visible: root.cardLifted && !root.showTitle
            Text { width: parent.width; text: root.game ? String(root.game.title || qsTr("Game")) : qsTr("Game"); color: DesktopTokens.text; elide: Text.ElideRight; font.family: DesktopTokens.bodyFont; font.pixelSize: 12; font.weight: Font.Bold }
            Rectangle {
                width: parent.width; height: 26; radius: 8; color: "#F2FFFFFF"
                Row {
                    anchors.centerIn: parent
                    spacing: 6
                    DesktopGlyph { width: 8; height: 10; icon: "desktop-play.svg" }
                    Text { text: qsTr("Play"); color: DesktopTokens.shell; font.family: DesktopTokens.bodyFont; font.pixelSize: 11; font.weight: Font.Bold }
                }
            }
        }
        Column {
            x: root.artGutter; y: root.artGutter + root.artHeight + 6; width: root.artWidth; spacing: 5
            visible: root.showTitle
            Text { width: parent.width; text: root.game ? String(root.game.title || qsTr("Game")) : qsTr("Game"); color: DesktopTokens.textHigh; elide: Text.ElideRight; font.family: DesktopTokens.bodyFont; font.pixelSize: 12; font.weight: Font.Bold }
            Text { width: parent.width; text: root.game && root.game.price ? String(root.game.price) : qsTr("Available"); color: DesktopTokens.textMuted; elide: Text.ElideRight; font.family: DesktopTokens.monoFont; font.pixelSize: 10; font.weight: Font.DemiBold }
        }
    }
    TapHandler {
        acceptedButtons: Qt.RightButton
        onTapped: point => {
            const scene = root.mapToItem(null, point.position.x, point.position.y)
            root.contextRequested(scene.x, scene.y)
        }
    }
}

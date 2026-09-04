pragma ComponentBehavior: Bound

import QtQuick
import OpenNOW

Item {
    id: root

    required property var game
    property int tileWidth: DesktopTokens.posterWidth
    property int tileHeight: DesktopTokens.posterHeight
    property bool current: false
    readonly property bool pointerHover: hoverHandler.hovered
    readonly property bool navigationFocus: current && AppController.inputMode !== "pointer"
    readonly property bool highlighted: pointerHover || navigationFocus
    readonly property string artwork: DesktopTokens.artworkUrl(game, false)

    signal activated()
    signal pointed()

    width: tileWidth
    height: tileHeight
    scale: highlighted ? DesktopTokens.cardHoverScale : 1
    transformOrigin: Item.Center
    z: highlighted ? 20 : 0
    Accessible.role: Accessible.Button
    Accessible.name: game ? String(game.title || qsTr("Game")) : qsTr("Game")

    Behavior on scale {
        NumberAnimation { duration: Theme.focusDuration; easing.type: Easing.OutCubic }
    }

    RoundedArtwork {
        anchors.fill: parent
        artwork: root.artwork
        fallbackColor: "#171B27"
        cornerRadius: 12
        scrimStart: 1
    }

    Rectangle {
        x: -DesktopTokens.cardOutlinePad
        y: -DesktopTokens.cardOutlinePad
        width: root.tileWidth + DesktopTokens.cardOutlinePad * 2
        height: root.tileHeight + DesktopTokens.cardOutlinePad * 2
        radius: 14
        color: "transparent"
        border.width: root.highlighted ? 2 : 1
        border.color: root.highlighted ? DesktopTokens.focus : DesktopTokens.cardOutlineIdle

        Behavior on border.color {
            ColorAnimation { duration: Theme.focusDuration }
        }
    }

    HoverHandler {
        id: hoverHandler
        acceptedDevices: PointerDevice.Mouse | PointerDevice.TouchPad | PointerDevice.Stylus
        cursorShape: Qt.PointingHandCursor
        onHoveredChanged: if (hovered) root.pointed()
    }

    TapHandler {
        acceptedDevices: PointerDevice.Mouse | PointerDevice.TouchPad | PointerDevice.Stylus | PointerDevice.TouchScreen
        onTapped: root.activated()
    }
}

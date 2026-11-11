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
    signal playRequested()
    signal pointed()

    function activateAt(position) {
        const play = overlay.playButton
        if (overlay.visible && play.contains(play.mapFromItem(root, position.x, position.y)))
            root.playRequested()
        else
            root.activated()
    }

    objectName: "desktopHomePoster-" + String(game && game.id || "")
    width: tileWidth
    height: tileHeight
    z: highlighted || visual.scale !== 1 ? 20 : 0
    Accessible.role: Accessible.Button
    Accessible.name: game ? String(game.title || qsTr("Game")) : qsTr("Game")
    Accessible.description: labels.summary

    Item {
        id: visual
        anchors.fill: parent
        scale: !AppController.reducedMotion && root.highlighted ? DesktopTokens.cardHoverScale : 1
        transformOrigin: Item.Center
        Behavior on scale { NumberAnimation { duration: Theme.focusDuration; easing.type: Easing.OutCubic } }
    RoundedArtwork {
        anchors.fill: parent
        artwork: root.artwork
        fallbackTitle: String(root.game && root.game.title || "")
        fallbackColor: "#171B27"
        cornerRadius: 12
        scrimStart: root.highlighted ? 0.3 : 1
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

    DesktopPosterOverlay {
        id: overlay
        anchors.left: parent.left
        anchors.right: parent.right
        anchors.bottom: parent.bottom
        anchors.leftMargin: 9
        anchors.rightMargin: 9
        anchors.bottomMargin: 12
        game: root.game
        visible: root.highlighted
    }

    DesktopGameLabels {
        id: labels
        x: 8
        y: 8
        width: root.tileWidth - 16
        height: root.tileHeight - 16
        game: root.game
        liftedBottom: overlay.visible ? overlay.height + 10 : 0
    }

    } // visual; hit-test handlers remain outside the transformed item

    DesktopGameLabelTip {
        anchors.fill: parent
        game: root.game
        requested: root.highlighted
    }

    HoverHandler {
        id: hoverHandler
        acceptedDevices: PointerDevice.Mouse | PointerDevice.TouchPad | PointerDevice.Stylus
        cursorShape: Qt.PointingHandCursor
        onHoveredChanged: if (hovered) root.pointed()
    }

    TapHandler {
        acceptedDevices: PointerDevice.Mouse | PointerDevice.TouchPad | PointerDevice.Stylus | PointerDevice.TouchScreen
        onTapped: eventPoint => root.activateAt(eventPoint.position)
    }
}

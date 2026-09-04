import QtQuick
import QtQuick.Controls
import OpenNOW

Item {
    id: root

    property var game: ({})
    property bool selected: false
    property string price: ""
    property string discount: ""
    property bool owned: false
    property bool freeToPlay: false
    property color fallbackColor: Theme.cartSteam
    property int tileWidth: 132
    property int tileHeight: 250
    readonly property int artHeight: Math.round(tileWidth * 198 / 132)

    signal activated(var game)
    signal pointed()

    width: tileWidth
    height: tileHeight
    scale: selected ? DesktopTokens.cardHoverScale : 1
    transformOrigin: Item.Center
    z: selected ? 20 : 0
    Accessible.role: Accessible.Button
    Accessible.name: String(game && game.title || qsTr("Game"))

    Behavior on scale {
        NumberAnimation { duration: Theme.focusDuration; easing.type: Easing.OutCubic }
    }

    RoundedArtwork {
        id: cover
        x: 0
        y: 0
        width: root.tileWidth
        height: root.artHeight
        cornerRadius: 12
        scrimStart: 1
        artwork: DesktopTokens.artworkUrl(root.game, false)
        fallbackColor: root.fallbackColor
    }

    Rectangle {
        x: -DesktopTokens.cardOutlinePad
        y: -DesktopTokens.cardOutlinePad
        width: root.tileWidth + DesktopTokens.cardOutlinePad * 2
        height: root.artHeight + DesktopTokens.cardOutlinePad * 2
        radius: 14
        color: "transparent"
        border.width: root.selected ? 2 : 1
        border.color: root.selected ? DesktopTokens.focus : DesktopTokens.cardOutlineIdle

        Behavior on border.color {
            ColorAnimation { duration: Theme.focusDuration }
        }
    }

    Text {
        x: 0
        y: root.artHeight + 8
        width: root.tileWidth
        height: 15
        text: root.game ? String(root.game.title || qsTr("Untitled game")) : qsTr("Untitled game")
        color: Qt.rgba(1, 1, 1, 0.88)
        font.family: Theme.bodyFont
        font.pixelSize: 12
        font.weight: Font.Bold
        elide: Text.ElideRight
        verticalAlignment: Text.AlignVCenter
    }

    Row {
        x: 0
        y: root.artHeight + 31
        width: root.tileWidth
        height: 17
        spacing: 7

        Rectangle {
            anchors.verticalCenter: parent.verticalCenter
            visible: root.discount.length > 0 && !root.owned
            width: discountLabel.implicitWidth + 10
            height: 17
            radius: 5
            color: Qt.rgba(0.431, 0.906, 0.718, 0.18)

            Text {
                id: discountLabel
                anchors.centerIn: parent
                text: root.discount
                color: DesktopTokens.green
                font.family: Theme.monoFont
                font.pixelSize: 9
                font.weight: Font.Bold
            }
        }

        DesktopGlyph {
            anchors.verticalCenter: parent.verticalCenter
            visible: root.owned
            width: 10
            height: 8
            icon: "desktop-check-light.svg"
        }

        Text {
            anchors.verticalCenter: parent.verticalCenter
            width: Math.max(0, parent.width - x)
            text: root.owned ? qsTr("In library")
                             : root.freeToPlay ? qsTr("Free to play")
                                               : root.price
            color: root.freeToPlay ? DesktopTokens.green
                                   : root.owned ? Qt.rgba(1, 1, 1, 0.50)
                                                : "#FFFFFF"
            font.family: Theme.monoFont
            font.pixelSize: 11
            font.weight: root.owned ? Font.DemiBold : Font.Bold
            elide: Text.ElideRight
        }
    }

    MouseArea {
        id: pointer
        anchors.fill: parent
        hoverEnabled: true
        cursorShape: Qt.PointingHandCursor
        onEntered: root.pointed()
        onClicked: root.activated(root.game)
    }
}

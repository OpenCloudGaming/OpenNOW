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
    property int tileWidth: DesktopTokens.libraryArtWidth
    property int tileHeight: artHeight + DesktopTokens.storeCardInfoHeight
    readonly property int artHeight: Math.round(tileWidth * 198 / 132)

    signal activated(var game)
    signal pointed()

    width: tileWidth
    height: tileHeight
    z: selected || visual.scale !== 1 ? 20 : 0
    Accessible.role: Accessible.Button
    Accessible.name: String(game && game.title || qsTr("Game"))
    ToolTip.visible: pointer.containsMouse
    ToolTip.delay: 700
    ToolTip.text: root.Accessible.name

    Item {
        id: visual
        anchors.fill: parent
        scale: !AppController.reducedMotion && root.selected ? DesktopTokens.cardHoverScale : 1
        transformOrigin: Item.Center
        Behavior on scale { NumberAnimation { duration: Theme.focusDuration; easing.type: Easing.OutCubic } }
    RoundedArtwork {
        id: cover
        x: 0
        y: 0
        width: root.tileWidth
        height: root.artHeight
        cornerRadius: DesktopTokens.px(12)
        scrimStart: 1
        artwork: DesktopTokens.artworkUrl(root.game, false)
        fallbackColor: root.fallbackColor
    }

    Rectangle {
        x: -DesktopTokens.cardOutlinePad
        y: -DesktopTokens.cardOutlinePad
        width: root.tileWidth + DesktopTokens.cardOutlinePad * 2
        height: root.artHeight + DesktopTokens.cardOutlinePad * 2
        radius: DesktopTokens.px(14)
        color: "transparent"
        border.width: root.selected ? 2 : 1
        border.color: root.selected ? DesktopTokens.focus : DesktopTokens.cardOutlineIdle

        Behavior on border.color {
            ColorAnimation { duration: Theme.focusDuration }
        }
    }

    Text {
        id: cardTitle
        objectName: "storeCardTitle"
        x: 0
        y: root.artHeight + DesktopTokens.px(8)
        width: root.tileWidth
        height: implicitHeight
        text: root.game ? String(root.game.title || qsTr("Untitled game")) : qsTr("Untitled game")
        color: DesktopTokens.text
        font.family: Theme.bodyFont
        font.pixelSize: DesktopTokens.monoSize
        font.weight: Font.Bold
        elide: Text.ElideRight
        wrapMode: Text.Wrap
        maximumLineCount: 2
        verticalAlignment: Text.AlignTop
    }

    Row {
        objectName: "storeCardMetadata"
        x: 0
        y: cardTitle.y + cardTitle.height + DesktopTokens.px(4)
        visible: root.owned || root.freeToPlay || root.price !== "" || root.discount !== ""
        width: root.tileWidth
        height: DesktopTokens.px(17)
        spacing: DesktopTokens.px(7)

        Rectangle {
            anchors.verticalCenter: parent.verticalCenter
            visible: root.discount.length > 0 && !root.owned
            width: discountLabel.implicitWidth + DesktopTokens.px(10)
            height: DesktopTokens.px(17)
            radius: DesktopTokens.px(5)
            color: Qt.rgba(0.431, 0.906, 0.718, 0.18)

            Text {
                id: discountLabel
                anchors.centerIn: parent
                text: root.discount
                color: DesktopTokens.green
                font.family: Theme.monoFont
                font.pixelSize: DesktopTokens.tinySize
                font.weight: Font.Bold
            }
        }

        DesktopSettingsIcon {
            anchors.verticalCenter: parent.verticalCenter
            visible: root.owned
            width: DesktopTokens.px(10)
            height: DesktopTokens.px(8)
            glyph: "check"
            ink: DesktopTokens.textMuted
        }

        Text {
            anchors.verticalCenter: parent.verticalCenter
            width: Math.max(0, parent.width - x)
            text: root.owned ? qsTr("In library")
                             : root.freeToPlay ? qsTr("Free to play")
                                               : root.price
            color: root.freeToPlay ? DesktopTokens.green
                                   : root.owned ? DesktopTokens.textMuted
                                                : DesktopTokens.text
            font.family: Theme.monoFont
            font.pixelSize: DesktopTokens.smallSize
            font.weight: root.owned ? Font.DemiBold : Font.Bold
            elide: Text.ElideRight
        }
    }

    } // visual; the pointer target does not move during zoom

    MouseArea {
        id: pointer
        anchors.fill: parent
        hoverEnabled: true
        cursorShape: Qt.PointingHandCursor
        onEntered: root.pointed()
        onClicked: root.activated(root.game)
    }
}

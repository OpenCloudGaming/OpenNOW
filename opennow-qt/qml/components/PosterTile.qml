import QtQuick
import QtQuick.Controls
import OpenNOW

ItemDelegate {
    id: root
    property string title: qsTr("Game")
    property string artwork: ""
    property string storeGlyph: "S"
    property color storeColor: Theme.cartSteam
    property bool currentItem: false
    property bool showStoreBadge: true
    property bool showLabel: ShellStore.settings.showTileLabels !== false
    highlighted: activeFocus || currentItem

    width: 142
    height: 210
    padding: 0
    focusPolicy: Qt.StrongFocus
    Accessible.name: title
    Accessible.role: Accessible.Button
    scale: highlighted ? 1.055 : 1
    z: highlighted ? 20 : 0

    background: RoundedArtwork {
        artwork: root.artwork
        fallbackColor: root.storeColor
        cornerRadius: 25
        scrimStart: 0.55
    }

    contentItem: Item {
        StoreBadge {
            visible: root.showStoreBadge
            x: 10; y: 10; width: 28; height: 28; radius: 8
            badgeSize: 28
            storeGlyph: root.storeGlyph
            storeColor: root.storeColor
        }
        Text {
            x: 12; y: parent.height - height - 12; width: parent.width - 24
            visible: root.showLabel
            text: root.title
            color: Theme.mediaForeground
            elide: Text.ElideRight
            font.family: Theme.displayFont
            font.pixelSize: 15
            font.weight: Font.Black
        }
    }

    FocusFrame { focused: root.highlighted; frameRadius: 28 }
    Behavior on scale { NumberAnimation { duration: Theme.focusDuration; easing.type: Easing.OutCubic } }
}

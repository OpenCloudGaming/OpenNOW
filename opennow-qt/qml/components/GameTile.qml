import QtQuick
import QtQuick.Controls
import OpenNOW

ItemDelegate {
    id: root
    property string title: qsTr("Game")
    property string artwork: ""
    property string storeGlyph: "S"
    property color storeColor: Theme.cartSteam
    property bool wide: false
    property bool session: false
    property bool addTile: false
    property string eyebrow: ""
    property bool currentItem: false
    signal menuRequested()
    highlighted: activeFocus || currentItem

    width: wide ? 392 : 176
    height: 176
    padding: 0
    focusPolicy: Qt.StrongFocus
    Accessible.name: title
    Accessible.description: eyebrow
    Accessible.role: Accessible.Button
    scale: highlighted ? 1.065 : 1
    z: highlighted ? 10 : 0

    background: RoundedArtwork {
        artwork: root.addTile ? "" : root.artwork
        fallbackColor: root.storeColor
        cornerRadius: 34
        scrimStart: 0.28
    }

    contentItem: Item {
        StoreBadge {
            x: 12; y: 12
            visible: !root.addTile
            storeGlyph: root.storeGlyph
            storeColor: root.storeColor
        }

        Column {
            visible: !root.addTile
            x: 14
            y: parent.height - height - 14
            width: root.session ? parent.width - 180 : parent.width - 28
            spacing: 3
            Text {
                visible: root.eyebrow.length > 0
                text: root.eyebrow
                color: Qt.rgba(1, 1, 1, 0.7)
                font.family: Theme.bodyFont
                font.pixelSize: 11
                font.weight: Font.Bold
                font.letterSpacing: 0.8
            }
            Text {
                width: parent.width
                visible: ShellStore.settings.showTileLabels !== false
                text: root.title
                color: Theme.mediaForeground
                elide: Text.ElideRight
                font.family: Theme.displayFont
                font.pixelSize: root.wide ? 22 : 17
                font.weight: Font.Black
            }
        }

        Rectangle {
            visible: root.session && !root.addTile
            anchors.right: parent.right; anchors.rightMargin: 14
            anchors.bottom: parent.bottom; anchors.bottomMargin: 14
            width: resumeText.implicitWidth + 42; height: 38; radius: 19
            color: Theme.face
            Row {
                anchors.centerIn: parent; spacing: 8
                Rectangle { width: 24; height: 24; radius: 12; color: Theme.faceText
                    Text { anchors.centerIn: parent; text: "A"; color: Theme.face; font.family: Theme.bodyFont; font.pixelSize: 11; font.weight: Font.Black }
                }
                Text { id: resumeText; anchors.verticalCenter: parent.verticalCenter; text: qsTr("Jump back in!"); color: Theme.faceText; font.family: Theme.bodyFont; font.pixelSize: 14; font.weight: Font.Black }
            }
        }

        Column {
            visible: root.addTile
            anchors.centerIn: parent
            spacing: 10
            Rectangle { anchors.horizontalCenter: parent.horizontalCenter; width: 38; height: 38; radius: 19; color: Qt.rgba(1, 1, 1, 0.12)
                Text { anchors.centerIn: parent; text: "+"; color: Theme.label; font.family: Theme.bodyFont; font.pixelSize: 30; font.weight: Font.Bold }
            }
            Text { anchors.horizontalCenter: parent.horizontalCenter; text: qsTr("Add a game"); color: Theme.label; font.family: Theme.bodyFont; font.pixelSize: 16; font.weight: Font.ExtraBold }
        }
    }

    FocusFrame { focused: root.highlighted; opacity: root.addTile ? 0.55 : 1 }

    TapHandler {
        acceptedButtons: Qt.RightButton
        enabled: !root.addTile
        onTapped: root.menuRequested()
    }

    Behavior on scale {
        NumberAnimation { duration: Theme.focusDuration; easing.type: Easing.OutCubic }
    }
}

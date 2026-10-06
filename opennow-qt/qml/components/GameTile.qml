import QtQuick
import QtQuick.Controls
import QtQuick.Shapes
import OpenNOW

ItemDelegate {
    id: root
    property string title: qsTr("Game")
    property string artwork: ""
    property string store: ""
    property bool wide: false
    property bool addTile: false
    property string eyebrow: ""
    property bool currentItem: false
    property bool parked: false
    readonly property real cornerRadius: highlighted ? 30 : 26
    readonly property bool labelVisible: !addTile && ShellStore.settings.showTileLabels !== false
        && (wide || highlighted || artwork === "")
    signal menuRequested()
    highlighted: activeFocus || currentItem

    width: wide ? 368 : 176
    height: 176
    padding: 0
    focusPolicy: Qt.StrongFocus
    Accessible.name: title
    Accessible.description: eyebrow
    Accessible.role: Accessible.Button
    scale: !AppController.reducedMotion && highlighted && !parked ? 1.04 : 1
    z: highlighted ? 10 : 0

    background: Item {
        RoundedArtwork {
            anchors.fill: parent
            visible: !root.addTile
            artwork: root.artwork
            fallbackColor: ConsoleStores.color(root.store)
            cornerRadius: root.cornerRadius
            scrimStart: root.labelVisible ? 0.4 : 1
        }
        Shape {
            anchors.fill: parent
            visible: root.addTile
            preferredRendererType: Shape.CurveRenderer
            ShapePath {
                strokeColor: root.highlighted ? Theme.face : Qt.rgba(Theme.label.r, Theme.label.g, Theme.label.b, 0.38)
                strokeWidth: 2
                strokeStyle: ShapePath.DashLine
                dashPattern: [3, 3]
                fillColor: Qt.rgba(Theme.face.r, Theme.face.g, Theme.face.b, root.highlighted ? 0.08 : 0.02)
                PathRectangle {
                    x: 1; y: 1
                    width: root.width - 2
                    height: root.height - 2
                    radius: root.cornerRadius
                }
            }
        }
    }

    contentItem: Item {
        ConsoleStoreMark {
            x: root.highlighted ? 13 : 11
            y: x
            visible: !root.addTile && root.store !== ""
            store: root.store
            markSize: 30
        }

        Column {
            visible: root.labelVisible
            x: 19
            y: parent.height - height - 15
            width: parent.width - 38
            spacing: 0
            Text {
                visible: root.eyebrow.length > 0
                width: parent.width
                text: root.eyebrow
                color: Theme.mediaMuted
                elide: Text.ElideRight
                font.family: Theme.monoFont
                font.pixelSize: 12
                font.weight: Font.DemiBold
                font.letterSpacing: 1.2
            }
            Text {
                width: parent.width
                text: root.title
                color: Theme.mediaForeground
                elide: Text.ElideRight
                font.family: Theme.displayFont
                font.pixelSize: root.wide ? 20 : 17
                font.weight: Font.Black
            }
        }

        Column {
            visible: root.addTile
            anchors.centerIn: parent
            spacing: 10
            Item {
                anchors.horizontalCenter: parent.horizontalCenter
                width: 30
                height: 30
                Rectangle { anchors.centerIn: parent; width: 20; height: 3; radius: 1.5; color: Theme.label }
                Rectangle { anchors.centerIn: parent; width: 3; height: 20; radius: 1.5; color: Theme.label }
            }
            Text {
                anchors.horizontalCenter: parent.horizontalCenter
                text: qsTr("Add a game")
                color: Theme.label
                font.family: Theme.bodyFont
                font.pixelSize: 15
                font.weight: Font.ExtraBold
            }
        }
    }

    FocusFrame {
        visible: !root.addTile || root.highlighted
        focused: root.highlighted
        parked: root.parked
        frameRadius: root.cornerRadius
    }

    TapHandler {
        acceptedButtons: Qt.RightButton
        enabled: !root.addTile
        onTapped: root.menuRequested()
    }

    Behavior on scale {
        NumberAnimation { duration: AppController.reducedMotion ? 0 : 90; easing.type: Easing.OutCubic }
    }
}

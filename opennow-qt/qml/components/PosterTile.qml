import QtQuick
import QtQuick.Controls
import OpenNOW

ItemDelegate {
    id: root
    property string title: qsTr("Game")
    property string artwork: ""
    property var stores: []
    property bool pinned: false
    property bool currentItem: false
    property bool parked: false
    property bool showLabel: ShellStore.settings.showTileLabels !== false
    readonly property bool labelVisible: showLabel || artwork === ""
    highlighted: activeFocus || currentItem

    width: 156
    height: 232
    padding: 0
    focusPolicy: Qt.StrongFocus
    Accessible.name: title
    Accessible.description: [pinned ? qsTr("Pinned to Home") : ""].concat(
        stores.map(store => ConsoleStores.label(store))).filter(Boolean).join(", ")
    Accessible.role: Accessible.Button
    scale: !AppController.reducedMotion && highlighted && !parked ? 1.04 : 1
    z: highlighted ? 20 : 0

    background: RoundedArtwork {
        artwork: root.artwork
        fallbackColor: ConsoleStores.color(root.stores.length ? root.stores[0] : "")
        cornerRadius: 18
        scrimStart: root.labelVisible || root.stores.length ? 0.6 : 1
    }

    contentItem: Item {
        Text {
            x: 12; y: marks.visible ? marks.y - height - 8 : parent.height - height - 12
            width: parent.width - 24
            visible: root.labelVisible
            text: root.title
            color: Theme.mediaForeground
            elide: Text.ElideRight
            maximumLineCount: 2
            wrapMode: Text.Wrap
            font.family: Theme.displayFont
            font.pixelSize: 15
            font.weight: Font.Black
        }
        Row {
            id: marks
            x: 10; y: parent.height - height - 10
            visible: root.stores.length > 0
            spacing: 4
            Repeater {
                model: root.stores.slice(0, 4)
                ConsoleStoreMark {
                    required property string modelData
                    store: modelData
                    markSize: 26
                }
            }
        }
        Rectangle {
            visible: root.pinned
            x: parent.width - width - 10; y: 10
            width: 28; height: 28; radius: 14
            color: Theme.yellow
            Text {
                anchors.centerIn: parent
                text: "★"
                color: Theme.contrastText(Theme.yellow)
                font.pixelSize: 15
                font.weight: Font.Black
            }
        }
    }

    FocusFrame { focused: root.highlighted; parked: root.parked; frameRadius: 18 }
    Behavior on scale {
        NumberAnimation { duration: AppController.reducedMotion ? 0 : 90; easing.type: Easing.OutCubic }
    }
}

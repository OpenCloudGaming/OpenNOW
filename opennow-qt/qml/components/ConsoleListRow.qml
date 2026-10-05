import QtQuick
import QtQuick.Controls
import OpenNOW

ItemDelegate {
    id: root
    property string title: ""
    property string description: ""
    property string badge: ""
    property color badgeColor: Theme.textMuted
    property bool currentItem: false
    property bool ringVisible: activeFocus
    property bool parked: currentItem && !ringVisible
    default property alias leading: leadingSlot.data
    readonly property bool showRing: ringVisible && !parked

    implicitHeight: Math.max(88, textColumn.implicitHeight + 28)
    padding: 0
    focusPolicy: Qt.StrongFocus
    highlighted: showRing
    Accessible.role: Accessible.Button
    Accessible.name: title
    Accessible.description: description + (badge !== "" ? ". " + badge : "")
    Accessible.selected: currentItem

    background: Item {
        Rectangle {
            anchors.fill: parent
            anchors.margins: -5
            radius: 29
            color: "transparent"
            border.width: 5
            border.color: Qt.rgba(Theme.focus.r, Theme.focus.g, Theme.focus.b, 0.4)
            opacity: root.showRing ? 1 : 0
            Behavior on opacity { NumberAnimation { duration: Theme.focusDuration; easing.type: Easing.OutCubic } }
        }
        Rectangle {
            anchors.fill: parent
            radius: 24
            color: root.showRing ? Qt.rgba(Theme.face.r, Theme.face.g, Theme.face.b, 0.10)
                : root.parked ? Qt.rgba(Theme.face.r, Theme.face.g, Theme.face.b, 0.08)
                : "transparent"
            border.width: root.showRing ? 3 : root.parked ? 2 : 0
            border.color: root.showRing ? Theme.face : Qt.rgba(Theme.face.r, Theme.face.g, Theme.face.b, 0.7)
        }
        Rectangle {
            visible: !root.showRing && !root.parked
            x: 18
            anchors.bottom: parent.bottom
            width: parent.width - 36
            height: 1
            color: Qt.rgba(Theme.face.r, Theme.face.g, Theme.face.b, 0.09)
        }
    }

    contentItem: Item {
        Item {
            id: leadingSlot
            x: 22
            anchors.verticalCenter: parent.verticalCenter
            width: childrenRect.width
            height: childrenRect.height
        }
        Column {
            id: textColumn
            anchors.left: leadingSlot.right
            anchors.leftMargin: leadingSlot.width > 0 ? 18 : 0
            anchors.right: badgeText.left
            anchors.rightMargin: 24
            anchors.verticalCenter: parent.verticalCenter
            spacing: 4
            Text {
                width: parent.width
                text: root.title
                color: root.enabled ? Theme.label : Theme.textMuted
                font.family: Theme.displayFont
                font.pixelSize: 21
                font.weight: Font.Black
                elide: Text.ElideRight
            }
            Text {
                width: parent.width
                visible: text !== ""
                text: root.description
                color: Theme.textMuted
                font.family: Theme.bodyFont
                font.pixelSize: 16
                font.weight: Font.DemiBold
                wrapMode: Text.WordWrap
                maximumLineCount: 2
                elide: Text.ElideRight
            }
        }
        Text {
            id: badgeText
            anchors.right: parent.right
            anchors.rightMargin: 22
            anchors.verticalCenter: parent.verticalCenter
            width: text !== "" ? Math.min(implicitWidth, 260) : 0
            text: root.badge
            color: root.badgeColor
            elide: Text.ElideRight
            horizontalAlignment: Text.AlignRight
            font.family: Theme.monoFont
            font.pixelSize: 13
            font.weight: Font.Bold
            font.letterSpacing: 1.3
            font.capitalization: Font.AllUppercase
        }
    }
}

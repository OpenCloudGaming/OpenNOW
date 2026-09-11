import QtQuick
import QtQuick.Controls
import OpenNOW

Item {
    id: root
    property bool paperStyle: false
    property string glyph: ""
    property bool expanded: false
    property bool expandable: false
    signal expansionRequested()
    property string title: ""
    property string description: ""
    property string value: ""
    property int rowHeight: paperStyle ? DesktopTokens.px(68) : DesktopTokens.rowHeight
    property bool showDivider: true
    property string leadingLetter: ""
    property url leadingIcon: ""
    property color leadingColor: DesktopTokens.raised
    readonly property bool hasLeading: glyph !== "" || leadingLetter !== "" || leadingIcon.toString() !== ""
    default property alias trailing: trailingSlot.data

    readonly property bool stacked: width < trailingSlot.implicitWidth + (hasLeading ? 340 : 290)
    implicitHeight: Math.max(rowHeight, stacked ? labels.implicitHeight + trailingSlot.height + 28
        : labels.implicitHeight + (paperStyle ? 12 : 24))

    Rectangle {
        id: leadingTile
        visible: root.hasLeading
        x: root.paperStyle ? DesktopTokens.px(20) : 0
        y: root.stacked ? 12 : (parent.height - height) / 2
        width: root.paperStyle ? DesktopTokens.px(40) : 36
        height: width
        radius: root.paperStyle ? 12 : 10
        color: root.expanded ? Theme.focus : root.leadingColor
        border.width: root.paperStyle ? 0 : 1
        border.color: Theme.seam
        DesktopSettingsIcon {
            anchors.centerIn: parent; width: 20; height: 20
            visible: root.glyph !== ""; glyph: root.glyph
            ink: root.expanded ? Theme.focusText : Theme.label
        }
        Image {
            anchors.centerIn: parent
            width: 19
            height: 19
            source: root.leadingIcon
            sourceSize: Qt.size(width, height)
            fillMode: Image.PreserveAspectFit
            visible: root.leadingIcon.toString() !== ""
        }
        Text {
            anchors.centerIn: parent
            text: root.leadingLetter
            visible: root.leadingIcon.toString() === "" && root.glyph === ""
            color: Theme.label
            font.family: DesktopTokens.bodyFont
            font.pixelSize: 16
            font.weight: Font.Black
        }
    }

    Column {
        id: labels
        anchors.left: parent.left
        anchors.leftMargin: root.paperStyle ? DesktopTokens.px(root.hasLeading ? 76 : 20) : (root.hasLeading ? 50 : 0)
        anchors.right: root.stacked ? parent.right : trailingSlot.left
        anchors.rightMargin: root.paperStyle ? 16 : 20
        y: root.stacked ? 10 : (parent.height - height) / 2
        spacing: 2
        Text {
            width: parent.width
            text: root.title
            color: Theme.label
            font.family: Theme.bodyFont
            font.pixelSize: DesktopTokens.bodySize
            font.weight: root.paperStyle ? Font.ExtraBold : Font.Bold
            wrapMode: Text.WordWrap
        }
        Text {
            width: parent.width
            visible: root.description !== ""
            text: root.description
            color: Theme.textMuted
            font.family: Theme.bodyFont
            font.pixelSize: DesktopTokens.captionSize
            font.weight: root.paperStyle ? Font.DemiBold : Font.Medium
            wrapMode: Text.WordWrap
        }
    }

    Row {
        id: trailingSlot
        anchors.right: parent.right
        anchors.rightMargin: root.paperStyle ? DesktopTokens.px(68) : 0
        y: root.stacked ? labels.y + labels.height + 8 : (parent.height - height) / 2
        spacing: 10
        height: DesktopTokens.controlHeight

        add: Transition {
            ScriptAction { script: root.centerTrailing() }
        }

        Text {
            visible: root.value !== ""
            text: root.value
            color: Theme.label
            font.family: Theme.monoFont
            font.pixelSize: DesktopTokens.monoSize
            font.weight: Font.Bold
            anchors.verticalCenter: parent.verticalCenter
        }
    }

    AbstractButton {
        visible: root.paperStyle && root.expandable
        anchors.right: parent.right; anchors.rightMargin: DesktopTokens.px(20)
        anchors.verticalCenter: parent.verticalCenter; width: 32; height: 32
        Accessible.name: root.title
        onClicked: root.expansionRequested()
        background: Rectangle { radius: 10; color: parent.activeFocus || parent.hovered ? DesktopTokens.raised : "transparent" }
        DesktopSettingsIcon {
            anchors.centerIn: parent; width: 14; height: 14; glyph: "chevron"
            rotation: root.expanded ? -90 : 90; ink: root.expanded ? Theme.focus : Theme.textMuted
            Behavior on rotation { enabled: !AppController.reducedMotion; NumberAnimation { duration: 160; easing.type: Easing.OutCubic } }
        }
    }

    function centerTrailing() {
        for (let i = 0; i < trailingSlot.children.length; ++i) {
            const item = trailingSlot.children[i]
            if (item)
                item.anchors.verticalCenter = trailingSlot.verticalCenter
        }
    }

    Component.onCompleted: centerTrailing()

    Rectangle {
        visible: root.showDivider
        anchors.left: parent.left
        anchors.right: parent.right
        anchors.bottom: parent.bottom
        height: 1
        color: DesktopTokens.seamSoft
    }
}

import QtQuick
import QtQuick.Controls
import QtQuick.Window
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
    property int rowHeight: DesktopTokens.rowHeight
    property bool showDivider: true
    property string leadingLetter: ""
    property url leadingIcon: ""
    property real leadingIconWidth: DesktopTokens.px(19)
    property color leadingColor: DesktopTokens.raised
    readonly property bool hasLeading: glyph !== "" || leadingLetter !== "" || leadingIcon.toString() !== ""
    default property alias trailing: trailingSlot.data

    readonly property real labelInset: paperStyle ? DesktopTokens.settingsLabelInset : (hasLeading ? DesktopTokens.px(50) : 0)
    readonly property real rightInset: paperStyle ? DesktopTokens.settingsInset : 0
    readonly property real controlWidth: Math.max(0, Math.min(DesktopTokens.settingsControlWidth, width - labelInset - rightInset))
    readonly property bool stacked: width < DesktopTokens.settingsCompactWidth && trailingSlot.implicitWidth > DesktopTokens.px(120)
    implicitHeight: Math.max(rowHeight, (stacked ? trailingSlot.y + trailingSlot.height : Math.max(labels.y + labels.height, trailingSlot.y + trailingSlot.height)) + DesktopTokens.px(10))

    Rectangle {
        id: leadingTile
        visible: root.hasLeading
        x: root.paperStyle ? DesktopTokens.settingsInset : 0
        y: DesktopTokens.px(10)
        width: DesktopTokens.px(root.paperStyle ? 40 : 36)
        height: width
        radius: DesktopTokens.px(root.paperStyle ? 12 : 10)
        color: root.expanded ? Theme.focus : root.leadingColor
        border.width: root.paperStyle ? 0 : 1
        border.color: Theme.seam
        DesktopSettingsIcon {
            anchors.centerIn: parent; width: DesktopTokens.px(20); height: width
            visible: root.glyph !== ""; glyph: root.glyph
            ink: root.expanded ? Theme.focusText : Theme.label
        }
        Image {
            anchors.centerIn: parent
            width: root.leadingIconWidth
            height: width
            source: root.leadingIcon
            sourceSize: Qt.size(Math.ceil(width * Screen.devicePixelRatio), Math.ceil(height * Screen.devicePixelRatio))
            fillMode: Image.PreserveAspectFit
            visible: root.leadingIcon.toString() !== ""
        }
        Text {
            anchors.centerIn: parent
            text: root.leadingLetter
            visible: root.leadingIcon.toString() === "" && root.glyph === ""
            color: Theme.label
            font.family: DesktopTokens.bodyFont
            font.pixelSize: DesktopTokens.px(16)
            font.weight: Font.Black
        }
    }

    Column {
        id: labels
        objectName: "settingsRowLabels"
        anchors.left: parent.left
        anchors.leftMargin: root.labelInset
        anchors.right: root.stacked ? parent.right : trailingSlot.left
        anchors.rightMargin: root.stacked ? root.rightInset : DesktopTokens.px(20)
        y: DesktopTokens.px(10) + Math.max(0, (DesktopTokens.px(40) - height) / 2)
        spacing: DesktopTokens.px(2)
        Text {
            id: titleLabel
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
        objectName: "settingsRowControls"
        readonly property real availableWidth: root.controlWidth
        anchors.right: parent.right
        anchors.rightMargin: root.rightInset + (root.expandable ? DesktopTokens.px(40) : 0)
        y: root.stacked ? labels.y + labels.height + DesktopTokens.px(8) : DesktopTokens.px(10)
        spacing: DesktopTokens.px(10)
        height: Math.max(DesktopTokens.px(40), implicitHeight)

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
        anchors.right: parent.right; anchors.rightMargin: root.rightInset
        y: DesktopTokens.px(14); width: DesktopTokens.px(32); height: width
        Accessible.name: root.title
        onClicked: root.expansionRequested()
        background: Rectangle { radius: DesktopTokens.px(10); color: parent.activeFocus || parent.hovered ? DesktopTokens.raised : "transparent" }
        DesktopSettingsIcon {
            anchors.centerIn: parent; width: DesktopTokens.px(14); height: width; glyph: "chevron"
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

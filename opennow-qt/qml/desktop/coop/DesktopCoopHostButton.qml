import QtQuick
import QtQuick.Controls
import OpenNOW

AbstractButton {
    id: root
    property color fill: "transparent"
    property color hoverFill: Qt.rgba(1, 1, 1, 0.08)
    property color edge: "#24FFFFFF"
    property real edgeWidth: 1
    property color ink: "#FFFFFF"
    property string glyph: ""
    property real glyphSize: 14
    property color glyphInk: ink
    property string keyText: ""
    property real fontSize: 13
    property real cornerRadius: 10
    property real sidePadding: 16
    readonly property bool focusVisible: activeFocus && AppController.inputMode !== "pointer"
    height: 38
    implicitWidth: row.implicitWidth + 2 * sidePadding
    width: implicitWidth
    hoverEnabled: true
    focusPolicy: Qt.StrongFocus
    opacity: enabled ? 1 : 0.45
    Accessible.role: Accessible.Button
    Accessible.name: text
    Keys.onReturnPressed: event => { root.clicked(); event.accepted = true }
    Keys.onEnterPressed: event => { root.clicked(); event.accepted = true }

    background: Rectangle {
        radius: root.cornerRadius
        color: root.hovered || root.down ? Qt.tint(root.fill, root.hoverFill) : root.fill
        border.width: root.edgeWidth
        border.color: root.edge
        Behavior on color { ColorAnimation { duration: DesktopTokens.quickDuration } }
        Rectangle {
            anchors.fill: parent
            anchors.margins: -3
            radius: parent.radius + 3
            color: "transparent"
            border.width: 2
            border.color: DesktopTokens.focus
            visible: root.focusVisible
        }
    }
    contentItem: Item {
        Row {
            id: row
            anchors.centerIn: parent
            spacing: 8
            DesktopCoopHostIcon {
                visible: root.glyph !== ""
                anchors.verticalCenter: parent.verticalCenter
                width: root.glyphSize
                height: root.glyphSize
                glyph: root.glyph
                ink: root.glyphInk
            }
            Text {
                anchors.verticalCenter: parent.verticalCenter
                text: root.text
                color: root.ink
                font.family: DesktopTokens.bodyFont
                font.pixelSize: root.fontSize
                font.weight: Font.ExtraBold
            }
            KeyboardGlyph {
                visible: root.keyText !== ""
                anchors.verticalCenter: parent.verticalCenter
                shortcut: root.keyText
                keySize: 20
                ink: Qt.rgba(root.ink.r, root.ink.g, root.ink.b, 0.72)
            }
        }
    }
}

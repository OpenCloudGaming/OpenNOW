pragma ComponentBehavior: Bound
import QtQuick
import QtQuick.Controls
import OpenNOW

AbstractButton {
    id: root
    property bool consoleMode: false
    property bool selected: false
    property color accent: Theme.accentColor("green")
    implicitHeight: content.implicitHeight + DesktopTokens.px(40)
    hoverEnabled: true
    Accessible.role: Accessible.RadioButton
    Accessible.name: consoleMode ? qsTr("Console mode") : qsTr("Desktop mode")
    Accessible.checked: selected

    background: Rectangle {
        radius: DesktopTokens.px(16)
        color: root.hovered ? DesktopTokens.raised : Theme.glass
        border.width: root.selected || root.activeFocus ? 2 : 1
        border.color: root.activeFocus ? Theme.focus : root.selected ? root.accent : Theme.seam
        Behavior on border.color { ColorAnimation { duration: DesktopTokens.quickDuration } }
    }

    Column {
        id: content
        x: DesktopTokens.px(20)
        y: DesktopTokens.px(20)
        width: root.width - DesktopTokens.px(40)
        spacing: DesktopTokens.px(16)

        Row {
            width: parent.width
            spacing: DesktopTokens.px(10)
            DesktopSettingsIcon {
                width: DesktopTokens.px(20); height: width
                glyph: root.consoleMode ? "controller" : "monitor"
                ink: root.selected ? root.accent : Theme.textMuted
            }
            Text {
                width: parent.width - DesktopTokens.px(60)
                text: root.consoleMode ? qsTr("GAMEPAD · TV") : qsTr("MOUSE + KEYBOARD")
                color: root.selected ? root.accent : Theme.textMuted
                font.family: Theme.monoFont
                font.pixelSize: DesktopTokens.monoSize
                font.weight: Font.Bold
                wrapMode: Text.Wrap
            }
            Rectangle {
                width: DesktopTokens.px(20); height: width; radius: width / 2
                color: root.selected ? root.accent : "transparent"
                border.width: 1; border.color: root.selected ? root.accent : Theme.seam
                Text {
                    anchors.centerIn: parent
                    text: root.selected ? "✓" : ""
                    color: Theme.contrastText(root.accent)
                    font.pixelSize: DesktopTokens.captionSize
                    font.weight: Font.Bold
                }
            }
        }

        Rectangle {
            width: parent.width
            height: DesktopTokens.px(160)
            radius: DesktopTokens.px(10)
            color: Theme.shell
            border.color: Theme.seam
            clip: true

            Image {
                anchors.fill: parent
                source: "qrc:/qt/qml/OpenNOW/res/brand/desktop-renew.jpg"
                fillMode: Image.PreserveAspectCrop
                asynchronous: true
                opacity: Theme.lightMode ? 0.12 : 0.24
            }
            Rectangle {
                visible: !root.consoleMode
                x: DesktopTokens.px(10); y: DesktopTokens.px(10)
                width: parent.width * 0.18; height: parent.height - DesktopTokens.px(20)
                radius: DesktopTokens.px(5); color: DesktopTokens.raised
                Column {
                    x: DesktopTokens.px(8); y: DesktopTokens.px(12)
                    width: parent.width - DesktopTokens.px(16)
                    spacing: DesktopTokens.px(12)
                    Repeater {
                        model: 4
                        Rectangle {
                            required property int index
                            width: parent.width; height: DesktopTokens.px(4)
                            radius: height / 2
                            color: index === 0 ? root.accent : Theme.seam
                        }
                    }
                }
            }
            Column {
                x: root.consoleMode ? DesktopTokens.px(22) : parent.width * 0.24
                y: root.consoleMode ? DesktopTokens.px(28) : DesktopTokens.px(20)
                width: parent.width - x - DesktopTokens.px(20)
                spacing: DesktopTokens.px(14)
                Rectangle {
                    width: parent.width * 0.46; height: DesktopTokens.px(6)
                    radius: height / 2; color: Theme.label; opacity: 0.5
                }
                Row {
                    width: parent.width
                    spacing: DesktopTokens.px(8)
                    Repeater {
                        model: root.consoleMode ? 3 : 4
                        Rectangle {
                            required property int index
                            width: (parent.width - (root.consoleMode ? 2 : 3) * DesktopTokens.px(8)) / (root.consoleMode ? 3 : 4)
                            height: DesktopTokens.px(root.consoleMode ? 86 : 92)
                            radius: DesktopTokens.px(5)
                            color: index === 0 ? Qt.rgba(root.accent.r, root.accent.g, root.accent.b, 0.16) : DesktopTokens.raised
                            border.width: root.consoleMode && index === 0 ? 2 : 1
                            border.color: index === 0 ? root.accent : Theme.seam
                        }
                    }
                }
            }
        }

        Text {
            width: parent.width
            text: root.consoleMode ? qsTr("Console mode") : qsTr("Desktop mode")
            color: Theme.label
            font.family: Theme.displayFont
            font.pixelSize: DesktopTokens.titleSize
            font.weight: Font.Black
            wrapMode: Text.Wrap
        }
        Text {
            width: parent.width
            text: root.consoleMode
                ? qsTr("Big cover art, clear focus rings and controller navigation. Made for a screen across the room.")
                : qsTr("A compact sidebar, search and a detailed library. Made for a monitor at arm's length.")
            color: Theme.textMuted
            font.family: Theme.bodyFont
            font.pixelSize: DesktopTokens.bodySize
            wrapMode: Text.Wrap
            lineHeight: 1.35
        }
    }
}

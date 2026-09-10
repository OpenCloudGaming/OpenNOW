pragma ComponentBehavior: Bound
import QtQuick
import QtQuick.Controls
import OpenNOW

AbstractButton {
    id: root
    property bool consoleMode: false
    property bool selected: false
    property color accent: Theme.accentColor("green")
    implicitHeight: preview.height + foot.height + DesktopTokens.px(4)
    hoverEnabled: true
    Accessible.role: Accessible.RadioButton
    Accessible.name: consoleMode ? qsTr("Console mode") : qsTr("Desktop mode")
    Accessible.checked: selected

    background: Rectangle {
        radius: DesktopTokens.px(16)
        color: Theme.lightMode ? Theme.glass : "#C70B0F1A"
        border.width: root.selected || root.activeFocus ? 2 : 1
        border.color: root.activeFocus ? Theme.focus : root.selected ? root.accent : Theme.seam
        Behavior on border.color { ColorAnimation { duration: DesktopTokens.quickDuration } }
    }

    Item {
        id: preview
        x: DesktopTokens.px(2)
        y: DesktopTokens.px(2)
        width: root.width - DesktopTokens.px(4)
        height: DesktopTokens.px(270)
        clip: true

        Image {
            width: parent.width; height: parent.height
            y: root.consoleMode ? DesktopTokens.px(18) : 0
            source: root.consoleMode ? "qrc:/qt/qml/OpenNOW/res/onboarding/console-preview.png"
                : "qrc:/qt/qml/OpenNOW/res/onboarding/desktop-preview.png"
            fillMode: Image.PreserveAspectCrop
        }

        Rectangle {
            x: DesktopTokens.px(12); y: DesktopTokens.px(12)
            width: badge.implicitWidth + DesktopTokens.px(20)
            height: DesktopTokens.px(26)
            radius: height / 2
            color: "#DB0B0F1A"; border.color: "#24FFFFFF"
            Text {
                id: badge
                anchors.centerIn: parent
                text: root.consoleMode ? qsTr("GAMEPAD · 10-FOOT UI") : qsTr("MOUSE + KEYBOARD")
                font.family: Theme.monoFont; font.pixelSize: DesktopTokens.px(10)
                font.weight: Font.Bold; font.letterSpacing: DesktopTokens.px(0.8)
                color: "#FFFFFF"
            }
        }

        Rectangle {
            visible: !root.consoleMode
            anchors.right: parent.right; anchors.top: parent.top
            anchors.margins: DesktopTokens.px(12)
            width: defaultLabel.implicitWidth + DesktopTokens.px(20)
            height: DesktopTokens.px(26); radius: height / 2
            color: "#6EE7B7"; border.color: "#596EE7B7"
            Text {
                id: defaultLabel
                anchors.centerIn: parent
                text: qsTr("DEFAULT · DESKTOP MODE")
                color: "#0B0F1A"; font.family: Theme.monoFont
                font.pixelSize: DesktopTokens.px(10); font.weight: Font.Bold
            }
        }
    }

    Item {
        id: foot
        x: DesktopTokens.px(2); y: preview.y + preview.height
        width: root.width - DesktopTokens.px(4)
        height: Math.max(DesktopTokens.px(100), descriptions.implicitHeight + DesktopTokens.px(36))
        Rectangle { width: parent.width; height: 1; color: Theme.seam }
        Column {
            id: descriptions
            x: DesktopTokens.px(20); y: DesktopTokens.px(18)
            width: parent.width - DesktopTokens.px(82)
            spacing: DesktopTokens.px(4)
            Text {
                width: parent.width
                text: root.consoleMode ? qsTr("Console mode") : qsTr("Desktop mode")
                color: Theme.label; font.family: Theme.displayFont
                font.pixelSize: DesktopTokens.px(20); font.weight: Font.Black
                height: lineCount * DesktopTokens.px(24)
                lineHeightMode: Text.FixedHeight; lineHeight: DesktopTokens.px(24)
                topPadding: -DesktopTokens.px(3)
                wrapMode: Text.Wrap
            }
            Text {
                width: parent.width
                text: root.consoleMode ? qsTr("Big cover art, clear focus rings and controller navigation. Made for a screen across the room.")
                    : qsTr("A compact sidebar, search and a detailed library. Made for a monitor at arm's length.")
                color: Theme.textMuted; font.family: Theme.bodyFont
                font.pixelSize: DesktopTokens.px(13); font.weight: Font.Medium
                lineHeightMode: Text.FixedHeight; lineHeight: DesktopTokens.px(18)
                wrapMode: Text.Wrap
            }
        }
        Rectangle {
            anchors.right: parent.right; anchors.rightMargin: DesktopTokens.px(20)
            anchors.verticalCenter: parent.verticalCenter
            width: DesktopTokens.px(26); height: width; radius: width / 2
            color: root.selected ? root.accent : "transparent"
            border.width: 2; border.color: root.selected ? root.accent : Theme.seam
            DesktopSettingsIcon {
                anchors.centerIn: parent
                width: DesktopTokens.px(16); height: width
                glyph: "check"; ink: Theme.contrastText(root.accent); visible: root.selected
            }
        }
    }
}

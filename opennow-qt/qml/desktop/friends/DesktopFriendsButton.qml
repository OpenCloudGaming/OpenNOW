import QtQuick
import QtQuick.Controls
import OpenNOW

AbstractButton {
    id: button
    property string kind: "secondary"
    property string glyph: ""
    property int glyphSize: 15
    property int fontSize: 13
    property int controlHeight: 32
    property int cornerRadius: 8
    property int sidePadding: 12
    property int badge: -1
    property int gap: kind === "link" ? 6 : 8
    readonly property color fillColor: {
        if (kind === "primary") return down ? Qt.darker(Theme.face, 1.12) : Theme.face
        if (kind === "accent") return down ? Qt.darker(DesktopFriendsFormat.accent, 1.12) : DesktopFriendsFormat.accent
        if (kind === "tinted") return DesktopFriendsFormat.tint(DesktopFriendsFormat.accent, hovered ? 0.16 : 0.10)
        if (kind === "outline") return hovered ? DesktopTokens.raised : "transparent"
        if (kind === "discord") return down ? "#4752C4" : "#5865F2"
        if (kind === "link") return hovered ? DesktopTokens.raised : "transparent"
        return hovered || activeFocus ? DesktopTokens.raisedStrong : DesktopTokens.raised
    }
    readonly property color inkColor: kind === "primary" ? Theme.faceText
        : kind === "accent" ? DesktopFriendsFormat.accentText
        : kind === "tinted" ? DesktopFriendsFormat.accent
        : kind === "outline" ? (hovered ? DesktopTokens.text : DesktopTokens.textMuted)
        : kind === "discord" ? "#FFFFFF"
        : kind === "link" ? DesktopFriendsFormat.accent
        : kind === "danger" ? DesktopFriendsFormat.red : DesktopTokens.text
    height: DesktopTokens.px(controlHeight)
    implicitWidth: contentRow.implicitWidth + 2 * DesktopTokens.px(sidePadding)
    implicitHeight: height
    focusPolicy: Qt.StrongFocus
    hoverEnabled: true
    Accessible.role: Accessible.Button
    Accessible.name: text
    Keys.onReturnPressed: event => { button.clicked(); event.accepted = true }
    Keys.onEnterPressed: event => { button.clicked(); event.accepted = true }

    background: Rectangle {
        radius: DesktopTokens.px(button.cornerRadius)
        color: button.fillColor
        border.width: ["primary", "accent", "discord", "link"].indexOf(button.kind) >= 0 ? 0 : 1
        border.color: button.kind === "outline" ? DesktopTokens.seam
            : button.kind === "tinted" ? DesktopFriendsFormat.tint(DesktopFriendsFormat.accent, 0.35) : DesktopTokens.raised
        Behavior on color { ColorAnimation { duration: DesktopTokens.quickDuration } }
        Rectangle {
            anchors.fill: parent
            anchors.margins: -3
            radius: parent.radius + 3
            color: "transparent"
            border.width: 2
            border.color: DesktopTokens.focus
            visible: button.visualFocus
        }
    }
    contentItem: Item {
        Row {
            id: contentRow
            anchors.centerIn: parent
            spacing: DesktopTokens.px(button.gap)
            DesktopFriendsIcon {
                visible: button.glyph !== ""
                anchors.verticalCenter: parent.verticalCenter
                width: DesktopTokens.px(button.glyphSize); height: width
                glyph: button.glyph
                ink: button.inkColor
            }
            DesktopFriendsLabel {
                visible: button.text !== ""
                anchors.verticalCenter: parent.verticalCenter
                text: button.text
                size: button.fontSize
                line: 16
                color: button.inkColor
                font.weight: Font.ExtraBold
            }
            Rectangle {
                visible: button.badge > 0
                anchors.verticalCenter: parent.verticalCenter
                width: Math.max(DesktopTokens.px(18), badgeText.implicitWidth + DesktopTokens.px(8)); height: DesktopTokens.px(18)
                radius: DesktopTokens.px(4)
                color: DesktopFriendsFormat.tint(Theme.face, 0.85)
                DesktopFriendsLabel {
                    id: badgeText
                    anchors.centerIn: parent
                    text: String(button.badge)
                    mono: true; size: 9; line: 16
                    color: Theme.faceText
                    font.weight: Font.Bold
                }
            }
        }
    }
}

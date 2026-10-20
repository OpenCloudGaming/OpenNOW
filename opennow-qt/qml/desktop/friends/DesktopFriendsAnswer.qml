import QtQuick
import OpenNOW

Rectangle {
    id: answer
    property string handle: ""
    property string glyph: "check"
    property color tone: DesktopFriendsFormat.green
    property string title: ""
    property string body: ""
    property string actionText: ""
    property string actionGlyph: ""
    signal actionClicked()
    height: DesktopTokens.px(60)
    radius: DesktopTokens.px(14)
    color: DesktopFriendsFormat.rowFill
    border.width: 1
    border.color: DesktopTokens.seamSoft
    Accessible.role: Accessible.AlertMessage
    Accessible.name: body === "" ? title : qsTr("%1. %2").arg(title).arg(body)

    DesktopFriendsLabel {
        id: handleText
        x: DesktopTokens.px(14)
        width: DesktopTokens.px(120)
        anchors.verticalCenter: parent.verticalCenter
        text: "@" + answer.handle
        elide: Text.ElideRight
        mono: true; size: 13; line: 16
        color: DesktopFriendsFormat.faint
        font.weight: Font.DemiBold
    }
    Rectangle {
        id: badge
        x: handleText.x + handleText.width + DesktopTokens.px(14)
        anchors.verticalCenter: parent.verticalCenter
        width: DesktopTokens.px(32); height: width; radius: width / 2
        color: DesktopFriendsFormat.tint(answer.tone, 0.12)
        border.width: 1
        border.color: DesktopFriendsFormat.tint(answer.tone, 0.35)
        DesktopFriendsIcon {
            anchors.centerIn: parent
            width: DesktopTokens.px(15); height: width
            glyph: answer.glyph
            ink: answer.tone
        }
    }
    Column {
        x: badge.x + badge.width + DesktopTokens.px(14)
        width: (action.visible ? action.x : answer.width) - x - DesktopTokens.px(14)
        anchors.verticalCenter: parent.verticalCenter
        spacing: DesktopTokens.px(2)
        DesktopFriendsLabel {
            width: parent.width
            elide: Text.ElideRight
            text: answer.title
            size: 14; line: 17
            font.weight: Font.ExtraBold
        }
        DesktopFriendsLabel {
            visible: answer.body !== ""
            width: parent.width
            elide: Text.ElideRight
            text: answer.body
            size: 12; line: 16
            color: DesktopTokens.textMuted
        }
    }
    DesktopFriendsButton {
        id: action
        objectName: "friendsAnswerAction"
        visible: answer.actionText !== ""
        anchors.right: parent.right; anchors.rightMargin: DesktopTokens.px(14)
        anchors.verticalCenter: parent.verticalCenter
        text: answer.actionText
        glyph: answer.actionGlyph
        kind: answer.actionGlyph !== "" ? "secondary" : "outline"
        onClicked: answer.actionClicked()
    }
}

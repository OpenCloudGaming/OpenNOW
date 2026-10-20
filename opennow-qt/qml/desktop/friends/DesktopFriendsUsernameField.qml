import QtQuick
import OpenNOW

Rectangle {
    id: field
    property alias text: input.text
    property alias input: input
    property string placeholder: qsTr("Friend’s Discord username")
    property int fontSize: 16
    property bool emphasized: false
    signal submitted(string username)
    signal edited(string username)
    height: DesktopTokens.px(56)
    radius: DesktopTokens.px(14)
    color: DesktopFriendsFormat.fieldFill
    border.width: field.emphasized || input.activeFocus ? 1.5 : 1
    border.color: field.emphasized || input.activeFocus ? DesktopFriendsFormat.accent : DesktopTokens.seam

    function focusInput() { input.forceActiveFocus() }

    DesktopFriendsLabel {
        id: at
        x: DesktopTokens.px(16)
        anchors.verticalCenter: parent.verticalCenter
        text: "@"
        mono: true; size: 18; line: 22
        color: DesktopFriendsFormat.faint
        font.weight: Font.Bold
    }
    TextInput {
        id: input
        objectName: "friendsUsernameInput"
        x: at.x + at.width + DesktopTokens.px(10)
        width: field.width - x - field.trailingWidth - DesktopTokens.px(16)
        anchors.verticalCenter: parent.verticalCenter
        color: DesktopTokens.text
        selectionColor: DesktopFriendsFormat.tint(DesktopFriendsFormat.accent, 0.4)
        cursorDelegate: Rectangle {
            width: DesktopTokens.px(2)
            color: DesktopFriendsFormat.accent
            visible: input.activeFocus
        }
        font.family: DesktopTokens.bodyFont
        font.pixelSize: DesktopTokens.px(field.fontSize)
        font.weight: input.text === "" ? Font.Normal : Font.Bold
        maximumLength: 33
        clip: true
        inputMethodHints: Qt.ImhNoAutoUppercase | Qt.ImhNoPredictiveText
        Accessible.role: Accessible.EditableText
        Accessible.name: field.placeholder
        onTextEdited: field.edited(text)
        Keys.onReturnPressed: field.submitted(text)
        Keys.onEnterPressed: field.submitted(text)
        DesktopFriendsLabel {
            visible: input.text === ""
            anchors.verticalCenter: parent.verticalCenter
            text: field.placeholder
            size: field.fontSize; line: 20
            color: DesktopFriendsFormat.faint
        }
    }
    property real trailingWidth: trailing.width
    default property alias trailingData: trailing.data
    Row {
        id: trailing
        anchors.right: parent.right
        anchors.rightMargin: DesktopTokens.px(6)
        anchors.verticalCenter: parent.verticalCenter
        spacing: DesktopTokens.px(10)
    }
}

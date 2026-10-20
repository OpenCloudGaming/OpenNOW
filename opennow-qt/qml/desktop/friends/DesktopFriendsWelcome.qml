import QtQuick
import OpenNOW

FocusScope {
    id: page
    objectName: "friendsLinked"
    anchors.fill: parent
    property Item screen: null
    readonly property var friends: ShellStore.friends
    readonly property var me: friends.user
    Accessible.role: Accessible.Pane
    Accessible.name: qsTr("You’re all set, %1").arg(friends.displayName(me))
    onActiveFocusChanged: if (activeFocus) field.focusInput()
    Keys.onEscapePressed: friends.dismissWelcome()

    function submit() {
        const name = friends.normalizeUsername(field.text)
        if (name === "")
            return
        friends.sendRequest(name)
        friends.lookupUser(name)
        AppController.navigate("friends-add")
    }

    Column {
        anchors.horizontalCenter: parent.horizontalCenter
        anchors.verticalCenter: parent.verticalCenter
        anchors.verticalCenterOffset: -DesktopTokens.px(30)
        width: Math.min(parent.width - DesktopTokens.px(48), DesktopTokens.px(600))
        spacing: DesktopTokens.px(10)

        Item {
            anchors.horizontalCenter: parent.horizontalCenter
            width: DesktopTokens.px(96); height: width
            DesktopFriendsAvatar {
                user: page.me
                size: 96
                showPresence: false
            }
            Rectangle {
                x: parent.width - width + DesktopTokens.px(4); y: parent.height - height + DesktopTokens.px(4)
                width: DesktopTokens.px(36); height: width; radius: width / 2
                color: "#5865F2"
                border.width: DesktopTokens.px(3); border.color: DesktopFriendsFormat.ring
                DesktopFriendsIcon {
                    anchors.centerIn: parent
                    width: DesktopTokens.px(18); height: DesktopTokens.px(14)
                    glyph: "discord"; ink: "#FFFFFF"
                }
            }
        }
        Row {
            anchors.horizontalCenter: parent.horizontalCenter
            topPadding: DesktopTokens.px(10)
            spacing: DesktopTokens.px(8)
            DesktopFriendsIcon {
                width: DesktopTokens.px(16); height: width
                glyph: "check"; ink: DesktopFriendsFormat.green
            }
            DesktopFriendsLabel {
                text: qsTr("Linked · @%1").arg(page.me ? page.me.username : "")
                mono: true; size: 12; line: 16; tracking: 0.14
                color: DesktopFriendsFormat.green
                font.weight: Font.DemiBold
                font.capitalization: Font.AllUppercase
            }
        }
        DesktopFriendsLabel {
            width: parent.width
            text: qsTr("You’re all set, %1").arg(page.friends.displayName(page.me))
            horizontalAlignment: Text.AlignHCenter
            wrapMode: Text.WordWrap
            size: 32; line: 40
            font.family: DesktopTokens.displayFont
            font.weight: Font.Black
        }
        DesktopFriendsLabel {
            width: parent.width
            text: qsTr("Friends can add you as @%1. Add your first friend now?").arg(page.me ? page.me.username : "")
            horizontalAlignment: Text.AlignHCenter
            wrapMode: Text.WordWrap
            size: 17; line: 24
            color: DesktopTokens.textMuted
        }
        Item { width: 1; height: DesktopTokens.px(4) }
        DesktopFriendsUsernameField {
            id: field
            anchors.horizontalCenter: parent.horizontalCenter
            width: Math.min(parent.width, DesktopTokens.px(520))
            emphasized: true
            onSubmitted: page.submit()
            DesktopFriendsButton {
                objectName: "friendsWelcomeAdd"
                text: qsTr("Add friend")
                glyph: "userPlus"; glyphSize: 16
                kind: "primary"
                controlHeight: 42; cornerRadius: 10; sidePadding: 16; fontSize: 14
                onClicked: page.submit()
            }
        }
        Item {
            width: parent.width; height: later.height + DesktopTokens.px(4)
            DesktopFriendsButton {
                id: later
                objectName: "friendsMaybeLater"
                y: DesktopTokens.px(4)
                anchors.horizontalCenter: parent.horizontalCenter
                text: qsTr("Maybe later")
                kind: "outline"
                controlHeight: 38; cornerRadius: 10; sidePadding: 16; fontSize: 14
                onClicked: page.friends.dismissWelcome()
            }
        }
    }
}

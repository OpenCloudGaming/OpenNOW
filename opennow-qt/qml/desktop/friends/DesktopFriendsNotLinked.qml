import QtQuick
import OpenNOW

FocusScope {
    id: page
    objectName: "friendsNotLinked"
    anchors.fill: parent
    Accessible.role: Accessible.Pane
    Accessible.name: qsTr("Link Discord to use Friends")
    onActiveFocusChanged: if (activeFocus) signIn.forceActiveFocus()

    Column {
        anchors.horizontalCenter: parent.horizontalCenter
        anchors.verticalCenter: parent.verticalCenter
        anchors.verticalCenterOffset: -DesktopTokens.px(30)
        width: Math.min(parent.width - DesktopTokens.px(48), DesktopTokens.px(600))
        spacing: DesktopTokens.px(12)

        Rectangle {
            anchors.horizontalCenter: parent.horizontalCenter
            width: DesktopTokens.px(72); height: width; radius: width / 2
            color: DesktopTokens.raised
            border.width: 1; border.color: DesktopTokens.seam
            DesktopFriendsIcon {
                anchors.centerIn: parent
                width: DesktopTokens.px(34); height: width
                glyph: "users"
            }
        }
        DesktopFriendsTag {
            objectName: "friendsNotLinkedEarlyAccess"
            anchors.horizontalCenter: parent.horizontalCenter
            text: qsTr("Early access")
            tone: DesktopFriendsFormat.amber
        }
        DesktopFriendsLabel {
            width: parent.width
            text: qsTr("Play with your friends")
            horizontalAlignment: Text.AlignHCenter
            size: 32; line: 40
            font.family: DesktopTokens.displayFont
            font.weight: Font.Black
            wrapMode: Text.WordWrap
        }
        DesktopFriendsLabel {
            anchors.horizontalCenter: parent.horizontalCenter
            width: Math.min(parent.width, DesktopTokens.px(520))
            text: qsTr("Sign in with Discord once, and friends can add you by your username and join your co-op.")
            horizontalAlignment: Text.AlignHCenter
            wrapMode: Text.WordWrap
            size: 17; line: 25
            color: DesktopTokens.textMuted
        }
        Item { width: 1; height: DesktopTokens.px(2) }
        DesktopFriendsButton {
            id: signIn
            objectName: "friendsSignInDiscord"
            anchors.horizontalCenter: parent.horizontalCenter
            kind: "discord"
            text: qsTr("Sign in with")
            controlHeight: 52; cornerRadius: 26; sidePadding: 22; fontSize: 15
            implicitWidth: signInRow.implicitWidth + 2 * DesktopTokens.px(22)
            Accessible.name: qsTr("Sign in with Discord")
            contentItem: Item {
                Row {
                    id: signInRow
                    anchors.centerIn: parent
                    spacing: DesktopTokens.px(10)
                    DesktopFriendsLabel {
                        anchors.verticalCenter: parent.verticalCenter
                        text: signIn.text
                        size: 15; line: 18
                        color: "#FFFFFF"
                        font.weight: Font.ExtraBold
                    }
                    DesktopFriendsIcon {
                        anchors.verticalCenter: parent.verticalCenter
                        width: DesktopTokens.px(126); height: DesktopTokens.px(19)
                        glyph: "discordWordmark"; ink: "#FFFFFF"
                    }
                }
            }
            onClicked: ShellStore.friends.startLink()
        }
        Row {
            anchors.horizontalCenter: parent.horizontalCenter
            topPadding: DesktopTokens.px(10)
            spacing: DesktopTokens.px(4)
            DesktopFriendsLabel {
                text: qsTr("No Discord? Co-op codes still work.")
                size: 14; line: 18
                color: DesktopFriendsFormat.faint
            }
            DesktopFriendsButton {
                objectName: "friendsJoinWithCode"
                text: qsTr("Join with a code")
                glyph: "hash"; glyphSize: 14
                controlHeight: 18; sidePadding: 4; fontSize: 14; cornerRadius: 6
                kind: "link"
                onClicked: ShellStore.coop.openJoin("")
            }
        }
    }
}

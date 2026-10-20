import QtQuick
import QtQuick.Effects
import OpenNOW

Rectangle {
    id: card
    property var friend: null
    property real nowMs: Date.now()
    property alias action: cardAction
    readonly property var user: friend ? friend.user : null
    readonly property var value: DesktopFriendsFormat.presence(friend)
    readonly property var activity: DesktopFriendsFormat.activity(friend, nowMs)
    readonly property string art: DesktopFriendsFormat.gameArt(value.game)
    readonly property string detail: {
        switch (ShellStore.friends.activity(friend)) {
        case "playing":
            return value.since ? qsTr("for %1 · solo").arg(DesktopFriendsFormat.ago(value.since, nowMs)) : qsTr("solo")
        case "in_lobby":
            return value.lobby && value.lobby.max ? qsTr("%1 of %2 players").arg(value.lobby.size || 0).arg(value.lobby.max) : ""
        case "in_queue":
            return value.queuePosition ? qsTr("#%1 in line").arg(value.queuePosition) : ""
        case "idle":
            return value.since ? qsTr("away %1").arg(DesktopFriendsFormat.ago(value.since, nowMs)) : ""
        }
        return ""
    }
    readonly property string note: {
        const sinceStamp = Date.parse(String(friend && friend.since || ""))
        const since = isNaN(sinceStamp) ? "" : Qt.locale().standaloneMonthName(new Date(sinceStamp).getMonth(), Locale.LongFormat)
        const offline = ShellStore.friends.activity(friend) === "offline"
        if (!ShellStore.coop.canInviteFriends)
            return since === "" ? "" : qsTr("Friends since %1").arg(since)
        if (since === "")
            return offline ? qsTr("An invite reaches them as a Discord DM") : qsTr("An invite pops up in their OpenNOW")
        return offline ? qsTr("Friends since %1 · an invite reaches them as a Discord DM").arg(since)
            : qsTr("Friends since %1 · an invite pops up in their OpenNOW").arg(since)
    }
    signal manageRequested(string userId)
    width: DesktopTokens.px(286)
    height: body.y + body.height + DesktopTokens.px(16)
    radius: DesktopTokens.px(18)
    color: DesktopFriendsFormat.popupFill
    border.width: 1
    border.color: DesktopTokens.seam
    Accessible.role: Accessible.Grouping
    Accessible.name: ShellStore.friends.displayName(user)

    MouseArea {
        anchors.fill: parent
        hoverEnabled: true
        acceptedButtons: Qt.NoButton
    }
    Item {
        id: header
        x: 1; y: 1
        width: parent.width - 2
        height: card.art !== "" ? DesktopTokens.px(104) - 1 : 0
        visible: height > 0
        Item {
            id: headerMask
            anchors.fill: parent
            visible: false
            layer.enabled: true
            Rectangle {
                width: parent.width; height: parent.height
                radius: card.radius - 1
                color: "white"
            }
            Rectangle {
                y: parent.height / 2
                width: parent.width; height: parent.height / 2
                color: "white"
            }
        }
        Item {
            anchors.fill: parent
            layer.enabled: GraphicsInfo.api !== GraphicsInfo.Software
            layer.smooth: true
            layer.effect: MultiEffect {
                maskEnabled: true
                maskSource: headerMask
                maskThresholdMin: 0.25
                maskSpreadAtMin: 0.2
            }
            Image {
                anchors.fill: parent
                source: card.art
                fillMode: Image.PreserveAspectCrop
                asynchronous: true
                sourceSize: Qt.size(DesktopTokens.px(572), DesktopTokens.px(208))
            }
        }
    }
    Column {
        id: body
        x: DesktopTokens.px(16)
        y: header.y + header.height + DesktopTokens.px(14)
        width: parent.width - 2 * x
        spacing: DesktopTokens.px(12)
        Row {
            spacing: DesktopTokens.px(12)
            DesktopFriendsAvatar {
                anchors.verticalCenter: parent.verticalCenter
                user: card.user
                friend: card.friend
                size: 44; dotSize: 14
                dimmed: ShellStore.friends.activity(card.friend) === "offline"
            }
            Column {
                anchors.verticalCenter: parent.verticalCenter
                spacing: DesktopTokens.px(1)
                DesktopFriendsLabel {
                    text: ShellStore.friends.displayName(card.user)
                    size: 18; line: 22
                    font.weight: Font.Black
                }
                DesktopFriendsLabel {
                    text: "@" + String(card.user && card.user.username || "")
                    mono: true; size: 12; line: 16
                    color: DesktopFriendsFormat.faint
                }
            }
        }
        Column {
            width: parent.width
            spacing: DesktopTokens.px(2)
            Row {
                spacing: DesktopTokens.px(6)
                Rectangle {
                    visible: card.activity.glyph === "dot"
                    anchors.verticalCenter: parent.verticalCenter
                    width: DesktopTokens.px(8); height: width; radius: width / 2
                    color: DesktopFriendsFormat.green
                }
                DesktopFriendsIcon {
                    visible: card.activity.glyph !== "" && card.activity.glyph !== "dot"
                    anchors.verticalCenter: parent.verticalCenter
                    width: DesktopTokens.px(13); height: width
                    glyph: card.activity.glyph || "users"
                    ink: card.activity.color
                }
                DesktopFriendsLabel {
                    text: card.activity.text
                    size: 14; line: 18
                    color: card.activity.color
                    font.weight: card.activity.strong ? Font.ExtraBold : Font.DemiBold
                }
            }
            DesktopFriendsLabel {
                visible: card.detail !== ""
                leftPadding: DesktopTokens.px(19)
                text: card.detail
                size: 13; line: 17
                color: DesktopTokens.textMuted
            }
        }
        Rectangle {
            visible: card.note !== ""
            width: parent.width; height: 1
            color: DesktopTokens.seamSoft
        }
        DesktopFriendsLabel {
            visible: card.note !== ""
            width: parent.width
            text: card.note
            wrapMode: Text.WordWrap
            size: 12; line: 17
            color: DesktopFriendsFormat.faint
        }
        Row {
            spacing: DesktopTokens.px(8)
            DesktopFriendsAction {
                id: cardAction
                anchors.verticalCenter: parent.verticalCenter
                friend: card.friend
                large: true
            }
            DesktopFriendsButton {
                objectName: "friendsCardManage"
                anchors.verticalCenter: parent.verticalCenter
                kind: "outline"
                glyph: "more"; glyphSize: 18
                controlHeight: 36; cornerRadius: 10; sidePadding: 9
                Accessible.name: qsTr("Manage %1").arg(ShellStore.friends.displayName(card.user))
                onClicked: card.manageRequested(card.user.id)
            }
        }
    }
}

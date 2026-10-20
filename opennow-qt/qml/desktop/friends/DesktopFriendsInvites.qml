import QtQuick
import QtQuick.Controls
import OpenNOW

FocusScope {
    id: page
    objectName: "friendsInvites"
    anchors.fill: parent
    property Item screen: null
    readonly property var friends: ShellStore.friends
    readonly property var coop: ShellStore.coop
    readonly property var sections: friends.sections("online", "")
    readonly property var online: sections.inCoop.concat(sections.playing, sections.online)
        .filter(friend => !friends.user || friend.user.id !== friends.user.id)
    Accessible.role: Accessible.Pane
    Accessible.name: qsTr("Invites")

    function lobbyOf(user) {
        const friend = user ? friends.friendById(user.id) : null
        return friend && friend.presence ? friend.presence.lobby : null
    }
    function inviteSubtitle(invite) {
        const lobby = lobbyOf(invite.from)
        const left = DesktopFriendsFormat.countdown(Date.parse(invite.expiresAt) - screen.nowMs)
        return lobby && lobby.max ? qsTr("%1 of %2 players · expires in %3").arg(lobby.size || 0).arg(lobby.max).arg(left)
            : qsTr("Expires in %1").arg(left)
    }
    function historySubtitle(invite) {
        const name = friends.displayName(invite.from)
        if (invite.state === "expired")
            return qsTr("Expired %1 · %2 didn’t start").arg(DesktopFriendsFormat.agoText(invite.expiresAt || invite.endedAt, screen.nowMs)).arg(name)
        if (invite.state === "cancelled")
            return qsTr("%1 ended co-op before you joined").arg(name)
        return qsTr("No longer open")
    }
    function askSubtitle(ask) {
        const name = friends.displayName(ask.host)
        if (ask.state === "full") {
            const lobby = lobbyOf(ask.host)
            return lobby && lobby.max ? qsTr("Lobby is full · %1 of %1 players").arg(lobby.max) : qsTr("Lobby is full")
        }
        if (ask.state === "declined")
            return qsTr("%1 said not now").arg(name)
        return qsTr("Waiting for %1 to let you in").arg(name)
    }

    Flickable {
        anchors.fill: parent
        contentHeight: column.height + DesktopTokens.px(30 + 24)
        boundsBehavior: Flickable.StopAtBounds
        clip: true
        ScrollBar.vertical: ScrollBar { policy: ScrollBar.AsNeeded }

        Column {
            id: column
            y: DesktopTokens.px(30)
            width: Math.min(page.width - DesktopTokens.px(48), DesktopTokens.px(900))
            x: Math.round((page.width - width) / 2)
            spacing: DesktopTokens.px(6)

            DesktopFriendsLabel {
                x: DesktopTokens.px(14)
                text: qsTr("Invites")
                size: 30; line: 36
                font.family: DesktopTokens.displayFont
                font.weight: Font.Black
                Accessible.role: Accessible.Heading
                Accessible.name: text
            }
            DesktopFriendsLabel {
                x: DesktopTokens.px(14)
                width: parent.width - 2 * x
                wrapMode: Text.WordWrap
                text: qsTr("An invite waits here for 10 minutes. Your asks to join show what happened.")
                size: 15; line: 21
                color: DesktopTokens.textMuted
            }
            Item { width: 1; height: DesktopTokens.px(12) }

            DesktopFriendsSection {
                label: qsTr("Invites")
                count: page.coop.incomingInvites.length + page.coop.inviteHistory.length
            }
            DesktopFriendsLabel {
                visible: page.coop.incomingInvites.length + page.coop.inviteHistory.length === 0
                x: DesktopTokens.px(14)
                text: qsTr("No invites right now. When a friend invites you, it shows up here.")
                size: 13; line: 20
                color: DesktopFriendsFormat.faint
            }
            Repeater {
                id: inviteRepeater
                model: page.coop.incomingInvites
                delegate: DesktopFriendsCardRow {
                    required property var modelData
                    required property int index
                    objectName: "friendsInvite_" + modelData.id
                    user: modelData.from
                    title: qsTr("%1 · %2").arg(page.friends.displayName(modelData.from)).arg(DesktopFriendsFormat.gameTitle(modelData.game))
                    subtitle: page.inviteSubtitle(modelData)
                    subtitleGlyph: "clock"
                    subtitleColor: DesktopFriendsFormat.accent
                    subtitleStrong: true
                    game: modelData.game
                    highlighted: true
                    actionWidth: 330
                    DesktopFriendsButton {
                        text: qsTr("Not now")
                        kind: "outline"
                        onClicked: page.coop.declineInvite(modelData.id)
                    }
                    DesktopFriendsButton {
                        objectName: "friendsInviteJoin"
                        text: qsTr("Join")
                        kind: "primary"
                        enabled: page.coop.role !== "host"
                        onClicked: page.coop.acceptInvite(modelData.id)
                    }
                }
            }
            Repeater {
                model: page.coop.inviteHistory
                delegate: DesktopFriendsCardRow {
                    required property var modelData
                    user: modelData.from
                    title: qsTr("%1 · %2").arg(page.friends.displayName(modelData.from)).arg(DesktopFriendsFormat.gameTitle(modelData.game))
                    subtitle: page.historySubtitle(modelData)
                    subtitleGlyph: modelData.state === "expired" ? "clock" : "x"
                    subtitleColor: modelData.state === "expired" ? DesktopFriendsFormat.amber : DesktopFriendsFormat.faint
                    subtitleStrong: true
                    game: modelData.game
                    faded: true
                    actionWidth: 330
                    DesktopFriendsButton {
                        text: qsTr("Remove")
                        kind: "outline"
                        onClicked: page.coop.removeInviteHistory(modelData.id)
                    }
                }
            }

            Item { visible: page.coop.myAsks.length > 0; width: 1; height: DesktopTokens.px(10) }
            DesktopFriendsSection {
                visible: page.coop.myAsks.length > 0
                label: qsTr("Your asks to join")
                count: page.coop.myAsks.length
            }
            Repeater {
                model: page.coop.myAsks
                delegate: DesktopFriendsCardRow {
                    id: askRow
                    required property var modelData
                    readonly property bool watching: modelData.host && page.coop.roomWatch.indexOf(modelData.host.id) >= 0
                    user: modelData.host
                    title: qsTr("%1 · %2").arg(page.friends.displayName(modelData.host)).arg(DesktopFriendsFormat.gameTitle(modelData.game))
                    subtitle: page.askSubtitle(modelData)
                    subtitleGlyph: modelData.state === "full" ? "users" : modelData.state === "declined" ? "x" : "clock"
                    subtitleColor: modelData.state === "full" ? DesktopFriendsFormat.amber
                        : modelData.state === "declined" ? DesktopFriendsFormat.faint : DesktopFriendsFormat.accent
                    subtitleStrong: true
                    game: modelData.game
                    actionWidth: 330
                    DesktopFriendsButton {
                        text: qsTr("Cancel")
                        kind: "outline"
                        onClicked: {
                            if (askRow.watching)
                                page.coop.stopWatchingForRoom(modelData.host.id)
                            page.coop.cancelAsk(modelData.id)
                        }
                    }
                    DesktopFriendsButton {
                        objectName: "friendsWatchForRoom"
                        visible: modelData.state === "full"
                        text: askRow.watching ? qsTr("We’ll tell you") : qsTr("Tell me when there’s room")
                        glyph: "bell"
                        kind: askRow.watching ? "tinted" : "secondary"
                        onClicked: askRow.watching ? page.coop.stopWatchingForRoom(modelData.host.id)
                            : page.coop.watchForRoom(modelData.host.id)
                    }
                }
            }

            Item { visible: page.online.length > 0; width: 1; height: DesktopTokens.px(10) }
            DesktopFriendsSection {
                visible: page.online.length > 0
                label: qsTr("Online")
                count: page.online.length
            }
            Repeater {
                model: page.online.slice(0, 100)
                delegate: DesktopFriendsRow {
                    id: onlineRow
                    required property var modelData
                    readonly property bool invitedMe: page.coop.incomingInvites.some(invite => invite.from && invite.from.id === modelData.user.id)
                    friend: modelData
                    nowMs: page.screen.nowMs
                    actionWidth: 330
                    onActivated: page.screen.showFriend(modelData.user.id)
                    DesktopFriendsTag {
                        visible: onlineRow.invitedMe
                        anchors.verticalCenter: parent.verticalCenter
                        text: qsTr("Invited you")
                    }
                    DesktopFriendsAction {
                        visible: !onlineRow.invitedMe && kind !== ""
                        anchors.verticalCenter: parent.verticalCenter
                        friend: onlineRow.friend
                    }
                }
            }
        }
    }
}

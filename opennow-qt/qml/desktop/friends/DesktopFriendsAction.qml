import QtQuick
import OpenNOW

Item {
    id: action
    property var friend: null
    property bool large: false
    readonly property var coop: ShellStore.coop
    readonly property var user: friend ? friend.user : null
    readonly property var lobby: DesktopFriendsFormat.presence(friend).lobby || null
    readonly property var member: user ? coop.members.find(item => item.userId === user.id) || null : null
    readonly property var invite: user && coop.isHost ? coop.outgoingInviteFor(user.id) : null
    readonly property var ask: lobby ? coop.myAsks.find(item => item.lobbyId === lobby.id) || null : null
    readonly property string kind: {
        if (!friend)
            return ""
        if (member && coop.isHost)
            return "member"
        if (lobby && !DesktopFriendsFormat.inMyLobby(friend))
            return ask ? (ask.state === "full" ? "full" : ask.state === "declined" ? "declined" : "asked") : lobby.joinable === false ? "" : "ask"
        if (!coop.isHost)
            return ""
        return invite && invite.state === "pending" ? "invited" : "invite"
    }
    function trigger() {
        if (kind === "ask")
            coop.askToJoin(friend)
        else if (kind === "invite")
            coop.invite(user.id)
    }
    readonly property Item shown: kind === "member" ? memberTag
        : ["invited", "asked", "full", "declined"].indexOf(kind) >= 0 ? waitingChip
        : kind === "" ? null : button
    implicitWidth: shown ? shown.implicitWidth : 0
    implicitHeight: shown ? shown.height : 0
    width: implicitWidth; height: implicitHeight
    visible: kind !== ""

    DesktopFriendsTag {
        id: memberTag
        visible: action.shown === memberTag
        readonly property var peer: action.user && action.member ? action.coop.peer(action.user.id) : null
        readonly property int ping: peer ? Math.round(Number(peer.peerToHostPingMs || 0)) : 0
        readonly property int slotNumber: action.member ? action.member.slot + 1 : 0
        implicitWidth: width
        text: ping > 0 ? qsTr("P%1 · %2 ms").arg(slotNumber).arg(ping) : qsTr("P%1").arg(slotNumber)
        tone: DesktopFriendsFormat.mint
    }
    DesktopFriendsButton {
        id: waitingChip
        visible: action.shown === waitingChip
        kind: "tinted"
        glyph: "clock"; glyphSize: 14; gap: 7
        text: action.kind === "invited" ? qsTr("Invited · waiting")
            : action.kind === "full" ? qsTr("Lobby is full")
            : action.kind === "declined" ? qsTr("Not this time") : qsTr("Asked · waiting")
        controlHeight: action.large ? 36 : 32
        cornerRadius: action.large ? 10 : 8
        Accessible.description: action.kind === "invited" ? qsTr("Cancel the invite") : ""
        onClicked: if (action.kind === "invited") action.coop.cancelInvite(action.invite.id)
    }
    DesktopFriendsButton {
        id: button
        objectName: action.kind === "ask" ? "friendsAskToJoin" : "friendsInvite"
        visible: action.shown === button
        kind: action.kind === "ask" ? "accent" : action.large ? "primary" : "secondary"
        glyph: action.kind === "ask" ? "hand" : "userPlus"
        text: action.kind === "ask" ? qsTr("Ask to join") : action.large ? qsTr("Invite to your lobby") : qsTr("Invite")
        controlHeight: action.large ? 36 : 32
        cornerRadius: action.large ? 10 : 8
        sidePadding: action.large ? 16 : 12
        onClicked: action.trigger()
    }
}

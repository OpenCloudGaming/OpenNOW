import QtQuick
import OpenNOW

FocusScope {
    id: page
    objectName: "friendsAdd"
    anchors.fill: parent
    property Item screen: null
    readonly property var friends: ShellStore.friends
    readonly property var coop: ShellStore.coop
    readonly property var lookup: friends.lookup
    readonly property string username: lookup ? lookup.username : ""
    readonly property var result: username !== "" ? friends.requestResults[username] || null : null
    readonly property var found: lookup && lookup.user ? lookup.user : null
    readonly property string name: friends.displayName(found)
    readonly property var friend: found ? friends.friendById(found.id) : null
    readonly property var outgoingRequest: found ? friends.outgoing.find(item => item.user && item.user.id === found.id) || null : null
    readonly property var incomingRequest: found ? friends.incoming.find(item => item.user && item.user.id === found.id) || null : null
    // "", "searching", "found", "sent", "accepted", "not_found", "friend", "outgoing", "incoming", "self", "blocked", "error"
    readonly property string answer: {
        if (!lookup)
            return ""
        if (result && result.state !== "sending")
            return result.state === "error" ? "error" : result.state
        if (lookup.state === "searching")
            return "searching"
        if (lookup.state === "error")
            return "error"
        if (friends.user && username === friends.user.username)
            return "self"
        const relation = String(lookup.relation || "none")
        if (relation === "none")
            return found ? "found" : "not_found"
        return relation
    }
    readonly property string errorCode: result && result.state === "error" ? result.code : lookup ? lookup.code : ""
    Accessible.role: Accessible.Pane
    Accessible.name: qsTr("Add a friend")
    onActiveFocusChanged: if (activeFocus) field.focusInput()
    Component.onCompleted: field.text = username
    onUsernameChanged: if (!debounce.running && friends.normalizeUsername(field.text) !== username) field.text = username

    function submit() {
        const name = friends.normalizeUsername(field.text)
        if (name === "")
            return
        debounce.stop()
        if (answer === "found" && username === name)
            friends.sendRequest(name)
        else
            friends.lookupUser(name)
    }
    Timer {
        id: debounce
        interval: 400
        onTriggered: page.friends.lookupUser(field.text)
    }

    Column {
        y: DesktopTokens.px(34)
        width: Math.min(parent.width - DesktopTokens.px(48), DesktopTokens.px(760))
        x: Math.round((parent.width - width) / 2)
        spacing: DesktopTokens.px(10)

        DesktopFriendsLabel {
            text: qsTr("Add a friend")
            size: 30; line: 36
            font.family: DesktopTokens.displayFont
            font.weight: Font.Black
            Accessible.role: Accessible.Heading
            Accessible.name: text
        }
        DesktopFriendsLabel {
            width: parent.width
            wrapMode: Text.WordWrap
            text: qsTr("Type their Discord username. They get a friend request in OpenNOW.")
            size: 16; line: 22
            color: DesktopTokens.textMuted
        }
        Item { width: 1; height: DesktopTokens.px(6) }
        DesktopFriendsUsernameField {
            id: field
            width: parent.width
            fontSize: 18
            emphasized: true
            placeholder: qsTr("Discord username")
            onEdited: debounce.restart()
            onSubmitted: page.submit()
            Row {
                anchors.verticalCenter: parent.verticalCenter
                rightPadding: DesktopTokens.px(10)
                spacing: DesktopTokens.px(8)
                visible: page.answer !== "" && page.friends.normalizeUsername(field.text) === page.username
                DesktopFriendsIcon {
                    visible: page.answer !== "searching"
                    anchors.verticalCenter: parent.verticalCenter
                    width: DesktopTokens.px(14); height: width
                    glyph: page.found ? "check" : "alert"
                    ink: matchText.color
                }
                DesktopFriendsLabel {
                    id: matchText
                    anchors.verticalCenter: parent.verticalCenter
                    text: page.answer === "searching" ? qsTr("Looking up…") : page.found ? qsTr("1 match") : qsTr("No match")
                    size: 12; line: 16
                    color: page.answer === "searching" ? DesktopFriendsFormat.faint : page.found ? DesktopFriendsFormat.green : DesktopFriendsFormat.amber
                    font.weight: Font.Bold
                }
            }
        }

        Rectangle {
            id: foundCard
            objectName: "friendsAddResult"
            visible: page.answer === "found"
            width: parent.width
            height: DesktopTokens.px(56 + 32) + 2
            radius: DesktopTokens.px(16)
            color: DesktopFriendsFormat.tint(DesktopFriendsFormat.accent, 0.07)
            border.width: 1
            border.color: DesktopFriendsFormat.tint(DesktopFriendsFormat.accent, 0.35)
            Accessible.role: Accessible.Grouping
            Accessible.name: qsTr("%1 is on OpenNOW").arg(page.name)
            DesktopFriendsAvatar {
                id: foundAvatar
                x: DesktopTokens.px(18)
                anchors.verticalCenter: parent.verticalCenter
                user: page.found
                size: 56
                showPresence: false
            }
            Column {
                x: foundAvatar.x + foundAvatar.width + DesktopTokens.px(16)
                width: sendButton.x - x - DesktopTokens.px(16)
                anchors.verticalCenter: parent.verticalCenter
                spacing: DesktopTokens.px(4)
                Row {
                    spacing: DesktopTokens.px(10)
                    DesktopFriendsLabel {
                        anchors.verticalCenter: parent.verticalCenter
                        text: page.name
                        size: 20; line: 24
                        font.weight: Font.Black
                    }
                    DesktopFriendsTag {
                        anchors.verticalCenter: parent.verticalCenter
                        text: qsTr("On OpenNOW")
                        tone: DesktopFriendsFormat.green
                    }
                }
                DesktopFriendsLabel {
                    width: parent.width
                    elide: Text.ElideRight
                    text: qsTr("@%1 · Discord display name and avatar").arg(page.found ? page.found.username : "")
                    size: 13; line: 17
                    color: DesktopTokens.textMuted
                }
            }
            DesktopFriendsButton {
                id: sendButton
                objectName: "friendsSendRequest"
                anchors.right: parent.right; anchors.rightMargin: DesktopTokens.px(18)
                anchors.verticalCenter: parent.verticalCenter
                text: qsTr("Send request")
                glyph: "userPlus"; glyphSize: 16
                kind: "primary"
                controlHeight: 42; cornerRadius: 10; sidePadding: 16; fontSize: 14
                onClicked: page.friends.sendRequest(page.username)
            }
        }

        DesktopFriendsAnswer {
            id: answerRow
            objectName: "friendsAddAnswer"
            visible: ["", "found"].indexOf(page.answer) < 0
            width: parent.width
            handle: page.username
            readonly property var spec: {
                const n = page.name
                switch (page.answer) {
                case "searching":
                    return {glyph: "search", tone: DesktopTokens.textMuted, title: qsTr("Looking up @%1…").arg(page.username), body: "", action: ""}
                case "sent":
                    return {glyph: "check", tone: DesktopFriendsFormat.green, title: qsTr("Request sent to %1").arg(n),
                        body: qsTr("They see it the next time they open OpenNOW."), action: qsTr("Undo")}
                case "accepted":
                    return {glyph: "check", tone: DesktopFriendsFormat.green, title: qsTr("You and %1 are friends now").arg(n),
                        body: qsTr("They had already asked you, so you’re connected."), action: qsTr("Show")}
                case "not_found":
                    return {glyph: "alert", tone: DesktopFriendsFormat.amber, title: qsTr("No OpenNOW user is called @%1").arg(page.username),
                        body: page.coop.isHost ? qsTr("Check the spelling, or send your co-op link instead.") : qsTr("Check the spelling and try again."),
                        action: page.coop.isHost ? qsTr("Copy invite link") : "", actionGlyph: "link"}
                case "friend":
                    return {glyph: "userCheck", tone: DesktopFriendsFormat.accent, title: qsTr("%1 is already your friend").arg(n),
                        body: page.friend && DesktopFriendsFormat.inMyLobby(page.friend) ? qsTr("In your co-op lobby right now.")
                            : DesktopFriendsFormat.activity(page.friend, page.screen.nowMs).text, action: qsTr("Show")}
                case "outgoing":
                    return {glyph: "clock", tone: DesktopFriendsFormat.lobby, title: qsTr("You already asked %1").arg(n),
                        body: page.outgoingRequest
                            ? qsTr("Sent %1. Waiting for them to accept.").arg(DesktopFriendsFormat.when(page.outgoingRequest.createdAt, page.screen.nowMs))
                            : qsTr("Waiting for them to accept."), action: qsTr("Cancel request")}
                case "incoming":
                    return {glyph: "userPlus", tone: DesktopFriendsFormat.accent, title: qsTr("%1 already asked you").arg(n),
                        body: qsTr("Accept to become friends."), action: page.incomingRequest ? qsTr("Accept") : ""}
                case "self":
                    return {glyph: "user", tone: DesktopTokens.text, title: qsTr("That’s you"), body: qsTr("Friends add you by this name."), action: ""}
                case "blocked":
                    return {glyph: "ban", tone: DesktopFriendsFormat.red, title: qsTr("You blocked %1").arg(n),
                        body: qsTr("Unblock them to send a request."), action: page.found ? qsTr("Unblock") : ""}
                }
                return {glyph: "alert", tone: DesktopFriendsFormat.amber, title: page.friends.errorText(page.errorCode), body: "", action: qsTr("Try again")}
            }
            glyph: spec.glyph
            tone: spec.tone
            title: spec.title
            body: spec.body
            actionText: spec.action
            actionGlyph: spec.actionGlyph || ""
            onActionClicked: {
                switch (page.answer) {
                case "sent":
                case "outgoing":
                    page.friends.cancelRequestTo(page.username)
                    break
                case "accepted":
                case "friend":
                    page.screen.showFriend(page.found.id)
                    break
                case "not_found":
                    page.coop.copyLink()
                    break
                case "incoming":
                    page.friends.acceptRequest(page.incomingRequest.id)
                    break
                case "blocked":
                    page.friends.unblockUser(page.found.id)
                    break
                default:
                    page.friends.lookupUser(page.username)
                }
            }
        }
    }
}

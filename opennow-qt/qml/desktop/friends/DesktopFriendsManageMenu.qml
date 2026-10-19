import QtQuick
import OpenNOW

FocusScope {
    id: menu
    property var friend: null
    property string armed: ""
    readonly property var user: friend ? friend.user : null
    readonly property string name: ShellStore.friends.displayName(user)
    readonly property bool offline: ShellStore.friends.activity(friend) === "offline"
    readonly property bool canInvite: ShellStore.coop.isHost
    readonly property var lobby: DesktopFriendsFormat.presence(friend).lobby || null
    readonly property bool canAsk: lobby !== null && lobby.joinable !== false && !ShellStore.coop.isHost
        && !DesktopFriendsFormat.inMyLobby(friend) && !ShellStore.coop.myAsks.some(item => item.lobbyId === lobby.id)
    readonly property var entries: (canAsk ? [{key: "ask", glyph: "play", tone: "text", title: qsTr("Ask to join"),
            body: qsTr("%1 gets a request to let you in").arg(name)}] : []).concat([
        {key: "invite", glyph: "send", tone: "text", title: qsTr("Invite by Discord DM"),
            body: !canInvite ? qsTr("Host a co-op lobby first")
                : offline ? qsTr("%1 is offline").arg(name) : qsTr("The OpenNOW bot sends your invite")},
        {key: "remove", glyph: "userX", tone: "text", title: qsTr("Remove friend"),
            body: armed === "remove" ? qsTr("Select again to remove %1").arg(name)
                : qsTr("You both disappear from each other’s list")},
        {key: "block", glyph: "ban", tone: "red", title: qsTr("Block"),
            body: armed === "block" ? qsTr("Select again to block %1").arg(name)
                : qsTr("They can’t see you, invite you or send requests. They aren’t told.")}
    ])
    signal closeRequested()
    implicitWidth: DesktopTokens.px(290)
    implicitHeight: frame.height
    onVisibleChanged: armed = ""
    Accessible.role: Accessible.PopupMenu
    Accessible.name: qsTr("Manage %1").arg(name)

    function focusFirst() {
        Qt.callLater(() => {
            const first = entries.findIndex(entry => entry.key !== "invite" || canInvite)
            if (items.itemAt(first))
                items.itemAt(first).forceItemFocus()
        })
    }
    function activate(key) {
        if (!user)
            return
        if (key === "ask") {
            if (canAsk)
                ShellStore.coop.askToJoin(friend)
            closeRequested()
            return
        }
        if (key === "invite") {
            if (!canInvite)
                return
            ShellStore.coop.invite(user.id)
            closeRequested()
            return
        }
        if (armed !== key) {
            armed = key
            return
        }
        if (key === "remove")
            ShellStore.friends.removeFriend(user.id)
        else
            ShellStore.friends.blockUser(user.id)
        closeRequested()
    }

    Rectangle {
        id: frame
        width: parent.width
        height: list.height + DesktopTokens.px(12)
        radius: DesktopTokens.px(14)
        color: DesktopFriendsFormat.popupFill
        border.width: 1
        border.color: DesktopTokens.seam
        Column {
            id: list
            x: DesktopTokens.px(6); y: DesktopTokens.px(6)
            width: parent.width - 2 * x
            spacing: DesktopTokens.px(2)
            Repeater {
                id: items
                model: menu.entries
                delegate: Column {
                    id: entry
                    required property var modelData
                    required property int index
                    width: list.width
                    spacing: DesktopTokens.px(2)
                    readonly property bool disabled: modelData.key === "invite" && !menu.canInvite
                    function forceItemFocus() { option.forceActiveFocus() }
                    Item { visible: entry.modelData.key === "block"; width: 1; height: 0 }
                    Rectangle {
                        visible: entry.modelData.key === "block"
                        width: parent.width; height: 1
                        color: DesktopTokens.seamSoft
                    }
                    Item { visible: entry.modelData.key === "block"; width: 1; height: 0 }
                    Rectangle {
                        id: option
                        objectName: "friendsManage_" + entry.modelData.key
                        width: parent.width
                        height: optionText.height + DesktopTokens.px(18)
                        radius: DesktopTokens.px(9)
                        activeFocusOnTab: true
                        color: optionHover.hovered || option.activeFocus ? DesktopTokens.raised : "transparent"
                        opacity: entry.disabled ? 0.55 : 1
                        Accessible.role: Accessible.MenuItem
                        Accessible.name: entry.modelData.title
                        Accessible.description: entry.modelData.body
                        Accessible.onPressAction: menu.activate(entry.modelData.key)
                        readonly property color ink: entry.modelData.tone === "red" ? DesktopFriendsFormat.red : DesktopTokens.text
                        HoverHandler { id: optionHover }
                        TapHandler { onTapped: menu.activate(entry.modelData.key) }
                        Keys.onReturnPressed: menu.activate(entry.modelData.key)
                        Keys.onEnterPressed: menu.activate(entry.modelData.key)
                        Keys.onSpacePressed: menu.activate(entry.modelData.key)
                        Keys.onUpPressed: items.itemAt((entry.index + items.count - 1) % items.count).forceItemFocus()
                        Keys.onDownPressed: items.itemAt((entry.index + 1) % items.count).forceItemFocus()
                        DesktopFriendsIcon {
                            x: DesktopTokens.px(10); y: DesktopTokens.px(9) + DesktopTokens.px(1)
                            width: DesktopTokens.px(16); height: width
                            glyph: entry.modelData.glyph
                            ink: option.ink
                        }
                        Column {
                            id: optionText
                            x: DesktopTokens.px(36); y: DesktopTokens.px(9)
                            width: parent.width - x - DesktopTokens.px(10)
                            spacing: DesktopTokens.px(2)
                            DesktopFriendsLabel {
                                text: entry.modelData.title
                                size: 14; line: 17
                                color: option.ink
                                font.weight: Font.ExtraBold
                            }
                            DesktopFriendsLabel {
                                width: parent.width
                                wrapMode: Text.WordWrap
                                text: entry.modelData.body
                                size: 12; line: 16
                                color: menu.armed === entry.modelData.key ? option.ink : DesktopFriendsFormat.faint
                            }
                        }
                    }
                }
            }
        }
    }
}

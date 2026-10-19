import QtQuick
import QtQuick.Controls
import OpenNOW

FocusScope {
    id: page
    objectName: "friendsList"
    anchors.fill: parent
    property Item screen: null
    readonly property var friends: ShellStore.friends
    readonly property var coop: ShellStore.coop
    readonly property var sections: friends.sections(screen.filter, screen.query)
    readonly property var groups: [
        {key: "inCoop", label: qsTr("In co-op"), items: sections.inCoop},
        {key: "playing", label: qsTr("Playing"), items: sections.playing},
        {key: "online", label: qsTr("Online"), items: sections.online}
    ].filter(group => group.items.length > 0)
    readonly property var ordered: sections.inCoop.concat(sections.playing, sections.online, sections.offline)
    readonly property var cardFriend: friends.friendById(screen.selectedUserId)
    property real cardRowY: 0
    property string contextUserId: ""
    readonly property var contextFriend: friends.friendById(contextUserId)

    // Right-click, the Menu key or Shift+F10: the manage menu for that friend, at the pointer or the row.
    function openContextMenu(userId, item, position) {
        screen.selectedUserId = userId
        const point = item.mapToItem(page, position.x, position.y)
        contextUserId = userId
        Qt.callLater(() => {
            const margin = DesktopTokens.px(12)
            contextPopup.x = Math.max(margin, Math.min(page.width - contextPopup.width - margin, point.x))
            contextPopup.y = point.y + contextPopup.height <= page.height - margin ? point.y
                : Math.max(margin, point.y - contextPopup.height)
        })
        contextMenu.focusFirst()
    }
    function closeContextMenu() {
        contextUserId = ""
        page.forceActiveFocus()
    }
    Accessible.role: Accessible.List
    Accessible.name: qsTr("Friends")

    function move(step) {
        if (ordered.length === 0)
            return
        const index = ordered.findIndex(friend => friend.user.id === screen.selectedUserId)
        const next = index < 0 ? (step > 0 ? 0 : ordered.length - 1) : Math.max(0, Math.min(ordered.length - 1, index + step))
        screen.selectedUserId = ordered[next].user.id
    }
    Keys.onPressed: event => {
        if ((event.key === Qt.Key_Menu || (event.key === Qt.Key_F10 && (event.modifiers & Qt.ShiftModifier)))
                && page.cardFriend) {
            page.openContextMenu(page.cardFriend.user.id, page, Qt.point(column.x + DesktopTokens.px(64),
                page.cardRowY + DesktopTokens.px(48)))
            event.accepted = true
        }
    }
    Keys.onUpPressed: move(-1)
    Keys.onDownPressed: move(1)
    Keys.onReturnPressed: card.action.trigger()
    Keys.onEnterPressed: card.action.trigger()
    Keys.onEscapePressed: event => {
        if (screen.selectedUserId === "") {
            event.accepted = false
            return
        }
        screen.selectedUserId = ""
    }
    Connections {
        target: page.screen
        function onFirstResultRequested() {
            if (page.ordered.length > 0)
                page.screen.selectedUserId = page.ordered[0].user.id
            page.forceActiveFocus()
        }
    }

    Flickable {
        id: flick
        anchors.fill: parent
        contentHeight: column.height + DesktopTokens.px(18 + 24)
        boundsBehavior: Flickable.StopAtBounds
        clip: true
        ScrollBar.vertical: ScrollBar { policy: ScrollBar.AsNeeded }

        Column {
            id: column
            y: DesktopTokens.px(18)
            width: Math.min(page.width - DesktopTokens.px(48), DesktopTokens.px(880))
            x: Math.round((page.width - width) / 2)
            spacing: DesktopTokens.px(2)

            Rectangle {
                objectName: "friendsLobbyBanner"
                visible: page.coop.isHost
                width: parent.width; height: visible ? DesktopTokens.px(64) : 0
                radius: DesktopTokens.px(14)
                color: DesktopFriendsFormat.tint(DesktopFriendsFormat.accent, 0.08)
                border.width: 1
                border.color: DesktopFriendsFormat.tint(DesktopFriendsFormat.accent, 0.3)
                Accessible.role: Accessible.Grouping
                Accessible.name: qsTr("Your co-op lobby is open")
                Rectangle {
                    id: lobbyArt
                    x: DesktopTokens.px(12)
                    anchors.verticalCenter: parent.verticalCenter
                    width: DesktopTokens.px(72); height: DesktopTokens.px(40)
                    radius: DesktopTokens.px(6)
                    color: "#1A1F2B"
                    clip: true
                    Image {
                        anchors.fill: parent
                        source: DesktopFriendsFormat.gameArt(page.coop.game) || DesktopTokens.artworkUrl(page.coop.game, true)
                        fillMode: Image.PreserveAspectCrop
                        asynchronous: true
                        sourceSize: Qt.size(DesktopTokens.px(144), DesktopTokens.px(80))
                    }
                }
                Column {
                    x: lobbyArt.x + lobbyArt.width + DesktopTokens.px(14)
                    anchors.verticalCenter: parent.verticalCenter
                    spacing: DesktopTokens.px(3)
                    DesktopFriendsLabel {
                        text: qsTr("Your co-op lobby is open")
                        size: 15; line: 18
                        font.weight: Font.ExtraBold
                    }
                    Row {
                        spacing: DesktopTokens.px(6)
                        DesktopFriendsLabel {
                            text: qsTr("%1 · %2 of %3 players · code").arg(DesktopFriendsFormat.gameTitle(page.coop.game))
                                .arg(page.coop.playerCount).arg(page.coop.maxPlayers)
                            size: 13; line: 17
                            color: DesktopTokens.textMuted
                            font.weight: Font.DemiBold
                        }
                        DesktopFriendsLabel {
                            text: page.coop.lobby ? String(page.coop.lobby.code || "") : ""
                            mono: true; size: 12; line: 16; tracking: 0.06
                            font.weight: Font.DemiBold
                        }
                    }
                }
                Row {
                    anchors.right: parent.right; anchors.rightMargin: DesktopTokens.px(12)
                    anchors.verticalCenter: parent.verticalCenter
                    spacing: DesktopTokens.px(14)
                    DesktopFriendsLabel {
                        anchors.verticalCenter: parent.verticalCenter
                        text: qsTr("Invite is one click on any row")
                        size: 13; line: 17
                        color: DesktopFriendsFormat.faint
                    }
                    DesktopFriendsButton {
                        objectName: "friendsBackToGame"
                        text: qsTr("Back to game")
                        controlHeight: 38; cornerRadius: 10; sidePadding: 16; fontSize: 14
                        visible: ShellStore.resumableSession !== null
                        onClicked: ShellStore.resumeActiveSession()
                    }
                }
            }
            Item { visible: page.coop.isHost; width: 1; height: DesktopTokens.px(2) }

            Repeater {
                model: page.groups
                delegate: Column {
                    id: group
                    required property var modelData
                    required property int index
                    width: column.width
                    spacing: DesktopTokens.px(2)
                    Item { visible: group.index > 0; width: 1; height: DesktopTokens.px(8) }
                    DesktopFriendsSection {
                        label: group.modelData.label
                        count: group.modelData.items.length
                    }
                    Repeater {
                        model: group.modelData.items
                        delegate: DesktopFriendsRow {
                            id: row
                            required property var modelData
                            objectName: "friendsRow-" + modelData.user.id
                            friend: modelData
                            nowMs: page.screen.nowMs
                            selected: page.screen.selectedUserId === modelData.user.id
                            onHoveredChanged: if (hovered) page.screen.selectedUserId = modelData.user.id
                            onActivated: {
                                page.screen.selectedUserId = modelData.user.id
                                page.forceActiveFocus()
                            }
                            onContextMenuRequested: position => page.openContextMenu(modelData.user.id, row, position)
                            Binding {
                                when: row.selected
                                target: page
                                property: "cardRowY"
                                value: row.mapToItem(page, 0, 0).y + 0 * (flick.contentY + row.y + group.y + column.height)
                            }
                            DesktopFriendsAction { friend: row.friend }
                        }
                    }
                }
            }

            Column {
                visible: page.sections.offline.length > 0
                width: parent.width
                spacing: DesktopTokens.px(2)
                Item { visible: page.groups.length > 0; width: 1; height: DesktopTokens.px(8) }
                DesktopFriendsSection {
                    label: qsTr("Offline")
                    count: page.sections.offline.length
                    note: page.coop.isHost ? qsTr("Invites reach them as a Discord DM") : ""
                }
                Flow {
                    x: DesktopTokens.px(14)
                    width: parent.width - 2 * x
                    spacing: DesktopTokens.px(8)
                    Repeater {
                        model: page.sections.offline.slice(0, 60)
                        delegate: Rectangle {
                            id: chip
                            required property var modelData
                            objectName: "friendsChip-" + modelData.user.id
                            readonly property bool selected: page.screen.selectedUserId === modelData.user.id
                            width: chipRow.implicitWidth + DesktopTokens.px(8 + 14)
                            height: DesktopTokens.px(44)
                            radius: DesktopTokens.px(12)
                            color: selected ? DesktopTokens.raised : DesktopFriendsFormat.rowFill
                            border.width: 1
                            border.color: selected ? DesktopTokens.seam : DesktopTokens.seamSoft
                            Accessible.role: Accessible.ListItem
                            Accessible.name: qsTr("%1, %2").arg(page.friends.displayName(modelData.user))
                                .arg(DesktopFriendsFormat.lastSeen(modelData.presence && modelData.presence.lastSeenAt, page.screen.nowMs))
                            HoverHandler { onHoveredChanged: if (hovered) page.screen.selectedUserId = chip.modelData.user.id }
                            TapHandler {
                                onTapped: {
                                    page.screen.selectedUserId = chip.modelData.user.id
                                    page.forceActiveFocus()
                                }
                            }
                            TapHandler {
                                acceptedButtons: Qt.RightButton
                                onTapped: eventPoint => page.openContextMenu(chip.modelData.user.id, chip, eventPoint.position)
                            }
                            Binding {
                                when: chip.selected
                                target: page
                                property: "cardRowY"
                                value: chip.mapToItem(page, 0, 0).y + 0 * (flick.contentY + chip.y + column.height)
                            }
                            Row {
                                id: chipRow
                                x: DesktopTokens.px(8)
                                anchors.verticalCenter: parent.verticalCenter
                                spacing: DesktopTokens.px(10)
                                DesktopFriendsAvatar {
                                    anchors.verticalCenter: parent.verticalCenter
                                    user: chip.modelData.user
                                    friend: chip.modelData
                                    size: 28; dotSize: 10
                                    dimmed: true
                                }
                                Column {
                                    anchors.verticalCenter: parent.verticalCenter
                                    spacing: DesktopTokens.px(1)
                                    DesktopFriendsLabel {
                                        text: page.friends.displayName(chip.modelData.user)
                                        size: 14; line: 16
                                        color: DesktopFriendsFormat.tint(DesktopTokens.text, 0.7)
                                        font.weight: Font.ExtraBold
                                    }
                                    DesktopFriendsLabel {
                                        text: DesktopFriendsFormat.lastSeen(chip.modelData.presence && chip.modelData.presence.lastSeenAt,
                                            page.screen.nowMs)
                                        size: 11; line: 14
                                        color: DesktopFriendsFormat.faint
                                        font.weight: Font.DemiBold
                                    }
                                }
                            }
                        }
                    }
                }
            }

            Column {
                visible: page.ordered.length === 0 && page.friends.loaded
                width: parent.width
                topPadding: DesktopTokens.px(80)
                spacing: DesktopTokens.px(10)
                DesktopFriendsLabel {
                    width: parent.width
                    horizontalAlignment: Text.AlignHCenter
                    text: page.screen.query !== "" ? qsTr("No friends match “%1”").arg(page.screen.query)
                        : page.friends.friends.length === 0 ? qsTr("No friends yet") : qsTr("Nobody here right now")
                    size: 20; line: 26
                    font.weight: Font.Black
                }
                DesktopFriendsLabel {
                    width: parent.width
                    horizontalAlignment: Text.AlignHCenter
                    wrapMode: Text.WordWrap
                    visible: page.friends.friends.length === 0
                    text: qsTr("Add a friend by their Discord username. They get a friend request in OpenNOW.")
                    size: 15; line: 21
                    color: DesktopTokens.textMuted
                }
            }
            DesktopFriendsLabel {
                visible: page.friends.lastError !== ""
                width: parent.width
                topPadding: DesktopTokens.px(12)
                horizontalAlignment: Text.AlignHCenter
                wrapMode: Text.WordWrap
                text: page.friends.lastError
                size: 13; line: 17
                color: DesktopFriendsFormat.amber
            }
        }
    }

    DesktopFriendsCard {
        id: card
        objectName: "friendsHoverCard"
        friend: page.cardFriend
        nowMs: page.screen.nowMs
        readonly property bool docked: page.width - (column.x + column.width) >= width + DesktopTokens.px(24 + 14)
        readonly property real rowBottom: page.cardRowY + DesktopTokens.px(54 + 6)
        readonly property real floatingY: rowBottom + height <= page.height - DesktopTokens.px(12)
            ? rowBottom : page.cardRowY - height - DesktopTokens.px(6)
        visible: friend !== null
        x: page.width - width - DesktopTokens.px(24)
        y: Math.max(DesktopTokens.px(12), Math.min(page.height - height - DesktopTokens.px(12),
            docked ? page.cardRowY - DesktopTokens.px(14) : floatingY))
        onManageRequested: userId => page.screen.openManage(userId)
    }

    Popup {
        id: contextPopup
        visible: page.contextFriend !== null
        padding: 0
        margins: DesktopTokens.px(12)
        focus: true
        closePolicy: Popup.CloseOnEscape | Popup.CloseOnPressOutside
        background: null
        onClosed: if (page.contextUserId !== "") page.closeContextMenu()
        contentItem: DesktopFriendsManageMenu {
            id: contextMenu
            objectName: "friendsContextMenu"
            friend: page.contextFriend
            focus: true
            onCloseRequested: page.closeContextMenu()
        }
    }
}

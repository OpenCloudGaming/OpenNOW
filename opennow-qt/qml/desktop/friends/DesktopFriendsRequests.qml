import QtQuick
import QtQuick.Controls
import OpenNOW

FocusScope {
    id: page
    objectName: "friendsRequests"
    anchors.fill: parent
    property Item screen: null
    readonly property var friends: ShellStore.friends
    readonly property var managed: friends.friendById(screen.manageUserId)
    Accessible.role: Accessible.Pane
    Accessible.name: qsTr("Friend requests")
    onActiveFocusChanged: if (activeFocus && screen.manageMenuOpen && managed) menu.focusFirst()

    function closeMenu() {
        screen.manageMenuOpen = false
        moreButton.forceActiveFocus()
    }
    Keys.onEscapePressed: event => {
        if (!screen.manageMenuOpen) {
            event.accepted = false
            return
        }
        closeMenu()
    }

    Flickable {
        anchors.fill: parent
        contentHeight: Math.max(column.y + column.height, menu.visible ? menu.mapToItem(column, 0, menu.height).y + column.y : 0)
            + DesktopTokens.px(24)
        boundsBehavior: Flickable.StopAtBounds
        clip: true
        ScrollBar.vertical: ScrollBar { policy: ScrollBar.AsNeeded }

        Column {
            id: column
            y: DesktopTokens.px(34)
            width: Math.min(page.width - DesktopTokens.px(48), DesktopTokens.px(760))
            x: Math.round((page.width - width) / 2)
            spacing: DesktopTokens.px(8)

            Item {
                width: parent.width; height: DesktopTokens.px(38)
                DesktopFriendsLabel {
                    anchors.verticalCenter: parent.verticalCenter
                    text: qsTr("Friend requests")
                    size: 30; line: 36
                    font.family: DesktopTokens.displayFont
                    font.weight: Font.Black
                    Accessible.role: Accessible.Heading
                    Accessible.name: text
                }
                DesktopFriendsButton {
                    anchors.right: parent.right
                    anchors.verticalCenter: parent.verticalCenter
                    text: qsTr("Add friend")
                    glyph: "userPlus"; glyphSize: 16
                    kind: "primary"
                    controlHeight: 38; cornerRadius: 10; sidePadding: 16; fontSize: 14
                    onClicked: AppController.navigate("friends-add")
                }
            }
            Item { width: 1; height: DesktopTokens.px(6) }

            DesktopFriendsSection { label: qsTr("Incoming"); count: page.friends.incoming.length }
            DesktopFriendsLabel {
                visible: page.friends.incoming.length === 0
                x: DesktopTokens.px(14)
                text: qsTr("No requests waiting.")
                size: 13; line: 20
                color: DesktopFriendsFormat.faint
            }
            Repeater {
                model: page.friends.incoming
                delegate: DesktopFriendsCardRow {
                    required property var modelData
                    rowHeight: 60
                    user: modelData.user
                    title: page.friends.displayName(modelData.user)
                    handle: String(modelData.user.username || "")
                    subtitle: qsTr("Sent %1").arg(DesktopFriendsFormat.when(modelData.createdAt, page.screen.nowMs))
                    DesktopFriendsButton {
                        text: qsTr("Decline")
                        kind: "outline"
                        Accessible.name: qsTr("Decline %1").arg(page.friends.displayName(modelData.user))
                        onClicked: page.friends.declineRequest(modelData.id)
                    }
                    DesktopFriendsButton {
                        objectName: "friendsAcceptRequest"
                        text: qsTr("Accept")
                        glyph: "check"
                        kind: "primary"
                        Accessible.name: qsTr("Accept %1").arg(page.friends.displayName(modelData.user))
                        onClicked: page.friends.acceptRequest(modelData.id)
                    }
                }
            }
            Item { width: 1; height: DesktopTokens.px(10) }
            DesktopFriendsSection { label: qsTr("Outgoing"); count: page.friends.outgoing.length }
            DesktopFriendsLabel {
                visible: page.friends.outgoing.length === 0
                x: DesktopTokens.px(14)
                text: qsTr("You haven’t asked anyone yet.")
                size: 13; line: 20
                color: DesktopFriendsFormat.faint
            }
            Repeater {
                model: page.friends.outgoing
                delegate: DesktopFriendsCardRow {
                    required property var modelData
                    rowHeight: 60
                    user: modelData.user
                    title: page.friends.displayName(modelData.user)
                    handle: String(modelData.user.username || "")
                    subtitle: qsTr("Sent %1 · waiting").arg(DesktopFriendsFormat.when(modelData.createdAt, page.screen.nowMs))
                    DesktopFriendsButton {
                        text: qsTr("Cancel request")
                        kind: "outline"
                        Accessible.name: qsTr("Cancel request to %1").arg(page.friends.displayName(modelData.user))
                        onClicked: page.friends.cancelRequest(modelData.id)
                    }
                }
            }

            Item { visible: page.managed !== null; width: 1; height: DesktopTokens.px(10) }
            DesktopFriendsSection { visible: page.managed !== null; label: qsTr("Manage a friend") }
            Item {
                visible: page.managed !== null
                z: 2
                width: parent.width
                height: managedRow.height
                DesktopFriendsCardRow {
                    id: managedRow
                    rowHeight: 60
                    avatarDimmed: offline
                    readonly property bool offline: page.friends.activity(page.managed) === "offline"
                    user: page.managed ? page.managed.user : null
                    title: page.friends.displayName(user)
                    handle: user ? String(user.username || "") : ""
                    subtitle: page.managed === null ? "" : offline
                        ? qsTr("Offline · last seen %1").arg(DesktopFriendsFormat.agoText(page.managed.presence && page.managed.presence.lastSeenAt, page.screen.nowMs))
                        : DesktopFriendsFormat.activity(page.managed, page.screen.nowMs).text
                    DesktopFriendsButton {
                        id: moreButton
                        objectName: "friendsManageButton"
                        width: DesktopTokens.px(36)
                        controlHeight: 36; cornerRadius: 10; sidePadding: 9
                        glyph: "more"; glyphSize: 18
                        kind: page.screen.manageMenuOpen ? "secondary" : "outline"
                        background: Rectangle {
                            radius: DesktopTokens.px(10)
                            color: page.screen.manageMenuOpen || moreButton.hovered ? DesktopTokens.raisedStrong : "transparent"
                            border.width: moreButton.visualFocus ? 2 : 1
                            border.color: moreButton.visualFocus ? DesktopTokens.focus : DesktopTokens.seam
                        }
                        Accessible.name: qsTr("Manage %1").arg(managedRow.title)
                        Accessible.role: Accessible.ButtonMenu
                        onClicked: {
                            page.screen.manageMenuOpen = !page.screen.manageMenuOpen
                            if (page.screen.manageMenuOpen)
                                menu.focusFirst()
                        }
                    }
                }
                DesktopFriendsManageMenu {
                    id: menu
                    objectName: "friendsManageMenu"
                    visible: page.screen.manageMenuOpen && page.managed !== null
                    friend: page.managed
                    anchors.right: parent.right; anchors.rightMargin: DesktopTokens.px(14)
                    y: DesktopTokens.px(58)
                    onCloseRequested: page.closeMenu()
                }
            }
        }
    }
}

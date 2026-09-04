pragma ComponentBehavior: Bound
import QtQuick
import QtQuick.Controls
import OpenNOW

FocusScope {
    id: root
    anchors.fill: parent
    clip: true
    focus: true
    Accessible.role: Accessible.Pane
    Accessible.name: qsTr("Friends")

    readonly property var capabilities: ShellStore.socialCapabilities || ({})
    readonly property bool friendsAvailable: Boolean(root.capabilities.friendsAvailable)
    readonly property bool localPlayAvailable: root.capabilities.localControllerJoin !== false
    readonly property var friends: []
    readonly property var controllers: ControllerInput.controllers || []
    readonly property string unavailableReason: String(root.capabilities.reason || "")
    readonly property string statusLabel: root.friendsAvailable ? qsTr("LIVE") : qsTr("UNAVAILABLE")
    readonly property string bodyText: {
        if (!root.friendsAvailable)
            return root.unavailableReason || qsTr("GeForce NOW does not expose a friends API OpenNOW can use.")
        if (root.friends.length > 0)
            return ""
        return qsTr("No friends yet. Your provider marked friends as available, but OpenNOW has no contacts to show.")
    }

    function friendName(friend) {
        if (friend === undefined || friend === null)
            return ""
        if (typeof friend === "string")
            return friend
        return String(friend.displayName || friend.name || friend.id || "")
    }

    function friendStatus(friend) {
        if (!friend || typeof friend !== "object")
            return ""
        return String(friend.status || friend.presence || "")
    }

    function friendInitial(friend) {
        const name = root.friendName(friend)
        return name.length ? name.charAt(0).toUpperCase() : ""
    }

    function openLinkedAccounts() {
        AppController.navigate("settings-stores")
    }

    Item {
        anchors.fill: parent
        anchors.leftMargin: 24
        anchors.rightMargin: 24
        anchors.topMargin: 18
        anchors.bottomMargin: 18

        Rectangle {
            id: mainPanel
            anchors.left: parent.left
            anchors.top: parent.top
            anchors.bottom: parent.bottom
            anchors.right: sideColumn.left
            anchors.rightMargin: 16
            radius: 16
            color: "#B80B0F1A"
            border.width: 1
            border.color: "#14FFFFFF"

            Row {
                id: headerRow
                anchors.left: parent.left
                anchors.right: parent.right
                anchors.top: parent.top
                anchors.leftMargin: 16
                anchors.rightMargin: 16
                anchors.topMargin: 16
                spacing: 10
                height: 30

                Text {
                    anchors.verticalCenter: parent.verticalCenter
                    text: qsTr("Friends")
                    color: DesktopTokens.text
                    font.family: DesktopTokens.bodyFont
                    font.pixelSize: 14
                    font.weight: Font.Black
                }

                Rectangle {
                    anchors.verticalCenter: parent.verticalCenter
                    width: statusBadge.implicitWidth + 14
                    height: 20
                    radius: 6
                    color: root.friendsAvailable ? "#146EE7B7" : "#14FFFFFF"
                    border.width: 1
                    border.color: root.friendsAvailable ? "#2E6EE7B7" : "#1FFFFFFF"
                    Text {
                        id: statusBadge
                        anchors.centerIn: parent
                        text: root.statusLabel
                        color: root.friendsAvailable ? DesktopTokens.mint : DesktopTokens.textMuted
                        font.family: DesktopTokens.monoFont
                        font.pixelSize: 9
                        font.weight: Font.Bold
                        font.letterSpacing: 0.7
                    }
                }
            }

            ListView {
                id: friendList
                anchors.left: parent.left
                anchors.right: parent.right
                anchors.top: headerRow.bottom
                anchors.bottom: parent.bottom
                anchors.leftMargin: 16
                anchors.rightMargin: 16
                anchors.topMargin: 12
                anchors.bottomMargin: 16
                visible: root.friendsAvailable && root.friends.length > 0
                clip: true
                spacing: 4
                model: root.friends
                delegate: ItemDelegate {
                    id: friendRow
                    required property var modelData
                    width: ListView.view.width
                    height: 52
                    padding: 0
                    background: Rectangle {
                        radius: 12
                        color: friendRow.hovered || friendRow.activeFocus ? "#0FFFFFFF" : "transparent"
                    }
                    contentItem: Item {
                        Rectangle {
                            x: 10
                            anchors.verticalCenter: parent.verticalCenter
                            width: 30
                            height: 30
                            radius: 15
                            color: "#14FFFFFF"
                            border.width: 1
                            border.color: "#1FFFFFFF"
                            Text {
                                anchors.centerIn: parent
                                text: root.friendInitial(friendRow.modelData)
                                color: DesktopTokens.textHigh
                                font.family: DesktopTokens.bodyFont
                                font.pixelSize: 12
                                font.weight: Font.Black
                            }
                        }
                        Column {
                            x: 50
                            width: parent.width - 62
                            anchors.verticalCenter: parent.verticalCenter
                            spacing: 3
                            Text {
                                width: parent.width
                                text: root.friendName(friendRow.modelData)
                                color: DesktopTokens.text
                                font.family: DesktopTokens.bodyFont
                                font.pixelSize: 14
                                font.weight: Font.Bold
                                elide: Text.ElideRight
                            }
                            Text {
                                visible: root.friendStatus(friendRow.modelData) !== ""
                                width: parent.width
                                text: root.friendStatus(friendRow.modelData)
                                color: DesktopTokens.textMuted
                                font.family: DesktopTokens.bodyFont
                                font.pixelSize: 12
                                elide: Text.ElideRight
                            }
                        }
                    }
                }
            }

            Column {
                id: emptyState
                visible: !friendList.visible
                anchors.left: parent.left
                anchors.right: parent.right
                anchors.verticalCenter: parent.verticalCenter
                anchors.leftMargin: 32
                anchors.rightMargin: 32
                spacing: 14

                Rectangle {
                    anchors.horizontalCenter: parent.horizontalCenter
                    width: 48
                    height: 48
                    radius: 24
                    color: "#14FFFFFF"
                    border.width: 1
                    border.color: "#1FFFFFFF"
                    DesktopGlyph {
                        anchors.centerIn: parent
                        width: 18
                        height: 18
                        icon: root.friendsAvailable ? "desktop-coop.svg" : "desktop-lock.svg"
                    }
                }

                Text {
                    anchors.horizontalCenter: parent.horizontalCenter
                    width: Math.min(parent.width, 460)
                    horizontalAlignment: Text.AlignHCenter
                    wrapMode: Text.WordWrap
                    text: root.friendsAvailable
                        ? qsTr("No friends yet")
                        : qsTr("GeForce NOW friends are unavailable")
                    color: DesktopTokens.text
                    font.family: DesktopTokens.bodyFont
                    font.pixelSize: 18
                    font.weight: Font.Bold
                }

                Text {
                    anchors.horizontalCenter: parent.horizontalCenter
                    width: Math.min(parent.width, 460)
                    horizontalAlignment: Text.AlignHCenter
                    wrapMode: Text.WordWrap
                    text: root.bodyText
                    color: DesktopTokens.textMuted
                    font.family: DesktopTokens.bodyFont
                    font.pixelSize: 13
                    lineHeight: 1.4
                }

                DesktopButton {
                    id: manageAccounts
                    anchors.horizontalCenter: parent.horizontalCenter
                    primary: true
                    height: 36
                    text: qsTr("Manage linked accounts")
                    onClicked: root.openLinkedAccounts()
                }
            }
        }

        Column {
            id: sideColumn
            anchors.right: parent.right
            anchors.top: parent.top
            width: Math.max(240, Math.min(352, parent.width * 0.32))
            spacing: 16

            Rectangle {
                visible: root.localPlayAvailable
                width: parent.width
                height: localPlayBody.implicitHeight + 32
                radius: 16
                color: "#C70B0F1A"
                border.width: 1
                border.color: "#1FFFFFFF"

                Column {
                    id: localPlayBody
                    anchors.left: parent.left
                    anchors.right: parent.right
                    anchors.top: parent.top
                    anchors.leftMargin: 16
                    anchors.rightMargin: 16
                    anchors.topMargin: 16
                    spacing: 10

                    Row {
                        width: parent.width
                        spacing: 8
                        Text {
                            text: qsTr("Local play")
                            color: DesktopTokens.text
                            font.family: DesktopTokens.bodyFont
                            font.pixelSize: 14
                            font.weight: Font.Black
                        }
                        Text {
                            anchors.verticalCenter: parent.verticalCenter
                            text: root.controllers.length
                                ? qsTr("%1 ready").arg(root.controllers.length)
                                : qsTr("NO PADS")
                            color: DesktopTokens.focus
                            font.family: DesktopTokens.monoFont
                            font.pixelSize: 10
                            font.weight: Font.DemiBold
                            font.letterSpacing: 0.6
                        }
                    }

                    Text {
                        width: parent.width
                        wrapMode: Text.WordWrap
                        text: qsTr("OpenNOW can still do local co-op with extra controllers on this machine. That is not an online party.")
                        color: DesktopTokens.textMuted
                        font.family: DesktopTokens.bodyFont
                        font.pixelSize: 12
                        lineHeight: 1.4
                    }

                    Repeater {
                        model: root.controllers
                        delegate: Rectangle {
                            required property var modelData
                            width: localPlayBody.width
                            height: 44
                            radius: 12
                            color: "#0FFFFFFF"
                            Rectangle {
                                x: 10
                                anchors.verticalCenter: parent.verticalCenter
                                width: 26
                                height: 26
                                radius: 13
                                color: "#29FFFFFF"
                                Text {
                                    anchors.centerIn: parent
                                    text: "P" + (modelData.slot !== undefined ? modelData.slot : "")
                                    color: DesktopTokens.text
                                    font.family: DesktopTokens.monoFont
                                    font.pixelSize: 9
                                    font.weight: Font.Bold
                                }
                            }
                            Text {
                                x: 46
                                width: parent.width - 58
                                anchors.verticalCenter: parent.verticalCenter
                                text: String(modelData.name || qsTr("Controller"))
                                color: DesktopTokens.textHigh
                                font.family: DesktopTokens.bodyFont
                                font.pixelSize: 13
                                font.weight: Font.Bold
                                elide: Text.ElideRight
                            }
                        }
                    }

                    DesktopButton {
                        width: parent.width
                        height: 36
                        text: qsTr("Controller settings")
                        onClicked: AppController.navigate("controllers")
                    }
                }
            }

            Rectangle {
                width: parent.width
                height: accountsNote.implicitHeight + 32
                radius: 16
                color: "#8A0B0F1A"
                border.width: 1
                border.color: "#0FFFFFFF"

                Column {
                    id: accountsNote
                    anchors.left: parent.left
                    anchors.right: parent.right
                    anchors.top: parent.top
                    anchors.leftMargin: 16
                    anchors.rightMargin: 16
                    anchors.topMargin: 16
                    spacing: 9

                    Text {
                        width: parent.width
                        wrapMode: Text.WordWrap
                        text: qsTr("Friends come from your account")
                        color: DesktopTokens.textHigh
                        font.family: DesktopTokens.bodyFont
                        font.pixelSize: 13
                        font.weight: Font.Bold
                    }
                    Text {
                        width: parent.width
                        wrapMode: Text.WordWrap
                        text: qsTr("GeForce NOW does not give OpenNOW a friends, presence, or invite service. Linked stores still unlock your library — they do not invent contacts.")
                        color: "#8AFFFFFF"
                        font.family: DesktopTokens.bodyFont
                        font.pixelSize: 12
                        lineHeight: 1.45
                    }
                    Text {
                        text: qsTr("Manage linked accounts  ›")
                        color: DesktopTokens.focus
                        font.family: DesktopTokens.bodyFont
                        font.pixelSize: 12
                        font.weight: Font.Bold
                        Accessible.role: Accessible.Button
                        Accessible.name: qsTr("Manage linked accounts")
                        HoverHandler { cursorShape: Qt.PointingHandCursor }
                        TapHandler { onTapped: root.openLinkedAccounts() }
                    }
                }
            }
        }
    }
}

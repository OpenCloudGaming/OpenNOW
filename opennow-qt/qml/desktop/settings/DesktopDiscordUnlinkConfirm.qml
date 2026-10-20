import QtQuick
import QtQuick.Controls
import OpenNOW

// Unlink Discord confirmation (overlay discord-unlink-confirm), artboard N41-1.
FocusScope {
    id: root
    objectName: "discordUnlinkConfirmHost"
    property bool opened: false
    readonly property var friends: ShellStore.friends
    readonly property var user: friends.user
    readonly property int friendCount: friends.friends.length
    readonly property int requestCount: friends.incoming.length
    visible: reveal.present
    enabled: opened
    focus: opened
    MotionProgress { id: reveal; shown: root.opened }

    function cancel() {
        AppController.showOverlay("")
    }
    function confirm() {
        root.friends.unlink()
        AppController.showOverlay("")
    }

    onOpenedChanged: if (opened) Qt.callLater(() => { if (root.opened) keepButton.forceActiveFocus() })
    Keys.onPressed: event => {
        if (event.key === Qt.Key_Escape || event.key === Qt.Key_Back) {
            root.cancel()
            event.accepted = true
        }
    }

    Rectangle {
        anchors.fill: parent
        color: "#B805070C"
        opacity: reveal.progress
        TapHandler { onTapped: root.cancel() }
    }

    Rectangle {
        id: card
        objectName: "discordUnlinkConfirm"
        anchors.centerIn: parent
        width: Math.min(540, root.width - 48)
        height: body.implicitHeight + 48
        radius: 22
        color: "#F00E1018"
        border.width: 1
        border.color: "#24FFFFFF"
        opacity: reveal.progress
        scale: reveal.zoom
        Accessible.role: Accessible.Dialog
        Accessible.name: qsTr("Unlink Discord?")

        Column {
            id: body
            x: 26
            y: 26
            width: parent.width - 52
            spacing: 18

            Row {
                spacing: 12
                Item {
                    width: 36
                    height: 36
                    DesktopCoopNoticeAvatar { width: 36; user: root.user }
                    Rectangle {
                        x: 22
                        y: 22
                        width: 18
                        height: 18
                        radius: 9
                        color: "#5865F2"
                        border.width: 2
                        border.color: "#0B0F1A"
                        Image {
                            anchors.centerIn: parent
                            width: 11
                            height: 8
                            source: "qrc:/qt/qml/OpenNOW/res/icons/desktop-discord.svg"
                            sourceSize: Qt.size(22, 16)
                        }
                    }
                }
                Column {
                    anchors.verticalCenter: parent.verticalCenter
                    spacing: 1
                    Text {
                        text: root.user ? qsTr("%1 · @%2").arg(root.friends.displayName(root.user)).arg(root.user.username) : ""
                        color: "#FFFFFF"
                        font.family: DesktopTokens.bodyFont
                        font.pixelSize: 14
                        font.weight: Font.ExtraBold
                        height: 17
                        verticalAlignment: Text.AlignVCenter
                    }
                    Text {
                        text: qsTr("Discord account linked to OpenNOW")
                        color: "#66FFFFFF"
                        font.family: DesktopTokens.bodyFont
                        font.pixelSize: 12
                        height: 16
                        verticalAlignment: Text.AlignVCenter
                    }
                }
            }

            Column {
                width: parent.width
                spacing: 6
                Text {
                    width: parent.width
                    text: qsTr("Unlink Discord?")
                    color: "#FFFFFF"
                    font.family: DesktopTokens.displayFont
                    font.pixelSize: 26
                    font.weight: Font.Black
                    height: 32
                    verticalAlignment: Text.AlignVCenter
                }
                Text {
                    width: parent.width
                    text: qsTr("You stay signed in to OpenNOW. Here’s what changes:")
                    color: "#A3FFFFFF"
                    font.family: DesktopTokens.bodyFont
                    font.pixelSize: 15
                    lineHeightMode: Text.FixedHeight
                    lineHeight: 21
                    wrapMode: Text.WordWrap
                }
            }

            Column {
                width: parent.width
                spacing: 10
                Repeater {
                    model: [
                        {icon: "desktop-users.svg", text: root.requestCount > 0
                            ? qsTr("Your %1 friends and %2 requests are kept on our server, hidden until you link again.").arg(root.friendCount).arg(root.requestCount)
                            : root.friendCount === 1 ? qsTr("Your friend is kept on our server, hidden until you link again.")
                            : qsTr("Your %1 friends are kept on our server, hidden until you link again.").arg(root.friendCount)},
                        {icon: "desktop-user-x-amber.svg", text: qsTr("You can’t add or invite anyone, and friends can’t see or invite you.")},
                        {icon: "desktop-hash-green.svg", text: qsTr("Co-op codes and links keep working, for you and your guests.")}
                    ]
                    delegate: Row {
                        required property var modelData
                        width: body.width
                        spacing: 12
                        Rectangle {
                            anchors.verticalCenter: parent.verticalCenter
                            width: 30
                            height: 30
                            radius: 9
                            color: "#14FFFFFF"
                            Image {
                                anchors.centerIn: parent
                                width: 15
                                height: 15
                                source: "qrc:/qt/qml/OpenNOW/res/icons/" + modelData.icon
                                sourceSize: Qt.size(30, 30)
                            }
                        }
                        Text {
                            anchors.verticalCenter: parent.verticalCenter
                            width: parent.width - 42
                            text: modelData.text
                            color: "#D9FFFFFF"
                            font.family: DesktopTokens.bodyFont
                            font.pixelSize: 14
                            lineHeightMode: Text.FixedHeight
                            lineHeight: 20
                            wrapMode: Text.WordWrap
                        }
                    }
                }
            }

            Text {
                width: parent.width
                text: qsTr("Link the same Discord account later and everything comes back.")
                color: "#66FFFFFF"
                font.family: DesktopTokens.bodyFont
                font.pixelSize: 13
                lineHeightMode: Text.FixedHeight
                lineHeight: 18
                wrapMode: Text.WordWrap
            }

            Item {
                width: parent.width
                height: 42
                Row {
                    anchors.right: parent.right
                    anchors.bottom: parent.bottom
                    spacing: 10
                    Button {
                        id: keepButton
                        objectName: "discordUnlinkKeep"
                        height: 38
                        leftPadding: 16
                        rightPadding: 16
                        text: qsTr("Keep linked")
                        Accessible.role: Accessible.Button
                        Accessible.name: text
                        KeyNavigation.right: unlinkButton
                        Keys.onReturnPressed: clicked()
                        Keys.onEnterPressed: clicked()
                        onClicked: root.cancel()
                        background: Rectangle {
                            radius: 10
                            color: keepButton.hovered ? "#1FFFFFFF" : "#14FFFFFF"
                            border.width: keepButton.activeFocus ? 2 : 1
                            border.color: keepButton.activeFocus ? DesktopTokens.focus : "#14FFFFFF"
                        }
                        contentItem: Text {
                            text: keepButton.text
                            color: "#FFFFFF"
                            font.family: DesktopTokens.bodyFont
                            font.pixelSize: 14
                            font.weight: Font.ExtraBold
                            horizontalAlignment: Text.AlignHCenter
                            verticalAlignment: Text.AlignVCenter
                        }
                    }
                    Button {
                        id: unlinkButton
                        objectName: "discordUnlinkConfirmButton"
                        height: 38
                        leftPadding: 16
                        rightPadding: 16
                        text: qsTr("Unlink Discord")
                        Accessible.role: Accessible.Button
                        Accessible.name: text
                        KeyNavigation.left: keepButton
                        Keys.onReturnPressed: clicked()
                        Keys.onEnterPressed: clicked()
                        onClicked: root.confirm()
                        background: Rectangle {
                            radius: 10
                            color: unlinkButton.down ? "#E25F5F" : "#F87171"
                            border.width: unlinkButton.activeFocus ? 2 : 0
                            border.color: "#FFFFFF"
                        }
                        contentItem: Row {
                            spacing: 8
                            Image {
                                anchors.verticalCenter: parent.verticalCenter
                                width: 15
                                height: 15
                                source: "qrc:/qt/qml/OpenNOW/res/icons/desktop-unlink-ink.svg"
                                sourceSize: Qt.size(30, 30)
                            }
                            Text {
                                anchors.verticalCenter: parent.verticalCenter
                                text: unlinkButton.text
                                color: "#0B0F1A"
                                font.family: DesktopTokens.bodyFont
                                font.pixelSize: 14
                                font.weight: Font.ExtraBold
                            }
                        }
                    }
                }
            }
        }
    }
}

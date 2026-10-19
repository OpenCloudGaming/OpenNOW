import QtQuick
import OpenNOW

FocusScope {
    id: page
    objectName: "friendsLinking"
    anchors.fill: parent
    readonly property var friends: ShellStore.friends
    readonly property string failure: friends.linkErrorText()
    readonly property string pageUrl: String(friends.link && friends.link.loginUrl || "").replace(/^https?:\/\//, "")
    readonly property var account: ShellStore.authSession && ShellStore.authSession.user || null
    Accessible.role: Accessible.Pane
    Accessible.name: failure !== "" ? failure : qsTr("Finish signing in in your browser")
    onActiveFocusChanged: if (activeFocus) primaryAction.forceActiveFocus()
    Keys.onEscapePressed: friends.cancelLink()

    Column {
        anchors.horizontalCenter: parent.horizontalCenter
        anchors.verticalCenter: parent.verticalCenter
        anchors.verticalCenterOffset: -DesktopTokens.px(20)
        width: Math.min(parent.width - DesktopTokens.px(48), DesktopTokens.px(720))
        spacing: DesktopTokens.px(12)

        Row {
            anchors.horizontalCenter: parent.horizontalCenter
            spacing: DesktopTokens.px(10)
            Item {
                visible: page.failure === ""
                anchors.verticalCenter: parent.verticalCenter
                width: DesktopTokens.px(16); height: width
                Rectangle {
                    anchors.fill: parent
                    radius: width / 2
                    color: "transparent"
                    border.width: DesktopTokens.px(2)
                    border.color: DesktopTokens.seam
                }
                Canvas {
                    id: arc
                    anchors.fill: parent
                    onPaint: {
                        const context = getContext("2d")
                        context.reset()
                        context.lineWidth = DesktopTokens.px(2)
                        context.strokeStyle = DesktopFriendsFormat.accent
                        context.beginPath()
                        context.arc(width / 2, height / 2, width / 2 - context.lineWidth / 2, -Math.PI * 0.75, Math.PI * 0.25)
                        context.stroke()
                    }
                    RotationAnimator on rotation {
                        from: 0; to: 360; duration: 1000
                        loops: Animation.Infinite
                        running: !AppController.reducedMotion && page.visible && page.failure === ""
                    }
                }
            }
            DesktopFriendsLabel {
                anchors.verticalCenter: parent.verticalCenter
                text: page.failure === "" ? qsTr("Waiting for Discord") : qsTr("Discord sign-in stopped")
                mono: true; size: 12; line: 16; tracking: 0.14
                color: page.failure === "" ? DesktopFriendsFormat.accent : DesktopFriendsFormat.amber
                font.weight: Font.DemiBold
                font.capitalization: Font.AllUppercase
            }
        }
        DesktopFriendsLabel {
            width: parent.width
            text: page.failure === "" ? qsTr("Finish signing in in your browser") : page.failure
            horizontalAlignment: Text.AlignHCenter
            wrapMode: Text.WordWrap
            size: 30; line: 38
            font.family: DesktopTokens.displayFont
            font.weight: Font.Black
        }
        DesktopFriendsLabel {
            width: parent.width
            text: qsTr("Discord only shares your username, display name and avatar with OpenNOW.")
            horizontalAlignment: Text.AlignHCenter
            wrapMode: Text.WordWrap
            bottomPadding: DesktopTokens.px(10)
            size: 16; line: 22
            color: DesktopTokens.textMuted
        }
        Rectangle {
            id: preview
            anchors.horizontalCenter: parent.horizontalCenter
            width: Math.min(parent.width, DesktopTokens.px(680))
            height: chrome.height + consent.height
            radius: DesktopTokens.px(14)
            color: "#1E1F22"
            border.width: 1; border.color: DesktopTokens.seam
            clip: true
            Accessible.role: Accessible.Grouping
            Accessible.name: qsTr("Discord asks to share your username, avatar and banner with OpenNOW")
            Rectangle {
                id: chrome
                x: 1; y: 1
                width: parent.width - 2; height: DesktopTokens.px(40)
                topLeftRadius: DesktopTokens.px(13); topRightRadius: DesktopTokens.px(13)
                color: "#2A2C31"
                Row {
                    x: DesktopTokens.px(14)
                    anchors.verticalCenter: parent.verticalCenter
                    spacing: DesktopTokens.px(10)
                    Repeater {
                        model: ["#FF5F57", "#FEBC2E", "#28C840"]
                        delegate: Rectangle {
                            required property string modelData
                            anchors.verticalCenter: parent.verticalCenter
                            width: DesktopTokens.px(10); height: width; radius: width / 2
                            color: modelData
                        }
                    }
                    Rectangle {
                        anchors.verticalCenter: parent.verticalCenter
                        width: chrome.width - DesktopTokens.px(14 * 2 + 3 * 10 + 30)
                        height: DesktopTokens.px(24)
                        radius: DesktopTokens.px(6)
                        color: "#1E1F22"
                        Row {
                            x: DesktopTokens.px(10)
                            anchors.verticalCenter: parent.verticalCenter
                            spacing: DesktopTokens.px(6)
                            DesktopFriendsIcon {
                                anchors.verticalCenter: parent.verticalCenter
                                width: DesktopTokens.px(11); height: width
                                glyph: "lock"; ink: "#949BA4"
                            }
                            DesktopFriendsLabel {
                                width: chrome.width - DesktopTokens.px(150)
                                text: page.pageUrl !== "" ? page.pageUrl : "discord.com/oauth2/authorize"
                                elide: Text.ElideRight
                                mono: true; size: 11; line: 14
                                color: "#949BA4"
                            }
                        }
                    }
                }
            }
            Rectangle {
                id: consent
                y: chrome.height + 1
                x: 1
                width: parent.width - 2; height: consentColumn.height + DesktopTokens.px(26 + 24)
                bottomLeftRadius: DesktopTokens.px(13); bottomRightRadius: DesktopTokens.px(13)
                color: "#313338"
                Column {
                    id: consentColumn
                    x: DesktopTokens.px(30); y: DesktopTokens.px(26)
                    width: parent.width - 2 * x
                    spacing: DesktopTokens.px(14)
                    Row {
                        anchors.horizontalCenter: parent.horizontalCenter
                        spacing: DesktopTokens.px(14)
                        Rectangle {
                            width: DesktopTokens.px(52); height: width; radius: width / 2
                            color: "#0B0F1A"
                            border.width: 1; border.color: DesktopTokens.seam
                            Image {
                                anchors.centerIn: parent
                                width: DesktopTokens.px(32); height: DesktopTokens.px(19)
                                source: "qrc:/qt/qml/OpenNOW/res/brand/opennow-mark.png"
                                fillMode: Image.PreserveAspectFit
                                mipmap: true
                            }
                        }
                        Row {
                            anchors.verticalCenter: parent.verticalCenter
                            spacing: DesktopTokens.px(4)
                            Repeater {
                                model: 3
                                delegate: Rectangle {
                                    width: DesktopTokens.px(5); height: width; radius: width / 2
                                    color: "#949BA4"
                                }
                            }
                        }
                        Rectangle {
                            width: DesktopTokens.px(52); height: width; radius: width / 2
                            color: DesktopFriendsFormat.accent
                            DesktopFriendsLabel {
                                anchors.centerIn: parent
                                text: page.account ? String(page.account.displayName || "?").charAt(0).toUpperCase() : "?"
                                size: 22; line: 28
                                color: DesktopFriendsFormat.avatarInk
                                font.weight: Font.Black
                            }
                            DesktopAvatarPhoto {
                                anchors.fill: parent
                                url: page.account ? String(page.account.avatarUrl || "") : ""
                            }
                        }
                    }
                    DesktopFriendsLabel {
                        anchors.horizontalCenter: parent.horizontalCenter
                        text: qsTr("OpenNOW")
                        font.capitalization: Font.AllUppercase
                        size: 12; line: 16; tracking: 0.06
                        color: "#949BA4"
                        font.weight: Font.ExtraBold
                    }
                    DesktopFriendsLabel {
                        anchors.horizontalCenter: parent.horizontalCenter
                        text: qsTr("wants to access your Discord account")
                        size: 18; line: 22
                        color: "#F2F3F5"
                        font.weight: Font.ExtraBold
                    }
                    Rectangle { width: parent.width; height: 1; color: "#3F4147" }
                    DesktopFriendsLabel {
                        text: qsTr("This will allow the developer of OpenNOW to:")
                        size: 11; line: 14
                        color: "#949BA4"
                        font.weight: Font.ExtraBold
                        font.capitalization: Font.AllUppercase
                    }
                    Item {
                        width: parent.width; height: DesktopTokens.px(22)
                        Rectangle {
                            id: allowed
                            width: DesktopTokens.px(22); height: width; radius: width / 2
                            color: "#23A55A"
                            DesktopFriendsIcon {
                                anchors.centerIn: parent
                                width: DesktopTokens.px(13); height: width
                                glyph: "check"; ink: "#FFFFFF"
                            }
                        }
                        DesktopFriendsLabel {
                            x: allowed.width + DesktopTokens.px(10)
                            anchors.verticalCenter: parent.verticalCenter
                            text: qsTr("Access your username, avatar, and banner")
                            size: 14; line: 18
                            color: "#DBDEE1"
                            font.weight: Font.DemiBold
                        }
                        DesktopFriendsTag {
                            anchors.right: parent.right
                            anchors.verticalCenter: parent.verticalCenter
                            text: qsTr("Scope · identify")
                            tone: "#949BA4"
                        }
                    }
                    Rectangle { width: parent.width; height: 1; color: "#3F4147" }
                    Row {
                        anchors.right: parent.right
                        spacing: DesktopTokens.px(10)
                        Item {
                            width: cancelPreview.implicitWidth + DesktopTokens.px(36); height: DesktopTokens.px(38)
                            DesktopFriendsLabel {
                                id: cancelPreview
                                anchors.centerIn: parent
                                text: qsTr("Cancel")
                                size: 14; line: 18
                                color: "#F2F3F5"
                                font.weight: Font.Bold
                            }
                        }
                        Rectangle {
                            width: authorizePreview.implicitWidth + DesktopTokens.px(44); height: DesktopTokens.px(38)
                            radius: DesktopTokens.px(8)
                            color: "#5865F2"
                            DesktopFriendsLabel {
                                id: authorizePreview
                                anchors.centerIn: parent
                                text: qsTr("Authorize")
                                size: 14; line: 18
                                color: "#FFFFFF"
                                font.weight: Font.ExtraBold
                            }
                        }
                    }
                }
            }
        }
        Row {
            anchors.horizontalCenter: parent.horizontalCenter
            topPadding: DesktopTokens.px(8)
            spacing: DesktopTokens.px(10)
            DesktopFriendsButton {
                id: primaryAction
                objectName: page.failure === "" ? "friendsReopenLink" : "friendsRetryLink"
                text: page.failure === "" ? qsTr("Open the page again") : qsTr("Try again")
                glyph: page.failure === "" ? "external" : ""
                glyphSize: 16
                controlHeight: 38; cornerRadius: 10; sidePadding: 16; fontSize: 14
                enabled: page.failure !== "" || page.pageUrl !== ""
                onClicked: page.failure === "" ? page.friends.reopenLinkPage() : page.friends.startLink()
            }
            DesktopFriendsButton {
                objectName: "friendsStopLinking"
                text: page.failure === "" ? qsTr("Stop waiting") : qsTr("Not now")
                kind: "outline"
                controlHeight: 38; cornerRadius: 10; sidePadding: 16; fontSize: 14
                onClicked: page.friends.cancelLink()
            }
        }
    }
}

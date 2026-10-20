import QtQuick
import QtQuick.Controls
import QtQuick.Layouts
import OpenNOW

Column {
    id: page
    objectName: "desktopAccountSettings"
    required property real availableWidth
    required property var settingsScreen
    required property Component profilePageComponent
    required property Component subscriptionPageComponent
    required property Component storesPageComponent

    readonly property var friends: ShellStore.friends
    readonly property string iconBase: "qrc:/qt/qml/OpenNOW/res/icons/"

    component AudienceChoice: Rectangle {
        id: choice
        property string value: "friends"
        property bool interactive: true
        signal chosen(string value)
        implicitWidth: options.implicitWidth + DesktopTokens.px(10)
        implicitHeight: DesktopTokens.px(42)
        radius: DesktopTokens.px(12)
        color: DesktopTokens.raised
        border.width: 1
        border.color: DesktopTokens.seamSoft
        opacity: interactive ? 1 : 0.5
        Row {
            id: options
            x: DesktopTokens.px(5); y: DesktopTokens.px(5)
            spacing: DesktopTokens.px(4)
            Repeater {
                model: [{label: qsTr("Friends"), value: "friends"}, {label: qsTr("No one"), value: "nobody"}]
                delegate: AbstractButton {
                    id: option
                    required property var modelData
                    readonly property bool on: choice.value === modelData.value
                    enabled: choice.interactive
                    width: optionLabel.implicitWidth + DesktopTokens.px(28)
                    height: DesktopTokens.px(32)
                    hoverEnabled: true
                    Accessible.role: Accessible.RadioButton
                    Accessible.name: modelData.label
                    Accessible.checked: on
                    onClicked: if (!on) choice.chosen(modelData.value)
                    background: Rectangle {
                        radius: DesktopTokens.px(9)
                        color: option.on ? Theme.focus : option.hovered ? DesktopTokens.raised : "transparent"
                        border.width: option.activeFocus ? 2 : 0
                        border.color: Theme.focus
                    }
                    contentItem: Text {
                        id: optionLabel
                        text: option.modelData.label
                        color: option.on ? Theme.focusText : Theme.textMuted
                        font.family: Theme.bodyFont
                        font.pixelSize: DesktopTokens.px(14)
                        font.weight: Font.ExtraBold
                        horizontalAlignment: Text.AlignHCenter
                        verticalAlignment: Text.AlignVCenter
                    }
                }
            }
        }
    }

    width: page.availableWidth; spacing: DesktopTokens.px(12)
    Loader { width: parent.width; sourceComponent: page.profilePageComponent }
    DesktopSettingsPanel {
        objectName: "settingsDiscord"
        width: parent.width; paperStyle: true
        DesktopSettingsSection {
            text: qsTr("DISCORD")
            Image {
                anchors.verticalCenter: parent.verticalCenter
                width: DesktopTokens.px(79); height: DesktopTokens.px(12)
                source: page.iconBase + (Theme.lightMode ? "desktop-discord-wordmark-on-light.svg" : "desktop-discord-wordmark.svg")
                sourceSize: Qt.size(width * 2, height * 2)
                Accessible.ignored: true
            }
            Rectangle {
                visible: page.friends.linked
                anchors.verticalCenter: parent.verticalCenter
                width: linkedLabel.implicitWidth + DesktopTokens.px(18); height: DesktopTokens.px(22)
                radius: height / 2
                color: "#1F4ADE80"; border.width: 1; border.color: "#554ADE80"
                Text {
                    id: linkedLabel
                    anchors.centerIn: parent
                    text: qsTr("LINKED")
                    color: Theme.lightMode ? "#15803D" : "#4ADE80"
                    font.family: Theme.monoFont; font.pixelSize: DesktopTokens.px(10); font.weight: Font.Bold; font.letterSpacing: 0.8
                }
            }
        }
        Item {
            visible: page.friends.linked
            width: parent.width; height: linkedRow.implicitHeight
            DesktopSettingsRow {
                id: linkedRow
                width: parent.width; paperStyle: true
                title: page.friends.user ? qsTr("%1 · @%2").arg(page.friends.displayName(page.friends.user)).arg(page.friends.user.username) : ""
                description: qsTr("Linked to your OpenNOW friends. OpenNOW only sees your username, display name and avatar.")
                Button {
                    id: unlinkButton
                    objectName: "settingsDiscordUnlink"
                    height: DesktopTokens.controlHeight
                    leftPadding: DesktopTokens.px(16); rightPadding: DesktopTokens.px(16)
                    text: qsTr("Unlink")
                    hoverEnabled: true
                    Accessible.role: Accessible.Button
                    Accessible.name: qsTr("Unlink Discord")
                    onClicked: AppController.showOverlay("discord-unlink-confirm")
                    background: Rectangle {
                        radius: DesktopTokens.px(10)
                        color: unlinkButton.down ? "#24F87171" : unlinkButton.hovered ? "#18F87171" : "#0FF87171"
                        border.width: unlinkButton.activeFocus ? 2 : 1
                        border.color: unlinkButton.activeFocus ? Theme.focus : "#66F87171"
                    }
                    contentItem: Row {
                        spacing: DesktopTokens.px(8)
                        Image {
                            anchors.verticalCenter: parent.verticalCenter
                            width: DesktopTokens.px(16); height: width
                            source: page.iconBase + "desktop-unlink.svg"
                            sourceSize: Qt.size(width * 2, height * 2)
                        }
                        Text {
                            anchors.verticalCenter: parent.verticalCenter
                            text: unlinkButton.text
                            color: Theme.lightMode ? "#B91C1C" : "#F87171"
                            font.family: Theme.bodyFont; font.pixelSize: DesktopTokens.px(14); font.weight: Font.ExtraBold
                        }
                    }
                }
            }
            Item {
                x: DesktopTokens.settingsInset
                y: DesktopTokens.px(12)
                width: DesktopTokens.px(36); height: width
                DesktopCoopNoticeAvatar { width: parent.width; user: page.friends.user }
                Rectangle {
                    x: DesktopTokens.px(22); y: DesktopTokens.px(22)
                    width: DesktopTokens.px(18); height: width; radius: width / 2
                    color: "#5865F2"; border.width: 2; border.color: Theme.shell
                    Image {
                        anchors.centerIn: parent
                        width: DesktopTokens.px(11); height: DesktopTokens.px(8)
                        source: page.iconBase + "desktop-discord.svg"
                        sourceSize: Qt.size(22, 16)
                    }
                }
            }
        }
        DesktopSettingsRow {
            visible: page.friends.linked
            objectName: "settingsDiscordBotDm"
            width: parent.width; paperStyle: true; showDivider: false
            leadingIcon: page.iconBase + "desktop-discord.svg"; leadingColor: "#5865F2"; leadingIconWidth: DesktopTokens.px(18)
            title: qsTr("Let OpenNOW bot DM me invites")
            description: qsTr("When OpenNOW is closed, a friend’s invite arrives as a Discord DM with a Join link.")
            DesktopSettingsToggle {
                enabled: page.friends.settingsLoaded
                checked: page.friends.settings.botDmInvites
                Accessible.name: qsTr("Let OpenNOW bot DM me invites")
                onValueChangedByUser: value => page.friends.updateSettings({botDmInvites: value})
            }
        }
        DesktopSettingsRow {
            visible: !page.friends.linked
            objectName: "settingsDiscordLink"
            width: parent.width; paperStyle: true; showDivider: false
            leadingIcon: page.iconBase + "desktop-discord.svg"; leadingColor: "#5865F2"; leadingIconWidth: DesktopTokens.px(18)
            title: qsTr("Link Discord")
            description: page.friends.linking ? qsTr("Finish signing in to Discord in your browser.")
                : page.friends.linkErrorText() || qsTr("Add friends, see what they play and invite them to co-op. OpenNOW only sees your username, display name and avatar.")
            DesktopSettingsButton {
                text: page.friends.linking ? qsTr("Waiting…") : qsTr("Link Discord")
                primary: !page.friends.linking
                enabled: !page.friends.linking
                onClicked: {
                    page.friends.startLink()
                    AppController.navigate("friends")
                }
            }
        }
    }
    DesktopSettingsPanel {
        objectName: "settingsFriendsCoop"
        width: parent.width; paperStyle: true
        DesktopSettingsSection { text: qsTr("FRIENDS & CO-OP") }
        DesktopSettingsRow {
            visible: page.friends.linked && !page.friends.settingsLoaded
            objectName: "settingsFriendsLoading"
            width: parent.width; paperStyle: true; glyph: "clock"
            title: qsTr("Loading your Friends settings…")
            description: qsTr("They come from your OpenNOW friends account.")
        }
        DesktopSettingsRow {
            visible: page.friends.settingsLoaded
            width: parent.width; paperStyle: true
            leadingIcon: page.iconBase + (Theme.lightMode ? "desktop-eye-on-light.svg" : "desktop-eye.svg"); leadingIconWidth: DesktopTokens.px(18)
            title: qsTr("Who can see you’re in a co-op lobby")
            description: qsTr("Your row in their friends list shows the game and how many players.")
            AudienceChoice {
                objectName: "settingsPresenceVisibility"
                value: page.friends.settings.presenceVisibility
                onChosen: value => page.friends.updateSettings({presenceVisibility: value})
            }
        }
        DesktopSettingsRow {
            visible: page.friends.settingsLoaded
            width: parent.width; paperStyle: true
            leadingIcon: page.iconBase + (Theme.lightMode ? "desktop-hand-on-light.svg" : "desktop-hand.svg"); leadingIconWidth: DesktopTokens.px(18)
            title: qsTr("Who can ask to join")
            description: qsTr("Asks show up as a small toast while you play. You let them in or not.")
            AudienceChoice {
                objectName: "settingsJoinRequests"
                value: page.friends.settings.joinRequests
                onChosen: value => page.friends.updateSettings({joinRequests: value})
            }
        }
        DesktopSettingsRow {
            visible: page.friends.settingsLoaded
            width: parent.width; paperStyle: true; glyph: "controller"
            title: qsTr("Show what I’m playing")
            description: qsTr("On: friends see your game, lobby, queue or what you’re browsing. Off: they only see “Online”.")
            DesktopSettingsToggle {
                checked: page.friends.settings.shareActivity
                Accessible.name: qsTr("Show what I’m playing")
                onValueChangedByUser: value => page.friends.updateSettings({shareActivity: value})
            }
        }
        DesktopSettingsRow {
            objectName: "accountActivitySharing"
            width: parent.width; paperStyle: true; showDivider: false
            leadingIcon: page.iconBase + "desktop-discord.svg"; leadingColor: "#5865F2"; leadingIconWidth: DesktopTokens.px(18)
            title: qsTr("Also show it on my Discord profile")
            description: qsTr("Discord activity sharing. Separate from what OpenNOW friends see.")
            DesktopSettingsToggle {
                checked: page.settingsScreen.boolSetting("discordRichPresence", false)
                Accessible.name: qsTr("Also show it on my Discord profile")
                onValueChangedByUser: value => page.settingsScreen.setSetting("discordRichPresence", value)
            }
        }
    }
    Loader { width: parent.width; sourceComponent: page.subscriptionPageComponent }
    DesktopSettingsPanel {
        width: parent.width; paperStyle: true
        DesktopSettingsRow {
            width: parent.width; paperStyle: true; glyph: "person"; title: qsTr("Profiles")
            description: ShellStore.savedAccounts.length === 1 ? qsTr("1 account on this PC")
                : qsTr("%1 accounts on this PC").arg(ShellStore.savedAccounts.length)
            DesktopSettingsButton { objectName: "accountSettingsManage"; text: qsTr("Manage"); onClicked: AppController.navigate("accounts") }
        }
        DesktopSettingsRow {
            objectName: "accountStartAsLastUsed"
            width: parent.width; paperStyle: true; glyph: "person"; title: qsTr("Skip “Who’s playing?”")
            description: qsTr("Skip this screen and start as the last-used account"); showDivider: false
            DesktopSettingsToggle { checked: page.settingsScreen.boolSetting("startAsLastUsedAccount",false); onValueChangedByUser: value => page.settingsScreen.setSetting("startAsLastUsedAccount",value) }
        }
    }
    Loader { width: parent.width; sourceComponent: page.storesPageComponent }
    DesktopSettingsPanel {
        width: parent.width; paperStyle: true
        DesktopSettingsSection { text: qsTr("PRIVACY") }
        DesktopSettingsRow { objectName: "accountAutomaticBugReports"; width: parent.width; paperStyle: true; glyph: "info"; title: qsTr("Usage & bug reports · Experimental"); description: qsTr("Send usage statistics and error reports with logs to the developer, identified by your GeForce NOW username or e-mail"); showDivider: false
            DesktopSettingsToggle { checked: ShellStore.bugReports.enabled; onValueChangedByUser: value => ShellStore.bugReports.setEnabled(value, "settings") }
        }
    }
    DesktopSettingsPanel {
        visible: ShellStore.signedIn
        width: parent.width; paperStyle: true
        DesktopSettingsRow {
            width: parent.width; paperStyle: true; glyph: "person"; title: qsTr("Sign out"); showDivider: false
            DesktopSettingsButton { text: qsTr("Sign out"); danger: true; onClicked: ShellStore.logout() }
        }
    }
}

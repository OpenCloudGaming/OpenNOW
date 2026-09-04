import QtQuick
import QtQuick.Controls
import OpenNOW

FocusScope {
    id: root
    property bool confirmLogoutAll: false
    readonly property string activeUserId: ShellStore.authSession ? ShellStore.authSession.user.userId : ""
    readonly property var selectedAccount: accountList.currentIndex >= 0 && accountList.currentIndex < ShellStore.savedAccounts.length
        ? ShellStore.savedAccounts[accountList.currentIndex] : null

    ScreenBackground { tint: "#171E35" }

    GlassPanel {
        x: 96; y: 128; width: root.width * 0.52; height: root.height - 264; panelRadius: 40
        Column {
            anchors.fill: parent; anchors.margins: 34; spacing: 14
            Text { text: qsTr("Who’s playing?"); color: Theme.label; font.family: Theme.displayFont; font.pixelSize: 38; font.weight: Font.Black }
            Text { text: qsTr("Saved NVIDIA sessions stay protected by your operating system."); color: Theme.textMuted; font.family: Theme.bodyFont; font.pixelSize: 16 }
            ListView {
                id: accountList
                width: parent.width; height: parent.height - 110; spacing: 10; clip: true
                model: ShellStore.savedAccounts
                Component.onCompleted: currentIndex = model.length ? Math.min(ShellStore.focusIndex("accounts"), model.length - 1) : -1
                onCountChanged: if (count > 0 && currentIndex < 0) currentIndex = Math.min(ShellStore.focusIndex("accounts"), count - 1)
                onCurrentIndexChanged: if (currentIndex >= 0) ShellStore.rememberFocus("accounts", currentIndex)
                focus: true
                KeyNavigation.right: switchButton
                delegate: ItemDelegate {
                    required property var modelData
                    required property int index
                    width: accountList.width; height: 92; focusPolicy: Qt.StrongFocus
                    highlighted: ListView.isCurrentItem
                    onClicked: accountList.currentIndex = index
                    background: Rectangle {
                        radius: 26
                        color: parent.highlighted ? Theme.glassStrong : Theme.glass
                        border.color: parent.highlighted ? Theme.focus : Theme.seam
                        border.width: parent.highlighted ? 3 : 1
                    }
                    contentItem: Row {
                        spacing: 16
                        Rectangle {
                            width: 58; height: 58; radius: 20; color: modelData.userId === root.activeUserId ? Theme.mint : Theme.violet
                            Text { anchors.centerIn: parent; text: String(modelData.displayName || "P").slice(0, 1).toUpperCase(); color: Theme.contrastText(modelData.userId === root.activeUserId ? Theme.mint : Theme.violet); font.family: Theme.displayFont; font.pixelSize: 24; font.weight: Font.Black }
                        }
                        Column {
                            anchors.verticalCenter: parent.verticalCenter; width: parent.width - 190; spacing: 3
                            Text { text: modelData.displayName || qsTr("NVIDIA profile"); color: Theme.label; font.family: Theme.bodyFont; font.pixelSize: 19; font.weight: Font.Bold }
                            Text { text: modelData.email || modelData.providerCode || "GeForce NOW"; color: Theme.textMuted; font.family: Theme.bodyFont; font.pixelSize: 14 }
                        }
                        GlassPanel {
                            anchors.verticalCenter: parent.verticalCenter; width: 92; height: 36; panelRadius: 18; strong: true
                            Text { anchors.centerIn: parent; text: modelData.hasPin ? qsTr("PIN locked") : (modelData.userId === root.activeUserId ? qsTr("Active") : qsTr("Saved")); color: modelData.userId === root.activeUserId ? Theme.mint : Theme.label; font.family: Theme.bodyFont; font.pixelSize: 12; font.weight: Font.Black }
                        }
                    }
                }
            }
        }
    }

    GlassPanel {
        x: root.width * 0.65; y: 180; width: root.width * 0.27; height: 510; panelRadius: 36; strong: true
        Column {
            anchors.fill: parent; anchors.margins: 28; spacing: 13
            Text { text: root.selectedAccount ? root.selectedAccount.displayName : qsTr("Add a profile"); color: Theme.label; font.family: Theme.displayFont; font.pixelSize: 28; font.weight: Font.Black }
            Text { width: parent.width; wrapMode: Text.WordWrap; text: root.selectedAccount && root.selectedAccount.hasPin ? qsTr("A four-digit living-room PIN is required before this account can become active.") : qsTr("Profile PINs are local to this device and never sent to NVIDIA."); color: Theme.textMuted; font.family: Theme.bodyFont; font.pixelSize: 15; lineHeight: 1.2 }
            GlassButton {
                id: switchButton; width: parent.width; glyph: "A"; primary: true
                text: !root.selectedAccount ? qsTr("Add NVIDIA account") : (root.selectedAccount.userId === root.activeUserId ? qsTr("Currently active") : qsTr("Switch profile"))
                enabled: !root.selectedAccount || root.selectedAccount.userId !== root.activeUserId
                onClicked: {
                    if (!root.selectedAccount)
                        AppController.navigate("sign-in")
                    else if (root.selectedAccount.hasPin)
                        ShellStore.openPin("unlock", root.selectedAccount)
                    else
                        ShellStore.switchAccount(root.selectedAccount.userId, "")
                }
                Component.onCompleted: forceActiveFocus()
            }
            GlassButton {
                width: parent.width; glyph: "●"
                text: root.selectedAccount && root.selectedAccount.hasPin ? qsTr("Remove profile PIN") : qsTr("Set profile PIN")
                enabled: root.selectedAccount !== null
                onClicked: ShellStore.openPin(root.selectedAccount.hasPin ? "clear" : "set", root.selectedAccount)
            }
            GlassButton {
                width: parent.width; glyph: "+"; text: qsTr("Add another account")
                onClicked: AppController.navigate("sign-in")
            }
            GlassButton {
                width: parent.width; glyph: "×"; text: qsTr("Forget this profile")
                enabled: root.selectedAccount !== null
                onClicked: ShellStore.removeAccount(root.selectedAccount.userId)
            }
            GlassButton {
                width: parent.width; glyph: "!"; text: qsTr("Sign out all profiles"); danger: true
                enabled: ShellStore.savedAccounts.length > 0
                onClicked: root.confirmLogoutAll = true
            }
            GlassButton { width: parent.width; glyph: "B"; text: qsTr("Back to account settings"); onClicked: AppController.navigate("settings-account") }
        }
    }

    Rectangle {
        visible: root.confirmLogoutAll; anchors.fill: parent; color: Qt.rgba(0, 0, 0, 0.34); z: 19
        MouseArea { anchors.fill: parent; onClicked: root.confirmLogoutAll = false }
    }
    GlassPanel {
        visible: root.confirmLogoutAll; anchors.centerIn: parent; width: 610; height: 250
        z: 20; panelRadius: 34; strong: true; focus: visible
        onVisibleChanged: if (visible) Qt.callLater(cancelLogoutAll.forceActiveFocus)
        Keys.onEscapePressed: root.confirmLogoutAll = false
        Column {
            anchors.fill: parent; anchors.margins: 28; spacing: 15
            Text { text: qsTr("Sign out every saved profile?"); color: Theme.label; font.family: Theme.displayFont; font.pixelSize: 27; font.weight: Font.Black }
            Text { width: parent.width; wrapMode: Text.WordWrap; text: qsTr("This removes all saved NVIDIA sessions and every local profile PIN. Captures and settings stay on this device."); color: Theme.textMuted; font.family: Theme.bodyFont; font.pixelSize: 15 }
            Row {
                spacing: 12
                GlassButton { id: cancelLogoutAll; width: 260; text: qsTr("Keep profiles"); primary: true; onClicked: root.confirmLogoutAll = false }
                GlassButton { width: 260; text: qsTr("Sign out all"); danger: true; onClicked: { root.confirmLogoutAll = false; ShellStore.logoutAll() } }
            }
        }
    }

    AppChrome { anchors.fill: parent; title: qsTr("Saved accounts"); currentRoute: "settings"; onRouteRequested: route => AppController.navigate(route) }
}

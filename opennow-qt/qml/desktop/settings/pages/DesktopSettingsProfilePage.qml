import QtQuick
import QtQuick.Controls
import QtQuick.Layouts
import OpenNOW

DesktopSettingsPanel {
    id: profilePanel
    required property real availableWidth
    required property var settingsScreen

    width: profilePanel.availableWidth; paperStyle: true
    DesktopSettingsSection { text: qsTr("NVIDIA ACCOUNT") }
    DesktopSettingsRow {
        width: parent.width; paperStyle: true; title: profilePanel.settingsScreen.profileName(); description: profilePanel.settingsScreen.maskedEmail()
        leadingLetter: profilePanel.settingsScreen.profileInitial(); leadingColor: Theme.focus; rowHeight: DesktopTokens.px(76); showDivider: false
        DesktopSettingsButton { visible: !ShellStore.signedIn; text: qsTr("Sign in"); onClicked: AppController.navigate("sign-in") }
    }
}

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
        leadingLetter: profilePanel.settingsScreen.profileInitial(); leadingColor: Theme.focus; rowHeight: 76
        DesktopSettingsButton { text: ShellStore.signedIn ? qsTr("Sign out") : qsTr("Sign in"); onClicked: ShellStore.signedIn ? ShellStore.logout() : AppController.navigate("sign-in") }
    }
    DesktopSettingsRow {
        width: parent.width; paperStyle: true; glyph: "crown"; title: profilePanel.settingsScreen.liveTierBadge() || qsTr("Membership unavailable")
        description: profilePanel.settingsScreen.planChips().join(" · "); expandable: true; expanded: profilePanel.settingsScreen.advancedOpen
        onExpansionRequested: profilePanel.settingsScreen.advancedOpen = !profilePanel.settingsScreen.advancedOpen
        Text { visible: ShellStore.subscription && ShellStore.subscription.remainingHours !== undefined; text: qsTr("%1 h left").arg(ShellStore.subscription ? ShellStore.subscription.remainingHours : ""); color: Theme.label; font.family: Theme.monoFont; font.pixelSize: 12; font.weight: Font.Bold }
        DesktopSettingsButton { text: qsTr("Manage"); onClicked: Qt.openUrlExternally("https://www.nvidia.com/en-us/account/") }
    }
    DesktopSettingsRow { objectName: "accountActivitySharing"; width: parent.width; paperStyle: true; glyph: "person"; title: qsTr("Show what I am playing"); description: qsTr("Discord activity sharing")
        DesktopSettingsToggle { checked: profilePanel.settingsScreen.boolSetting("discordRichPresence",false); onValueChangedByUser: value => profilePanel.settingsScreen.setSetting("discordRichPresence",value) }
    }
    DesktopSettingsRow { objectName: "accountCrashReports"; width: parent.width; paperStyle: true; glyph: "info"; title: qsTr("Crash reports"); description: qsTr("Optional error reporting"); showDivider: false
        DesktopSettingsToggle { checked: ShellStore.settings.errorReportingConsent === "granted"; onValueChangedByUser: value => profilePanel.settingsScreen.setSetting("errorReportingConsent",value ? "granted" : "denied") }
    }
}

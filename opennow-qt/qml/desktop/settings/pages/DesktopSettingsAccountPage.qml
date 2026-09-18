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

    width: page.availableWidth; spacing: DesktopTokens.px(12)
    Loader { width: parent.width; sourceComponent: page.profilePageComponent }
    Loader { width: parent.width; sourceComponent: page.subscriptionPageComponent }
    DesktopSettingsPanel {
        width: parent.width; paperStyle: true
        DesktopSettingsRow {
            width: parent.width; paperStyle: true; glyph: "person"; title: qsTr("Profiles")
            description: qsTr("Manage saved account profiles"); showDivider: false
            DesktopSettingsButton { text: qsTr("Manage"); onClicked: AppController.navigate("accounts") }
        }
    }
    Loader { width: parent.width; sourceComponent: page.storesPageComponent }
    DesktopSettingsPanel {
        width: parent.width; paperStyle: true
        DesktopSettingsSection { text: qsTr("PRIVACY") }
        DesktopSettingsRow { objectName: "accountActivitySharing"; width: parent.width; paperStyle: true; glyph: "person"; title: qsTr("Show what I am playing"); description: qsTr("Discord activity sharing")
            DesktopSettingsToggle { checked: page.settingsScreen.boolSetting("discordRichPresence",false); onValueChangedByUser: value => page.settingsScreen.setSetting("discordRichPresence",value) }
        }
        DesktopSettingsRow { objectName: "accountCrashReports"; width: parent.width; paperStyle: true; glyph: "info"; title: qsTr("Crash reports"); description: qsTr("Optional error reporting"); showDivider: false
            DesktopSettingsToggle { checked: ShellStore.settings.errorReportingConsent === "granted"; onValueChangedByUser: value => page.settingsScreen.setSetting("errorReportingConsent",value ? "granted" : "denied") }
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

import QtQuick
import QtQuick.Controls
import QtQuick.Layouts
import OpenNOW

DesktopSettingsPanel {
    id: page
    required property real availableWidth
    required property var settingsScreen

    width: page.availableWidth; paperStyle: true
    DesktopSettingsSection { text: qsTr("SUBSCRIPTION") }
    DesktopSettingsRow {
        width: parent.width; paperStyle: true; glyph: "crown"
        title: page.settingsScreen.liveTierBadge() || qsTr("Membership unavailable")
        description: qsTr("Plans and billing are managed by NVIDIA, not OpenNOW")
        DesktopSettingsButton { text: qsTr("Manage on NVIDIA"); onClicked: AppController.openExternalUrl("https://www.nvidia.com/en-us/account/") }
    }
    DesktopSettingsRow {
        width: parent.width; paperStyle: true; glyph: "monitor"; title: qsTr("Entitlements reported by NVIDIA")
        description: page.settingsScreen.planChips().join(" · ")
        DesktopSettingsButton { text: qsTr("Refresh entitlements"); onClicked: ShellStore.refreshAccountServices() }
    }
    DesktopSettingsRow {
        width: parent.width; paperStyle: true; glyph: "person"; title: qsTr("Profiles")
        description: qsTr("Manage saved account profiles"); showDivider: false
        DesktopSettingsButton { text: qsTr("Manage"); onClicked: AppController.navigate("accounts") }
    }
}

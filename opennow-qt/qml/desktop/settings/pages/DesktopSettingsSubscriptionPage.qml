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
        description: page.settingsScreen.planChips().join(" · ")
        showDivider: false
        DesktopSettingsButton { text: qsTr("Refresh entitlements"); onClicked: ShellStore.refreshAccountServices() }
        DesktopSettingsButton { text: qsTr("Manage on NVIDIA"); onClicked: AppController.openExternalUrl("https://www.nvidia.com/en-us/account/") }
    }
}

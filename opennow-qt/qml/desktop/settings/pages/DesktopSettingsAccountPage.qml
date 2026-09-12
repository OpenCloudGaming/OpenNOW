import QtQuick
import QtQuick.Controls
import QtQuick.Layouts
import OpenNOW

Column {
    id: page
    required property real availableWidth
    required property var settingsScreen
    required property Component profilePageComponent
    required property Component subscriptionPageComponent
    required property Component storesPageComponent

    width: page.availableWidth; spacing: 20
    Loader { width: parent.width; sourceComponent: page.profilePageComponent }
    Loader { width: parent.width; sourceComponent: page.storesPageComponent }
    DesktopSettingsAdvanced { detail: qsTr("Subscription · Profiles"); expanded: page.settingsScreen.advancedOpen; onClicked: page.settingsScreen.advancedOpen = !page.settingsScreen.advancedOpen }
    DesktopSettingsDisclosure { objectName: "accountAdvancedDisclosure"; width: parent.width; expanded: page.settingsScreen.advancedOpen; sourceComponent: page.subscriptionPageComponent }
}

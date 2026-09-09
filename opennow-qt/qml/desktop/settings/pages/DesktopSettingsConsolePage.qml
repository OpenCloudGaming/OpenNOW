import QtQuick
import QtQuick.Controls
import QtQuick.Layouts
import OpenNOW

DesktopSettingsPanel {
    id: page
    required property real availableWidth
    required property var settingsScreen

    width: page.availableWidth; paperStyle: true
    DesktopSettingsSection { text: qsTr("CONSOLE MODE") }
    DesktopSettingsRow {
        width: parent.width; paperStyle: true; glyph: "controller"; title: qsTr("One app, two shells")
        description: qsTr("Same session, settings and themes · switching does not restart the stream")
        DesktopSettingsButton { text: ShellStore.consoleSurfaceRequestId === "" ? qsTr("Switch now") : qsTr("Switching…"); suffix: "F10"; primary: true; enabled: ShellStore.consoleSurfaceRequestId === ""; onClicked: ShellStore.requestConsoleSurface(true) }
    }
    DesktopSettingsRow {
        width: parent.width; paperStyle: true; glyph: "controller"; title: qsTr("Start in console mode")
        description: qsTr("Remember this choice for the next time OpenNOW launches")
        DesktopSettingsToggle { checked: page.settingsScreen.boolSetting("launchInConsoleMode",false); onValueChangedByUser: value => page.settingsScreen.setSetting("launchInConsoleMode",value) }
    }
    DesktopSettingsRow {
        width: parent.width; paperStyle: true; glyph: "controller"; title: qsTr("Enter console mode when a gamepad is the only input")
        description: qsTr("Ignored while a mouse has moved in the last 30 seconds")
        DesktopSettingsToggle { checked: page.settingsScreen.boolSetting("switchToConsoleOnPad",false); onValueChangedByUser: value => page.settingsScreen.setSetting("switchToConsoleOnPad",value) }
    }
    DesktopSettingsRow {
        width: parent.width; paperStyle: true; glyph: "mouse"; title: qsTr("Leave console mode on keyboard or mouse input")
        description: qsTr("Keeps your place in the grid when the shell swaps")
        DesktopSettingsToggle { checked: page.settingsScreen.boolSetting("leaveConsoleOnPointer",true); onValueChangedByUser: value => page.settingsScreen.setSetting("leaveConsoleOnPointer",value) }
    }
    DesktopSettingsRow {
        width: parent.width; paperStyle: true; glyph: "person"; title: qsTr("Controller profile picker")
        description: qsTr("Choose a saved profile when console mode starts"); showDivider: false
        DesktopSettingsToggle { checked: page.settingsScreen.boolSetting("consoleProfilePickerOnLaunch",true); onValueChangedByUser: value => page.settingsScreen.setSetting("consoleProfilePickerOnLaunch",value) }
    }
}

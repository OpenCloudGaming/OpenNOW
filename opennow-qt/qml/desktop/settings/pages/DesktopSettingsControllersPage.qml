import QtQuick
import QtQuick.Controls
import QtQuick.Layouts
import OpenNOW

DesktopSettingsPanel {
    id: page
    required property real availableWidth
    required property var settingsScreen

    width: page.availableWidth; paperStyle: true
    DesktopSettingsSection { text: qsTr("CONTROLLER BEHAVIOR") }
    DesktopSettingsRow {
        width: parent.width; paperStyle: true; glyph: "controller"; title: qsTr("Controller input")
        description: qsTr("%1 CONNECTED").arg(AppController.controllerCount)
        DesktopSettingsToggle { checked: page.settingsScreen.boolSetting("controllerMode",true); onValueChangedByUser: value => page.settingsScreen.setSetting("controllerMode",value) }
    }
    DesktopSettingsRow {
        width: parent.width; paperStyle: true; glyph: "controller"; title: qsTr("Switch to console mode when a pad wakes up")
        description: qsTr("Open the gamepad-first shell when a controller becomes active")
        DesktopSettingsToggle { checked: page.settingsScreen.boolSetting("switchToConsoleOnPad",false); onValueChangedByUser: value => page.settingsScreen.setSetting("switchToConsoleOnPad",value) }
    }
    DesktopSettingsRow {
        width: parent.width; paperStyle: true; glyph: "mouse"; title: qsTr("Return to desktop on pointer input")
        description: qsTr("Mouse movement leaves console mode after the current input hold"); showDivider: false
        DesktopSettingsToggle { checked: page.settingsScreen.boolSetting("leaveConsoleOnPointer",true); onValueChangedByUser: value => page.settingsScreen.setSetting("leaveConsoleOnPointer",value) }
    }
}

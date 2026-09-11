import QtQuick
import QtQuick.Controls
import QtQuick.Layouts
import OpenNOW

Column {
    id: controlsRoot
    objectName: "desktopControllerSettings"
    required property real availableWidth
    required property var settingsScreen
    required property Component controllersPageComponent
    required property Component shortcutsPageComponent

    function controllerGlyph(controller) {
        return controller && (controller.family === "playstation" || controller.family === "xbox")
            ? controller.family : "controller"
    }

    function batteryLabel(controller) {
        const percent = Number(controller.batteryPercent)
        const hasPercent = Number.isFinite(percent) && percent >= 0 && percent <= 100
        switch (controller.powerState) {
        case "charging": return hasPercent ? qsTr("Charging · %1%").arg(percent) : qsTr("Charging")
        case "charged": return qsTr("Fully charged")
        case "noBattery": return qsTr("Wired power")
        case "onBattery": return hasPercent ? qsTr("Battery %1%").arg(percent) : qsTr("On battery")
        default: return qsTr("Battery unavailable")
        }
    }

    property bool shortcutsOpen: controlsRoot.settingsScreen.selectedSection === 10
    width: controlsRoot.availableWidth; spacing: 20
    DesktopSettingsPanel {
        width: parent.width; paperStyle: true
        DesktopSettingsSection { text: qsTr("CONTROLLERS") }
        DesktopSettingsChoice {
            objectName: "controllerSourceChoice"
            width: parent.width
            title: qsTr("Controller input source")
            description: qsTr("Choose one device as Player 1 if a controller appears twice. Selection lasts until app restart; select again after reconnecting.")
            glyph: current ? current.glyph || "controller" : "controller"
            items: [{value: 0, label: qsTr("All controllers (multiplayer)")}].concat(
                ControllerInput.availableControllers.map(controller => ({
                    value: controller.instanceId,
                    label: qsTr("Device %1 · %2").arg(controller.slot).arg(controller.name),
                    glyph: controlsRoot.controllerGlyph(controller),
                    detail: controlsRoot.batteryLabel(controller)
                })))
            value: ControllerInput.inputControllerId
            valueLabel: current ? current.label : qsTr("Selected controller disconnected")
            onSelected: value => ControllerInput.inputControllerId = Number(value)
        }
        Repeater {
            model: ControllerInput.controllers
            delegate: DesktopSettingsRow {
                required property var modelData
                objectName: "controllerRow-" + modelData.instanceId
                width: parent.width; paperStyle: true
                glyph: controlsRoot.controllerGlyph(modelData); title: modelData.name
                description: qsTr("Player %1").arg(modelData.slot) + " · " + controlsRoot.batteryLabel(modelData)
                DesktopSettingsButton { text: "P" + modelData.slot; onClicked: AppController.navigate("joining") }
            }
        }
        DesktopSettingsRow {
            visible: ControllerInput.controllers.length === 0; width: parent.width; paperStyle: true; glyph: "controller"
            title: qsTr("No controllers connected"); description: qsTr("Connect a controller to assign a player")
            DesktopSettingsButton { text: qsTr("Controller order"); onClicked: AppController.navigate("joining") }
        }
        DesktopSettingsRow { width: parent.width; paperStyle: true; glyph: "globe"; title: qsTr("Gyroscope"); description: qsTr("Motion aiming on supported pads")
            DesktopSettingsToggle { checked: controlsRoot.settingsScreen.boolSetting("enableGyroscopeControls",false); onValueChangedByUser: value => controlsRoot.settingsScreen.setSetting("enableGyroscopeControls",value) }
        }
        DesktopSettingsRow {
            width: parent.width; paperStyle: true; glyph: "controller"; title: qsTr("Left stick dead zone")
            description: qsTr("Ignore stick drift during gameplay. Default: 24%. The remaining travel is rescaled to full range.")
            DesktopSettingsSlider {
                objectName: "controllerLeftStickDeadzoneSlider"
                from: 0; to: 50; stepSize: 1
                value: Number(controlsRoot.settingsScreen.valueSetting("controllerLeftStickDeadzone", 24))
                onCommitted: value => controlsRoot.settingsScreen.setSetting("controllerLeftStickDeadzone", value)
            }
        }
        DesktopSettingsRow {
            width: parent.width; paperStyle: true; glyph: "controller"; title: qsTr("Right stick dead zone")
            description: qsTr("Ignore stick drift during gameplay. Default: 27%. Set to 0% to leave dead zones to the game.")
            DesktopSettingsSlider {
                objectName: "controllerRightStickDeadzoneSlider"
                from: 0; to: 50; stepSize: 1
                value: Number(controlsRoot.settingsScreen.valueSetting("controllerRightStickDeadzone", 27))
                onCommitted: value => controlsRoot.settingsScreen.setSetting("controllerRightStickDeadzone", value)
            }
        }
        DesktopSettingsRow {
            width: parent.width; paperStyle: true; glyph: "controller"; title: qsTr("Controller vibration")
            description: qsTr("Scale game vibration on supported controllers. Set to 0% to disable."); showDivider: false
            DesktopSettingsSlider {
                objectName: "controllerVibrationIntensitySlider"
                from: 0; to: 100; stepSize: 1
                value: Number(controlsRoot.settingsScreen.valueSetting("controllerVibrationIntensity", 100))
                onCommitted: value => controlsRoot.settingsScreen.setSetting("controllerVibrationIntensity", value)
            }
        }
    }
    DesktopSettingsPanel {
        width: parent.width; paperStyle: true
        DesktopSettingsSection { text: qsTr("MOUSE & KEYBOARD") }
        DesktopSettingsRow {
            width: parent.width; paperStyle: true; glyph: "keyboard"; title: qsTr("Clipboard paste")
            description: qsTr("Paste local text into the stream with Ctrl+V (Command+V on macOS). Up to 64 KiB per paste. No automatic clipboard sync.")
            DesktopSettingsToggle {
                objectName: "clipboardPasteToggle"
                checked: controlsRoot.settingsScreen.boolSetting("clipboardPaste", false)
                onValueChangedByUser: value => controlsRoot.settingsScreen.setSetting("clipboardPaste", value)
            }
        }
        DesktopSettingsRow { width: parent.width; paperStyle: true; glyph: "mouse"; title: qsTr("Mouse capture"); description: qsTr("Follows the remote cursor · F8 toggles capture")
            DesktopSettingsSegmented { options: [qsTr("Automatic")]; optionWidth: 112; selectedIndex: 0 }
        }
        DesktopSettingsRow { width: parent.width; paperStyle: true; glyph: "arrows"; title: qsTr("Mouse sensitivity"); description: qsTr("Applied to native relative mouse input")
            DesktopSettingsSlider { trackWidth: 320; from: 0.1; to: 3; stepSize: 0.05; decimals: 2; suffix: "×"; value: Number(controlsRoot.settingsScreen.valueSetting("mouseSensitivity",1)); onCommitted: value => controlsRoot.settingsScreen.setSetting("mouseSensitivity",value) }
        }
        DesktopSettingsRow {
            width: parent.width; paperStyle: true; glyph: "keyboard"; title: qsTr("Shortcuts")
            objectName: "renewShortcutsDisclosure"
            description: qsTr("Local shortcuts are consumed before gameplay input"); showDivider: false; expandable: true
            expanded: controlsRoot.shortcutsOpen
            onExpansionRequested: controlsRoot.shortcutsOpen = !controlsRoot.shortcutsOpen
            Row { spacing: 10
                DesktopKeyHint { keyText: String(controlsRoot.settingsScreen.valueSetting("shortcutToggleStats","Ctrl+N")); label: qsTr("stats") }
                DesktopKeyHint { keyText: "Ctrl G"; label: qsTr("menu") }
                DesktopKeyHint { keyText: "F11"; label: qsTr("fullscreen") }
            }
        }
    }
    DesktopSettingsDisclosure { objectName: "renewInlineShortcuts"; width: parent.width; expanded: controlsRoot.shortcutsOpen; sourceComponent: controlsRoot.shortcutsPageComponent }
    DesktopSettingsDisclosure {
        width: parent.width; expanded: controlsRoot.shortcutsOpen
        sourceComponent: DesktopSettingsPanel {
            width: controlsRoot.availableWidth; paperStyle: true
            DesktopSettingsRow { width: parent.width; paperStyle: true; glyph: "keyboard"; title: qsTr("Statistics shortcut"); showDivider: false
                DesktopSettingsField { width: DesktopTokens.px(200); text: String(controlsRoot.settingsScreen.valueSetting("shortcutToggleStats","Ctrl+N")); Accessible.name: qsTr("Statistics shortcut"); onEditingFinished: controlsRoot.settingsScreen.setSetting("shortcutToggleStats",text) }
            }
        }
    }
    DesktopSettingsAdvanced { detail: qsTr("Controller behavior · Cursor"); expanded: controlsRoot.settingsScreen.advancedOpen; onClicked: controlsRoot.settingsScreen.advancedOpen = !controlsRoot.settingsScreen.advancedOpen }
    DesktopSettingsDisclosure { width: parent.width; expanded: controlsRoot.settingsScreen.advancedOpen; sourceComponent: controlsRoot.controllersPageComponent }
    DesktopSettingsDisclosure {
        width: parent.width; expanded: controlsRoot.settingsScreen.advancedOpen
        sourceComponent: DesktopSettingsPanel {
            width: controlsRoot.availableWidth; paperStyle: true
            DesktopSettingsSection { text: qsTr("KEYBOARD & CURSOR") }
            DesktopSettingsRow { width: parent.width; paperStyle: true; glyph: "mouse"; title: qsTr("Cursor overlay"); showDivider: false
                DesktopSettingsToggle { checked: controlsRoot.settingsScreen.boolSetting("nativeCursorOverlay",true); onValueChangedByUser: value => controlsRoot.settingsScreen.setSetting("nativeCursorOverlay",value) }
            }
        }
    }
}

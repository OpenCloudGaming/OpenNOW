import QtQuick
import OpenNOW

DesktopSettingsPanel {
    id: root
    objectName: "desktopFlightControlsPanel"
    paperStyle: true
    property var settingsScreen: null
    property var devices: ControllerInput.flightDevices
    property var supportedDevices: ControllerInput.supportedFlightDevices
    property bool forwarding: ControllerInput.forwardFlightControls
    property bool supportedExpanded: false
    readonly property var supportedGroups: {
        const groups = [
            {
                label: qsTr("THRUSTMASTER"),
                count: 0,
                devices: []
            },
            {
                label: qsTr("SAITEK"),
                count: 0,
                devices: []
            },
            {
                label: qsTr("LOGITECH G · RACING"),
                count: 0,
                devices: []
            }
        ];
        for (const device of supportedDevices) {
            const group = groups[String(device.vidPid).toUpperCase().startsWith("044F:") ? 0 : device.type === "wheel" ? 2 : 1];
            ++group.count;
            if (!group.devices.some(item => item.name === device.name))
                group.devices.push(device);
        }
        return groups;
    }

    function deviceType(type) {
        switch (type) {
        case "throttle":
            return qsTr("Throttle");
        case "pedals":
            return qsTr("Rudder pedals");
        case "wheel":
            return qsTr("Wheel");
        default:
            return qsTr("Flight stick");
        }
    }

    function deviceDescription(device) {
        if (device.status === "declined")
            return qsTr("%1 · The server declined this device. It stays idle and is not emulated as a controller.").arg(deviceType(device.type));
        if (device.status === "no-hid-path")
            return qsTr("%1 · The system exposes no hidraw device for it, so it can't be forwarded. Check its kernel driver, or turn forwarding off to use it as a controller.").arg(deviceType(device.type));
        if (device.status === "no-access")
            return qsTr("%1 · OpenNOW has no permission to open this device. Add a udev rule that grants access to its hidraw node.").arg(deviceType(device.type));
        if (device.status === "controller")
            return qsTr("Mapped as an Xbox controller. Some axes and buttons may not reach the game.");
        if (device.status === "ignored")
            return qsTr("%1 · SDL has no controller mapping for it, so the game doesn't receive it").arg(deviceType(device.type));
        return qsTr("%1 · USB %2").arg(deviceType(device.type)).arg(device.vidPid);
    }

    function statusText(device) {
        switch (device.status) {
        case "active":
            return qsTr("ACTIVE IN SESSION");
        case "declined":
            return qsTr("NOT ACCEPTED BY SERVER");
        case "no-access":
            return qsTr("NO PERMISSION");
        case "no-hid-path":
            return qsTr("NO HID PATH");
        case "controller":
            return qsTr("CONTROLLER · PLAYER %1").arg(device.player);
        case "ignored":
            return qsTr("IGNORED");
        default:
            return qsTr("READY");
        }
    }

    function statusColor(status) {
        if (status === "declined" || status === "no-access" || status === "no-hid-path")
            return DesktopTokens.flightDeclined;
        if (status === "controller")
            return Theme.label;
        if (status === "ignored")
            return Theme.textMuted;
        return DesktopTokens.flightReady;
    }

    DesktopSettingsSection {
        text: qsTr("FLIGHT CONTROLS & WHEELS")
        description: qsTr("Sticks, throttles, pedals and wheels on the supported list reach the game as the real device, not as an Xbox controller.")
    }
    DesktopSettingsRow {
        width: parent.width
        paperStyle: true
        glyph: "stick"
        title: qsTr("Forward flight sticks and wheels as native devices")
        description: qsTr("Games read every axis, button and hat switch. Supported devices skip controller emulation, so they never appear twice.")
        DesktopSettingsToggle {
            objectName: "forwardFlightControlsToggle"
            checked: root.forwarding
            onValueChangedByUser: value => {
                if (root.settingsScreen)
                    root.settingsScreen.setSetting("forwardFlightControls", value);
                else
                    ShellStore.setSetting("forwardFlightControls", value);
            }
        }
    }
    DesktopSettingsRow {
        width: parent.width
        visible: !root.forwarding
        paperStyle: true
        glyph: "controller"
        title: qsTr("These devices now use the regular controller path")
        description: qsTr("If SDL recognises a device as a gamepad, the game gets an Xbox controller with fewer axes and buttons. Anything SDL doesn't recognise is ignored and never reaches the game.")
    }
    DesktopSettingsSection {
        text: root.devices.length ? qsTr("CONNECTED · %1").arg(root.devices.length) : qsTr("CONNECTED")
        implicitHeight: DesktopTokens.px(36)
    }
    Repeater {
        model: root.devices
        delegate: DesktopSettingsRow {
            id: deviceRow
            required property var modelData
            width: root.width
            paperStyle: true
            glyph: modelData.type
            title: modelData.name
            description: root.deviceDescription(modelData)
            Row {
                width: DesktopTokens.px(176)
                spacing: DesktopTokens.px(6)
                Rectangle {
                    anchors.verticalCenter: parent.verticalCenter
                    width: DesktopTokens.px(6)
                    height: width
                    radius: width / 2
                    color: root.statusColor(deviceRow.modelData.status)
                }
                Text {
                    objectName: "flightStatus-" + deviceRow.modelData.deviceId
                    width: parent.width - DesktopTokens.px(12)
                    text: root.statusText(deviceRow.modelData)
                    color: root.statusColor(deviceRow.modelData.status)
                    font.family: Theme.monoFont
                    font.pixelSize: DesktopTokens.microSize
                    font.weight: Font.Bold
                    font.letterSpacing: 0.2
                    wrapMode: Text.WordWrap
                }
            }
        }
    }
    DesktopSettingsRow {
        width: parent.width
        visible: root.devices.length === 0
        paperStyle: true
        glyph: "stick"
        title: qsTr("No flight controls connected")
        description: qsTr("Plug in a supported stick, throttle, pedals or wheel. It appears here as soon as OpenNOW detects it.")
    }
    DesktopSettingsRow {
        width: parent.width
        visible: root.devices.length === 0 && root.forwarding
        paperStyle: true
        glyph: "info"
        leadingColor: Qt.rgba(DesktopTokens.amber.r, DesktopTokens.amber.g, DesktopTokens.amber.b, 0.14)
        leadingInk: DesktopTokens.amber
        title: qsTr("Using a T.Flight HOTAS 4?")
        description: qsTr("Unplug it, hold SHARE + OPTIONS and reconnect USB to enter HID mode each time you start a session.")
        Row {
            spacing: DesktopTokens.px(6)
            DesktopKeyHint {
                keyText: "SHARE"
                label: ""
                compact: true
            }
            Text {
                text: "+"
                color: Theme.textMuted
                font.pixelSize: DesktopTokens.monoSize
            }
            DesktopKeyHint {
                keyText: "OPTIONS"
                label: ""
                compact: true
            }
        }
    }
    DesktopSettingsRow {
        objectName: "supportedFlightDevicesDisclosure"
        width: parent.width
        paperStyle: true
        glyph: "info"
        title: qsTr("Supported devices")
        description: qsTr("18 product IDs across Thrustmaster, Logitech and Saitek sticks, throttles, pedals and wheels")
        expandable: true
        expanded: root.supportedExpanded
        showDivider: root.supportedExpanded
        onExpansionRequested: root.supportedExpanded = !root.supportedExpanded
    }
    DesktopSettingsDisclosure {
        width: parent.width
        expanded: root.supportedExpanded
        sourceComponent: Item {
            width: root.width
            implicitHeight: supportedContent.implicitHeight
            Column {
                id: supportedContent
                width: parent.width - DesktopTokens.settingsLabelInset - DesktopTokens.settingsInset
                x: DesktopTokens.settingsLabelInset
                topPadding: DesktopTokens.px(8)
                bottomPadding: DesktopTokens.px(18)
                spacing: DesktopTokens.px(16)
                Repeater {
                    model: root.supportedGroups
                    delegate: Column {
                        id: supportedGroup
                        required property var modelData
                        width: parent.width
                        spacing: DesktopTokens.px(6)
                        Text {
                            text: qsTr("%1  %2 IDS").arg(supportedGroup.modelData.label).arg(supportedGroup.modelData.count)
                            color: Theme.textMuted
                            font.family: Theme.monoFont
                            font.pixelSize: DesktopTokens.microSize
                            font.weight: Font.Bold
                            font.letterSpacing: 0.8
                        }
                        Grid {
                            width: parent.width
                            columns: width < DesktopTokens.px(560) ? 1 : 2
                            columnSpacing: DesktopTokens.px(18)
                            rowSpacing: DesktopTokens.px(8)
                            Repeater {
                                model: supportedGroup.modelData.devices
                                delegate: Row {
                                    required property var modelData
                                    width: (parent.width - (parent.columns - 1) * parent.columnSpacing) / parent.columns
                                    spacing: DesktopTokens.px(10)
                                    DesktopSettingsIcon {
                                        width: DesktopTokens.px(16)
                                        height: width
                                        glyph: parent.modelData.type
                                        ink: Theme.textMuted
                                    }
                                    Text {
                                        width: parent.width - DesktopTokens.px(26)
                                        text: parent.modelData.name.replace(/^(Thrustmaster|Logitech) /, "") + "  ·  " + root.deviceType(parent.modelData.type)
                                        color: Theme.label
                                        font.family: Theme.bodyFont
                                        font.pixelSize: DesktopTokens.captionSize
                                        font.weight: Font.DemiBold
                                        wrapMode: Text.WordWrap
                                    }
                                }
                            }
                        }
                    }
                }
                Text {
                    width: parent.width
                    text: qsTr("Matched by USB vendor and product ID. A listed device can still be declined by the server, and some need a firmware mode switch first.")
                    color: Theme.textMuted
                    font.family: Theme.bodyFont
                    font.pixelSize: DesktopTokens.captionSize
                    wrapMode: Text.WordWrap
                }
                Text {
                    width: parent.width
                    text: qsTr("Using a T.Flight HOTAS 4?") + " " + qsTr("Unplug it, hold SHARE + OPTIONS and reconnect USB to enter HID mode each time you start a session.")
                    color: Theme.textMuted
                    font.family: Theme.bodyFont
                    font.pixelSize: DesktopTokens.captionSize
                    wrapMode: Text.WordWrap
                }
            }
        }
    }
}

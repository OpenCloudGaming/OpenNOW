import QtQuick
import QtQuick.Controls
import QtQuick.Layouts
import OpenNOW

DesktopSettingsPanel {
    id: page
    required property real availableWidth
    required property var settingsScreen

    readonly property string selectedDevice: String(page.settingsScreen.valueSetting("audioOutputDevice", ""))
    readonly property var deviceItems: {
        const items = [{label: qsTr("System default"), value: ""}]
        for (const device of ShellStore.audioOutputDevices)
            items.push({label: device.name, value: device.id})
        if (selectedDevice && !items.some(item => item.value === selectedDevice))
            items.push({label: qsTr("%1 (unavailable)").arg(selectedDevice), value: selectedDevice, disabled: true})
        return items
    }

    Component.onCompleted: ShellStore.refreshAudioOutputDevices()
    Connections {
        target: ShellStore
        function onNativeRuntimeReadyChanged() {
            if (ShellStore.nativeRuntimeReady)
                ShellStore.refreshAudioOutputDevices()
        }
    }

    width: page.availableWidth; paperStyle: true
    DesktopSettingsSection { text: qsTr("AUDIO") }
    DesktopSettingsChoice {
        objectName: "audioOutputDeviceChoice"
        width: parent.width; glyph: "wave"; title: qsTr("Output device")
        description: qsTr("Applies to your next streaming session. A fixed device must be available when the session starts.")
        items: page.deviceItems
        value: page.selectedDevice
        onSelected: value => page.settingsScreen.setSetting("audioOutputDevice", value)
        onExpandedChanged: {
            if (expanded)
                ShellStore.refreshAudioOutputDevices()
        }
    }
    DesktopSettingsRow {
        width: parent.width; paperStyle: true; glyph: "arrows"; title: qsTr("Available outputs")
        description: ShellStore.audioOutputDevicesError || (!ShellStore.nativeRuntimeReady
            ? qsTr("Waiting for the native streamer")
            : qsTr("Refresh after connecting or disconnecting an audio device"))
        DesktopSettingsButton {
            objectName: "refreshAudioOutputDevices"
            text: ShellStore.audioOutputDevicesBusy ? qsTr("Loading…") : qsTr("Refresh devices")
            enabled: ShellStore.nativeRuntimeReady && !ShellStore.audioOutputDevicesBusy
            onClicked: ShellStore.refreshAudioOutputDevices()
        }
    }
    DesktopSettingsRow { width: parent.width; paperStyle: true; glyph: "sliders"; title: qsTr("Game volume"); description: qsTr("Use the system mixer or the game's own audio settings"); value: qsTr("SYSTEM MIXER") }
    DesktopSettingsRow {
        width: parent.width; paperStyle: true; glyph: "wave"
        title: qsTr("Microphone")
        description: ShellStore.microphoneCaptureSupported ? ShellStore.microphoneDescription
            : qsTr("Microphone capture is unavailable in this build.")
        DesktopSettingsSegmented {
            objectName: "microphoneModeOptions"
            options: [{label: qsTr("Disabled"), value: "disabled", width: 100},
                {label: qsTr("Open microphone"), value: "voice-activity", width: 170,
                    enabled: ShellStore.microphoneCaptureSupported}]
            selectedIndex: ShellStore.settings.microphoneMode === "voice-activity" ? 1 : 0
            onSelected: (index, value) => ShellStore.setSetting("microphoneMode", value)
        }
    }
    DesktopSettingsRow { width: parent.width; paperStyle: true; glyph: "info"; title: qsTr("Audio format"); description: qsTr("Audio format and channel count are negotiated with the active GeForce NOW session."); showDivider: false }
}

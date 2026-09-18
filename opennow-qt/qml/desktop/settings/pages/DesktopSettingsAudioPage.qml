import QtQuick
import OpenNOW

DesktopSettingsPanel {
    id: page
    objectName: "desktopAudioSettings"
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
        id: outputDeviceChoice
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
        Column {
            visible: outputDeviceChoice.expanded || ShellStore.audioOutputDevicesError !== ""
            width: parent.width
            spacing: DesktopTokens.px(8)
            bottomPadding: DesktopTokens.px(4)
            Text {
                width: parent.width
                text: ShellStore.audioOutputDevicesError || (!ShellStore.nativeRuntimeReady
                    ? qsTr("Waiting for the native streamer")
                    : qsTr("Refresh after connecting or disconnecting an audio device"))
                wrapMode: Text.WordWrap; color: Theme.textMuted
                font.family: Theme.bodyFont; font.pixelSize: DesktopTokens.captionSize
            }
            DesktopSettingsButton {
                objectName: "refreshAudioOutputDevices"
                text: ShellStore.audioOutputDevicesBusy ? qsTr("Loading…") : qsTr("Refresh devices")
                enabled: ShellStore.nativeRuntimeReady && !ShellStore.audioOutputDevicesBusy
                onClicked: ShellStore.refreshAudioOutputDevices()
            }
        }
    }
    Text {
        x: DesktopTokens.px(76); width: parent.width - x - DesktopTokens.px(20)
        topPadding: DesktopTokens.px(8); bottomPadding: DesktopTokens.px(12)
        text: qsTr("Game volume: use the system mixer or the game's own audio settings.")
        wrapMode: Text.WordWrap; color: Theme.textMuted
        font.family: Theme.bodyFont; font.pixelSize: DesktopTokens.captionSize
    }
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
            onSelected: (index, item) => ShellStore.setSetting("microphoneMode", item.value)
        }
    }
    DesktopSettingsRow {
        width: parent.width; paperStyle: true; glyph: "wave"
        title: qsTr("Mute when out of focus")
        description: qsTr("Silence stream audio while using another app. Audio returns when you switch back to OpenNOW.")
        DesktopSettingsToggle {
            objectName: "muteWhenOutOfFocusToggle"
            checked: page.settingsScreen.valueSetting("muteWhenOutOfFocus", false) === true
            onValueChangedByUser: value => page.settingsScreen.setSetting("muteWhenOutOfFocus", value)
        }
    }
    Text {
        x: DesktopTokens.px(76); width: parent.width - x - DesktopTokens.px(20)
        topPadding: DesktopTokens.px(12); bottomPadding: DesktopTokens.px(16)
        text: qsTr("Audio format and channel count are negotiated with the active GeForce NOW session.")
        wrapMode: Text.WordWrap; color: Theme.textMuted
        font.family: Theme.bodyFont; font.pixelSize: DesktopTokens.captionSize
    }
}

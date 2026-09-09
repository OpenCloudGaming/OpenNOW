import QtQuick
import OpenNOW

Column {
    id: page
    required property real availableWidth
    required property var settingsScreen

    width: availableWidth
    spacing: 14

    Component.onCompleted: ShellStore.refreshMedia()

    DesktopSettingsPanel {
        width: parent.width; paperStyle: true
        DesktopSettingsSection { text: qsTr("SOURCE-QUALITY CAPTURE") }
        DesktopSettingsRow {
            width: parent.width; paperStyle: true; glyph: "image"
            title: qsTr("Resolution, frame rate and quality")
            description: qsTr("Recordings and clips follow the incoming stream, without re-encoding. Independent downscaling requires re-encoding and is not available in low-overhead mode.")
        }
        DesktopSettingsRow {
            width: parent.width; paperStyle: true; glyph: "monitor"
            title: qsTr("Requested stream settings")
            description: qsTr("%1 · %2 FPS · up to %3 Mbps. The negotiated stream may differ.")
                .arg(String(page.settingsScreen.valueSetting("resolution", "1920x1080")))
                .arg(page.settingsScreen.valueSetting("fps", 60))
                .arg(page.settingsScreen.valueSetting("maxBitrateMbps", 75))
            DesktopSettingsButton {
                objectName: "recordingStreamSettings"
                text: qsTr("Stream settings")
                onClicked: page.settingsScreen.selectedSection = 3
            }
        }
        DesktopSettingsRow {
            width: parent.width; paperStyle: true; glyph: "wave"
            title: qsTr("Recording format")
            description: qsTr("Source video and game audio in a Matroska (.mkv) file. No extra video encoder runs while you play.")
            value: "MKV"
        }
        DesktopSettingsRow {
            width: parent.width; paperStyle: true; glyph: "folder"
            title: qsTr("Save location")
            description: ShellStore.mediaRootPath ? ShellStore.mediaRootPath + "/Recordings" : qsTr("Pictures/OpenNOW/Recordings")
            showDivider: false
            DesktopSettingsButton {
                objectName: "openRecordingsFolder"
                text: qsTr("Open folder")
                enabled: ShellStore.mediaRootPath !== ""
                onClicked: AppController.openLocalPath(ShellStore.mediaRootPath + "/Recordings", false)
            }
        }
    }
    DesktopSettingsPanel {
        width: parent.width; paperStyle: true
        DesktopSettingsSection { text: qsTr("REPLAY BUFFER") }
        DesktopSettingsRow {
            width: parent.width; paperStyle: true; glyph: "clock"
            title: qsTr("Enable replay buffer")
            description: qsTr("Off by default. Keep recent source video and audio in memory to save a clip. Enabling takes effect next session; disabling clears the buffer immediately.")
            DesktopSettingsToggle {
                objectName: "replayBufferEnabledToggle"
                checked: page.settingsScreen.boolSetting("replayBufferEnabled", false)
                Accessible.name: qsTr("Enable replay buffer")
                onValueChangedByUser: value => page.settingsScreen.setSetting("replayBufferEnabled", value)
            }
        }
        DesktopSettingsChoice {
            objectName: "replayBufferSecondsChoice"
            width: parent.width; glyph: "clock"
            title: qsTr("Replay duration")
            description: qsTr("Target clip length. Memory limits and source keyframes may shorten clips or require waiting for a new keyframe. Changes apply next session.")
            items: [15, 30, 60, 120].map(value => ({label: qsTr("%1 seconds").arg(value), value: value}))
            value: page.settingsScreen.valueSetting("replayBufferSeconds", 30)
            onSelected: value => page.settingsScreen.setSetting("replayBufferSeconds", value)
        }
        DesktopSettingsChoice {
            objectName: "replayBufferMemoryChoice"
            width: parent.width; glyph: "sliders"
            title: qsTr("Replay memory limit")
            description: qsTr("Maximum memory for buffered media. Higher stream bitrates fill it sooner. Changes take effect next session.")
            items: [64, 128, 256, 512].map(value => ({label: qsTr("%1 MiB").arg(value), value: value}))
            value: page.settingsScreen.valueSetting("replayBufferMemoryMiB", 256)
            showDivider: false
            onSelected: value => page.settingsScreen.setSetting("replayBufferMemoryMiB", value)
        }
    }
    DesktopSettingsPanel {
        width: parent.width; paperStyle: true
        DesktopSettingsSection { text: qsTr("CAPTURE SHORTCUTS") }
        DesktopSettingsRow {
            width: parent.width; paperStyle: true; glyph: "keyboard"
            title: qsTr("Toggle recording")
            description: qsTr("Start or stop a source-quality recording during a stream.")
            DesktopSettingsButton {
                objectName: "editRecordingShortcut"
                text: String(page.settingsScreen.valueSetting("shortcutToggleRecording", "F12"))
                Accessible.name: qsTr("Toggle recording") + ": " + text
                onClicked: shortcutEditor.edit("shortcutToggleRecording", qsTr("Toggle recording"))
            }
        }
        DesktopSettingsRow {
            width: parent.width; paperStyle: true; glyph: "keyboard"
            title: qsTr("Save replay clip")
            description: qsTr("Save the buffered video and audio. Requires the replay buffer to be enabled for this session.")
            showDivider: false
            DesktopSettingsButton {
                objectName: "editSaveClipShortcut"
                text: String(page.settingsScreen.valueSetting("shortcutSaveClip", "Ctrl+F12"))
                Accessible.name: qsTr("Save replay clip") + ": " + text
                onClicked: shortcutEditor.edit("shortcutSaveClip", qsTr("Save replay clip"))
            }
        }
    }
    DesktopSettingsShortcutEditor { id: shortcutEditor }
}

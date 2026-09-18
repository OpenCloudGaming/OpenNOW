import QtQuick
import OpenNOW

Column {
    id: page
    objectName: "desktopRecordingSettings"
    required property real availableWidth
    required property var settingsScreen

    width: availableWidth
    spacing: DesktopTokens.px(20)

    Component.onCompleted: ShellStore.refreshMedia()

    DesktopSettingsPanel {
        width: parent.width; paperStyle: true
        DesktopSettingsSection { text: qsTr("SAVE LOCATIONS") }
        DesktopSettingsRow {
            width: parent.width; paperStyle: true; glyph: "folder"
            title: qsTr("Save location")
            description: ShellStore.mediaRootPath ? ShellStore.mediaRootPath + "/Recordings" : qsTr("Pictures/OpenNOW/Recordings")
            DesktopSettingsButton {
                objectName: "openRecordingsFolder"
                text: qsTr("Open folder")
                enabled: ShellStore.mediaRootPath !== ""
                onClicked: AppController.openLocalPath(ShellStore.mediaRootPath + "/Recordings", false)
            }
        }
        DesktopSettingsRow {
            width: parent.width; paperStyle: true; glyph: "folder"
            title: qsTr("Captures folder")
            description: ShellStore.mediaRootPath || qsTr("Pictures/OpenNOW")
            showDivider: false
            DesktopSettingsButton {
                objectName: "openCapturesFolder"
                text: qsTr("Open folder")
                enabled: ShellStore.mediaRootPath !== ""
                onClicked: AppController.openLocalPath(ShellStore.mediaRootPath, false)
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
            visible: page.settingsScreen.boolSetting("replayBufferEnabled", false)
            width: parent.width; glyph: "clock"
            title: qsTr("Replay duration")
            description: qsTr("Target clip length. Memory limits and source keyframes may shorten clips or require waiting for a new keyframe. Changes apply next session.")
            items: [15, 30, 60, 120].map(value => ({label: qsTr("%1 seconds").arg(value), value: value}))
            value: page.settingsScreen.valueSetting("replayBufferSeconds", 30)
            onSelected: value => page.settingsScreen.setSetting("replayBufferSeconds", value)
        }
        DesktopSettingsChoice {
            objectName: "replayBufferMemoryChoice"
            visible: page.settingsScreen.boolSetting("replayBufferEnabled", false)
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
        Column {
            x: DesktopTokens.px(20); width: parent.width - DesktopTokens.px(40)
            topPadding: DesktopTokens.px(8); bottomPadding: DesktopTokens.px(16)
            spacing: DesktopTokens.px(12)
            Flow {
                width: parent.width; spacing: DesktopTokens.px(20)
                DesktopKeyHint {
                    objectName: "recordingShortcutHint"
                    keyText: String(page.settingsScreen.valueSetting("shortcutToggleRecording", "F12"))
                    label: qsTr("Toggle recording")
                    compact: true
                }
                DesktopKeyHint {
                    objectName: "replayShortcutHint"
                    keyText: String(page.settingsScreen.valueSetting("shortcutSaveClip", "Ctrl+F12"))
                    label: qsTr("Save replay clip")
                    compact: true
                }
            }
            DesktopSettingsButton {
                objectName: "recordingKeyboardShortcuts"
                text: qsTr("Keyboard shortcuts")
                onClicked: page.settingsScreen.selectedSection = 10
            }
        }
    }
    DesktopSettingsPanel {
        width: parent.width; paperStyle: true
        Column {
            x: DesktopTokens.px(20); width: parent.width - DesktopTokens.px(40)
            topPadding: DesktopTokens.px(16); bottomPadding: DesktopTokens.px(16)
            spacing: DesktopTokens.px(12)
            Text {
                width: parent.width
                text: qsTr("Recordings and clips follow the incoming stream, without re-encoding. Independent downscaling requires re-encoding and is not available in low-overhead mode.")
                    + "\n" + qsTr("Requested stream settings") + ": "
                    + qsTr("%1 · %2 FPS · up to %3 Mbps. The negotiated stream may differ.")
                        .arg(String(page.settingsScreen.valueSetting("resolution", "1920x1080")))
                        .arg(page.settingsScreen.valueSetting("fps", 60))
                        .arg(page.settingsScreen.valueSetting("maxBitrateMbps", 75))
                    + "\n" + qsTr("Source video and game audio in a Matroska (.mkv) file. No extra video encoder runs while you play.")
                wrapMode: Text.WordWrap; color: Theme.textMuted
                font.family: Theme.bodyFont; font.pixelSize: DesktopTokens.captionSize
            }
            DesktopSettingsButton {
                objectName: "recordingStreamSettings"
                text: qsTr("Stream settings")
                onClicked: page.settingsScreen.selectedSection = 3
            }
        }
    }
}

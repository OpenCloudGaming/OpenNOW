import QtQuick

QtObject {
    id: root
    required property var settings
    required property bool applicationActive
    required property bool streaming
    required property bool nativeRuntimeReady
    required property var sendNativeCommand
    signal reminderRequested()

    readonly property bool audioMuted: settings.muteWhenOutOfFocus === true && !applicationActive
    readonly property bool reminderRunning: settings.backgroundStreamReminder === true
        && streaming && !applicationActive

    onAudioMutedChanged: syncAudioMute()
    onNativeRuntimeReadyChanged: syncAudioMute()

    function syncAudioMute() {
        if (nativeRuntimeReady)
            sendNativeCommand("setAudioMuted", {muted: audioMuted}, "setAudioMuted")
    }

    property Timer reminderTimer: Timer {
        interval: 300000
        repeat: true
        running: root.reminderRunning
        onTriggered: {
            if (root.reminderRunning)
                root.reminderRequested()
        }
    }
}

import QtQuick
import QtTest
import OpenNOW

TestCase {
    name: "SessionSetupProgress"
    when: windowShown
    width: 1000
    height: 700

    SessionSetupProgress {
        id: progress
        session: null
    }

    DesktopSessionStarting {
        id: screen
        anchors.fill: parent
    }

    function init() {
        ShellStore.activeSession = {}
        ShellStore.streamer = {}
        ShellStore.streamState = "preparing"
        ShellStore.streamerRestartAttempts = 0
        ShellStore.sessionReconnectAttempts = 0
        ShellStore.streamMessage = ""
        AppController.route = "inserting"
        AppController.overlay = ""
    }

    function cleanup() {
        ShellStore.activeSession = {queuePosition: 21}
        ShellStore.streamer = {}
        ShellStore.streamState = "preparing"
        AppController.route = "inserting"
    }

    function test_setupSteps_data() {
        return [
            {tag: "missing", session: {}, title: "Preparing your game"},
            {tag: "null", session: {seatSetupStep: null}, title: "Preparing your game"},
            {tag: "unknown", session: {seatSetupStep: 72}, title: "Preparing your game"},
            {tag: "negative", session: {seatSetupStep: -1}, title: "Preparing your game"},
            {tag: "invalid", session: {seatSetupStep: "5"}, title: "Preparing your game"},
            {tag: "fraction", session: {seatSetupStep: 2.5}, title: "Preparing your game"},
            {tag: "connecting", session: {seatSetupStep: 0}, title: "Connecting to GeForce NOW"},
            {tag: "queue-without-position", session: {seatSetupStep: 1}, title: "Waiting for an available rig"},
            {tag: "queue", session: {seatSetupStep: 1, queuePosition: 21}, title: "Queue position 21"},
            {tag: "position-only", session: {queuePosition: 3}, title: "Queue position 3"},
            {tag: "configuring-2", session: {seatSetupStep: 2}, title: "Configuring your cloud gaming rig"},
            {tag: "configuring-3-not-native-enum", session: {seatSetupStep: 3}, title: "Configuring your cloud gaming rig"},
            {tag: "configuring-4-not-ready", session: {seatSetupStep: 4}, title: "Configuring your cloud gaming rig"},
            {tag: "cleanup", session: {seatSetupStep: 5, queuePosition: 21}, title: "Cleaning up your previous session"},
            {tag: "storage", session: {seatSetupStep: 6, queuePosition: 21}, title: "Waiting for cloud storage"}
        ]
    }

    function test_setupSteps(data) {
        progress.session = data.session
        ShellStore.activeSession = data.session
        compare(progress.title, data.title)
        compare(screen.statusText, data.title)
        compare(screen.detailText, progress.detail)
        verify(progress.detail.length > 0)
    }

    function test_queuePositionBounds() {
        for (const invalid of [-1, "bad", Infinity, NaN]) {
            progress.session = {seatSetupStep: 1, queuePosition: invalid}
            compare(progress.queuePosition, 0)
            compare(progress.title, "Waiting for an available rig")
        }
        progress.session = {queuePosition: 2.7}
        compare(progress.queuePosition, 2)
    }

    function test_lifecyclePrecedence() {
        ShellStore.activeSession = {seatSetupStep: 5, queuePosition: 21}
        for (const entry of [
            ["checking", "Checking session availability"],
            ["requesting", "Requesting your session"],
            ["resuming", "Reconnecting to your game"],
            ["reconnecting", "Reconnecting to your session"],
            ["failed", "Session could not start"],
            ["stopping", "Closing your session"]
        ]) {
            ShellStore.streamState = entry[0]
            compare(screen.statusText, entry[1])
        }
    }

    function test_terminalErrorOutranksReconnectCounters() {
        AppController.route = "stream"
        ShellStore.sessionReconnectAttempts = 8
        ShellStore.streamerRestartAttempts = 2
        ShellStore.streamState = "error"
        ShellStore.streamMessage = "The streaming session could not be recovered"
        ShellStore.streamer = {status: "stopped"}
        compare(screen.failed, true)
        compare(screen.reconnecting, true)
        compare(screen.statusText, "Session could not start")
        compare(screen.detailText, "The streaming session could not be recovered")
    }

    function test_activeReconnectIgnoresStoppedRuntime() {
        AppController.route = "stream"
        ShellStore.streamState = "reconnecting"
        ShellStore.sessionReconnectAttempts = 1
        ShellStore.streamer = {status: "stopped", message: "The native media runtime stopped unexpectedly"}
        compare(screen.failed, false)
        compare(screen.statusText, "Reconnecting to your session")
    }

    function test_idleSessionIgnoresStoppedRuntime() {
        AppController.route = "stream"
        ShellStore.streamState = "idle"
        ShellStore.streamer = {status: "stopped"}
        compare(screen.failed, false)
    }

    function test_nativeConnectionProgress() {
        AppController.route = "stream"
        ShellStore.activeSession = {seatSetupStep: 5}
        ShellStore.streamer = {status: "starting"}
        compare(screen.statusText, "Connecting to your game")
        compare(screen.detailText, "Initializing the native streaming connection.")
        ShellStore.streamer = {status: "streaming"}
        compare(screen.detailText, "Connected. Waiting for the first video frame.")
        ShellStore.streamer = {status: "error", message: "Connection failed"}
        compare(screen.statusText, "Session could not start")
        compare(screen.detailText, "Connection failed")
    }

    function test_progressCanMoveBackward() {
        for (const step of [1, 2, 5, 0, 6, 1]) {
            ShellStore.activeSession = {seatSetupStep: step}
            progress.session = ShellStore.activeSession
            compare(screen.statusText, progress.title)
        }
        ShellStore.activeSession = null
        compare(screen.statusText, "Preparing your game")
    }
}

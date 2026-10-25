import QtQuick
import OpenNOW

QtObject {
    id: root
    property var notice: null
    property var presented: null
    property bool copied: false
    readonly property string kind: root.presented ? String(root.presented.kind || "error") : "error"
    readonly property bool failure: root.kind !== "idle" && root.kind !== "time-limit"
    readonly property string codeText: root.presented ? String(root.presented.codeText || "") : ""
    readonly property bool codeCopyable: root.failure && root.codeText !== ""
    readonly property bool quietCodeVisible: !root.failure && root.codeText !== ""
    readonly property string gameTitle: ShellStore.selectedGame && ShellStore.selectedGame.title
        ? String(ShellStore.selectedGame.title) : ""
    readonly property string eyebrow: root.failure ? qsTr("STREAM ENDED") : qsTr("SESSION ENDED")
    readonly property string headline: root.kind === "idle" ? qsTr("Closed after inactivity")
        : root.kind === "time-limit" ? qsTr("Session time limit reached")
        : qsTr("The stream ended unexpectedly")
    readonly property string explanation: {
        if (root.kind === "idle")
            return root.gameTitle !== ""
                ? qsTr("%1 didn't get any input for a while, so the session was closed. Start it again whenever you're ready.").arg(root.gameTitle)
                : qsTr("Your game didn't get any input for a while, so the session was closed. Start it again whenever you're ready.")
        if (root.kind === "time-limit")
            return root.gameTitle !== ""
                ? qsTr("%1 ran for the longest session your membership allows. Start a new session to keep playing.").arg(root.gameTitle)
                : qsTr("Your game ran for the longest session your membership allows. Start a new session to keep playing.")
        return root.gameTitle !== ""
            ? qsTr("The cloud server closed your %1 session. You can start it again from here.").arg(root.gameTitle)
            : qsTr("The cloud server closed your session. You can start it again from here.")
    }
    readonly property string codeLine: qsTr("Error code: %1").arg(root.codeText)
    readonly property string quietCodeLine: qsTr("Code %1").arg(root.codeText)
    readonly property string primaryText: root.failure ? qsTr("Dismiss") : qsTr("Got it")
    readonly property string copyText: root.copied ? qsTr("Copied") : qsTr("Copy")
    readonly property string copyAccessibleName: root.copied ? qsTr("Error code copied") : qsTr("Copy error code")

    // Keep the last notice while the dialog animates out after dismissal.
    onNoticeChanged: if (root.notice) {
        root.presented = root.notice
        root.copied = false
    }
    Component.onCompleted: if (root.notice) root.presented = root.notice

    function copyCode() {
        if (!root.codeCopyable)
            return
        root.copied = AppController.writeClipboardText(root.codeText)
        if (root.copied)
            copiedTimer.restart()
    }
    function dismiss() { ShellStore.dismissStreamEndNotice() }

    property Timer copiedTimer: Timer {
        interval: 1600
        onTriggered: root.copied = false
    }
}

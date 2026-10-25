import QtQuick
import QtQuick.Controls
import QtQuick.Window
import OpenNOW

FocusScope {
    id: root
    objectName: "desktopStreamEndNotice"
    property var notice: null
    property bool opened: false
    property bool inputSuspended: false
    readonly property bool ownsInput: opened && !inputSuspended
    property var presented: null
    property bool copied: false
    readonly property string kind: root.presented ? String(root.presented.kind || "error") : "error"
    readonly property bool failure: root.kind !== "idle" && root.kind !== "time-limit"
    readonly property string codeText: root.presented ? String(root.presented.codeText || "") : ""
    readonly property bool codeCopyable: root.failure && root.codeText !== ""
    readonly property color accent: root.failure ? DesktopTokens.flightDeclined : DesktopTokens.focus
    readonly property string gameTitle: ShellStore.selectedGame && ShellStore.selectedGame.title
        ? String(ShellStore.selectedGame.title) : ""
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
    visible: reveal.present
    enabled: ownsInput
    focus: ownsInput
    MotionProgress { id: reveal; shown: root.opened }
    Accessible.role: Accessible.Dialog
    Accessible.name: root.headline
    Accessible.description: root.explanation

    onNoticeChanged: if (root.notice) {
        root.presented = root.notice
        root.copied = false
    }
    Component.onCompleted: if (root.notice) root.presented = root.notice
    onOwnsInputChanged: if (ownsInput) Qt.callLater(root.restoreFocus)

    function restoreFocus() {
        if (root.ownsInput && !copyButton.activeFocus && !dismissButton.activeFocus)
            dismissButton.forceActiveFocus()
    }
    function copyCode() {
        root.copied = AppController.writeClipboardText(root.codeText)
        if (root.copied)
            copiedTimer.restart()
    }

    // Game details open in the same frame and focus their Play button; the
    // notice keeps input until it is dismissed.
    Connections {
        target: root.Window.window
        enabled: root.ownsInput
        function onActiveFocusItemChanged() { Qt.callLater(root.restoreFocus) }
    }
    Timer {
        id: copiedTimer
        interval: 1600
        onTriggered: root.copied = false
    }

    Keys.onShortcutOverride: event => event.accepted = event.key !== Qt.Key_F11
    Keys.onPressed: event => {
        if (event.key === Qt.Key_Escape || event.key === Qt.Key_Back) {
            ShellStore.dismissStreamEndNotice()
        } else if (event.key === Qt.Key_Return || event.key === Qt.Key_Enter) {
            if (copyButton.activeFocus)
                root.copyCode()
            else
                ShellStore.dismissStreamEndNotice()
        } else if (event.key === Qt.Key_Tab || event.key === Qt.Key_Backtab) {
            if (root.codeCopyable)
                (copyButton.activeFocus ? dismissButton : copyButton).forceActiveFocus()
        } else if (event.key === Qt.Key_Up || event.key === Qt.Key_Left) {
            if (root.codeCopyable)
                copyButton.forceActiveFocus()
        } else if (event.key === Qt.Key_Down || event.key === Qt.Key_Right) {
            dismissButton.forceActiveFocus()
        } else if (event.key === Qt.Key_Space) {
            return
        }
        event.accepted = true
    }

    Rectangle {
        anchors.fill: parent
        color: Qt.rgba(0, 0, 0, 0.55)
        opacity: reveal.progress
        MouseArea {
            anchors.fill: parent
            hoverEnabled: true
            acceptedButtons: Qt.AllButtons
            onWheel: wheel => wheel.accepted = true
        }
    }

    Rectangle {
        id: card
        objectName: "streamEndNoticeCard"
        anchors.centerIn: parent
        width: Math.min(DesktopTokens.px(520), root.width - DesktopTokens.px(48))
        height: Math.min(root.height - DesktopTokens.px(48), content.implicitHeight + DesktopTokens.px(56))
        radius: DesktopTokens.px(24)
        color: Theme.shell
        border.width: 1
        border.color: DesktopTokens.seam
        opacity: reveal.progress
        scale: reveal.zoom
        clip: true
        MouseArea { anchors.fill: parent; acceptedButtons: Qt.AllButtons }

        Column {
            id: content
            x: DesktopTokens.px(28)
            y: DesktopTokens.px(28)
            width: parent.width - DesktopTokens.px(56)

            Row {
                spacing: DesktopTokens.px(8)
                Rectangle {
                    anchors.verticalCenter: parent.verticalCenter
                    width: DesktopTokens.px(7)
                    height: width
                    radius: width / 2
                    color: root.accent
                }
                Text {
                    text: root.failure ? qsTr("STREAM ENDED") : qsTr("SESSION ENDED")
                    color: root.accent
                    font.family: DesktopTokens.monoFont
                    font.pixelSize: DesktopTokens.microSize
                    font.weight: Font.Bold
                    font.letterSpacing: 1.4 * DesktopTokens.uiScale
                }
            }
            Text {
                objectName: "streamEndNoticeTitle"
                width: parent.width
                topPadding: DesktopTokens.px(10)
                text: root.headline
                color: DesktopTokens.text
                font.family: DesktopTokens.displayFont
                font.pixelSize: DesktopTokens.titleSize
                font.weight: Font.Black
                wrapMode: Text.WordWrap
                maximumLineCount: 2
                elide: Text.ElideRight
            }
            Text {
                objectName: "streamEndNoticeBody"
                width: parent.width
                topPadding: DesktopTokens.px(10)
                text: root.explanation
                color: DesktopTokens.textBody
                font.family: DesktopTokens.bodyFont
                font.pixelSize: DesktopTokens.bodySize
                font.weight: Font.Medium
                wrapMode: Text.WordWrap
                lineHeight: 1.25
                maximumLineCount: 4
                elide: Text.ElideRight
            }
            Item {
                width: 1
                height: DesktopTokens.px(20)
                visible: codeRow.visible
            }
            Rectangle {
                id: codeRow
                objectName: "streamEndNoticeCodeRow"
                visible: root.codeCopyable
                width: parent.width
                height: DesktopTokens.controlHeight + DesktopTokens.px(20)
                radius: DesktopTokens.px(14)
                color: DesktopTokens.raised
                border.width: 1
                border.color: DesktopTokens.seamSoft
                Text {
                    objectName: "streamEndNoticeCode"
                    x: DesktopTokens.px(16)
                    anchors.verticalCenter: parent.verticalCenter
                    width: copyButton.x - x - DesktopTokens.px(12)
                    text: qsTr("Error code: %1").arg(root.codeText)
                    color: DesktopTokens.text
                    font.family: DesktopTokens.monoFont
                    font.pixelSize: DesktopTokens.captionSize
                    font.weight: Font.DemiBold
                    elide: Text.ElideRight
                }
                DesktopButton {
                    id: copyButton
                    objectName: "streamEndNoticeCopy"
                    anchors.right: parent.right
                    anchors.rightMargin: DesktopTokens.px(10)
                    anchors.verticalCenter: parent.verticalCenter
                    height: DesktopTokens.controlHeight
                    font.pixelSize: DesktopTokens.captionSize
                    themedGlyph: root.copied ? "check" : "copy"
                    glyphSize: DesktopTokens.px(14)
                    text: root.copied ? qsTr("Copied") : qsTr("Copy")
                    Accessible.name: root.copied ? qsTr("Error code copied") : qsTr("Copy error code")
                    onClicked: root.copyCode()
                }
            }
            Item {
                width: 1
                height: DesktopTokens.px(24)
            }
            Item {
                width: parent.width
                height: dismissButton.height
                Text {
                    objectName: "streamEndNoticeQuietCode"
                    anchors.verticalCenter: parent.verticalCenter
                    width: dismissButton.x - DesktopTokens.px(12)
                    visible: !root.failure && root.codeText !== ""
                    text: qsTr("Code %1").arg(root.codeText)
                    color: DesktopTokens.textFaint
                    font.family: DesktopTokens.monoFont
                    font.pixelSize: DesktopTokens.smallSize
                    elide: Text.ElideRight
                }
                DesktopButton {
                    id: dismissButton
                    objectName: "streamEndNoticeDismiss"
                    anchors.right: parent.right
                    height: DesktopTokens.controlHeight
                    font.pixelSize: DesktopTokens.captionSize
                    primary: true
                    text: root.failure ? qsTr("Dismiss") : qsTr("Got it")
                    shortcutText: qsTr("Esc")
                    onClicked: ShellStore.dismissStreamEndNotice()
                }
            }
        }
    }
}

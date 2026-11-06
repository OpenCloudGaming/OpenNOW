import QtQuick
import QtQuick.Controls
import OpenNOW

Rectangle {
    id: root
    objectName: "streamPointerLockHint"
    property bool active: false
    property bool streaming: false
    property string sessionId: ""
    property bool pointerLockEnabled: true
    property bool streamStarted: false
    signal enableRequested()
    visible: active && !pointerLockEnabled && noticeTimer.running
    onActiveChanged: {
        if (!active) noticeTimer.stop()
        else Qt.callLater(root.observeStreamStart)
    }
    onStreamingChanged: {
        if (!streaming) noticeTimer.stop()
        Qt.callLater(root.observeStreamStart)
    }
    onSessionIdChanged: {
        streamStarted = false
        noticeTimer.stop()
        Qt.callLater(root.observeStreamStart)
    }
    onPointerLockEnabledChanged: {
        if (pointerLockEnabled) noticeTimer.stop()
        else if (streamStarted && streaming && active) noticeTimer.restart()
    }
    Component.onCompleted: observeStreamStart()

    function observeStreamStart() {
        if (!streaming || !active || streamStarted) return
        streamStarted = true
        if (!pointerLockEnabled) noticeTimer.restart()
    }

    Timer {
        id: noticeTimer
        objectName: "streamPointerLockNoticeTimer"
        interval: 6000
    }
    anchors.horizontalCenter: parent.horizontalCenter
    anchors.bottom: parent.bottom
    anchors.bottomMargin: 24
    width: Math.min(620, parent.width - 48)
    height: content.implicitHeight + 32
    radius: 12
    color: Theme.glassStrong
    border.color: Theme.seam
    Accessible.role: Accessible.Pane
    Accessible.name: qsTr("Pointer lock is off")

    Column {
        id: content
        x: 16; y: 16; width: parent.width - 32
        spacing: 8
        Text {
            width: parent.width
            text: qsTr("Pointer lock is off. Mouse movement can stop at screen edges.")
            color: Theme.label; font.family: Theme.bodyFont; font.pixelSize: 14
            wrapMode: Text.WordWrap
        }
        Text {
            width: parent.width
            text: qsTr("Change pointer lock in the stream menu (Ctrl+G) or Settings > Controls.")
            color: Theme.textMuted; font.family: Theme.bodyFont; font.pixelSize: 13
            wrapMode: Text.WordWrap
        }
        DesktopSettingsButton {
            objectName: "enablePointerLockButton"
            primary: true
            compact: true
            text: qsTr("Enable pointer lock")
            onClicked: {
                noticeTimer.stop()
                root.enableRequested()
            }
        }
    }
}

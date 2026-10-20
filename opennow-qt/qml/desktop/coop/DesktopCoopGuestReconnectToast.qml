import QtQuick
import OpenNOW

// Guest reconnecting toast over the paused stream (artboard 62P-0, state 1). The bar counts down
// the 60 s the host holds the guest's slot.
Rectangle {
    id: root
    objectName: "coopReconnectingToast"
    readonly property var coop: ShellStore.coop
    readonly property string host: String(coop.hostName || "")
    readonly property int holdSeconds: 60
    property double startedMs: 0
    property double nowMs: 0
    readonly property real elapsed: startedMs > 0 ? Math.min(1, (nowMs - startedMs) / (holdSeconds * 1000)) : 0
    width: 384
    height: Math.max(38, textColumn.implicitHeight + 4) + 30
    radius: 20
    color: "#F00E1018"
    border.width: 1
    border.color: "#24FFFFFF"
    clip: true
    Accessible.role: Accessible.AlertMessage
    Accessible.name: titleText.text
    Accessible.description: bodyText.text

    onVisibleChanged: {
        startedMs = visible ? Date.now() : 0
        nowMs = startedMs
    }
    Component.onCompleted: if (visible) { startedMs = Date.now(); nowMs = startedMs }
    Timer {
        interval: 500
        repeat: true
        running: root.visible && root.startedMs > 0
        onTriggered: root.nowMs = Date.now()
    }

    Rectangle {
        x: 15
        y: 14 + (root.height - 30 - height) / 2
        width: 38
        height: 38
        radius: 12
        color: "#247FD4FF"
        border.width: 1
        border.color: "#4D7FD4FF"
        DesktopCoopGuestIcon {
            anchors.centerIn: parent
            size: 19
            stroke: "#7FD4FF"
            path: "M3 12a9 9 0 0 1 9-9 9.75 9.75 0 0 1 6.74 2.74L21 8M21 3v5h-5M21 12a9 9 0 0 1-9 9 9.75 9.75 0 0 1-6.74-2.74L3 16M8 16H3v5"
        }
    }
    Column {
        id: textColumn
        x: 15 + 38 + 12
        y: 18
        width: parent.width - x - 16 - 18 - 12
        spacing: 3
        DesktopCoopGuestText {
            id: titleText
            width: parent.width
            elide: Text.ElideRight
            text: root.host !== "" ? qsTr("Reconnecting to %1…").arg(root.host) : qsTr("Reconnecting to the host…")
            size: 15
            lineHeightPx: 18
            font.weight: Font.ExtraBold
        }
        DesktopCoopGuestText {
            id: bodyText
            width: parent.width
            wrapMode: Text.WordWrap
            text: qsTr("The picture is paused. Your P%1 slot is held for 60 s.").arg(root.coop.slot + 1)
            color: "#A8FFFFFF"
            size: 13
            lineHeightPx: 18
            font.weight: Font.Normal
        }
    }
    DesktopCoopGuestSpinner {
        anchors.right: parent.right
        anchors.rightMargin: 16
        y: 14 + (root.height - 30 - height) / 2
        size: 18
    }
    Rectangle {
        x: 12
        anchors.bottom: parent.bottom
        width: 360
        height: 2
        color: "#14FFFFFF"
        Rectangle {
            width: parent.width * root.elapsed
            height: parent.height
            color: "#7FD4FF"
        }
    }
}

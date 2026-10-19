import QtQuick
import QtQuick.Controls
import OpenNOW

// One-click invite chip (C2 lobby strip and the Ctrl+G co-op block). inviteState: "idle" | "pending" | "declined" |
// "expired" | "dm".
AbstractButton {
    id: root
    property var user: null
    property var presence: null
    property string inviteState: "idle"
    property string detail: ""
    property real nowMs: Date.now()
    readonly property bool online: Boolean(presence && presence.status === "online")
    readonly property string activity: online ? String(presence.activity || "online") : "offline"
    readonly property string name: user ? String(user.displayName || user.username || "") : ""
    readonly property bool focusVisible: activeFocus && AppController.inputMode !== "pointer"
    readonly property var avatarPalette: ["#93C5FD", "#FDBA74", "#BEF264", "#F9A8D4", "#C4B5FD", "#FBCFE8", "#7FD4FF",
        "#6EE7B7", "#86EFAC", "#A5F3FC", "#FDE68A", "#FCD34D"]
    width: 204
    height: 56
    hoverEnabled: true
    focusPolicy: Qt.StrongFocus
    Accessible.role: Accessible.Button
    Accessible.name: name + ", " + subtitle
    Keys.onReturnPressed: event => { root.clicked(); event.accepted = true }
    Keys.onEnterPressed: event => { root.clicked(); event.accepted = true }

    function minutesSince(value) {
        const since = Date.parse(String(value || ""))
        return isNaN(since) ? 0 : Math.max(1, Math.floor((nowMs - since) / 60000))
    }
    readonly property string presenceText: {
        if (activity === "playing" || activity === "in_lobby")
            return presence.game && presence.game.title ? String(presence.game.title) : qsTr("Playing")
        if (activity === "in_queue")
            return qsTr("In queue")
        if (activity === "browsing_store")
            return qsTr("In the store")
        if (activity === "browsing_library")
            return qsTr("In the library")
        if (activity === "in_settings")
            return qsTr("In settings")
        if (activity === "idle")
            return qsTr("Idle · %1 min").arg(minutesSince(presence.since))
        return activity === "offline" ? qsTr("Offline") : qsTr("Online")
    }
    readonly property string subtitle: inviteState === "pending" ? (detail !== "" ? detail : qsTr("Invited · waiting"))
        : inviteState === "declined" ? qsTr("Declined")
        : inviteState === "expired" ? qsTr("Expired · resend")
        : inviteState === "dm" ? qsTr("Sent as DM")
        : presenceText
    readonly property color subtitleColor: inviteState === "pending" ? "#7FD4FF"
        : inviteState === "declined" ? "#66FFFFFF"
        : inviteState === "expired" ? "#F5A623"
        : inviteState === "dm" ? "#C7CBFF"
        : activity === "playing" ? "#7FD4FF" : "#A3FFFFFF"
    readonly property color avatarColor: colorFor(name, online)
    function colorFor(personName, isOnline) {
        const initial = String(personName || "").toUpperCase()
        return !isOnline ? "#80D6D3D1" : avatarPalette[(initial.length > 0 ? initial.charCodeAt(0) : 0) % avatarPalette.length]
    }
    readonly property color dotColor: !online ? "#0B0F1A" : activity === "idle" ? "#F5A623"
        : activity === "playing" || activity === "in_queue" || activity === "in_lobby" ? "#7FD4FF" : "#4ADE80"

    background: Rectangle {
        radius: 14
        color: root.inviteState === "pending" ? "#1A7FD4FF" : root.inviteState === "declined" ? "#990E1018"
            : root.inviteState === "expired" ? "#0FF5A623" : root.inviteState === "dm" ? "#1A5865F2"
            : root.hovered ? "#F2161A24" : "#D90E1018"
        border.width: 1
        border.color: root.inviteState === "pending" ? "#807FD4FF" : root.inviteState === "declined" ? "#0FFFFFFF"
            : root.inviteState === "expired" ? "#66F5A623" : root.inviteState === "dm" ? "#805865F2"
            : root.hovered ? "#3DFFFFFF" : "#24FFFFFF"
        Rectangle {
            anchors.fill: parent
            anchors.margins: -3
            radius: parent.radius + 3
            color: "transparent"
            border.width: 2
            border.color: DesktopTokens.focus
            visible: root.focusVisible
        }
    }
    contentItem: Item {
        Item {
            id: avatar
            x: 10
            anchors.verticalCenter: parent.verticalCenter
            width: 36
            height: 36
            Rectangle {
                anchors.fill: parent
                radius: 18
                color: root.avatarColor
                Text {
                    anchors.centerIn: parent
                    text: root.name.slice(0, 1).toUpperCase()
                    color: "#0B0F1A"
                    font.family: DesktopTokens.bodyFont
                    font.pixelSize: 15
                    font.weight: Font.Black
                }
                DesktopAvatarPhoto {
                    anchors.fill: parent
                    url: root.user ? String(root.user.avatarUrl || "") : ""
                }
            }
            Rectangle {
                x: parent.width - 10
                y: parent.height - 10
                width: 12
                height: 12
                radius: 6
                color: root.dotColor
                border.width: root.online ? 3 : 2.5
                border.color: root.online ? "#0B0F1A" : "#6B7280"
            }
        }
        Column {
            x: 56
            anchors.verticalCenter: parent.verticalCenter
            width: parent.width - x - trailing.width - 22
            spacing: 1
            Text {
                width: parent.width
                text: root.name
                color: "#FFFFFF"
                font.family: DesktopTokens.bodyFont
                font.pixelSize: 14
                font.weight: Font.ExtraBold
                elide: Text.ElideRight
            }
            Text {
                width: parent.width
                text: root.subtitle
                color: root.subtitleColor
                font.family: DesktopTokens.bodyFont
                font.pixelSize: 12
                font.weight: Font.Bold
                elide: Text.ElideRight
            }
        }
        DesktopCoopHostIcon {
            id: trailing
            anchors.right: parent.right
            anchors.rightMargin: 14
            anchors.verticalCenter: parent.verticalCenter
            width: root.inviteState === "idle" ? 17 : root.inviteState === "pending" || root.inviteState === "dm" ? 16 : 15
            height: width
            glyph: root.inviteState === "pending" ? "clock" : root.inviteState === "declined" ? "close"
                : root.inviteState === "expired" ? "refresh" : root.inviteState === "dm" ? "discord" : "user-plus"
            ink: root.inviteState === "declined" ? "#66FFFFFF" : root.inviteState === "expired" ? "#F5A623"
                : root.inviteState === "dm" ? "#FFFFFF" : "#7FD4FF"
        }
    }
}

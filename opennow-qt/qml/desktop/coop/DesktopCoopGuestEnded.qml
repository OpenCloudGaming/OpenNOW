import QtQuick
import OpenNOW

FocusScope {
    id: root
    objectName: "coopEnded"
    required property var coop
    readonly property string reason: String(coop.endReason || "host_ended")
    readonly property string host: String(coop.hostName || "")
    readonly property string hostId: coop.joinContext ? String(coop.joinContext.hostId || "") : ""
    readonly property bool streamLost: reason === "stream_lost" || reason === "host_lost"
    readonly property bool removed: reason === "kicked" || reason === "declined"
    readonly property bool mediaUnavailable: reason === "media_unavailable"
    readonly property int waitSeconds: 120
    property double waitStartedMs: 0
    property double nowMs: Date.now()
    readonly property int waitLeft: waitStartedMs > 0 ? Math.max(0, waitSeconds - Math.floor((nowMs - waitStartedMs) / 1000)) : waitSeconds
    focus: true
    Accessible.role: Accessible.AlertMessage
    Accessible.name: titleText.text
    Accessible.description: bodyText.text

    function startWaiting() {
        if (waitStartedMs > 0)
            return
        waitStartedMs = Date.now()
        nowMs = waitStartedMs
        if (hostId !== "")
            coop.watchForRoom(hostId)
    }
    function leave() {
        if (hostId !== "" && waitStartedMs > 0)
            coop.stopWatchingForRoom(hostId)
        coop.closeGuestFlow()
    }

    Timer {
        interval: 1000
        repeat: true
        running: root.waitStartedMs > 0
        onTriggered: {
            root.nowMs = Date.now()
            if (root.waitLeft === 0)
                root.leave()
        }
    }

    Keys.onPressed: event => {
        if (event.key !== Qt.Key_Escape && event.key !== Qt.Key_Back)
            return
        event.accepted = true
        if (event.isAutoRepeat)
            return
        if (root.streamLost && root.waitStartedMs === 0)
            root.startWaiting()
        else
            root.leave()
    }

    Column {
        x: 520; y: 300
        width: 600
        spacing: 10

        DesktopCoopGuestText {
            text: root.reason === "declined" ? qsTr("NOT LET IN") : root.reason === "kicked" ? qsTr("REMOVED")
                : root.mediaUnavailable ? qsTr("CAN'T PLAY HERE")
                : root.streamLost ? qsTr("STREAM LOST") : qsTr("CO-OP ENDED")
            color: root.removed || root.mediaUnavailable ? "#FFD166" : root.streamLost ? "#FF8A80" : "#7FD4FF"
            mono: true
            size: 11
            lineHeightPx: 14
            tracking: 0.18
        }
        DesktopCoopGuestText {
            id: titleText
            width: parent.width
            wrapMode: Text.WordWrap
            text: root.reason === "declined"
                ? (root.host !== "" ? qsTr("%1 didn't let you in").arg(root.host) : qsTr("The host didn't let you in"))
                : root.mediaUnavailable
                ? (root.host !== "" ? qsTr("This device can't play %1's stream").arg(root.host) : qsTr("This device can't play the host's stream"))
                : root.reason === "kicked"
                ? (root.host !== "" ? qsTr("You left %1's game").arg(root.host) : qsTr("You left the game"))
                : root.streamLost
                ? (root.host !== "" ? qsTr("%1's stream stopped").arg(root.host) : qsTr("The host's stream stopped"))
                : (root.host !== "" ? qsTr("%1 ended co-op").arg(root.host) : qsTr("The host ended co-op"))
            size: 32
            lineHeightPx: 40
            tracking: -0.01
            font.weight: Font.Black
            Accessible.role: Accessible.Heading
        }
        DesktopCoopGuestText {
            id: bodyText
            width: parent.width
            wrapMode: Text.WordWrap
            text: {
                if (root.waitStartedMs > 0)
                    return root.host !== "" ? qsTr("Waiting for %1 to come back. You'll see a notice when there's room.").arg(root.host)
                        : qsTr("Waiting for the host to come back. You'll see a notice when there's room.")
                if (root.mediaUnavailable)
                    return root.host !== "" ? qsTr("Your device couldn't decode the video, so you left the lobby. %1 keeps playing.").arg(root.host)
                        : qsTr("Your device couldn't decode the video, so you left the lobby. The host keeps playing.")
                if (root.reason === "declined")
                    return root.host !== "" ? qsTr("%1 approves everyone who joins with a code. Ask them for an invite instead.").arg(root.host)
                        : qsTr("This host approves everyone who joins with a code. Ask them for an invite instead.")
                if (root.reason === "kicked")
                    return root.host !== "" ? qsTr("%1 removed you from the lobby. A new code lets you back in.").arg(root.host)
                        : qsTr("The host removed you from the lobby. A new code lets you back in.")
                if (root.reason === "stream_lost")
                    return root.host !== "" ? qsTr("GeForce NOW ended %1's session, so co-op closed for everyone. Nothing went wrong on your side.").arg(root.host)
                        : qsTr("GeForce NOW ended the host's session, so co-op closed for everyone. Nothing went wrong on your side.")
                if (root.reason === "host_lost")
                    return root.host !== "" ? qsTr("%1's connection dropped, so co-op closed for everyone. Nothing went wrong on your side.").arg(root.host)
                        : qsTr("The host's connection dropped, so co-op closed for everyone. Nothing went wrong on your side.")
                return root.host !== "" ? qsTr("Thanks for playing. %1 keeps streaming on their own.").arg(root.host)
                    : qsTr("Thanks for playing. The host keeps streaming on their own.")
            }
            color: "#A3FFFFFF"
            size: 14
            lineHeightPx: 21
            font.weight: Font.Medium
        }
        DesktopCoopGuestText {
            objectName: "coopEndedDetail"
            visible: root.mediaUnavailable && text !== ""
            width: parent.width
            wrapMode: Text.WrapAnywhere
            textFormat: Text.PlainText
            text: String(root.coop.endDetail || "")
            color: "#66FFFFFF"
            mono: true
            size: 11
            lineHeightPx: 15
        }
        Item { width: 1; height: 2 }
        Row {
            spacing: 8
            DesktopCoopGuestButton {
                id: primaryButton
                focus: true
                primary: true
                glyph: "A"
                text: root.removed ? qsTr("OK") : root.streamLost ? qsTr("Leave") : qsTr("Back to Home")
                KeyNavigation.right: waitButton.visible ? waitButton : null
                onClicked: root.leave()
            }
            DesktopCoopGuestButton {
                id: waitButton
                visible: root.streamLost
                enabled: root.waitStartedMs === 0
                glyph: root.waitStartedMs === 0 ? "B" : ""
                text: root.waitStartedMs > 0
                    ? qsTr("Waiting %1:%2").arg(Math.floor(root.waitLeft / 60)).arg(String(root.waitLeft % 60).padStart(2, "0"))
                    : root.host !== "" ? qsTr("Wait 2 min for %1").arg(root.host) : qsTr("Wait 2 min for the host")
                KeyNavigation.left: primaryButton
                onClicked: root.startWaiting()
            }
        }
    }
}

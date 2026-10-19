import QtQuick
import OpenNOW

FocusScope {
    id: root
    objectName: "coopConnecting"
    required property var coop
    readonly property string host: String(coop.hostName || "")
    readonly property string stage: coop.guestPhase === "joining" ? "negotiating" : String(coop.connectStage || "negotiating")
    readonly property bool blocked: stage === "direct_blocked" || stage === "relay" || coop.guestPath === "relay"
    readonly property var game: coop.game
    focus: true
    Accessible.role: Accessible.Pane
    Accessible.name: statusText.text
    Accessible.description: bodyText.text

    function stepState(index) {
        const order = ["negotiating", "trying_direct", "direct_blocked", "trying_relay", "relay", "connected"]
        const reached = Math.max(0, order.indexOf(stage))
        if (index === 2 && blocked)
            return "blocked"
        if (index === 0)
            return reached === 0 ? "current" : "done"
        if (index === 1)
            return reached >= 1 ? "done" : "pending"
        if (index === 2)
            return stage === "trying_direct" || stage === "trying_relay" ? "current" : stage === "connected" ? "done" : "pending"
        if (index === 3)
            return stage === "relay" || stage === "trying_relay" ? "current" : stage === "connected" && blocked ? "done" : "pending"
        return stage === "connected" ? "done" : "pending"
    }
    readonly property var steps: [qsTr("HOST"), qsTr("STUN"), blocked ? qsTr("BLOCKED") : qsTr("DIRECT"), qsTr("RELAY"), qsTr("IN")]

    Keys.onPressed: event => {
        if (event.key === Qt.Key_Escape || event.key === Qt.Key_Back) {
            event.accepted = true
            if (!event.isAutoRepeat)
                coop.closeGuestFlow()
        }
    }

    Column {
        x: 520; y: 300
        width: 620

        DesktopCoopGuestText {
            text: {
                const source = root.coop.joinContext ? String(root.coop.joinContext.source || "") : ""
                if (root.host === "")
                    return qsTr("JOINING CO-OP")
                if (source === "invite")
                    return qsTr("%1 INVITED YOU · JOINING THEIR CO-OP").arg(root.host.toUpperCase())
                return qsTr("JOINING %1'S CO-OP").arg(root.host.toUpperCase())
            }
            color: "#7FD4FF"
            mono: true
            size: 11
            lineHeightPx: 14
            tracking: 0.18
        }
        Item { width: 1; height: 14 }
        DesktopCoopGuestText {
            width: parent.width
            text: root.game && root.game.title ? String(root.game.title)
                : root.coop.enteredCode !== "" ? qsTr("Code %1").arg(root.coop.enteredCode) : qsTr("Co-op")
            elide: Text.ElideRight
            size: 44
            lineHeightPx: 54
            tracking: -0.01
            font.weight: Font.Black
            Accessible.role: Accessible.Heading
        }
        Item { width: 1; height: 30 }
        Row {
            height: 24
            spacing: 14
            DesktopCoopGuestSpinner {
                anchors.verticalCenter: parent.verticalCenter
                size: 20
                visible: root.stage !== "connected"
            }
            DesktopCoopGuestText {
                id: statusText
                anchors.verticalCenter: parent.verticalCenter
                text: root.stage === "trying_direct" ? qsTr("Trying a direct link")
                    : root.stage === "direct_blocked" ? qsTr("Direct link blocked")
                    : root.stage === "trying_relay" ? qsTr("Trying a relay…")
                    : root.stage === "relay" ? qsTr("Connecting through relay")
                    : root.stage === "connected" ? qsTr("Connected")
                    : root.host !== "" ? qsTr("Reaching %1").arg(root.host) : qsTr("Reaching the host")
                size: 19
                lineHeightPx: 24
            }
        }
        Item { width: 1; height: 12 }
        DesktopCoopGuestText {
            id: bodyText
            width: parent.width
            wrapMode: Text.WordWrap
            text: root.stage === "trying_direct" ? qsTr("A direct link to the host has the least delay.")
                : root.stage === "direct_blocked" ? qsTr("Your network blocked a direct link. Switching to a relay.")
                : root.stage === "trying_relay" ? qsTr("The direct link is slow to answer, so OpenNOW is also trying a relay.")
                : root.stage === "relay" ? qsTr("Your network blocked a direct link, so OpenNOW is using a relay. It adds a little delay.")
                : root.stage === "connected" ? qsTr("You're in. Waiting for the host to start.")
                : qsTr("Setting up a link to the host's PC.")
            color: "#A3FFFFFF"
            size: 14
            lineHeightPx: 21
            font.weight: Font.Medium
        }
        Item { width: 1; height: 26 }
        Row {
            spacing: 8
            Accessible.role: Accessible.ProgressBar
            Accessible.name: qsTr("Connection steps")
            Repeater {
                model: root.steps
                delegate: Column {
                    id: step
                    required property string modelData
                    required property int index
                    readonly property string kind: root.stepState(index)
                    width: 72
                    spacing: 7
                    Rectangle {
                        width: parent.width
                        height: 4
                        radius: 2
                        color: step.kind === "done" ? "#6EE7B7" : step.kind === "current" ? "#7FD4FF"
                            : step.kind === "blocked" ? "#F5A623" : "#24FFFFFF"
                    }
                    DesktopCoopGuestText {
                        text: step.modelData
                        color: step.kind === "blocked" ? "#F5A623" : step.kind === "pending" ? "#52FFFFFF" : "#A3FFFFFF"
                        mono: true
                        size: 9
                        lineHeightPx: 12
                        tracking: 0.08
                    }
                }
            }
        }
        Item { width: 1; height: 34 }
        DesktopCoopGuestButton {
            id: cancelButton
            focus: true
            text: qsTr("Cancel")
            glyph: "B"
            onClicked: root.coop.closeGuestFlow()
        }
    }
}

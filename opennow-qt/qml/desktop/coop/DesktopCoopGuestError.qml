import QtQuick
import OpenNOW

FocusScope {
    id: root
    objectName: "coopJoinError"
    required property var coop
    readonly property var failure: coop.joinFailure || ({kind: "cant_connect", eyebrow: "", title: "", body: ""})
    readonly property string kind: String(failure.kind || "")
    readonly property string primaryText: kind === "invalid" ? qsTr("Edit code")
        : kind === "ended" ? qsTr("New code")
        : kind === "full" || kind === "cant_connect" ? qsTr("Try again") : ""
    focus: true
    Accessible.role: Accessible.AlertMessage
    Accessible.name: String(failure.title || "")
    Accessible.description: String(failure.body || "")

    function runPrimary() {
        if (kind === "invalid" || kind === "ended")
            coop.editCode()
        else
            coop.retryJoin()
    }

    Keys.onPressed: event => {
        if (event.key === Qt.Key_Escape || event.key === Qt.Key_Back) {
            event.accepted = true
            if (!event.isAutoRepeat)
                coop.closeGuestFlow()
        }
    }

    Column {
        x: 520; y: 300
        width: 560
        spacing: 10

        DesktopCoopGuestText {
            text: String(root.failure.eyebrow || "")
            color: "#FF8A80"
            mono: true
            size: 11
            lineHeightPx: 14
            tracking: 0.18
        }
        DesktopCoopGuestText {
            width: parent.width
            wrapMode: Text.WordWrap
            text: String(root.failure.title || "")
            size: 32
            lineHeightPx: 40
            tracking: -0.01
            font.weight: Font.Black
            Accessible.role: Accessible.Heading
        }
        DesktopCoopGuestText {
            width: parent.width
            wrapMode: Text.WordWrap
            text: String(root.failure.body || "")
            color: "#A3FFFFFF"
            size: 14
            lineHeightPx: 21
            font.weight: Font.Medium
        }
        Item { width: 1; height: 4 }
        Row {
            spacing: 8
            DesktopCoopGuestButton {
                id: primaryButton
                visible: root.primaryText !== ""
                focus: visible
                primary: true
                text: root.primaryText
                glyph: "A"
                KeyNavigation.right: backButton
                onClicked: root.runPrimary()
            }
            DesktopCoopGuestButton {
                id: backButton
                focus: !primaryButton.visible
                primary: !primaryButton.visible
                text: qsTr("Back")
                glyph: "B"
                KeyNavigation.left: primaryButton.visible ? primaryButton : null
                onClicked: root.coop.closeGuestFlow()
            }
        }
    }
}

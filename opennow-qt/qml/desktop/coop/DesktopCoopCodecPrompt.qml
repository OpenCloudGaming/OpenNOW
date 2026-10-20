import QtQuick
import OpenNOW

// Host notice: a guest can't decode the running codec. A running session keeps its codec, so the
// answer is a restart with a codec everyone can play, through the end-session confirmation.
Rectangle {
    id: root
    objectName: "coopCodecPrompt"
    required property var prompt
    readonly property var coop: ShellStore.coop
    readonly property string name: String(prompt.member.displayName || "")
    readonly property string codec: coop.codecName(prompt.codec)
    readonly property string best: prompt.best !== "" ? coop.codecName(prompt.best) : ""

    width: 384
    height: Math.max(40, content.implicitHeight) + 32
    radius: 20
    color: "#F00E1018"
    border.width: 1
    border.color: "#66F5A623"
    Accessible.role: Accessible.AlertMessage
    Accessible.name: qsTr("%1. %2").arg(heading.text).arg(detail.text)

    DesktopCoopNoticeAvatar {
        x: 17
        y: 15
        user: ({id: root.prompt.member.userId, displayName: root.name})
    }
    Column {
        id: content
        x: 69
        y: 15
        width: parent.width - x - 17
        spacing: 3
        Text {
            id: heading
            width: parent.width
            text: qsTr("%1's device can't play %2").arg(root.name).arg(root.codec)
            color: "#FFFFFF"
            font.family: DesktopTokens.bodyFont
            font.pixelSize: 15
            font.weight: Font.ExtraBold
            elide: Text.ElideRight
        }
        Text {
            id: detail
            objectName: "coopCodecPromptBody"
            width: parent.width
            text: root.best !== ""
                ? qsTr("Restart the game with %1? Everyone in the lobby can play it, and the game restarts.").arg(root.best)
                : qsTr("No video format works for everyone in the lobby, so %1 can't see the game.").arg(root.name)
            color: "#A8FFFFFF"
            font.family: DesktopTokens.bodyFont
            font.pixelSize: 13
            lineHeightMode: Text.FixedHeight
            lineHeight: 18
            wrapMode: Text.WordWrap
        }
        Row {
            topPadding: 8
            spacing: 8
            DesktopCoopToastButton {
                objectName: "coopCodecRestart"
                visible: root.best !== ""
                compact: true
                primary: true
                text: qsTr("Restart with %1").arg(root.best)
                onClicked: root.coop.requestCodecRestart(root.prompt.best)
            }
            DesktopCoopToastButton {
                objectName: "coopCodecNotNow"
                compact: true
                quiet: root.best !== ""
                text: root.best !== "" ? qsTr("Not now") : qsTr("OK")
                onClicked: root.coop.dismissCodecPrompt()
            }
        }
    }
}

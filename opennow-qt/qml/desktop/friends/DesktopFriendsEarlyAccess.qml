import QtQuick
import OpenNOW

// Early-access card over the Friends tab: opens by itself until the linked user dismisses it, and
// again from the header's Early access pill. Got it (Enter, A) and Esc (B) close it.
FocusScope {
    id: card
    objectName: "friendsEarlyAccess"
    property bool opened: false
    readonly property string discordUrl: "https://discord.gg/8EJYaJcNfD"
    readonly property bool present: reveal.present
    signal closeRequested()
    visible: present
    enabled: opened
    focus: opened
    MotionProgress { id: reveal; shown: card.opened }
    Accessible.role: Accessible.Dialog
    Accessible.name: title.text
    Accessible.description: body.text
    onOpenedChanged: if (opened) Qt.callLater(() => { if (card.opened) gotIt.forceActiveFocus() })
    Keys.onPressed: event => {
        if (event.key === Qt.Key_Escape || event.key === Qt.Key_Back) {
            event.accepted = true
            if (!event.isAutoRepeat)
                card.closeRequested()
        }
    }

    Rectangle {
        anchors.fill: parent
        color: "#B805070C"
        opacity: reveal.progress
        MouseArea { anchors.fill: parent; onClicked: card.closeRequested() }
    }

    Rectangle {
        id: sheet
        anchors.centerIn: parent
        width: Math.min(DesktopTokens.px(560), card.width - DesktopTokens.px(48))
        height: content.implicitHeight + DesktopTokens.px(60)
        radius: DesktopTokens.px(22)
        color: "#F00E1018"
        border.width: 1
        border.color: "#24FFFFFF"
        opacity: reveal.progress
        scale: reveal.zoom
        clip: true
        MouseArea { anchors.fill: parent }

        Rectangle {
            width: parent.width
            height: DesktopTokens.px(3)
            color: DesktopFriendsFormat.amber
        }

        Column {
            id: content
            x: DesktopTokens.px(30)
            y: DesktopTokens.px(30)
            width: parent.width - DesktopTokens.px(60)
            spacing: DesktopTokens.px(12)

            DesktopFriendsTag {
                objectName: "friendsEarlyAccessTag"
                text: qsTr("Early access")
                tone: DesktopFriendsFormat.amber
            }
            DesktopFriendsLabel {
                id: title
                width: parent.width
                text: qsTr("Friends and co-op are brand new")
                size: 28; line: 34
                color: "#FFFFFF"
                font.family: DesktopTokens.displayFont
                font.weight: Font.Black
                wrapMode: Text.WordWrap
                Accessible.role: Accessible.Heading
            }
            DesktopFriendsLabel {
                id: body
                objectName: "friendsEarlyAccessBody"
                width: parent.width
                text: qsTr("This is a very early version. Expect bugs, dropped connections and things that change between updates. When something breaks, tell us on Discord.")
                size: 15; line: 22
                color: "#A3FFFFFF"
                wrapMode: Text.WordWrap
            }
            Column {
                width: parent.width
                topPadding: DesktopTokens.px(4)
                spacing: DesktopTokens.px(8)
                Repeater {
                    model: [{glyph: "alert", text: qsTr("Bugs and rough edges")},
                        {glyph: "link", text: qsTr("Co-op links that drop now and then")},
                        {glyph: "clock", text: qsTr("Features that change between updates")}]
                    delegate: Row {
                        id: point
                        required property var modelData
                        spacing: DesktopTokens.px(10)
                        Rectangle {
                            width: DesktopTokens.px(26); height: width; radius: DesktopTokens.px(8)
                            color: DesktopFriendsFormat.tint(DesktopFriendsFormat.amber, 0.12)
                            DesktopFriendsIcon {
                                anchors.centerIn: parent
                                width: DesktopTokens.px(14); height: width
                                glyph: point.modelData.glyph
                                ink: DesktopFriendsFormat.amber
                            }
                        }
                        DesktopFriendsLabel {
                            anchors.verticalCenter: parent.verticalCenter
                            text: point.modelData.text
                            size: 14; line: 18
                            color: "#FFFFFF"
                            font.weight: Font.Bold
                        }
                    }
                }
            }
            Item { width: 1; height: DesktopTokens.px(6) }
            Row {
                anchors.right: parent.right
                spacing: DesktopTokens.px(10)
                DesktopFriendsButton {
                    id: discordButton
                    objectName: "friendsEarlyAccessDiscord"
                    kind: "discord"
                    glyph: "discord"; glyphSize: 16
                    text: qsTr("Report on Discord")
                    controlHeight: 42; cornerRadius: 12; sidePadding: 18; fontSize: 14
                    KeyNavigation.right: gotIt
                    KeyNavigation.tab: gotIt
                    Keys.onReturnPressed: clicked()
                    Keys.onEnterPressed: clicked()
                    onClicked: AppController.openExternalUrl(card.discordUrl)
                }
                DesktopFriendsButton {
                    id: gotIt
                    objectName: "friendsEarlyAccessGotIt"
                    kind: "accent"
                    text: qsTr("Got it")
                    controlHeight: 42; cornerRadius: 12; sidePadding: 24; fontSize: 14
                    KeyNavigation.left: discordButton
                    KeyNavigation.backtab: discordButton
                    Keys.onReturnPressed: clicked()
                    Keys.onEnterPressed: clicked()
                    onClicked: card.closeRequested()
                }
            }
        }
    }
}

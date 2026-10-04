import QtQuick
import QtQuick.Controls
import OpenNOW

FocusScope {
    id: root
    objectName: "desktopBugReportNotice"
    property bool opened: false
    visible: reveal.present
    enabled: opened
    focus: opened
    onOpenedChanged: if (opened) Qt.callLater(root.restoreFocus)
    MotionProgress { id: reveal; shown: root.opened }
    Accessible.role: Accessible.Dialog
    Accessible.name: qsTr("Help fix OpenNOW faster")

    readonly property var items: [
        qsTr("Your GeForce NOW username, or your account e-mail if you have no username, and your provider"),
        qsTr("The game you're playing and your recent games"),
        qsTr("Session errors, repeated frame drops, and failures such as the library not loading"),
        qsTr("Diagnostic logs with tokens, URLs, and local paths removed"),
        qsTr("Usage statistics such as app starts, game sessions, and stream quality")
    ]

    function choose(enabled) {
        ShellStore.bugReports.setEnabled(enabled, "first_run_sheet")
    }

    function restoreFocus() {
        if (root.opened && !keepButton.activeFocus && !turnOffButton.activeFocus)
            keepButton.forceActiveFocus()
    }

    Keys.onPressed: event => {
        if (event.key === Qt.Key_Tab || event.key === Qt.Key_Backtab) {
            if (keepButton.activeFocus) turnOffButton.forceActiveFocus()
            else keepButton.forceActiveFocus()
        } else if (event.key === Qt.Key_Left) {
            turnOffButton.forceActiveFocus()
        } else if (event.key === Qt.Key_Right) {
            keepButton.forceActiveFocus()
        } else if (event.key === Qt.Key_Up || event.key === Qt.Key_Down
                   || event.key === Qt.Key_PageUp || event.key === Qt.Key_PageDown) {
            const direction = event.key === Qt.Key_Up || event.key === Qt.Key_PageUp ? -1 : 1
            const distance = event.key === Qt.Key_PageUp || event.key === Qt.Key_PageDown
                ? scroll.height : DesktopTokens.px(40)
            scroll.contentItem.contentY = Math.max(0, Math.min(scroll.contentHeight - scroll.height,
                scroll.contentItem.contentY + direction * distance))
        } else if (event.key === Qt.Key_Escape || event.key === Qt.Key_Back) {
            root.choose(true)
        } else if (event.key === Qt.Key_Return || event.key === Qt.Key_Enter) {
            root.choose(!turnOffButton.activeFocus)
        } else if (event.key === Qt.Key_Space) {
            return
        }
        event.accepted = true
    }

    Rectangle {
        anchors.fill: parent
        color: Qt.rgba(0, 0, 0, 0.62)
        opacity: reveal.progress
        MouseArea { anchors.fill: parent; hoverEnabled: true; acceptedButtons: Qt.AllButtons }
    }

    Rectangle {
        id: sheet
        objectName: "bugReportNoticeSheet"
        anchors.centerIn: parent
        width: Math.min(DesktopTokens.px(560), root.width - DesktopTokens.px(48))
        height: Math.min(root.height - DesktopTokens.px(48),
            content.implicitHeight + footer.implicitHeight + DesktopTokens.px(70))
        radius: DesktopTokens.px(22)
        color: Theme.shell
        border.width: 1
        border.color: DesktopTokens.seam
        opacity: reveal.progress
        transform: Translate { y: (1 - reveal.progress) * DesktopTokens.px(16) }

        ScrollView {
            id: scroll
            objectName: "bugReportNoticeScroll"
            x: DesktopTokens.px(28)
            y: DesktopTokens.px(28)
            width: parent.width - DesktopTokens.px(56)
            height: parent.height - footer.implicitHeight - DesktopTokens.px(70)
            contentWidth: availableWidth
            contentHeight: content.implicitHeight
            clip: true
            ScrollBar.horizontal.policy: ScrollBar.AlwaysOff
            Column {
                id: content
                width: scroll.availableWidth
                spacing: DesktopTokens.px(14)

                Rectangle {
                    width: pillText.implicitWidth + DesktopTokens.px(16)
                    height: DesktopTokens.px(22)
                    radius: height / 2
                    color: "#29FFD166"
                    Text {
                        id: pillText
                        anchors.centerIn: parent
                        text: qsTr("EXPERIMENTAL · OPT-OUT")
                        color: DesktopTokens.amber
                        font.family: DesktopTokens.monoFont
                        font.pixelSize: DesktopTokens.microSize
                        font.weight: Font.Bold
                        font.letterSpacing: 1.2 * DesktopTokens.uiScale
                    }
                }
                Text {
                    width: parent.width
                    text: qsTr("Help fix OpenNOW faster")
                    color: DesktopTokens.text
                    font.family: DesktopTokens.displayFont
                    font.pixelSize: DesktopTokens.titleSize
                    font.weight: Font.Black
                    wrapMode: Text.WordWrap
                }
                Text {
                    width: parent.width
                    text: qsTr("OpenNOW is still experimental, so usage & bug reports are on by default. Usage statistics show what works, and when something breaks, a report goes straight to the developer.")
                    color: DesktopTokens.textBody
                    font.family: DesktopTokens.bodyFont
                    font.pixelSize: DesktopTokens.bodySize
                    wrapMode: Text.WordWrap
                    lineHeight: 1.25
                }
                Rectangle {
                    width: parent.width
                    height: includes.implicitHeight + DesktopTokens.px(28)
                    radius: DesktopTokens.px(14)
                    color: DesktopTokens.raised
                    border.width: 1
                    border.color: DesktopTokens.seamSoft
                    Column {
                        id: includes
                        x: DesktopTokens.px(16)
                        y: DesktopTokens.px(14)
                        width: parent.width - DesktopTokens.px(32)
                        spacing: DesktopTokens.px(10)
                        Text {
                            text: qsTr("WHAT IS SENT")
                            color: DesktopTokens.textFaint
                            font.family: DesktopTokens.monoFont
                            font.pixelSize: DesktopTokens.microSize
                            font.weight: Font.Bold
                            font.letterSpacing: 1.2 * DesktopTokens.uiScale
                        }
                        Repeater {
                            model: root.items
                            Row {
                                id: includeRow
                                required property string modelData
                                width: includes.width
                                spacing: DesktopTokens.px(10)
                                Rectangle {
                                    y: DesktopTokens.px(6)
                                    width: DesktopTokens.px(6); height: width; radius: width / 2
                                    color: DesktopTokens.mint
                                }
                                Text {
                                    width: parent.width - DesktopTokens.px(16)
                                    text: includeRow.modelData
                                    color: DesktopTokens.text
                                    font.family: DesktopTokens.bodyFont
                                    font.pixelSize: DesktopTokens.captionSize
                                    font.weight: Font.DemiBold
                                    wrapMode: Text.WordWrap
                                }
                            }
                        }
                    }
                }
            }
        }
        Column {
            id: footer
            x: DesktopTokens.px(28)
            y: scroll.y + scroll.height + DesktopTokens.px(14)
            width: parent.width - DesktopTokens.px(56)
            spacing: DesktopTokens.px(14)
            Item {
                width: parent.width
                height: DesktopTokens.controlHeight + DesktopTokens.px(6)
                Row {
                    anchors.right: parent.right
                    anchors.bottom: parent.bottom
                    spacing: DesktopTokens.px(10)
                    DesktopButton {
                        id: turnOffButton
                        objectName: "bugReportNoticeTurnOff"
                        KeyNavigation.priority: KeyNavigation.BeforeItem
                        KeyNavigation.tab: keepButton
                        KeyNavigation.backtab: keepButton
                        KeyNavigation.right: keepButton
                        height: DesktopTokens.controlHeight
                        font.pixelSize: DesktopTokens.captionSize
                        text: qsTr("Turn off")
                        onClicked: root.choose(false)
                    }
                    DesktopButton {
                        id: keepButton
                        objectName: "bugReportNoticeKeep"
                        KeyNavigation.priority: KeyNavigation.BeforeItem
                        KeyNavigation.tab: turnOffButton
                        KeyNavigation.backtab: turnOffButton
                        KeyNavigation.left: turnOffButton
                        height: DesktopTokens.controlHeight
                        font.pixelSize: DesktopTokens.captionSize
                        primary: true
                        text: qsTr("Keep reports on")
                        onClicked: root.choose(true)
                    }
                }
            }
            Text {
                width: parent.width
                text: qsTr("Change this anytime in Settings → Account → Privacy.")
                color: DesktopTokens.textFaint
                font.family: DesktopTokens.bodyFont
                font.pixelSize: DesktopTokens.smallSize
                wrapMode: Text.WordWrap
            }
        }
    }
}

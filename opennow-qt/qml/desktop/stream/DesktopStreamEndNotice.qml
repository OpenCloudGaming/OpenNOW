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
    StreamEndNoticeCopy { id: copy; notice: root.notice }
    readonly property color accent: copy.failure ? DesktopTokens.flightDeclined : DesktopTokens.focus
    visible: reveal.present
    enabled: ownsInput
    focus: ownsInput
    MotionProgress { id: reveal; shown: root.opened }
    Accessible.role: Accessible.Dialog
    Accessible.name: copy.headline
    Accessible.description: copy.explanation

    onOwnsInputChanged: if (ownsInput) Qt.callLater(root.restoreFocus)

    function restoreFocus() {
        if (root.ownsInput && !copyButton.activeFocus && !dismissButton.activeFocus)
            dismissButton.forceActiveFocus()
    }

    // Game details open in the same frame and focus their Play button; the
    // notice keeps input until it is dismissed.
    Connections {
        target: root.Window.window
        enabled: root.ownsInput
        function onActiveFocusItemChanged() { Qt.callLater(root.restoreFocus) }
    }

    Keys.onShortcutOverride: event => event.accepted = event.key !== Qt.Key_F11
    Keys.onPressed: event => {
        if (event.key === Qt.Key_Escape || event.key === Qt.Key_Back) {
            copy.dismiss()
        } else if (event.key === Qt.Key_Return || event.key === Qt.Key_Enter) {
            if (copyButton.activeFocus)
                copy.copyCode()
            else
                copy.dismiss()
        } else if (event.key === Qt.Key_Tab || event.key === Qt.Key_Backtab) {
            if (copy.codeCopyable)
                (copyButton.activeFocus ? dismissButton : copyButton).forceActiveFocus()
        } else if (event.key === Qt.Key_Up || event.key === Qt.Key_Left) {
            if (copy.codeCopyable)
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
                    text: copy.eyebrow
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
                text: copy.headline
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
                text: copy.explanation
                textFormat: Text.PlainText
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
                visible: copy.codeCopyable
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
                    text: copy.codeLine
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
                    themedGlyph: copy.copied ? "check" : "copy"
                    glyphSize: DesktopTokens.px(14)
                    text: copy.copyText
                    Accessible.name: copy.copyAccessibleName
                    onClicked: copy.copyCode()
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
                    visible: copy.quietCodeVisible
                    text: copy.quietCodeLine
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
                    text: copy.primaryText
                    shortcutText: qsTr("Esc")
                    onClicked: copy.dismiss()
                }
            }
        }
    }
}

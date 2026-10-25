import QtQuick
import OpenNOW

FocusScope {
    id: root
    objectName: "consoleStreamEndNotice"
    property bool opened: false
    StreamEndNoticeCopy { id: copy; notice: ShellStore.streamEndNotice }
    readonly property color toneColor: copy.failure ? Theme.coral : Theme.focus
    readonly property color toneInk: Theme.lightMode && copy.failure ? Theme.accentColor("coral", true) : root.toneColor
    readonly property bool present: reveal.present
    visible: reveal.present
    enabled: opened
    Accessible.role: Accessible.Dialog
    Accessible.name: copy.headline
    Accessible.description: copy.explanation
    MotionProgress {
        id: reveal
        shown: root.opened
        enterDuration: 200
        exitDuration: 160
    }

    onOpenedChanged: if (opened) Qt.callLater(() => { if (root.opened) dismissButton.forceActiveFocus() })

    Keys.onPressed: event => {
        if (!root.opened)
            return
        if (event.key === Qt.Key_Escape || event.key === Qt.Key_Back) {
            if (!event.isAutoRepeat) copy.dismiss()
        } else if (event.key === Qt.Key_X) {
            if (!event.isAutoRepeat) copy.copyCode()
        } else if (event.key === Qt.Key_Up) {
            if (copy.codeCopyable) copyButton.forceActiveFocus()
        } else if (event.key === Qt.Key_Down) {
            dismissButton.forceActiveFocus()
        } else if (event.key === Qt.Key_Tab || event.key === Qt.Key_Backtab) {
            if (copy.codeCopyable) (copyButton.activeFocus ? dismissButton : copyButton).forceActiveFocus()
        }
        event.accepted = true
    }
    Keys.onReleased: event => { if (root.opened) event.accepted = true }

    Rectangle {
        anchors.fill: parent
        color: Qt.rgba(0.02, 0.027, 0.047, 0.72)
        opacity: reveal.progress
        MouseArea {
            anchors.fill: parent
            enabled: root.opened
            acceptedButtons: Qt.AllButtons
            onWheel: wheel => wheel.accepted = true
        }
    }

    Rectangle {
        id: card
        objectName: "consoleStreamEndNoticeCard"
        anchors.centerIn: parent
        width: 780
        height: content.implicitHeight + 96
        radius: 40
        color: Theme.lightMode ? Qt.tint(Theme.shell, Qt.rgba(1, 1, 1, 0.72)) : Qt.tint(Theme.shell, Qt.rgba(1, 1, 1, 0.03))
        border.width: 1
        border.color: Theme.seam
        opacity: reveal.progress
        scale: reveal.zoom
        MouseArea { anchors.fill: parent; acceptedButtons: Qt.AllButtons }

        Column {
            id: content
            x: 56
            y: 56
            width: parent.width - 112

            Rectangle {
                anchors.horizontalCenter: parent.horizontalCenter
                width: 88
                height: 88
                radius: 44
                color: root.toneColor
                Text {
                    anchors.centerIn: parent
                    visible: copy.failure
                    text: "!"
                    color: Theme.contrastText(root.toneColor)
                    font.family: Theme.displayFont
                    font.pixelSize: 46
                    font.weight: Font.Black
                }
                DesktopSettingsIcon {
                    anchors.centerIn: parent
                    visible: !copy.failure
                    width: 40
                    height: 40
                    glyph: copy.kind === "idle" ? "moon" : "clock"
                    ink: Theme.contrastText(root.toneColor)
                }
            }
            Text {
                width: parent.width
                topPadding: 28
                horizontalAlignment: Text.AlignHCenter
                text: copy.eyebrow
                color: root.toneInk
                font.family: Theme.monoFont
                font.pixelSize: 14
                font.weight: Font.Bold
                font.letterSpacing: 2
            }
            Text {
                objectName: "consoleStreamEndNoticeTitle"
                width: parent.width
                topPadding: 12
                horizontalAlignment: Text.AlignHCenter
                text: copy.headline
                color: Theme.label
                wrapMode: Text.WordWrap
                maximumLineCount: 2
                elide: Text.ElideRight
                font.family: Theme.displayFont
                font.pixelSize: 44
                font.weight: Font.Black
                font.letterSpacing: -0.9
                lineHeightMode: Text.FixedHeight
                lineHeight: 48
            }
            Text {
                objectName: "consoleStreamEndNoticeBody"
                width: parent.width
                topPadding: 16
                horizontalAlignment: Text.AlignHCenter
                text: copy.explanation
                color: Qt.rgba(Theme.label.r, Theme.label.g, Theme.label.b, 0.76)
                wrapMode: Text.WordWrap
                maximumLineCount: 4
                elide: Text.ElideRight
                font.family: Theme.bodyFont
                font.pixelSize: 20
                font.weight: Font.DemiBold
                lineHeightMode: Text.FixedHeight
                lineHeight: 30
            }
            Item {
                width: 1
                height: 32
                visible: codeRow.visible
            }
            Rectangle {
                id: codeRow
                objectName: "consoleStreamEndNoticeCodeRow"
                visible: copy.codeCopyable
                width: parent.width
                height: 80
                radius: 24
                color: Qt.rgba(root.toneColor.r, root.toneColor.g, root.toneColor.b, 0.07)
                border.width: 1
                border.color: Qt.rgba(root.toneColor.r, root.toneColor.g, root.toneColor.b, 0.22)
                Text {
                    objectName: "consoleStreamEndNoticeCode"
                    x: 26
                    anchors.verticalCenter: parent.verticalCenter
                    width: copyButton.x - x - 16
                    text: copy.codeLine
                    color: Theme.label
                    elide: Text.ElideRight
                    font.family: Theme.monoFont
                    font.pixelSize: 22
                    font.weight: Font.DemiBold
                }
                ConsoleActionButton {
                    id: copyButton
                    objectName: "consoleStreamEndNoticeCopy"
                    anchors.right: parent.right
                    anchors.rightMargin: 10
                    anchors.verticalCenter: parent.verticalCenter
                    height: 60
                    glyph: "X"
                    glyphSize: 28
                    labelSize: 19
                    text: copy.copyText
                    Accessible.name: copy.copyAccessibleName
                    onClicked: if (root.opened) copy.copyCode()
                }
            }
            Item {
                width: 1
                height: 36
            }
            ConsoleActionButton {
                id: dismissButton
                objectName: "consoleStreamEndNoticeDismiss"
                width: parent.width
                height: 76
                focus: true
                primary: true
                glyph: "A"
                text: copy.primaryText
                onClicked: if (root.opened) copy.dismiss()
            }
            Text {
                objectName: "consoleStreamEndNoticeQuietCode"
                visible: copy.quietCodeVisible
                width: parent.width
                topPadding: 20
                horizontalAlignment: Text.AlignHCenter
                text: copy.quietCodeLine
                color: Qt.rgba(Theme.label.r, Theme.label.g, Theme.label.b, 0.40)
                font.family: Theme.monoFont
                font.pixelSize: 16
            }
            Item {
                width: 1
                height: 32
            }
            Row {
                anchors.horizontalCenter: parent.horizontalCenter
                spacing: 22
                ControllerGlyph { glyph: "A"; label: qsTr("Choose focused"); glyphSize: 28 }
                ControllerGlyph { glyph: "B"; label: copy.primaryText; glyphSize: 28 }
                ControllerGlyph { visible: copy.codeCopyable; glyph: "X"; label: qsTr("Copy code"); glyphSize: 28 }
            }
        }
    }
}

import QtQuick
import QtQuick.Controls
import OpenNOW

Dialog {
    id: root
    objectName: "tenBitWarningDialog"
    required property var settingsStore

    function notifySelection(previous, value) {
        if (previous === value || ["10bit_420", "10bit_444"].indexOf(value) < 0
                || settingsStore.settings.suppressTenBitWarning === true)
            return
        dontNotify.checked = false
        open()
    }

    parent: Overlay.overlay
    anchors.centerIn: parent
    width: Math.min(DesktopTokens.px(520), parent.width - DesktopTokens.px(32))
    contentWidth: width - leftPadding - rightPadding
    implicitHeight: header.implicitHeight + copy.implicitHeight + footer.implicitHeight
        + topPadding + bottomPadding
    padding: DesktopTokens.px(22)
    spacing: 0
    modal: true
    focus: true
    closePolicy: Popup.CloseOnEscape
    title: qsTr("10-bit color")
    header: Text {
        text: root.title
        padding: DesktopTokens.px(22)
        bottomPadding: 0
        color: Theme.label
        font.family: Theme.bodyFont
        font.pixelSize: DesktopTokens.px(20)
        font.weight: Font.ExtraBold
        wrapMode: Text.WordWrap
    }
    background: Rectangle {
        radius: DesktopTokens.px(16)
        color: Theme.shell
        border.color: Theme.seam
    }
    contentItem: Column {
        id: copy
        width: root.contentWidth
        spacing: DesktopTokens.px(16)
        Text {
            width: parent.width
            text: qsTr("10-bit color may cause stuttering on some systems. If you notice stuttering, switch back to 8-bit.")
            color: Theme.textMuted
            font.family: Theme.bodyFont
            font.pixelSize: DesktopTokens.bodySize
            wrapMode: Text.WordWrap
        }
        CheckBox {
            id: dontNotify
            objectName: "tenBitWarningDontNotify"
            width: parent.width
            text: qsTr("Don't notify me again")
            spacing: DesktopTokens.px(10)
            padding: DesktopTokens.px(4)
            indicator: Rectangle {
                x: dontNotify.leftPadding
                y: (dontNotify.height - height) / 2
                width: DesktopTokens.px(20)
                height: width
                radius: DesktopTokens.px(4)
                color: dontNotify.checked ? Theme.focus : "transparent"
                border.color: dontNotify.activeFocus ? Theme.focus : Theme.textMuted
                border.width: dontNotify.activeFocus ? 2 : 1
                Text {
                    anchors.centerIn: parent
                    text: "✓"
                    visible: dontNotify.checked
                    color: Theme.focusText
                    font.pixelSize: DesktopTokens.px(15)
                }
            }
            contentItem: Text {
                leftPadding: dontNotify.indicator.width + dontNotify.spacing
                text: dontNotify.text
                color: Theme.label
                font.family: Theme.bodyFont
                font.pixelSize: DesktopTokens.bodySize
                wrapMode: Text.WordWrap
                verticalAlignment: Text.AlignVCenter
            }
        }
    }
    footer: DialogButtonBox {
        padding: DesktopTokens.px(16)
        implicitHeight: DesktopTokens.controlHeight + topPadding + bottomPadding
        background: Item {}
        DesktopSettingsButton {
            objectName: "tenBitWarningDismiss"
            text: qsTr("Got it")
            primary: true
            DialogButtonBox.buttonRole: DialogButtonBox.AcceptRole
        }
    }
    onClosed: {
        if (dontNotify.checked) {
            settingsStore.applySetting("suppressTenBitWarning", true)
            settingsStore.setSetting("suppressTenBitWarning", true)
        }
    }
}

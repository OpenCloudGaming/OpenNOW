import QtQuick
import QtQuick.Controls
import OpenNOW

Popup {
    id: root
    property string settingKey: ""
    property string shortcutTitle: ""
    property string message: ""

    parent: Overlay.overlay
    anchors.centerIn: parent
    width: Math.min(520, parent ? parent.width - 40 : 520)
    padding: 24
    modal: true
    focus: true
    closePolicy: Popup.CloseOnEscape | Popup.CloseOnPressOutside
    onOpened: capture.forceActiveFocus()

    function edit(key, title) {
        settingKey = key
        shortcutTitle = title
        message = qsTr("Press the new shortcut. Escape cancels.")
        open()
    }

    DesktopSettingsShortcutBinding { id: binding }

    background: Rectangle {
        radius: 18
        color: DesktopTokens.surface
        border.width: 1
        border.color: Theme.seam
    }

    contentItem: FocusScope {
        id: capture
        implicitHeight: content.implicitHeight
        Keys.onShortcutOverride: event => { event.accepted = true }
        Keys.onPressed: event => {
            event.accepted = true
            if (event.isAutoRepeat)
                return
            if (event.key === Qt.Key_Escape) {
                root.close()
                return
            }
            const result = binding.validate(root.settingKey, event)
            if (result.error) {
                root.message = result.error
                return
            }
            ShellStore.setSetting(root.settingKey, result.chord)
            root.close()
        }
        Column {
            id: content
            width: parent.width
            spacing: 18
            Text {
                width: parent.width
                text: root.shortcutTitle
                color: Theme.label
                font.family: Theme.displayFont
                font.pixelSize: DesktopTokens.px(24)
                font.weight: Font.Bold
                wrapMode: Text.WordWrap
            }
            Rectangle {
                width: parent.width; height: 66; radius: 12
                color: DesktopTokens.raised
                border.color: Theme.focus; border.width: 2
                Text {
                    anchors.centerIn: parent
                    text: qsTr("Press a key combination…")
                    color: Theme.label
                    font.family: Theme.monoFont
                    font.pixelSize: DesktopTokens.px(16)
                }
            }
            Text {
                width: parent.width
                text: root.message
                color: Theme.textMuted
                font.family: Theme.bodyFont
                font.pixelSize: DesktopTokens.captionSize
                wrapMode: Text.WordWrap
            }
            DesktopSettingsButton {
                text: qsTr("Cancel")
                onClicked: root.close()
            }
        }
    }
}

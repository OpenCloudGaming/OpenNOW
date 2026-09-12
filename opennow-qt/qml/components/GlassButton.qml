import QtQuick
import QtQuick.Controls
import OpenNOW

Button {
    id: root
    property string glyph: "A"
    property string shortcutText: ""
    property bool primary: false
    property bool danger: false
    property bool currentItem: false
    highlighted: activeFocus || currentItem

    implicitHeight: 56
    leftPadding: 14
    rightPadding: 24
    focusPolicy: Qt.StrongFocus
    Accessible.name: I18n.source(text, I18n.revision) + (shortcutText !== "" ? " · " + shortcutText : "")
    Accessible.role: Accessible.Button

    Keys.onPressed: event => {
        if (event.key === Qt.Key_Return || event.key === Qt.Key_Enter) {
            if (!event.isAutoRepeat)
                root.click()
            event.accepted = true
        }
    }

    background: Rectangle {
        radius: height / 2
        color: root.primary ? Theme.face
                            : root.danger ? Qt.rgba(1, 0.28, 0.3, root.activeFocus ? 0.32 : 0.16)
                                          : root.highlighted ? Qt.rgba(1, 1, 1, 0.24) : Theme.glassStrong
        border.color: root.highlighted ? Theme.focus : root.danger ? Theme.coral : Theme.seam
        border.width: root.highlighted ? 4 : 1
        Behavior on color { ColorAnimation { duration: Theme.focusDuration } }
        Behavior on border.color { ColorAnimation { duration: Theme.focusDuration } }
    }

    contentItem: Row {
        spacing: 12
        ControllerGlyph {
            anchors.verticalCenter: parent.verticalCenter
            visible: root.glyph !== ""
            glyph: root.glyph
            label: ""
            glyphSize: 28
            glyphColor: root.primary ? Theme.faceText : Theme.face
        }
        Text {
            anchors.verticalCenter: parent.verticalCenter
            text: I18n.source(root.text, I18n.revision)
            color: root.primary ? Theme.faceText : root.danger ? Theme.coral : Theme.label
            font.family: Theme.bodyFont
            font.pixelSize: 17
            font.weight: Font.Bold
        }
        KeyboardGlyph {
            visible: root.shortcutText !== ""
            anchors.verticalCenter: parent.verticalCenter
            shortcut: root.shortcutText
            keySize: 26
            ink: root.primary ? Theme.faceText : Theme.label
        }
    }
}

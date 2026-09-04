import QtQuick
import QtQuick.Controls
import OpenNOW

Button {
    id: root
    property string glyph: "A"
    property bool primary: false
    property bool danger: false
    property bool currentItem: false
    highlighted: activeFocus || currentItem

    implicitHeight: 56
    leftPadding: 14
    rightPadding: 24
    focusPolicy: Qt.StrongFocus
    Accessible.name: I18n.source(text, I18n.revision)
    Accessible.role: Accessible.Button

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
        Rectangle {
            anchors.verticalCenter: parent.verticalCenter
            width: 28; height: 28; radius: 14
            color: root.primary ? Theme.faceText : Theme.face
            Text {
                anchors.centerIn: parent
                text: root.glyph
                color: root.primary ? Theme.face : Theme.faceText
                font.family: Theme.displayFont
                font.pixelSize: 12
                font.weight: Font.Black
            }
        }
        Text {
            anchors.verticalCenter: parent.verticalCenter
            text: I18n.source(root.text, I18n.revision)
            color: root.primary ? Theme.faceText : root.danger ? Theme.coral : Theme.label
            font.family: Theme.bodyFont
            font.pixelSize: 17
            font.weight: Font.Bold
        }
    }
}

import QtQuick
import QtQuick.Controls
import OpenNOW

Button {
    id: root
    property bool primary: false
    property bool danger: false
    property string glyph: ""
    property bool currentItem: false
    property real cornerRadius: Math.min(26, height / 2)
    property real labelSize: 22
    property int labelWeight: Font.Black
    property real glyphSize: 32
    property real contentSpacing: 14
    readonly property bool ringVisible: activeFocus || currentItem
    readonly property color fillColor: primary ? Theme.face
        : danger ? Qt.rgba(Theme.coral.r, Theme.coral.g, Theme.coral.b, 0.12)
        : Qt.rgba(Theme.face.r, Theme.face.g, Theme.face.b, 0.07)
    readonly property color dangerInk: Theme.lightMode ? Theme.accentColor("coral", true) : Theme.coral
    readonly property color inkColor: primary ? Theme.faceText : danger ? dangerInk : Theme.label

    implicitHeight: 72
    implicitWidth: Math.max(160, labelMetrics.advanceWidth + (glyph !== "" ? glyphSize + contentSpacing : 0) + leftPadding + rightPadding + 2)
    leftPadding: glyph !== "" ? 18 : 24
    rightPadding: 24
    topPadding: 0
    bottomPadding: 0
    focusPolicy: Qt.StrongFocus
    Accessible.role: Accessible.Button
    Accessible.name: text

    Keys.onPressed: event => {
        if (event.key !== Qt.Key_Return && event.key !== Qt.Key_Enter && event.key !== Qt.Key_Space)
            return
        event.accepted = true
        if (!event.isAutoRepeat && root.enabled)
            root.clicked()
    }
    Keys.onReleased: event => {
        if (event.key === Qt.Key_Return || event.key === Qt.Key_Enter || event.key === Qt.Key_Space)
            event.accepted = true
    }

    background: Item {
        Rectangle {
            id: halo
            anchors.fill: parent
            anchors.margins: -9
            radius: root.cornerRadius + 9
            color: "transparent"
            border.width: 5
            border.color: Qt.rgba(Theme.focus.r, Theme.focus.g, Theme.focus.b, 0.55)
            opacity: root.ringVisible ? 1 : 0
            Behavior on opacity { NumberAnimation { duration: Theme.focusDuration; easing.type: Easing.OutCubic } }
        }
        Rectangle {
            anchors.fill: parent
            anchors.margins: -4
            radius: root.cornerRadius + 4
            color: "transparent"
            border.width: 4
            border.color: Theme.shell
            opacity: root.ringVisible ? 1 : 0
        }
        Rectangle {
            anchors.fill: parent
            radius: root.cornerRadius
            color: root.fillColor
            opacity: root.enabled ? 1 : 0.42
            border.width: root.ringVisible ? 3 : root.danger ? 1.5 : 1
            border.color: root.ringVisible ? Theme.face
                : root.danger ? Qt.rgba(root.dangerInk.r, root.dangerInk.g, root.dangerInk.b, 0.6)
                : root.primary ? "transparent" : Theme.seam
            Behavior on color { ColorAnimation { duration: Theme.focusDuration } }
        }
    }

    TextMetrics {
        id: labelMetrics
        text: root.text
        font.family: Theme.displayFont
        font.pixelSize: root.labelSize
        font.weight: root.labelWeight
    }

    contentItem: Item {
        implicitWidth: contentRow.implicitWidth
        implicitHeight: contentRow.implicitHeight
        Row {
            id: contentRow
            anchors.verticalCenter: parent.verticalCenter
            spacing: root.contentSpacing
            opacity: root.enabled ? 1 : 0.5
            ControllerGlyph {
                anchors.verticalCenter: parent.verticalCenter
                visible: root.glyph !== ""
                glyph: root.glyph
                label: ""
                glyphSize: root.glyphSize
                glyphColor: root.primary ? Theme.faceText : Theme.face
            }
            Text {
                anchors.verticalCenter: parent.verticalCenter
                width: Math.max(0, Math.min(labelMetrics.advanceWidth + 2, root.availableWidth - (root.glyph !== "" ? root.glyphSize + root.contentSpacing : 0)))
                text: root.text
                color: root.inkColor
                elide: Text.ElideRight
                font.family: Theme.displayFont
                font.pixelSize: root.labelSize
                font.weight: root.labelWeight
            }
        }
    }
}

import QtQuick
import QtQuick.Controls
import OpenNOW

Button {
    id: root
    property bool primary: false
    property bool danger: false
    property string glyph: ""
    property bool currentItem: false
    readonly property bool ringVisible: activeFocus || currentItem
    readonly property color fillColor: primary ? Theme.face
        : danger ? Qt.rgba(Theme.coral.r, Theme.coral.g, Theme.coral.b, 0.12)
        : Qt.rgba(Theme.face.r, Theme.face.g, Theme.face.b, 0.07)
    readonly property color dangerInk: Theme.lightMode ? Theme.accentColor("coral", true) : Theme.coral
    readonly property color inkColor: primary ? Theme.faceText : danger ? dangerInk : Theme.label

    implicitHeight: 72
    implicitWidth: Math.max(160, labelMetrics.advanceWidth + (glyph !== "" ? 46 : 0) + leftPadding + rightPadding + 2)
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
            radius: height / 2 > 26 + 9 ? 26 + 9 : height / 2
            color: "transparent"
            border.width: 5
            border.color: Qt.rgba(Theme.focus.r, Theme.focus.g, Theme.focus.b, 0.55)
            opacity: root.ringVisible ? 1 : 0
            Behavior on opacity { NumberAnimation { duration: Theme.focusDuration; easing.type: Easing.OutCubic } }
        }
        Rectangle {
            anchors.fill: parent
            radius: Math.min(26, height / 2)
            color: root.fillColor
            opacity: root.enabled ? 1 : 0.42
            border.width: root.ringVisible ? 3 : root.danger ? 1.5 : 1
            border.color: root.ringVisible ? (root.primary ? Theme.shell : Theme.face)
                : root.danger ? Qt.rgba(root.dangerInk.r, root.dangerInk.g, root.dangerInk.b, 0.6)
                : root.primary ? "transparent" : Theme.seam
            Behavior on color { ColorAnimation { duration: Theme.focusDuration } }
        }
        Rectangle {
            visible: root.ringVisible && root.primary
            anchors.fill: parent
            anchors.margins: -4
            radius: Math.min(26, height / 2) + 4
            color: "transparent"
            border.width: 2
            border.color: Theme.face
        }
    }

    TextMetrics {
        id: labelMetrics
        text: root.text
        font.family: Theme.displayFont
        font.pixelSize: 22
        font.weight: Font.Black
    }

    contentItem: Item {
        implicitWidth: contentRow.implicitWidth
        implicitHeight: contentRow.implicitHeight
        Row {
            id: contentRow
            anchors.verticalCenter: parent.verticalCenter
            spacing: 14
            opacity: root.enabled ? 1 : 0.5
            ControllerGlyph {
                anchors.verticalCenter: parent.verticalCenter
                visible: root.glyph !== ""
                glyph: root.glyph
                label: ""
                glyphSize: 32
                glyphColor: root.primary ? Theme.faceText : Theme.face
            }
            Text {
                anchors.verticalCenter: parent.verticalCenter
                width: Math.max(0, Math.min(labelMetrics.advanceWidth + 2, root.availableWidth - (root.glyph !== "" ? 46 : 0)))
                text: root.text
                color: root.inkColor
                elide: Text.ElideRight
                font.family: Theme.displayFont
                font.pixelSize: 22
                font.weight: Font.Black
            }
        }
    }
}

import QtQuick
import OpenNOW

Item {
    id: root
    property bool opened: false
    property string panelSide: "right"
    property real panelWidth: 760
    property real panelInset: 0
    readonly property bool inset: panelInset > 0
    property color toneColor: "transparent"
    property real contentInset: 64
    property real contentTop: 72
    readonly property bool present: reveal.present
    readonly property real progress: reveal.progress
    default property alias content: body.data
    signal scrimClicked()

    anchors.fill: parent
    visible: reveal.present
    z: 200

    MotionProgress {
        id: reveal
        shown: root.opened
        enterDuration: 200
        exitDuration: 160
    }

    Rectangle {
        anchors.fill: parent
        color: Qt.rgba(0.02, 0.027, 0.047, 0.58)
        opacity: reveal.progress
        MouseArea {
            anchors.fill: parent
            enabled: root.opened
            onClicked: root.scrimClicked()
        }
    }

    Rectangle {
        id: panel
        objectName: "consoleSheetPanel"
        readonly property real travel: AppController.reducedMotion ? 0 : 48 * (1 - reveal.progress)
        width: root.panelWidth
        height: root.inset ? root.height - 2 * root.panelInset : root.height
        y: root.inset ? root.panelInset : 0
        x: root.inset ? root.panelInset - travel : root.panelSide === "left" ? -travel : root.width - width + travel
        opacity: reveal.progress
        color: Theme.lightMode ? Qt.tint(Theme.shell, Qt.rgba(1, 1, 1, 0.72)) : Qt.tint(Theme.shell, Qt.rgba(1, 1, 1, 0.03))
        radius: 40
        border.color: Theme.seam
        border.width: 1

        Rectangle {
            visible: !root.inset
            width: 44
            height: parent.height
            x: root.panelSide === "left" ? 0 : parent.width - width
            color: parent.color
        }
        Rectangle {
            visible: root.toneColor.a > 0 && !root.inset
            x: root.panelSide === "left" ? parent.width - width : 0
            y: 120
            width: 4
            height: 160
            radius: 2
            color: root.toneColor
        }
        MouseArea { anchors.fill: parent; acceptedButtons: Qt.AllButtons }

        Item {
            id: body
            anchors.fill: parent
            anchors.leftMargin: root.panelSide === "left" && !root.inset ? 72 : panel.border.width + root.contentInset
            anchors.rightMargin: root.panelSide === "left" || root.inset ? panel.border.width + root.contentInset : 72
            anchors.topMargin: root.inset ? panel.border.width + root.contentInset : root.contentTop
            anchors.bottomMargin: root.inset ? panel.border.width + root.contentInset : 56
        }
    }
}

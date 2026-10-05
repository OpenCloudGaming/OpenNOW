import QtQuick
import OpenNOW

Item {
    id: root
    property bool opened: false
    property string panelSide: "right"
    property real panelWidth: 760
    property color toneColor: "transparent"
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
        readonly property real travel: AppController.reducedMotion ? 0 : 48 * (1 - reveal.progress)
        width: root.panelWidth
        height: root.height
        x: root.panelSide === "left" ? -travel : root.width - width + travel
        opacity: reveal.progress
        color: Theme.lightMode ? Qt.tint(Theme.shell, Qt.rgba(1, 1, 1, 0.72)) : Qt.tint(Theme.shell, Qt.rgba(1, 1, 1, 0.03))
        radius: 40
        border.color: Theme.seam
        border.width: 1

        Rectangle {
            width: 44
            height: parent.height
            x: root.panelSide === "left" ? 0 : parent.width - width
            color: parent.color
        }
        Rectangle {
            visible: root.toneColor.a > 0
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
            anchors.leftMargin: root.panelSide === "left" ? 72 : 64
            anchors.rightMargin: root.panelSide === "left" ? 64 : 72
            anchors.topMargin: 72
            anchors.bottomMargin: 56
        }
    }
}

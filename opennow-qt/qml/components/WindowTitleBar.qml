import QtQuick
import QtQuick.Window
import OpenNOW

Rectangle {
    id: root
    required property Window targetWindow
    readonly property bool maximized: targetWindow.visibility === Window.Maximized
    readonly property color glyphColor: Theme.label
    readonly property color hoverFill: Theme.lightMode ? Qt.rgba(0.04, 0.06, 0.10, 0.08) : Qt.rgba(1, 1, 1, 0.10)

    function toggleMaximized() {
        if (root.maximized)
            root.targetWindow.showNormal()
        else
            root.targetWindow.showMaximized()
    }

    implicitHeight: 34
    color: Theme.shell
    layer.enabled: HdrOutput.chromeRequired
    layer.effect: HdrChromeEffect {}

    Rectangle {
        anchors.bottom: parent.bottom
        width: parent.width
        height: 1
        color: Theme.seam
    }

    Item {
        anchors.left: parent.left
        anchors.right: controls.left
        anchors.top: parent.top
        anchors.bottom: parent.bottom

        DragHandler {
            target: null
            onActiveChanged: if (active) root.targetWindow.startSystemMove()
        }
        TapHandler {
            onDoubleTapped: root.toggleMaximized()
        }

        Text {
            anchors.verticalCenter: parent.verticalCenter
            x: 14
            width: parent.width - x
            text: root.targetWindow.title
            elide: Text.ElideRight
            color: Theme.textMuted
            opacity: root.targetWindow.active ? 1 : 0.6
            font.family: Theme.bodyFont
            font.pixelSize: 13
            font.weight: Font.DemiBold
        }
    }

    component ControlButton: Item {
        id: button
        property bool destructive: false
        readonly property bool hovered: hover.hovered
        default property alias glyph: glyphArea.data
        signal activated()

        width: 46
        height: root.height - 1
        activeFocusOnTab: true
        enabled: visible
        Accessible.role: Accessible.Button
        Accessible.focusable: true
        Accessible.focused: activeFocus
        Accessible.onPressAction: button.activated()
        Keys.onPressed: event => {
            if (event.key === Qt.Key_Return || event.key === Qt.Key_Enter || event.key === Qt.Key_Space) {
                event.accepted = true
                if (!event.isAutoRepeat)
                    button.activated()
            }
        }
        Keys.onReleased: event => {
            if (event.key === Qt.Key_Return || event.key === Qt.Key_Enter || event.key === Qt.Key_Space)
                event.accepted = true
        }

        Rectangle {
            anchors.fill: parent
            color: !hover.hovered ? "transparent" : button.destructive ? "#E5484D" : root.hoverFill
        }
        Rectangle {
            objectName: "windowControlFocusRing"
            anchors.fill: parent
            anchors.margins: 3
            radius: 3
            color: "transparent"
            border.width: 2
            border.color: Theme.focus
            visible: button.activeFocus
        }
        Item {
            id: glyphArea
            anchors.centerIn: parent
            width: 12
            height: 12
            opacity: root.targetWindow.active || hover.hovered ? 1 : 0.55
        }
        HoverHandler { id: hover }
        TapHandler { onTapped: button.activated() }
    }

    Row {
        id: controls
        anchors.right: parent.right
        anchors.top: parent.top

        ControlButton {
            objectName: "windowMinimizeButton"
            Accessible.name: qsTr("Minimize")
            onActivated: root.targetWindow.showMinimized()
            Rectangle {
                anchors.centerIn: parent
                width: 11
                height: 1
                color: root.glyphColor
            }
        }

        ControlButton {
            objectName: "windowMaximizeButton"
            Accessible.name: root.maximized ? qsTr("Restore") : qsTr("Maximize")
            onActivated: root.toggleMaximized()
            Rectangle {
                visible: !root.maximized
                anchors.centerIn: parent
                width: 10
                height: 10
                color: "transparent"
                border.width: 1
                border.color: root.glyphColor
            }
            Rectangle {
                visible: root.maximized
                x: 3
                width: 8
                height: 1
                color: root.glyphColor
            }
            Rectangle {
                visible: root.maximized
                x: 10
                width: 1
                height: 8
                color: root.glyphColor
            }
            Rectangle {
                visible: root.maximized
                y: 3
                width: 8
                height: 8
                color: "transparent"
                border.width: 1
                border.color: root.glyphColor
            }
        }

        ControlButton {
            id: closeButton
            objectName: "windowCloseButton"
            destructive: true
            Accessible.name: qsTr("Close")
            onActivated: root.targetWindow.close()
            Repeater {
                model: [45, -45]
                Rectangle {
                    required property int modelData
                    anchors.centerIn: parent
                    width: 14
                    height: 1
                    antialiasing: true
                    rotation: modelData
                    color: closeButton.hovered ? "#FFFFFF" : root.glyphColor
                }
            }
        }
    }
}

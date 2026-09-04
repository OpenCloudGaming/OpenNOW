import QtQuick
import QtQuick.Controls
import OpenNOW

Row {
    id: root
    property alias from: slider.from
    property alias to: slider.to
    property alias stepSize: slider.stepSize
    property alias value: slider.value
    property string suffix: "%"
    property int decimals: 0
    signal moved(real value)
    signal committed(real value)
    spacing: 14

    Timer {
        id: commitTimer
        interval: 150
        repeat: false
        onTriggered: root.committed(slider.value)
    }

    Slider {
        id: slider
        width: DesktopTokens.px(200)
        height: DesktopTokens.px(28)
        live: true
        onMoved: {
            root.moved(value)
            if (!pressed)
                commitTimer.restart()
        }
        onPressedChanged: {
            if (!pressed) {
                commitTimer.stop()
                root.committed(value)
            }
        }
        background: Rectangle {
            x: slider.leftPadding
            y: slider.topPadding + slider.availableHeight / 2 - height / 2
            width: slider.availableWidth
            height: DesktopTokens.px(6)
            radius: DesktopTokens.px(3)
            color: Qt.rgba(1, 1, 1, 0.10)
            Rectangle {
                width: slider.visualPosition * parent.width
                height: parent.height
                radius: parent.radius
                color: slider.activeFocus ? DesktopTokens.focus : Qt.rgba(1, 1, 1, 0.76)
            }
        }
        handle: Rectangle {
            x: slider.leftPadding + slider.visualPosition * (slider.availableWidth - width)
            y: slider.topPadding + slider.availableHeight / 2 - height / 2
            width: DesktopTokens.px(14)
            height: DesktopTokens.px(14)
            radius: DesktopTokens.px(7)
            color: "#FFFFFF"
            border.width: slider.activeFocus ? 2 : 0
            border.color: DesktopTokens.focus
        }
    }
    Text {
        width: DesktopTokens.px(58)
        height: DesktopTokens.px(28)
        text: Number(slider.value).toFixed(root.decimals) + root.suffix
        color: Theme.label
        font.family: Theme.monoFont
        font.pixelSize: DesktopTokens.microSize
        font.weight: Font.Bold
        horizontalAlignment: Text.AlignRight
        verticalAlignment: Text.AlignVCenter
    }
}

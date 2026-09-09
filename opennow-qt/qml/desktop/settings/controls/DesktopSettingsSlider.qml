import QtQuick
import QtQuick.Controls
import OpenNOW

Row {
    id: root
    property alias from: slider.from
    property alias to: slider.to
    property alias stepSize: slider.stepSize
    property alias value: slider.value
    property real trackWidth: DesktopTokens.px(200)
    property string suffix: "%"
    property int decimals: 0
    property string accessibleName: ""
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
        Accessible.name: root.accessibleName
        width: root.trackWidth
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
            height: DesktopTokens.px(8)
            radius: DesktopTokens.px(3)
            color: DesktopTokens.raisedStrong
            Rectangle {
                width: slider.visualPosition * parent.width
                height: parent.height
                radius: parent.radius
                color: DesktopTokens.focus
            }
        }
        handle: Rectangle {
            x: slider.leftPadding + slider.visualPosition * (slider.availableWidth - width)
            y: slider.topPadding + slider.availableHeight / 2 - height / 2
            width: DesktopTokens.px(22)
            height: DesktopTokens.px(22)
            radius: DesktopTokens.px(11)
            color: Theme.label
            border.width: slider.activeFocus ? 2 : 0
            border.color: DesktopTokens.focus
        }
    }
    Text {
        width: DesktopTokens.px(84)
        height: DesktopTokens.px(28)
        text: Number(slider.value).toFixed(root.decimals) + root.suffix
        color: Theme.label
        font.family: Theme.monoFont
        font.pixelSize: DesktopTokens.px(14)
        font.weight: Font.Bold
        horizontalAlignment: Text.AlignRight
        verticalAlignment: Text.AlignVCenter
    }
}

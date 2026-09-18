import QtQuick
import QtQuick.Window
import OpenNOW

Item {
    id: root
    property real markHeight: DesktopTokens.px(16)
    property real fontPixelSize: DesktopTokens.px(18)
    property real spacing: DesktopTokens.px(10)
    property color ink: DesktopTokens.text
    property real textReveal: 1
    readonly property real markAspect: 362 / 208
    implicitWidth: mark.width + root.spacing + label.implicitWidth
    implicitHeight: Math.max(mark.height, label.implicitHeight)
    width: implicitWidth
    height: implicitHeight

    Image {
        id: mark
        anchors.verticalCenter: parent.verticalCenter
        height: root.markHeight
        width: Math.max(1, Math.round(height * root.markAspect))
        source: "qrc:/qt/qml/OpenNOW/res/brand/opennow-mark.png"
        fillMode: Image.PreserveAspectFit
        mipmap: true
        sourceSize: Qt.size(Math.ceil(width * Screen.devicePixelRatio), Math.ceil(height * Screen.devicePixelRatio))
    }
    Text {
        id: label
        anchors.verticalCenter: parent.verticalCenter
        anchors.left: mark.right
        anchors.leftMargin: root.spacing
        visible: root.textReveal > 0
        opacity: root.textReveal
        text: "OpenNOW"
        color: root.ink
        font.family: DesktopTokens.displayFont
        font.pixelSize: root.fontPixelSize
        font.weight: Font.Black
        font.letterSpacing: -root.fontPixelSize * 0.02
    }
}

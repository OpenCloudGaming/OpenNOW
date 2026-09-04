import QtQuick
import QtQuick.Window
import OpenNOW

Image {
    id: root
    property string icon: ""
    property bool active: false
    readonly property string fileName: {
        if (root.active && root.icon.indexOf("desktop-nav-") === 0)
            return root.icon.replace(".svg", "-active.svg")
        return root.icon
    }
    readonly property real dpr: Math.max(1, Screen.devicePixelRatio)

    source: root.icon === "" ? "" : "qrc:/qt/qml/OpenNOW/res/icons/" + root.fileName
    sourceSize: Qt.size(Math.max(1, Math.round(width * dpr)), Math.max(1, Math.round(height * dpr)))
    fillMode: Image.PreserveAspectFit
    smooth: false
    mipmap: false
    asynchronous: false
    cache: true
}

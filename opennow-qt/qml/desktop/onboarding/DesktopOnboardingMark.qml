import QtQuick
import QtQuick.Shapes
import OpenNOW

Item {
    id: root
    readonly property color ink: Theme.accentColor("green")
    Shape {
        width: 26; height: 16
        transform: Scale { xScale: root.width / 26; yScale: root.height / 16 }
        ShapePath {
            strokeWidth: 0; strokeColor: "transparent"; fillColor: root.ink
            PathSvg { path: "M10 15h11a4.5 4.5 0 0 0 .6-8.96A6 6 0 0 0 10.2 4.6 4.5 4.5 0 0 0 10 15Z" }
        }
    }
    Rectangle { x: 0; y: parent.height * 6 / 16; width: parent.width * 6 / 26; height: parent.height * 2 / 16; radius: height / 2; color: root.ink; opacity: 0.7 }
    Rectangle { x: parent.width * 2 / 26; y: parent.height * 10 / 16; width: parent.width * 5 / 26; height: parent.height * 2 / 16; radius: height / 2; color: root.ink; opacity: 0.45 }
}

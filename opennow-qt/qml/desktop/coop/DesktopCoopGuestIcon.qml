import QtQuick
import QtQuick.Shapes

Item {
    id: root
    property string path: ""
    property color stroke: "#FFFFFF"
    property real size: 16
    width: size
    height: size
    Accessible.ignored: true

    Shape {
        width: 24
        height: 24
        scale: root.size / 24
        transformOrigin: Item.TopLeft
        preferredRendererType: Shape.CurveRenderer
        ShapePath {
            strokeColor: root.stroke
            strokeWidth: 2
            fillColor: "transparent"
            capStyle: ShapePath.RoundCap
            joinStyle: ShapePath.RoundJoin
            PathSvg { path: root.path }
        }
    }
}

import QtQuick
import QtQuick.Shapes
import OpenNOW

Item {
    id: root
    property real size: 20
    width: size
    height: size
    Accessible.ignored: true

    Rectangle {
        anchors.fill: parent
        radius: width / 2
        color: "transparent"
        border.width: 2
        border.color: "#2EFFFFFF"
    }
    Shape {
        id: arc
        anchors.fill: parent
        preferredRendererType: Shape.CurveRenderer
        ShapePath {
            strokeColor: "#7FD4FF"
            strokeWidth: 2
            fillColor: "transparent"
            capStyle: ShapePath.FlatCap
            PathAngleArc {
                centerX: root.size / 2
                centerY: root.size / 2
                radiusX: root.size / 2 - 1
                radiusY: root.size / 2 - 1
                startAngle: -135
                sweepAngle: 90
            }
        }
        RotationAnimator on rotation {
            from: 0
            to: 360
            duration: 900
            loops: Animation.Infinite
            running: root.visible && !AppController.reducedMotion
        }
    }
}

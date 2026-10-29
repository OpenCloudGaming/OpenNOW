import QtQuick
import QtQuick.Window

Item {
    id: root
    required property Window targetWindow
    readonly property int grip: 6

    component Edge: MouseArea {
        required property int edges
        acceptedButtons: Qt.LeftButton
        onPressed: root.targetWindow.startSystemResize(edges)
    }

    Edge {
        edges: Qt.TopEdge
        cursorShape: Qt.SizeVerCursor
        x: root.grip * 2
        width: parent.width - root.grip * 4
        height: root.grip
    }
    Edge {
        edges: Qt.BottomEdge
        cursorShape: Qt.SizeVerCursor
        x: root.grip * 2
        y: parent.height - height
        width: parent.width - root.grip * 4
        height: root.grip
    }
    Edge {
        edges: Qt.LeftEdge
        cursorShape: Qt.SizeHorCursor
        y: root.grip * 2
        width: root.grip
        height: parent.height - root.grip * 4
    }
    Edge {
        edges: Qt.RightEdge
        cursorShape: Qt.SizeHorCursor
        x: parent.width - width
        y: root.grip * 2
        width: root.grip
        height: parent.height - root.grip * 4
    }
    Edge {
        edges: Qt.TopEdge | Qt.LeftEdge
        cursorShape: Qt.SizeFDiagCursor
        width: root.grip * 2
        height: root.grip * 2
    }
    Edge {
        edges: Qt.TopEdge | Qt.RightEdge
        cursorShape: Qt.SizeBDiagCursor
        x: parent.width - width
        width: root.grip * 2
        height: root.grip * 2
    }
    Edge {
        edges: Qt.BottomEdge | Qt.LeftEdge
        cursorShape: Qt.SizeBDiagCursor
        y: parent.height - height
        width: root.grip * 2
        height: root.grip * 2
    }
    Edge {
        edges: Qt.BottomEdge | Qt.RightEdge
        cursorShape: Qt.SizeFDiagCursor
        x: parent.width - width
        y: parent.height - height
        width: root.grip * 2
        height: root.grip * 2
    }
}

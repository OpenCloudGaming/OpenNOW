import QtQuick
import QtQuick.Shapes
import OpenNOW

Rectangle {
    id: root
    property string title: ""
    property string subtitle: ""
    property string controllerFamily: "controller"
    property bool warning: false
    property int batteryPercent: -1
    property var history: []
    property real lifetimeFraction: 1
    readonly property color accent: warning ? "#F5A623" : "#1DB954"
    width: 384
    height: 68
    radius: 20
    color: "#F00E1018"
    border.color: "#24FFFFFF"
    border.width: 1
    Accessible.role: Accessible.AlertMessage
    Accessible.name: title + ". " + subtitle + (battery.visible ? ". " + battery.text : "")

    Rectangle {
        x: 16
        anchors.verticalCenter: parent.verticalCenter
        width: 38; height: 38; radius: 12
        color: root.warning ? "#26F5A623" : "#241DB954"
        border.color: root.warning ? "#4DF5A623" : "#471DB954"
        Image {
            anchors.centerIn: parent
            width: 22; height: 22
            visible: !root.warning
            source: root.warning ? "" : InputPromptIcons.sourceFor(root.controllerFamily, "white")
            sourceSize: Qt.size(width * Screen.devicePixelRatio, height * Screen.devicePixelRatio)
            fillMode: Image.PreserveAspectFit
        }
        Shape {
            anchors.centerIn: parent
            width: 19; height: 19
            visible: root.warning
            ShapePath {
                strokeColor: root.accent
                strokeWidth: 1.35
                fillColor: "transparent"
                capStyle: ShapePath.RoundCap
                joinStyle: ShapePath.RoundJoin
                PathSvg { path: "M9.5 15.83h.01 M6.73 13a3.96 3.96 0 0 1 5.54 0 M3.96 10.18a7.92 7.92 0 0 1 4.1-2.13 M15.04 10.18a7.92 7.92 0 0 0-1.59-1.21 M1.58 6.98a11.88 11.88 0 0 1 3.31-2.09 M17.42 6.98a11.88 11.88 0 0 0-8.94-2.98 M1.58 1.58l15.84 15.84" }
            }
        }
    }

    Column {
        x: 66
        anchors.verticalCenter: parent.verticalCenter
        width: Math.max(0, root.width - x - 16 - (trailing.width > 0 ? trailing.width + 12 : 0))
        spacing: 3
        Text {
            width: parent.width
            text: root.title
            color: "white"
            font.family: Theme.bodyFont
            font.pointSize: 14.5 * 72 / (Screen.logicalPixelDensity * 25.4)
            font.weight: Font.ExtraBold
            elide: Text.ElideRight
        }
        Text {
            width: parent.width
            text: root.subtitle
            color: "#A8FFFFFF"
            font.family: Theme.bodyFont
            font.pixelSize: 13
            elide: Text.ElideRight
        }
    }

    Item {
        id: trailing
        anchors.right: parent.right
        anchors.rightMargin: 16
        anchors.verticalCenter: parent.verticalCenter
        width: root.warning ? 64 : battery.visible ? battery.implicitWidth + 29 : 0
        height: 22
        Text {
            id: battery
            objectName: "streamToastBattery"
            anchors.right: parent.right
            anchors.verticalCenter: parent.verticalCenter
            visible: !root.warning && root.batteryPercent >= 0 && root.batteryPercent <= 100
            text: root.batteryPercent + "%"
            color: "white"
            font.family: Theme.monoFont
            font.pixelSize: 12
            font.weight: Font.Medium
        }
        Image {
            anchors.left: parent.left
            anchors.verticalCenter: parent.verticalCenter
            visible: battery.visible
            width: 24; height: 24
            source: InputPromptIcons.sourceFor(root.controllerFamily, "white")
            sourceSize: Qt.size(width * Screen.devicePixelRatio, height * Screen.devicePixelRatio)
            fillMode: Image.PreserveAspectFit
        }
        Canvas {
            id: graph
            objectName: "streamToastLossHistory"
            anchors.fill: parent
            visible: root.warning && root.history.length >= 2
            onVisibleChanged: if (visible) requestPaint()
            onPaint: {
                const ctx = getContext("2d")
                ctx.reset()
                ctx.strokeStyle = "#1AFFFFFF"
                ctx.lineWidth = 1
                ctx.beginPath()
                ctx.moveTo(0, height - 1)
                ctx.lineTo(width, height - 1)
                ctx.stroke()
                const samples = root.history
                if (samples.length < 2) return
                const peak = Math.max(1, ...samples)
                ctx.strokeStyle = root.accent
                ctx.lineWidth = 1.5
                ctx.lineJoin = "round"
                ctx.lineCap = "round"
                ctx.beginPath()
                for (let i = 0; i < samples.length; ++i) {
                    const x = 1 + i * (width - 2) / (samples.length - 1)
                    const y = height - 3 - samples[i] / peak * (height - 6)
                    if (i === 0) ctx.moveTo(x, y)
                    else ctx.lineTo(x, y)
                }
                ctx.stroke()
            }
        }
    }
    onHistoryChanged: graph.requestPaint()

    Item {
        x: 12; y: parent.height - 2
        width: parent.width - 24; height: 2
        Rectangle { anchors.fill: parent; color: "#14FFFFFF" }
        Rectangle {
            width: parent.width * Math.max(0, Math.min(1, root.lifetimeFraction))
            height: 2
            color: root.accent
            opacity: 0.85
        }
    }
}

import QtQuick
import OpenNOW

// A three-finger tap switches touch between direct and trackpad
// (StreamVideoItem::switchTouchMode); say which one is on now.
GlassPanel {
    id: root
    required property StreamVideoItem surface
    property string mode: ""

    Connections {
        target: root.surface
        function onTouchModeSwitched(mode) {
            root.mode = mode
            noticeTimer.restart()
        }
    }
    Timer { id: noticeTimer; interval: 1500; onTriggered: root.mode = "" }
    MotionProgress { id: noticeMotion; shown: root.mode !== "" }

    visible: noticeMotion.present
    opacity: noticeMotion.progress
    layer.enabled: HdrOutput.chromeRequired
    layer.effect: HdrChromeEffect {}
    z: 12
    anchors.horizontalCenter: parent.horizontalCenter
    y: 34; width: 260; height: 58; panelRadius: 22; strong: true

    Text {
        anchors.centerIn: parent
        text: root.mode === "trackpad" ? qsTr("TOUCH: TRACKPAD") : qsTr("TOUCH: DIRECT")
        color: Theme.label; font.family: Theme.monoFont; font.pixelSize: 13; font.weight: Font.Black; font.letterSpacing: 0.8
    }
}

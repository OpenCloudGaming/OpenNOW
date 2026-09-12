import QtQuick
import OpenNOW

// Animate only the incoming page, never the shell or native video surface.
// Restarting replaces the previous transition; no delayed swaps or stale pages.
Item {
    id: root
    property real progress: 1
    readonly property real pageOpacity: 0.65 + 0.35 * progress
    readonly property real offset: 6 * (1 - progress)

    function restart() {
        entrance.stop()
        progress = AppController.reducedMotion ? 1 : 0
        if (!AppController.reducedMotion) entrance.start()
    }

    NumberAnimation {
        id: entrance
        target: root; property: "progress"; to: 1
        duration: 170; easing.type: Easing.OutCubic
    }
    Connections {
        target: AppController
        function onReducedMotionChanged() {
            if (AppController.reducedMotion) { entrance.stop(); root.progress = 1 }
        }
    }
}

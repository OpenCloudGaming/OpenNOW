import QtQuick
import OpenNOW

// One interruptible owner for a reveal. Consumers derive opacity/scale from
// progress, retain their content while present, and keep hit-test bounds fixed.
Item {
    id: root
    property bool shown: false
    property int enterDuration: 200
    property int exitDuration: 160
    property real closedScale: 0.97
    property real progress: 0
    readonly property bool present: shown || progress > 0
    readonly property real zoom: AppController.reducedMotion ? 1 : closedScale + (1 - closedScale) * progress
    property bool initialized: false
    signal hidden()

    function synchronize() {
        if (initialized) progress = shown ? 1 : 0
    }
    onShownChanged: synchronize()
    Component.onCompleted: { initialized = true; synchronize() }
    onProgressChanged: if (initialized && progress === 0 && !shown)
        Qt.callLater(() => { if (!root.shown && root.progress === 0) root.hidden() })

    Behavior on progress {
        enabled: !AppController.reducedMotion
        NumberAnimation {
            id: animation
            duration: root.shown ? root.enterDuration : root.exitDuration
            easing.type: Easing.OutCubic
        }
    }
    Connections {
        target: AppController
        function onReducedMotionChanged() {
            if (AppController.reducedMotion) {
                animation.stop()
                root.synchronize()
            }
        }
    }
}

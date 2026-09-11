import QtQuick
import OpenNOW

// Keep the content alive through closing and quick reversals. Only the clipped
// viewport changes height; the loaded content retains its natural layout.
Item {
    id: root
    property bool expanded: false
    property alias sourceComponent: content.sourceComponent
    readonly property real revealProgress: reveal.progress
    implicitHeight: content.height * reveal.progress
    visible: reveal.present
    clip: true
    MotionProgress { id: reveal; shown: root.expanded; enterDuration: 200; exitDuration: 160 }
    Loader {
        id: content
        width: parent.width
        active: reveal.present
        enabled: root.expanded
        opacity: reveal.progress
    }
}

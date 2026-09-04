import QtQuick
import OpenNOW

Rectangle {
    id: root
    property bool strong: false
    property real panelRadius: 28

    color: strong ? Theme.glassStrong : Theme.glass
    border.color: Theme.seam
    border.width: 1
    radius: panelRadius
}

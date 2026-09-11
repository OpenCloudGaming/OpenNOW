import QtQuick
import OpenNOW

Rectangle {
    id: panel
    property bool paperStyle: false
    property int padding: paperStyle ? 0 : 18
    default property alias content: body.data

    implicitHeight: body.implicitHeight + padding * 2
    radius: 14
    color: Theme.glass
    border.width: 1
    border.color: paperStyle ? DesktopTokens.seamSoft : Theme.seam

    Column {
        id: body
        x: panel.padding
        y: panel.padding
        width: panel.width - panel.padding * 2
    }
}

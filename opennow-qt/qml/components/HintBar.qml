import QtQuick
import OpenNOW

GlassPanel {
    id: root
    Accessible.ignored: true
    property var hints: [
        { glyph: "Y", label: qsTr("Search") },
        { glyph: "−", label: qsTr("Details") }
    ]
    implicitWidth: hintColumn.implicitWidth + 44
    implicitHeight: 96
    panelRadius: 32
    strong: true

    Column {
        id: hintColumn
        anchors.centerIn: parent
        spacing: 8
        Repeater {
            model: root.hints
            ControllerGlyph {
                required property var modelData
                glyph: modelData.glyph
                label: modelData.label
            }
        }
    }
}

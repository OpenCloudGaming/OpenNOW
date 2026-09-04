import QtQuick
import QtQuick.Controls
import OpenNOW

FocusScope {
    id: root
    anchors.fill: parent
    focus: true
    readonly property var report: ShellStore.lastSessionReport || ({})
    readonly property bool errorCountsKnown: report.decoderErrors !== undefined
        && report.decoderErrors !== null && report.outputErrors !== undefined
        && report.outputErrors !== null
    Accessible.name: qsTr("Session report")
    Accessible.role: Accessible.Dialog

    function duration(value) {
        const seconds = Math.max(0, Math.floor(Number(value || 0) / 1000))
        const hours = Math.floor(seconds / 3600)
        const minutes = Math.floor((seconds % 3600) / 60)
        const remainder = seconds % 60
        return (hours > 0 ? String(hours).padStart(2, "0") + ":" : "")
            + String(minutes).padStart(2, "0") + ":" + String(remainder).padStart(2, "0")
    }

    Rectangle { anchors.fill: parent; color: Qt.rgba(0, 0, 0, 0.62) }

    GlassPanel {
        anchors.centerIn: parent
        width: Math.min(parent.width - 80, 790)
        height: 560
        panelRadius: 42
        strong: true
        Column {
            anchors.fill: parent; anchors.margins: 42; spacing: 18
            Text { text: qsTr("SESSION COMPLETE"); color: Theme.mint; font.family: Theme.monoFont; font.pixelSize: 13; font.weight: Font.Black; font.letterSpacing: 1.5 }
            Text { width: parent.width; text: root.report.gameTitle || qsTr("GeForce NOW"); color: Theme.label; font.family: Theme.displayFont; font.pixelSize: 36; font.weight: Font.Black; elide: Text.ElideRight }
            Text { text: qsTr("Played for %1").arg(root.duration(root.report.durationMs)); color: Theme.textMuted; font.family: Theme.bodyFont; font.pixelSize: 18 }
            Grid {
                width: parent.width; columns: 3; columnSpacing: 12; rowSpacing: 12
                Repeater {
                    model: [
                        {label:qsTr("Transport"), value:root.report.transport || "—"},
                        {label:qsTr("Media backend"), value:root.report.mediaBackend || "—"},
                        {label:qsTr("First frame"), value:Number(root.report.firstFrameLatencyMs || 0) > 0 ? Number(root.report.firstFrameLatencyMs) + " ms" : "—"},
                        {label:qsTr("Recoveries"), value:root.report.recoveries !== undefined && root.report.recoveries !== null ? String(Number(root.report.recoveries)) : "—"},
                        {label:qsTr("Decoder errors"), value:root.report.decoderErrors !== undefined && root.report.decoderErrors !== null ? String(Number(root.report.decoderErrors)) : "—"},
                        {label:qsTr("Queue drops"), value:root.report.queueDrops !== undefined && root.report.queueDrops !== null ? String(Number(root.report.queueDrops)) : "—"}
                    ]
                    GlassPanel {
                        required property var modelData
                        width: (parent.width - 24) / 3; height: 92; panelRadius: 22
                        Column {
                            anchors.centerIn: parent; spacing: 5
                            Text { anchors.horizontalCenter: parent.horizontalCenter; text: modelData.label; color: Theme.textMuted; font.family: Theme.bodyFont; font.pixelSize: 12 }
                            Text { anchors.horizontalCenter: parent.horizontalCenter; text: modelData.value; color: Theme.label; font.family: Theme.monoFont; font.pixelSize: 18; font.weight: Font.Bold }
                        }
                    }
                }
            }
            Text {
                width: parent.width
                text: !root.errorCountsKnown
                    ? qsTr("Error telemetry was unavailable for this session.")
                    : Number(root.report.decoderErrors) + Number(root.report.outputErrors) === 0
                        ? qsTr("The native media path completed without decoder or presentation errors.")
                        : qsTr("Open Diagnostics for the redacted recovery timeline.")
                color: root.errorCountsKnown
                    && Number(root.report.decoderErrors) + Number(root.report.outputErrors) === 0
                    ? Theme.mint : Theme.yellow
                font.family: Theme.bodyFont; font.pixelSize: 15; wrapMode: Text.WordWrap
            }
            Row {
                anchors.horizontalCenter: parent.horizontalCenter; spacing: 14
                GlassButton { id: doneButton; width: 210; text: qsTr("Done"); glyph: "A"; primary: true; onClicked: AppController.showOverlay("") }
                GlassButton { width: 250; text: qsTr("Open diagnostics"); glyph: "X"; onClicked: { AppController.showOverlay(""); AppController.navigate("diagnostics") } }
            }
        }
        scale: root.visible ? 1 : 0.94
        Behavior on scale { NumberAnimation { duration: Theme.panelDuration; easing.type: Easing.OutBack } }
    }
    Component.onCompleted: doneButton.forceActiveFocus()
}

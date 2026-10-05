import QtQuick
import OpenNOW

FocusScope {
    id: root
    objectName: "consoleSessionReport"
    anchors.fill: parent
    focus: true
    Accessible.name: qsTr("Session report")
    Accessible.role: Accessible.Dialog

    readonly property var report: ShellStore.lastSessionReport || ({})
    readonly property bool errorCountsKnown: report.decoderErrors !== undefined
        && report.decoderErrors !== null && report.outputErrors !== undefined
        && report.outputErrors !== null
    readonly property bool clean: errorCountsKnown && Number(report.decoderErrors) + Number(report.outputErrors) === 0
    readonly property var game: {
        const selected = ShellStore.selectedGame
        return selected && selected.title === report.gameTitle ? selected : ({title: report.gameTitle || qsTr("GeForce NOW")})
    }

    function playedFor(value) {
        const minutesTotal = Math.floor(Math.max(0, Number(value || 0)) / 60000)
        const hours = Math.floor(minutesTotal / 60)
        const minutes = minutesTotal % 60
        if (hours > 0)
            return qsTr("Played for %1 h %2 min").arg(hours).arg(minutes)
        if (minutesTotal > 0)
            return qsTr("Played for %1 min").arg(minutes)
        return qsTr("Played for under a minute")
    }

    function countValue(value) {
        return value !== undefined && value !== null && Number.isFinite(Number(value)) ? String(Number(value)) : "—"
    }

    function dropValue(key, unit, decimals) {
        const value = root.report.drops && root.report.drops[key]
        return value !== undefined && value !== null && Number.isFinite(Number(value))
            ? (Number(value).toFixed(decimals || 0) + " " + unit).trim() : "—"
    }

    function openDiagnostics() {
        AppController.showOverlay("")
        AppController.navigate("diagnostics")
    }

    Component.onCompleted: doneButton.forceActiveFocus()

    Keys.onPressed: event => {
        if (event.key !== Qt.Key_X)
            return
        event.accepted = true
        if (!event.isAutoRepeat)
            root.openDiagnostics()
    }

    SessionGlyphs { id: glyphs }

    LaunchStage {
        anchors.fill: parent
        game: root.game
        subtitle: ""
        tone: Theme.mint
        statusText: qsTr("Session complete")
        eyebrow: qsTr("Session complete")
        headline: root.playedFor(root.report.durationMs)
        detail: !root.errorCountsKnown ? qsTr("Error telemetry was unavailable for this session.")
            : root.clean ? qsTr("The native media path completed without decoder or presentation errors.")
            : qsTr("Open Diagnostics for the redacted recovery timeline.")
        copyWidth: 820
        detailWidth: 760
        detailColor: root.clean ? Theme.mint : root.errorCountsKnown ? Theme.yellow : Theme.textMuted

        actions: [
            LaunchStage.LaunchAction {
                id: doneButton
                objectName: "sessionReportDoneButton"
                primary: true
                text: qsTr("Done")
                detail: qsTr("Close the report")
                glyph: glyphs.button("A")
                keyboardGlyph: glyphs.keyboard
                KeyNavigation.down: diagnosticsButton
                onClicked: AppController.showOverlay("")
            },
            LaunchStage.LaunchAction {
                id: diagnosticsButton
                objectName: "sessionReportDiagnosticsButton"
                text: qsTr("Open diagnostics")
                glyph: glyphs.button("X")
                keyboardGlyph: glyphs.keyboard
                KeyNavigation.up: doneButton
                onClicked: root.openDiagnostics()
            }
        ]

        aside: [
            LaunchStage.InfoCard {
                objectName: "sessionReportCard"
                x: parent.width - width - 40
                width: 520
                padding: 32
                gap: 16
                divider: false
                footnoteSize: 15
                footnoteLineHeight: 22
                title: qsTr("Session report")
                footnote: qsTr("Cells show \"—\" when the session did not report a value. Nothing is estimated.")
                Grid {
                    id: cells
                    width: parent.width
                    columns: 3
                    columnSpacing: 12
                    rowSpacing: 12
                    Repeater {
                        model: [
                            {label: qsTr("Transport"), value: root.report.transport || "—"},
                            {label: qsTr("Media backend"), value: root.report.mediaBackend || "—"},
                            {label: qsTr("First frame"), value: Number(root.report.firstFrameLatencyMs || 0) > 0 ? Number(root.report.firstFrameLatencyMs) + " ms" : "—"},
                            {label: qsTr("Recoveries"), value: root.countValue(root.report.recoveries)},
                            {label: qsTr("Decoder errors"), value: root.countValue(root.report.decoderErrors), warn: Number(root.report.decoderErrors || 0) > 0},
                            {label: qsTr("Video drops"), value: root.dropValue("videoDropCount", qsTr("frames")), warn: Number(root.report.drops && root.report.drops.videoDropCount || 0) > 0},
                            {label: qsTr("Audio discarded"), value: root.dropValue("audioDiscardedMs", "ms", 1)},
                            {label: qsTr("Audio queue drops"), value: root.dropValue("audioPacketDropCount", "")},
                            {label: qsTr("Callback drops"), value: root.dropValue("callbackDropCount", "")}
                        ]
                        Rectangle {
                            id: cell
                            required property var modelData
                            width: Math.min(142, (cells.width - 24) / 3)
                            height: 80
                            radius: 20
                            color: Qt.rgba(Theme.face.r, Theme.face.g, Theme.face.b, 0.06)
                            Accessible.role: Accessible.StaticText
                            Accessible.name: modelData.label + ", " + modelData.value
                            Text {
                                x: 16
                                anchors.baseline: parent.top
                                anchors.baselineOffset: 29.6
                                width: parent.width - 32
                                text: cell.modelData.label
                                elide: Text.ElideRight
                                color: Theme.textMuted
                                font.family: Theme.bodyFont; font.pixelSize: 14; font.weight: Font.Bold
                            }
                            Text {
                                x: 16
                                anchors.baseline: parent.top
                                anchors.baselineOffset: 59.5
                                width: parent.width - 32
                                text: cell.modelData.value
                                elide: Text.ElideRight
                                color: cell.modelData.warn ? Theme.yellow : Theme.label
                                font.family: Theme.monoFont; font.pixelSize: 20; font.weight: Font.Bold
                            }
                        }
                    }
                }
                Text {
                    visible: Number(root.report.drops && root.report.drops.otherQueueDropCount || 0) > 0
                    width: parent.width
                    text: qsTr("Unclassified drops: %1").arg(root.dropValue("otherQueueDropCount", qsTr("items")))
                    color: Theme.textMuted
                    font.family: Theme.bodyFont; font.pixelSize: 15
                }
            }
        ]
    }
}

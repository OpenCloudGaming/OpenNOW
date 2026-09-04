import QtQuick
import QtQuick.Controls
import OpenNOW

FocusScope {
    id: root
    Component.onCompleted: ShellStore.refreshDiagnostics()

    ScreenBackground { tint: "#15263A" }
    GlassPanel {
        x: 92; y: 112; width: parent.width - 184; height: parent.height - 238; panelRadius: 38
        Item {
            anchors.fill: parent; anchors.margins: 30
            Text { id: heading; text: qsTr("Diagnostics"); color: Theme.label; font.family: Theme.displayFont; font.pixelSize: 34; font.weight: Font.Black }
            Text { x: 0; y: 48; text: qsTr("Runtime breadcrumbs are bounded, persistent, and redacted before export."); color: Theme.textMuted; font.family: Theme.bodyFont; font.pixelSize: 15 }
            Row {
                anchors.right: parent.right; y: 0; spacing: 10
                GlassButton {
                    id: exportButton; width: 270
                    text: ShellStore.diagnosticsExportRequestId ? qsTr("Exporting…") : qsTr("Export redacted report")
                    glyph: "X"; enabled: !ShellStore.diagnosticsExportRequestId
                    onClicked: ShellStore.exportDiagnostics()
                    KeyNavigation.right: acceptanceButton
                    KeyNavigation.down: eventList
                }
                GlassButton {
                    id: acceptanceButton; width: 320
                    text: ShellStore.acceptanceExportRequestId ? qsTr("Collecting evidence…") : qsTr("Export live evidence")
                    primary: true; glyph: "A"; enabled: !ShellStore.acceptanceExportRequestId
                    onClicked: ShellStore.exportAcceptanceEvidence()
                    KeyNavigation.left: exportButton
                    KeyNavigation.down: eventList
                }
            }
            Text { x: 0; y: 86; text: ShellStore.diagnosticsMessage; color: Theme.mint; font.family: Theme.monoFont; font.pixelSize: 12 }
            ListView {
                id: eventList
                x: 0; y: 118; width: parent.width; height: parent.height - y
                model: ShellStore.diagnostics.entries || []; spacing: 6; clip: true; focus: true
                keyNavigationWraps: false; KeyNavigation.up: acceptanceButton
                Component.onCompleted: currentIndex = count ? Math.min(ShellStore.focusIndex("diagnostics"), count - 1) : -1
                onCountChanged: if (count > 0 && currentIndex < 0) currentIndex = Math.min(ShellStore.focusIndex("diagnostics"), count - 1)
                onCurrentIndexChanged: if (currentIndex >= 0) ShellStore.rememberFocus("diagnostics", currentIndex)
                delegate: GlassPanel {
                    required property var modelData
                    width: eventList.width; height: 66; panelRadius: 18; strong: ListView.isCurrentItem
                    Row {
                        anchors.fill: parent; anchors.margins: 14; spacing: 16
                        Text { width: 110; text: modelData.area || "core"; color: Theme.violet; font.family: Theme.monoFont; font.pixelSize: 12; font.weight: Font.Bold }
                        Text { width: 250; text: modelData.event || "event"; color: Theme.label; font.family: Theme.bodyFont; font.pixelSize: 14; font.weight: Font.Bold; elide: Text.ElideRight }
                        Text { width: parent.width - 390; text: modelData.detail || ""; color: Theme.textMuted; font.family: Theme.monoFont; font.pixelSize: 11; elide: Text.ElideRight }
                    }
                }
            }
        }
    }
    HintBar { anchors.horizontalCenter: parent.horizontalCenter; y: parent.height - height - 84; hints: [{glyph:"A",label:qsTr("Export")},{glyph:"B",label:qsTr("Back")}] }
    AppChrome { anchors.fill: parent; title: qsTr("Diagnostics"); currentRoute: "diagnostics"; onRouteRequested: route => AppController.navigate(route) }
}

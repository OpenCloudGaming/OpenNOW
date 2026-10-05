import QtQuick
import QtQuick.Controls
import OpenNOW

FocusScope {
    id: root
    Component.onCompleted: ShellStore.refreshDiagnostics()

    ScreenBackground { tint: "#15263A" }
    GlassPanel {
        x: 96; y: 124; width: parent.width - 192; height: parent.height - 288; panelRadius: 40
        Column {
            x: 56; y: 34; width: parent.width - 112 - actions.width - 24; spacing: 6
            Text { text: qsTr("Diagnostics"); color: Theme.label; font.family: Theme.displayFont; font.pixelSize: 32; font.weight: Font.Black; font.letterSpacing: -0.3 }
            Text { width: parent.width; text: qsTr("Runtime breadcrumbs are bounded, persistent, and redacted before export."); color: Theme.textMuted; font.family: Theme.bodyFont; font.pixelSize: 17; font.weight: Font.DemiBold; elide: Text.ElideRight }
            Text { width: parent.width; visible: text !== ""; text: ShellStore.diagnosticsMessage; color: Theme.mint; font.family: Theme.monoFont; font.pixelSize: 14; font.weight: Font.Bold; elide: Text.ElideRight }
        }
        Row {
            id: actions
            anchors.right: parent.right; anchors.rightMargin: 40; y: 36; spacing: 12
            ConsoleActionButton {
                id: exportButton; width: 300
                text: ShellStore.diagnosticsExportRequestId ? qsTr("Exporting…") : qsTr("Export redacted report")
                enabled: !ShellStore.diagnosticsExportRequestId
                onClicked: ShellStore.exportDiagnostics()
                KeyNavigation.right: acceptanceButton
                KeyNavigation.down: eventList
            }
            ConsoleActionButton {
                id: acceptanceButton; width: 300
                text: ShellStore.acceptanceExportRequestId ? qsTr("Collecting evidence…") : qsTr("Export live evidence")
                primary: true; enabled: !ShellStore.acceptanceExportRequestId
                onClicked: ShellStore.exportAcceptanceEvidence()
                KeyNavigation.left: exportButton
                KeyNavigation.down: eventList
            }
        }
        ListView {
            id: eventList
            anchors.fill: parent
            anchors.leftMargin: 24; anchors.rightMargin: 24
            anchors.topMargin: 150; anchors.bottomMargin: 18
            leftMargin: 10; rightMargin: 10; topMargin: 10; bottomMargin: 10
            model: ShellStore.diagnostics.entries || []; spacing: 2; clip: true; focus: true
            keyNavigationWraps: false; KeyNavigation.up: acceptanceButton
            highlightMoveDuration: AppController.reducedMotion ? 0 : 260
            Component.onCompleted: currentIndex = count ? Math.min(ShellStore.focusIndex("diagnostics"), count - 1) : -1
            onCountChanged: if (count > 0 && currentIndex < 0) currentIndex = Math.min(ShellStore.focusIndex("diagnostics"), count - 1)
            onCurrentIndexChanged: if (currentIndex >= 0) ShellStore.rememberFocus("diagnostics", currentIndex)
            delegate: ConsoleListRow {
                required property var modelData
                width: ListView.view.width - 20
                height: 76
                focusPolicy: Qt.NoFocus
                title: modelData.event || "event"
                description: modelData.detail || ""
                badge: modelData.area || "core"
                badgeColor: Theme.violet
                currentItem: false
                ringVisible: ListView.isCurrentItem && eventList.activeFocus
            }
            ScrollIndicator.vertical: ScrollIndicator {}
        }
    }
    AppChrome {
        anchors.fill: parent; title: qsTr("Diagnostics"); currentRoute: "diagnostics"
        leftHints: [{glyph:"B", label:qsTr("Back")}]
        rightHints: eventList.activeFocus ? [] : [{glyph:"A", label:qsTr("Export")}]
        onRouteRequested: route => AppController.navigate(route)
    }
}

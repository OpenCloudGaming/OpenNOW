import QtQuick
import QtQuick.Controls
import OpenNOW

FocusScope {
    id: root
    readonly property var state: ShellStore.updaterState || ({})
    readonly property bool available: state.status === "available"

    ScreenBackground { tint: "#16263D" }
    GlassPanel {
        anchors.centerIn: parent; width: Math.min(980, parent.width - 180); height: 590; panelRadius: 42; strong: true
        Column {
            anchors.fill: parent; anchors.margins: 38; spacing: 18
            Row {
                width: parent.width; height: 62; spacing: 20
                Rectangle {
                    width: 62; height: 62; radius: 20; color: root.available ? Theme.mint : Theme.violet
                    Text { anchors.centerIn: parent; text: root.available ? "↑" : "✓"; color: Theme.contrastText(root.available ? Theme.mint : Theme.violet); font.pixelSize: 30; font.weight: Font.Black }
                }
                Column {
                    anchors.verticalCenter: parent.verticalCenter; spacing: 3
                    Text { text: root.available ? qsTr("Update available") : qsTr("OpenNOW updates"); color: Theme.label; font.family: Theme.displayFont; font.pixelSize: 31; font.weight: Font.Black }
                    Text { text: qsTr("Installed version %1 · %2 channel").arg(root.state.currentVersion || qsTr("unknown")).arg(ShellStore.settings.updateChannel === "nightly" ? qsTr("Nightly") : qsTr("Stable")); color: Theme.textMuted; font.family: Theme.monoFont; font.pixelSize: 12 }
                }
            }
            Text {
                width: parent.width; wrapMode: Text.WordWrap
                text: root.state.message || qsTr("Check GitHub Releases for a newer OpenNOW build.")
                color: Theme.label; font.family: Theme.bodyFont; font.pixelSize: 17
            }
            GlassPanel {
                width: parent.width; height: 310; panelRadius: 26
                Flickable {
                    anchors.fill: parent; anchors.margins: 22; contentHeight: notes.height; clip: true
                    Text {
                        id: notes; width: parent.width
                        text: ShellStore.releaseHighlights.bodyMarkdown || qsTr("Release notes appear here after an update check. Qt packages are only offered after platform signing and update metadata verification are available.")
                        color: Theme.textMuted; font.family: Theme.bodyFont; font.pixelSize: 14; wrapMode: Text.WordWrap
                    }
                }
            }
            Row {
                spacing: 12
                GlassButton {
                    id: checkButton; width: 276
                    text: root.state.status === "checking" ? qsTr("Checking…") : qsTr("Check for updates")
                    primary: true; glyph: "A"; enabled: root.state.status !== "checking"
                    onClicked: ShellStore.checkForUpdates()
                    Component.onCompleted: forceActiveFocus()
                }
                GlassButton {
                    width: 276; text: qsTr("Open releases"); glyph: "↗"
                    enabled: Boolean(root.state.releaseUrl)
                    onClicked: AppController.openExternalUrl(root.state.releaseUrl || "")
                }
                GlassButton {
                    width: 276
                    visible: Boolean(root.state.canDownload)
                    text: root.state.status === "downloading" ? qsTr("Downloading…") : qsTr("Download verified update")
                    glyph: "↓"; primary: true
                    enabled: root.state.status !== "downloading"
                    onClicked: ShellStore.downloadUpdate()
                }
                GlassButton {
                    width: 276
                    visible: Boolean(root.state.canInstall)
                    text: qsTr("Install and restart")
                    glyph: "↑"; primary: true
                    onClicked: ShellStore.installUpdate()
                }
            }
        }
    }
    HintBar { anchors.horizontalCenter: parent.horizontalCenter; y: parent.height - height - 82; hints: [{glyph:"A",label:qsTr("Check")},{glyph:"B",label:qsTr("Back")}] }
    AppChrome { anchors.fill: parent; title: qsTr("Updates"); currentRoute: "updates"; onRouteRequested: route => AppController.navigate(route) }
}

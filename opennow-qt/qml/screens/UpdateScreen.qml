import QtQuick
import QtQuick.Controls
import OpenNOW

FocusScope {
    id: root
    readonly property var state: ShellStore.updaterState || ({})
    readonly property bool available: state.status === "available"
    Component.onCompleted: ShellStore.acknowledgeUpdateHighlights()

    Dialog {
        id: installConfirmation
        objectName: "updateInstallConfirmation"
        anchors.centerIn: parent
        width: Math.min(parent.width - 48, 500)
        implicitHeight: 220
        height: Math.min(root.height - 48, implicitHeight)
        modal: true
        focus: true
        title: root.state.downloadedVersion ? qsTr("Install %1 and restart").arg(root.state.downloadedVersion) : qsTr("Install and restart")
        standardButtons: Dialog.Ok | Dialog.Cancel
        onAccepted: ShellStore.installUpdate(true)
        contentItem: Label {
            wrapMode: Text.WordWrap
            text: qsTr("OpenNOW will prepare the verified update, close, replace this installation, and restart. Continue?")
        }
    }

    ScreenBackground { tint: "#16263D" }
    GlassPanel {
        anchors.centerIn: parent; width: Math.min(980, parent.width - 180); height: 590; panelRadius: 42; strong: true
        Column {
            anchors.fill: parent; anchors.margins: 38; spacing: 18
            Row {
                width: parent.width; height: 62; spacing: 20
                Rectangle {
                    width: 62; height: 62; radius: 20; color: root.available ? Theme.mint : Theme.violet
                    Text { anchors.centerIn: parent; text: root.state.status === "succeeded" ? "✓" : root.available ? "↑" : "↓"; color: Theme.contrastText(root.available ? Theme.mint : Theme.violet); font.pixelSize: 30; font.weight: Font.Black }
                }
                Column {
                    anchors.verticalCenter: parent.verticalCenter; spacing: 3
                    Text { text: root.available ? qsTr("Update available") : qsTr("OpenNOW updates"); color: Theme.label; font.family: Theme.displayFont; font.pixelSize: 31; font.weight: Font.Black }
                    Text { text: qsTr("Installed version %1 · %2 channel").arg(root.state.currentVersion || qsTr("unknown")).arg(ShellStore.settings.updateChannel === "nightly" ? qsTr("Nightly") : qsTr("Stable")); color: Theme.textMuted; font.family: Theme.monoFont; font.pixelSize: 12 }
                }
            }
            Text {
                width: parent.width; wrapMode: Text.WordWrap
                text: ShellStore.updaterError || root.state.message || qsTr("Check GitHub Releases for a newer OpenNOW build.")
                color: Theme.label; font.family: Theme.bodyFont; font.pixelSize: 17
                Accessible.role: Accessible.StaticText
                Accessible.name: text
            }
            ProgressBar {
                width: parent.width
                visible: ShellStore.updaterBusy
                indeterminate: true
                Accessible.name: root.state.message || qsTr("Update in progress")
            }
            Text {
                width: parent.width; wrapMode: Text.WordWrap
                visible: !ShellStore.updaterSessionSafe
                text: qsTr("End your streaming session before installing an update. Background updates will wait.")
                color: Theme.textMuted; font.family: Theme.bodyFont; font.pixelSize: 14
                Accessible.role: Accessible.StaticText
                Accessible.name: text
            }
            GlassPanel {
                width: parent.width; height: 240; panelRadius: 26
                Flickable {
                    anchors.fill: parent; anchors.margins: 22; contentHeight: notes.height; clip: true
                    ReleaseNotes {
                        id: notes; width: parent.width
                        text: ShellStore.releaseHighlights.bodyMarkdown || qsTr("Check for updates to load verified release information from GitHub.")
                        font.pixelSize: 14
                    }
                }
            }
            Flow {
                width: parent.width
                spacing: 12
                GlassButton {
                    id: checkButton; width: 250
                    text: root.state.status === "checking" ? qsTr("Checking…") : qsTr("Check for updates")
                    primary: true; glyph: "A"; enabled: !ShellStore.updaterBusy && root.state.canCheck === true
                    onClicked: ShellStore.checkForUpdates()
                    Component.onCompleted: forceActiveFocus()
                }
                GlassButton {
                    width: 250; text: qsTr("Open releases"); glyph: "↗"
                    enabled: Boolean(root.state.releaseUrl)
                    onClicked: AppController.openExternalUrl(root.state.releaseUrl || "")
                }
                GlassButton {
                    width: 250
                    visible: Boolean(root.state.canDownload)
                    text: root.state.status === "downloading" ? qsTr("Downloading…") : qsTr("Download verified update")
                    glyph: "↓"; primary: true
                    enabled: !ShellStore.updaterBusy && root.state.canDownload === true
                    onClicked: ShellStore.downloadUpdate()
                }
                GlassButton {
                    width: 250
                    objectName: "updateInstallButton"
                    visible: Boolean(root.state.canInstall)
                    text: root.state.downloadedVersion ? qsTr("Install %1 and restart").arg(root.state.downloadedVersion) : qsTr("Install and restart")
                    glyph: "↑"; primary: true
                    enabled: ShellStore.updaterCanInstall
                    onClicked: installConfirmation.open()
                }
            }
        }
    }
    HintBar { anchors.horizontalCenter: parent.horizontalCenter; y: parent.height - height - 82; hints: [{glyph:"A",label:qsTr("Check")},{glyph:"B",label:qsTr("Back")}] }
    AppChrome { anchors.fill: parent; title: qsTr("Updates"); currentRoute: "updates"; onRouteRequested: route => AppController.navigate(route) }
}

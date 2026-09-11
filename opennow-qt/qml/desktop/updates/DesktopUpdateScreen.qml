import QtQuick
import QtQuick.Controls
import OpenNOW

FocusScope {
    id: root
    objectName: "desktopUpdateScreen"
    readonly property var state: ShellStore.updaterState || ({})
    readonly property bool busy: ShellStore.updaterBusy
    Component.onCompleted: ShellStore.acknowledgeUpdateHighlights()

    Dialog {
        id: installConfirmation
        objectName: "updateInstallConfirmation"
        anchors.centerIn: parent
        width: Math.min(parent.width - 48, 440)
        // Bound the dialog independently of the style's header/footer sizing.
        implicitHeight: DesktopTokens.px(220)
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

    Flickable {
        anchors.fill: parent
        anchors.margins: DesktopTokens.px(24)
        contentWidth: width
        contentHeight: content.implicitHeight
        clip: true
        boundsBehavior: Flickable.StopAtBounds
        ScrollBar.vertical: ScrollBar {}

        Column {
            id: content
            width: parent.width
            spacing: DesktopTokens.px(20)

            DesktopSettingsPanel {
                width: parent.width
                paperStyle: true
                Column {
                    x: DesktopTokens.px(24)
                    width: parent.width - x * 2
                    spacing: DesktopTokens.px(16)
                    topPadding: DesktopTokens.px(24)
                    bottomPadding: DesktopTokens.px(24)
                    Text {
                        width: parent.width
                        text: root.state.status === "available" ? qsTr("Update available") : qsTr("OpenNOW updates")
                        color: Theme.label
                        font.family: Theme.displayFont
                        font.pixelSize: DesktopTokens.px(28)
                        font.bold: true
                        wrapMode: Text.WordWrap
                    }
                    Text {
                        width: parent.width
                        text: qsTr("Installed version %1 · %2 channel").arg(root.state.currentVersion || Qt.application.version).arg(ShellStore.settings.updateChannel === "nightly" ? qsTr("Nightly") : qsTr("Stable"))
                        color: Theme.textMuted
                        font.family: Theme.bodyFont
                        font.pixelSize: DesktopTokens.px(13)
                        wrapMode: Text.WordWrap
                    }
                    Text {
                        width: parent.width
                        visible: Boolean(root.state.availableVersion)
                        text: qsTr("Available version: %1").arg(root.state.availableVersion || "")
                        color: Theme.textMuted
                        font.family: Theme.bodyFont
                        font.pixelSize: DesktopTokens.px(13)
                        wrapMode: Text.WordWrap
                    }
                    Text {
                        width: parent.width
                        text: ShellStore.updaterError || root.state.message || qsTr("Check GitHub Releases for a newer OpenNOW build.")
                        textFormat: Text.PlainText
                        color: Theme.label
                        font.family: Theme.bodyFont
                        font.pixelSize: DesktopTokens.px(14)
                        wrapMode: Text.WordWrap
                        Accessible.role: Accessible.StaticText
                        Accessible.name: text
                    }
                    ProgressBar {
                        width: parent.width
                        visible: root.busy
                        indeterminate: true
                        Accessible.name: root.state.message || qsTr("Update in progress")
                    }
                    Text {
                        width: parent.width
                        visible: !ShellStore.updaterSessionSafe
                        text: qsTr("End your streaming session before installing an update. Background updates will wait.")
                        color: Theme.textMuted
                        wrapMode: Text.WordWrap
                        Accessible.role: Accessible.StaticText
                        Accessible.name: text
                    }
                    Flow {
                        width: parent.width
                        spacing: DesktopTokens.px(12)
                        DesktopSettingsButton {
                            text: root.state.status === "checking" ? qsTr("Checking…") : qsTr("Check for updates")
                            primary: true
                            enabled: !root.busy && root.state.canCheck === true
                            onClicked: ShellStore.checkForUpdates()
                        }
                        DesktopSettingsButton {
                            text: qsTr("Open releases")
                            enabled: Boolean(root.state.releaseUrl)
                            onClicked: AppController.openExternalUrl(root.state.releaseUrl || "")
                        }
                        DesktopSettingsButton {
                            visible: Boolean(root.state.canDownload)
                            text: root.state.status === "downloading" ? qsTr("Downloading…") : qsTr("Download verified update")
                            enabled: !root.busy && root.state.canDownload === true
                            onClicked: ShellStore.downloadUpdate()
                        }
                        DesktopSettingsButton {
                            visible: Boolean(root.state.canInstall)
                            text: root.state.downloadedVersion ? qsTr("Install %1 and restart").arg(root.state.downloadedVersion) : qsTr("Install and restart")
                            primary: true
                            objectName: "updateInstallButton"
                            enabled: ShellStore.updaterCanInstall
                            onClicked: installConfirmation.open()
                        }
                    }
                }
            }
            DesktopSettingsPanel {
                width: parent.width
                paperStyle: true
                DesktopSettingsSection { text: qsTr("RELEASE NOTES") }
                ReleaseNotes {
                    x: DesktopTokens.px(24)
                    width: parent.width - x * 2
                    bottomPadding: DesktopTokens.px(24)
                    text: ShellStore.releaseHighlights.bodyMarkdown || qsTr("Check for updates to load verified release information from GitHub.")
                    font.pixelSize: DesktopTokens.px(14)
                }
            }
        }
    }
}

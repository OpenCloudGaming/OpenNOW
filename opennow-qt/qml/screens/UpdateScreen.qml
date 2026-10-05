import QtQuick
import QtQuick.Controls
import OpenNOW

FocusScope {
    id: root
    readonly property var state: ShellStore.updaterState || ({})
    readonly property bool available: state.status === "available"
    readonly property string installText: root.state.downloadedVersion ? qsTr("Install %1 and restart").arg(root.state.downloadedVersion) : qsTr("Install and restart")
    Component.onCompleted: {
        ShellStore.acknowledgeUpdateHighlights()
        updateActions.focusFirst()
    }
    Keys.onPressed: event => {
        if (event.key === Qt.Key_Up || event.key === Qt.Key_Down || event.key === Qt.Key_Left || event.key === Qt.Key_Right) {
            updateActions.focusFirst()
            event.accepted = true
        }
    }

    ScreenBackground { tint: "#16263D" }

    GlassPanel {
        x: 96; y: 124; width: 1080; height: root.height - 288; panelRadius: 40
        Row {
            id: updateHeader
            x: 56; y: 40; width: parent.width - 112; height: 64; spacing: 20
            Rectangle {
                width: 64; height: 64; radius: 32; color: root.available ? Theme.mint : Theme.violet
                Text { anchors.centerIn: parent; text: root.state.status === "succeeded" ? "✓" : root.available ? "↑" : "↓"; color: Theme.contrastText(parent.color); font.pixelSize: 30; font.weight: Font.Black }
            }
            Column {
                anchors.verticalCenter: parent.verticalCenter; spacing: 4
                Text { text: root.available ? qsTr("Update available") : qsTr("OpenNOW updates"); color: Theme.label; font.family: Theme.displayFont; font.pixelSize: 32; font.weight: Font.Black; font.letterSpacing: -0.3 }
                Text { text: qsTr("Installed version %1 · %2 channel").arg(root.state.currentVersion || qsTr("unknown")).arg(ShellStore.settings.updateChannel === "nightly" ? qsTr("Nightly") : qsTr("Stable")); color: Theme.textMuted; font.family: Theme.monoFont; font.pixelSize: 14; font.weight: Font.Bold; font.letterSpacing: 0.6 }
            }
        }
        Column {
            id: statusColumn
            anchors.top: updateHeader.bottom; anchors.topMargin: 28
            x: 56; width: parent.width - 112; spacing: 14
            Text {
                width: parent.width; wrapMode: Text.WordWrap
                text: ShellStore.updaterError || root.state.message || qsTr("Check GitHub Releases for a newer OpenNOW build.")
                color: Theme.label; font.family: Theme.bodyFont; font.pixelSize: 19; font.weight: Font.DemiBold; lineHeight: 1.25
                Accessible.role: Accessible.StaticText
                Accessible.name: text
            }
            ProgressBar {
                width: parent.width
                visible: ShellStore.updaterBusy
                indeterminate: true
                Accessible.name: root.state.message || qsTr("Update in progress")
            }
        }
        Rectangle {
            id: notesFrame
            anchors.top: statusColumn.bottom; anchors.topMargin: 24
            anchors.bottom: parent.bottom; anchors.bottomMargin: 40
            x: 40; width: parent.width - 80
            radius: 28
            color: notesView.activeFocus ? Qt.rgba(Theme.face.r, Theme.face.g, Theme.face.b, 0.08) : Qt.rgba(Theme.face.r, Theme.face.g, Theme.face.b, 0.04)
            border.width: notesView.activeFocus ? 3 : 1
            border.color: notesView.activeFocus ? Theme.face : Theme.seam
            Rectangle {
                anchors.fill: parent; anchors.margins: -9; radius: parent.radius + 9
                color: "transparent"; border.width: 5
                border.color: Qt.rgba(Theme.focus.r, Theme.focus.g, Theme.focus.b, 0.4)
                visible: notesView.activeFocus
            }
            Flickable {
                id: notesView
                anchors.fill: parent; anchors.margins: 26; contentHeight: notes.height; clip: true
                boundsBehavior: Flickable.StopAtBounds
                activeFocusOnTab: true
                Accessible.role: Accessible.Document
                Keys.onPressed: event => {
                    if (event.key === Qt.Key_Down || event.key === Qt.Key_Up) {
                        const step = event.key === Qt.Key_Down ? 120 : -120
                        contentY = Math.max(0, Math.min(Math.max(0, contentHeight - height), contentY + step))
                        event.accepted = true
                    } else if (event.key === Qt.Key_Right) {
                        if (!event.isAutoRepeat) updateActions.focusFirst()
                        event.accepted = true
                    } else if (event.key === Qt.Key_Left) {
                        event.accepted = true
                    }
                }
                ReleaseNotes {
                    id: notes; width: parent.width
                    text: ShellStore.releaseHighlights.bodyMarkdown || qsTr("Check for updates to load verified release information from GitHub.")
                    font.pixelSize: 16
                }
                ScrollIndicator.vertical: ScrollIndicator {}
            }
        }
    }

    GlassPanel {
        x: 1200; y: 124; width: root.width - 1296; height: root.height - 288; panelRadius: 40
        ConsoleActionColumn {
            id: updateActions
            x: 40; y: 40; width: parent.width - 80
            returnTarget: notesView
            ConsoleActionButton {
                width: parent.width
                visible: Boolean(root.state.canDownload)
                text: root.state.status === "downloading" ? qsTr("Downloading…") : qsTr("Download verified update")
                primary: true
                enabled: !ShellStore.updaterBusy && root.state.canDownload === true
                onClicked: ShellStore.downloadUpdate()
            }
            ConsoleActionButton {
                id: installButton
                width: parent.width
                objectName: "updateInstallButton"
                visible: Boolean(root.state.canInstall)
                text: root.installText
                primary: true
                enabled: ShellStore.updaterCanInstall
                onClicked: installConfirmation.open()
            }
            ConsoleActionButton {
                width: parent.width
                text: root.state.status === "checking" ? qsTr("Checking…") : qsTr("Check for updates")
                primary: !root.state.canDownload && !root.state.canInstall
                enabled: !ShellStore.updaterBusy && root.state.canCheck === true
                onClicked: ShellStore.checkForUpdates()
            }
            ConsoleActionButton {
                width: parent.width; text: qsTr("Open releases")
                enabled: Boolean(root.state.releaseUrl)
                onClicked: AppController.openExternalUrl(root.state.releaseUrl || "")
            }
            Text {
                width: parent.width; wrapMode: Text.WordWrap
                visible: !ShellStore.updaterSessionSafe
                text: qsTr("End your streaming session before installing an update. Background updates will wait.")
                color: Theme.yellow; font.family: Theme.bodyFont; font.pixelSize: 17; font.weight: Font.DemiBold; lineHeight: 1.2
                Accessible.role: Accessible.StaticText
                Accessible.name: text
            }
        }
    }

    AppChrome {
        anchors.fill: parent; title: qsTr("Updates"); currentRoute: "updates"
        leftHints: [{glyph:"B", label:qsTr("Back")}]
        rightHints: [{glyph:"A", label:qsTr("Select")}]
        onRouteRequested: route => AppController.navigate(route)
    }

    ConsoleWarningSheet {
        id: installConfirmation
        objectName: "updateInstallConfirmation"
        signal accepted()
        function open() { opened = true }
        function close() {
            opened = false
            Qt.callLater(() => {
                if (installConfirmation.opened)
                    return
                if (installButton.enabled && installButton.visible) installButton.forceActiveFocus()
                else updateActions.focusFirst()
            })
        }
        danger: false
        eyebrow: qsTr("Updates")
        title: root.installText
        message: qsTr("OpenNOW will prepare the verified update, close, replace this installation, and restart. Continue?")
        safeText: qsTr("Not now")
        actionText: root.installText
        safeButtonObjectName: "updateInstallCancel"
        actionButtonObjectName: "updateInstallConfirm"
        onSafeRequested: close()
        onActionRequested: {
            close()
            accepted()
        }
        onAccepted: ShellStore.installUpdate(true)
    }
}

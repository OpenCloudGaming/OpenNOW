import QtQuick
import QtQuick.Controls
import QtQuick.Layouts
import OpenNOW

Column {
    id: page
    required property real availableWidth
    required property var settingsScreen

    width: page.availableWidth; spacing: 20
    DesktopSettingsPanel {
        width: parent.width; paperStyle: true
        DesktopSettingsSection { text: qsTr("OPENNOW") }
        DesktopSettingsRow {
            width: parent.width; paperStyle: true
            leadingIcon: "qrc:/qt/qml/OpenNOW/res/brand/opennow-mark.png"
            title: "OpenNOW " + String(ShellStore.updaterState.currentVersion || qsTr("unknown"))
            description: qsTr("Your games, anywhere.")
            DesktopSettingsButton { text: ShellStore.updaterState.status === "checking" ? qsTr("Checking…") : qsTr("Check for updates"); primary: true; enabled: !ShellStore.updaterBusy && ShellStore.updaterState.canCheck === true; onClicked: ShellStore.checkForUpdates() }
        }
        DesktopSettingsRow {
            width: parent.width; paperStyle: true; glyph: "arrows"; title: qsTr("Update channel")
            description: qsTr("Choose which releases OpenNOW checks")
            DesktopSettingsSegmented {
                options: [{label:qsTr("Stable"),value:"stable"},{label:qsTr("Nightly"),value:"nightly"}]
                objectName: "renewUpdateChannel"
                optionWidth: 96; selectedIndex: options.findIndex(item => item.value === page.settingsScreen.valueSetting("updateChannel","stable"))
                onSelected: (index,item) => page.settingsScreen.setChoice("updateChannel",item.value)
            }
        }
        DesktopSettingsRow {
            width: parent.width; paperStyle: true; title: qsTr("Automatically check for updates")
            description: qsTr("Check every six hours while no streaming session is active.")
            DesktopSettingsToggle {
                objectName: "autoCheckUpdatesToggle"
                Accessible.name: qsTr("Automatically check for updates")
                checked: page.settingsScreen.boolSetting("autoCheckForUpdates", false)
                onValueChangedByUser: value => page.settingsScreen.setSetting("autoCheckForUpdates", value)
            }
        }
        DesktopSettingsRow {
            width: parent.width; paperStyle: true; title: qsTr("Automatically download updates")
            description: qsTr("Download verified updates while idle. Installation always requires your confirmation.")
            DesktopSettingsToggle {
                objectName: "autoDownloadUpdatesToggle"
                Accessible.name: qsTr("Automatically download updates")
                checked: page.settingsScreen.boolSetting("autoDownloadUpdates", false)
                onValueChangedByUser: value => page.settingsScreen.setSetting("autoDownloadUpdates", value)
            }
        }
        DesktopSettingsRow {
            width: parent.width; paperStyle: true; glyph: "info"; title: qsTr("Update status")
            description: String(ShellStore.updaterState.message || ShellStore.updaterState.status || qsTr("idle"))
            showDivider: false
            DesktopSettingsButton { text: qsTr("Updates"); onClicked: AppController.navigate("updates") }
        }
    }
    DesktopSettingsPanel {
        width: parent.width; paperStyle: true
        DesktopSettingsSection { text: qsTr("RELEASE NOTES") }
        Column {
            x: DesktopTokens.px(20); width: parent.width-DesktopTokens.px(40); spacing: 8
            Text { width: parent.width; text: ShellStore.releaseHighlights.title || qsTr("Release notes"); color: Theme.label; font.family: Theme.bodyFont; font.pixelSize: DesktopTokens.px(15); font.weight: Font.ExtraBold; wrapMode: Text.WordWrap; textFormat: Text.PlainText }
            ReleaseNotes { width: parent.width; bottomPadding: 20; text: ShellStore.releaseHighlights.bodyMarkdown || qsTr("Check for updates to load verified release information from GitHub."); font.pixelSize: DesktopTokens.px(13) }
        }
    }
    DesktopSettingsPanel {
        width: parent.width; paperStyle: true
        DesktopSettingsSection { text: qsTr("PROJECT & DIAGNOSTICS") }
        Repeater {
            model: page.settingsScreen.projectLinks()
            delegate: DesktopSettingsRow {
                required property var modelData
                required property int index
                width: parent.width; paperStyle: true; glyph: modelData.id === "diagnostics" ? "wave" : modelData.id === "captures" ? "image" : "globe"
                title: modelData.label
                description: modelData.id === "diagnostics" ? qsTr("Generate a diagnostic report") : ""
                showDivider: index < 3
                DesktopSettingsButton { text: modelData.id === "diagnostics" ? qsTr("Export") : qsTr("Open"); onClicked: page.settingsScreen.runProjectLink(modelData) }
            }
        }
    }
    DesktopSettingsPanel {
        width: parent.width; paperStyle: true
        DesktopSettingsRow {
            width: parent.width; paperStyle: true; glyph: "info"; title: qsTr("Independent client")
            description: qsTr("OpenNOW is not affiliated with, endorsed by or supported by NVIDIA. GeForce NOW is a trademark of NVIDIA Corporation. You bring your own account and subscription.")
            showDivider: false
        }
    }
    DesktopSettingsPanel {
        width: parent.width; paperStyle: true
        DesktopSettingsRow {
            width: parent.width; paperStyle: true; glyph: "arrows"
            title: qsTr("Replay onboarding")
            description: ShellStore.onboardingReplayError || (ShellStore.activeSession || ShellStore.streamBusy
                || ["idle", "error"].indexOf(ShellStore.streamState) < 0
                ? qsTr("End your streaming session before replaying setup.")
                : qsTr("Restart OpenNOW and walk through setup again. Your preferences are kept."))
            showDivider: false
            DesktopSettingsButton {
                objectName: "replayOnboardingButton"
                text: ShellStore.onboardingReplaying ? qsTr("Restarting…") : qsTr("Replay onboarding")
                enabled: ShellStore.onboardingReplayAvailable
                onClicked: replayConfirmation.open()
            }
        }
    }

    Dialog {
        id: replayConfirmation
        objectName: "replayOnboardingConfirmation"
        parent: Overlay.overlay
        anchors.centerIn: parent
        width: Math.min(DesktopTokens.px(520), parent.width - DesktopTokens.px(32))
        contentWidth: width - leftPadding - rightPadding
        implicitHeight: header.implicitHeight + replayCopy.implicitHeight + footer.implicitHeight
            + topPadding + bottomPadding
        padding: DesktopTokens.px(22)
        modal: true
        focus: true
        closePolicy: Popup.CloseOnEscape
        title: qsTr("Replay onboarding?")
        header: Text {
            width: replayConfirmation.width
            text: replayConfirmation.title
            padding: DesktopTokens.px(22)
            bottomPadding: 0
            color: Theme.label
            font.family: Theme.bodyFont
            font.pixelSize: DesktopTokens.px(20)
            font.weight: Font.ExtraBold
            wrapMode: Text.WordWrap
        }
        background: Rectangle { radius: DesktopTokens.px(16); color: Theme.shell; border.color: Theme.seam }
        contentItem: Text {
            id: replayCopy
            width: replayConfirmation.contentWidth
            text: qsTr("OpenNOW will restart to show the introduction and setup steps. Your saved preferences and account will not be reset.")
            color: Theme.textMuted
            font.family: Theme.bodyFont
            font.pixelSize: DesktopTokens.bodySize
            wrapMode: Text.WordWrap
        }
        footer: DialogButtonBox {
            padding: DesktopTokens.px(16)
            implicitHeight: DesktopTokens.controlHeight + topPadding + bottomPadding
            background: Item {}
            DesktopSettingsButton {
                objectName: "replayOnboardingCancel"
                text: qsTr("Cancel")
                DialogButtonBox.buttonRole: DialogButtonBox.RejectRole
            }
            DesktopSettingsButton {
                objectName: "replayOnboardingConfirm"
                text: qsTr("Restart and replay")
                primary: true
                enabled: ShellStore.onboardingReplayAvailable
                DialogButtonBox.buttonRole: DialogButtonBox.AcceptRole
            }
        }
        onAccepted: ShellStore.replayOnboarding()
    }
}

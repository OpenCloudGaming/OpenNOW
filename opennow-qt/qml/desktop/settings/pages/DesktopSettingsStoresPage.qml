import QtQuick
import QtQuick.Controls
import QtQuick.Layouts
import OpenNOW

DesktopSettingsPanel {
    id: page
    objectName: "desktopStoresSettings"
    required property real availableWidth
    required property var settingsScreen
    readonly property var orderedAccounts: ShellStore.gameAccounts.slice().sort((a, b) =>
        Number(a.sortOrder || 0) - Number(b.sortOrder || 0)
        || String(a.provider).localeCompare(String(b.provider)))

    width: page.availableWidth; paperStyle: true
    DesktopSettingsSection {
        text: qsTr("GAME STORES")
        description: qsTr("%1 stores from your NVIDIA account").arg(ShellStore.gameAccounts.length)
        DesktopSettingsButton { text: qsTr("Refresh status"); enabled: ShellStore.gameAccountsState !== "loading"; onClicked: ShellStore.refreshGameAccounts() }
    }
    RowLayout {
        x: DesktopTokens.settingsInset
        width: parent.width - DesktopTokens.settingsInset * 2
        spacing: DesktopTokens.px(12)
        Text {
            objectName: "storeSyncNotice"
            Layout.fillWidth: true
            bottomPadding: DesktopTokens.px(12); wrapMode: Text.WordWrap
            text: ShellStore.gameAccountMessage || qsTr("Linking happens on NVIDIA's side.")
            color: Theme.textMuted; font.family: Theme.bodyFont; font.pixelSize: DesktopTokens.captionSize
        }
        DesktopSettingsButton {
            visible: ShellStore.syncOperation !== null
            text: qsTr("Stop waiting")
            onClicked: ShellStore.cancelSyncObservation()
        }
    }
    Repeater {
        model: page.orderedAccounts
        delegate: DesktopSettingsRow {
            required property int index
            required property var modelData
            readonly property var status: page.settingsScreen.storeStatus(modelData)
            width: parent.width; paperStyle: true; rowHeight: DesktopTokens.px(56)
            leadingLetter: page.settingsScreen.storeLetter(modelData); leadingIcon: page.settingsScreen.storeIcon(modelData); leadingColor: page.settingsScreen.storeAccent(modelData)
            title: modelData.label || modelData.provider; description: page.settingsScreen.storeDescription(modelData)
            showDivider: index < page.orderedAccounts.length-1
            Item {
                width: DesktopTokens.px(132); height: DesktopTokens.controlHeight
                Rectangle { width: DesktopTokens.px(6); height: width; radius: width / 2; anchors.verticalCenter: parent.verticalCenter; color: status.color }
                Text { x: DesktopTokens.px(12); width: parent.width - x; anchors.verticalCenter: parent.verticalCenter; text: status.text; color: status.color; font.family: Theme.monoFont; font.pixelSize: DesktopTokens.px(10); font.weight: Font.Bold; elide: Text.ElideRight }
            }
            DesktopSettingsButton { width: DesktopTokens.px(132); text: status.action; compact: true; primary: Boolean(status.primary); enabled: !ShellStore.syncOperation && ShellStore.gameAccountsState !== "loading" && (modelData.supportsLinking || modelData.supportsSync); onClicked: page.settingsScreen.runStoreAction(modelData) }
        }
    }
}

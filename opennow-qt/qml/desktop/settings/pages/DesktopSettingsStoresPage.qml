import QtQuick
import QtQuick.Controls
import QtQuick.Layouts
import OpenNOW

DesktopSettingsPanel {
    id: page
    required property real availableWidth
    required property var settingsScreen

    width: page.availableWidth; paperStyle: true
    Item {
        width: parent.width; height: 56
        Column {
            x: 20; y: 10; spacing: 2; width: parent.width-220
            Text { text: qsTr("GAME STORES"); color: Theme.textMuted; font.family: Theme.bodyFont; font.pixelSize: 12; font.weight: Font.ExtraBold; font.letterSpacing: 1.2 }
            Text { width: parent.width; text: qsTr("%1 stores from your NVIDIA account").arg(ShellStore.gameAccounts.length); elide: Text.ElideRight; color: Theme.textMuted; font.family: Theme.bodyFont; font.pixelSize: DesktopTokens.px(12.5) }
        }
        DesktopSettingsButton { anchors.right: parent.right; anchors.rightMargin: 16; anchors.verticalCenter: parent.verticalCenter; text: ShellStore.gameAccountsState === "loading" ? qsTr("Syncing…") : qsTr("Sync now"); enabled: ShellStore.gameAccountsState !== "loading"; onClicked: ShellStore.refreshGameAccounts() }
    }
    Repeater {
        model: ShellStore.gameAccounts
        delegate: DesktopSettingsRow {
            required property int index
            required property var modelData
            readonly property var status: page.settingsScreen.storeStatus(modelData)
            width: parent.width; paperStyle: true; rowHeight: 56
            leadingLetter: page.settingsScreen.storeLetter(modelData); leadingIcon: page.settingsScreen.storeIcon(modelData); leadingColor: page.settingsScreen.storeAccent(modelData)
            title: modelData.label || modelData.provider; description: page.settingsScreen.storeDescription(modelData)
            showDivider: index < ShellStore.gameAccounts.length-1
            Item {
                width: 112; height: 28
                Rectangle { width: 6; height: 6; radius: 3; anchors.verticalCenter: parent.verticalCenter; color: status.color }
                Text { x: 12; width: 100; anchors.verticalCenter: parent.verticalCenter; text: status.text; color: status.color; font.family: Theme.monoFont; font.pixelSize: 10; font.weight: Font.Bold; elide: Text.ElideRight }
            }
            DesktopSettingsButton { width: 92; text: status.action; compact: true; primary: Boolean(status.primary); enabled: ShellStore.gameAccountsState !== "loading"; onClicked: page.settingsScreen.runStoreAction(modelData) }
        }
    }
    Text {
        x: 20; width: parent.width-40; topPadding: 12; bottomPadding: 16; wrapMode: Text.WordWrap
        text: ShellStore.gameAccountMessage || qsTr("Linking happens on NVIDIA's side.")
        color: Theme.textMuted; font.family: Theme.bodyFont; font.pixelSize: 12
    }
}

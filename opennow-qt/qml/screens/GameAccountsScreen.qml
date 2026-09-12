import QtQuick
import QtQuick.Controls
import OpenNOW

FocusScope {
    id: root
    readonly property var selectedAccount: accountList.currentIndex >= 0 && accountList.currentIndex < ShellStore.gameAccounts.length
        ? ShellStore.gameAccounts[accountList.currentIndex] : null

    ScreenBackground { tint: "#162237" }

    GlassPanel {
        x: 96; y: 128; width: root.width * 0.56; height: root.height - 264; panelRadius: 40
        Column {
            anchors.fill: parent; anchors.margins: 34; spacing: 14
            Text { text: qsTr("Connected game stores"); color: Theme.label; font.family: Theme.displayFont; font.pixelSize: 36; font.weight: Font.Black }
            Text { text: qsTr("Link and sync your libraries directly with GeForce NOW."); color: Theme.textMuted; font.family: Theme.bodyFont; font.pixelSize: 16 }
            ListView {
                id: accountList
                width: parent.width; height: parent.height - 112; spacing: 8; clip: true
                model: ShellStore.gameAccounts
                Component.onCompleted: currentIndex = model.length ? Math.min(ShellStore.focusIndex("game-accounts"), model.length - 1) : -1
                onCountChanged: if (count > 0 && currentIndex < 0) currentIndex = Math.min(ShellStore.focusIndex("game-accounts"), count - 1)
                onCurrentIndexChanged: if (currentIndex >= 0) ShellStore.rememberFocus("game-accounts", currentIndex)
                focus: true
                KeyNavigation.right: actionButton
                delegate: ItemDelegate {
                    required property var modelData
                    required property int index
                    width: accountList.width; height: 82; focusPolicy: Qt.StrongFocus
                    highlighted: ListView.isCurrentItem
                    onClicked: accountList.currentIndex = index
                    background: Rectangle {
                        radius: 24; color: parent.highlighted ? Theme.glassStrong : Theme.glass
                        border.color: parent.highlighted ? Theme.focus : Theme.seam
                        border.width: parent.highlighted ? 3 : 1
                    }
                    contentItem: Row {
                        spacing: 16
                        Rectangle {
                            width: 52; height: 52; radius: 17
                            color: modelData.isConnected ? Theme.mint : Theme.violet
                            Text { anchors.centerIn: parent; text: String(modelData.label || "G").slice(0, 1); color: Theme.contrastText(modelData.isConnected ? Theme.mint : Theme.violet); font.pixelSize: 22; font.weight: Font.Black }
                        }
                        Column {
                            anchors.verticalCenter: parent.verticalCenter; width: parent.width - 205; spacing: 2
                            Text { text: modelData.label || modelData.provider; color: Theme.label; font.family: Theme.bodyFont; font.pixelSize: 18; font.weight: Font.Bold }
                            Text { text: modelData.displayName || (modelData.isConnected ? qsTr("%1 synced games").arg(modelData.syncedGames) : qsTr("Not connected")); color: Theme.textMuted; font.family: Theme.bodyFont; font.pixelSize: 13 }
                        }
                        Text {
                            anchors.verticalCenter: parent.verticalCenter
                            text: modelData.status === "connected" ? qsTr("CONNECTED") : modelData.status === "sync_error" ? qsTr("SYNC ERROR") : modelData.status === "expired" ? qsTr("EXPIRED") : qsTr("AVAILABLE")
                            color: modelData.status === "connected" ? Theme.mint : modelData.status === "not_connected" ? Theme.textMuted : Theme.coral
                            font.family: Theme.monoFont; font.pixelSize: 11; font.weight: Font.Black
                        }
                    }
                }
                ScrollIndicator.vertical: ScrollIndicator {}
            }
        }
    }

    GlassPanel {
        x: root.width * 0.69; y: 170; width: root.width * 0.24; height: 500; panelRadius: 36; strong: true
        Column {
            anchors.fill: parent; anchors.margins: 26; spacing: 13
            Text { width: parent.width; text: root.selectedAccount ? root.selectedAccount.label : qsTr("Game accounts"); color: Theme.label; font.family: Theme.displayFont; font.pixelSize: 27; font.weight: Font.Black; wrapMode: Text.WordWrap }
            Text { width: parent.width; text: ShellStore.gameAccountMessage || (root.selectedAccount && root.selectedAccount.isConnected ? qsTr("Your linked library is managed by NVIDIA.") : qsTr("Connect this store in your browser, then return to OpenNOW.")); color: Theme.textMuted; font.family: Theme.bodyFont; font.pixelSize: 15; wrapMode: Text.WordWrap; lineHeight: 1.2 }
            GlassButton {
                id: actionButton; width: parent.width; glyph: "A"; primary: true
                enabled: root.selectedAccount && (root.selectedAccount.supportsLinking || root.selectedAccount.supportsSync)
                text: root.selectedAccount && root.selectedAccount.isConnected
                      ? (root.selectedAccount.supportsSync ? qsTr("Sync library") : qsTr("Connected"))
                      : qsTr("Connect account")
                onClicked: {
                    if (!root.selectedAccount)
                        return
                    if (root.selectedAccount.isConnected)
                        ShellStore.syncGameAccount(root.selectedAccount.provider)
                    else
                        ShellStore.startAccountLink(root.selectedAccount.provider)
                }
                Component.onCompleted: forceActiveFocus()
            }
            GlassButton {
                width: parent.width; glyph: "X"; danger: true; text: qsTr("Disconnect")
                enabled: root.selectedAccount && root.selectedAccount.isConnected && root.selectedAccount.supportsLinking
                onClicked: ShellStore.unlinkGameAccount(root.selectedAccount.provider)
            }
            GlassButton { width: parent.width; glyph: "↻"; text: qsTr("Refresh status"); onClicked: ShellStore.refreshGameAccounts() }
            GlassButton { width: parent.width; glyph: "B"; text: qsTr("Back to settings"); onClicked: AppController.navigate("settings-account") }
        }
    }

    Component.onCompleted: ShellStore.refreshGameAccounts()
    AppChrome { anchors.fill: parent; title: qsTr("Game accounts"); currentRoute: "settings"; onRouteRequested: route => AppController.navigate(route) }
}

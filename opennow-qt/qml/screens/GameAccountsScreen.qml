import QtQuick
import QtQuick.Controls
import OpenNOW

FocusScope {
    id: root
    property var unlinkTarget: null
    property bool unlinkOpen: false
    property Item unlinkOrigin: null
    readonly property var selectedAccount: accountList.currentIndex >= 0 && accountList.currentIndex < ShellStore.gameAccounts.length
        ? ShellStore.gameAccounts[accountList.currentIndex] : null
    readonly property string primaryAction: ShellStore.gameAccountAction(selectedAccount)

    function requestUnlink(origin) {
        if (!selectedAccount)
            return
        unlinkTarget = selectedAccount
        unlinkOrigin = origin
        unlinkOpen = true
    }

    function closeUnlink() {
        unlinkOpen = false
        Qt.callLater(() => {
            if (root.unlinkOpen)
                return
            if (root.unlinkOrigin && root.unlinkOrigin.enabled && root.unlinkOrigin.visible) root.unlinkOrigin.forceActiveFocus()
            else root.focusDefault()
        })
    }

    function confirmUnlink() {
        const target = unlinkTarget
        closeUnlink()
        if (target)
            ShellStore.unlinkGameAccount(target.provider)
    }

    function focusDefault() {
        if (accountList.count > 0) accountList.forceActiveFocus()
        else accountActions.focusFirst()
    }

    Component.onCompleted: {
        ShellStore.refreshGameAccounts()
        focusDefault()
    }
    Keys.onPressed: event => {
        if (event.key === Qt.Key_Up || event.key === Qt.Key_Down || event.key === Qt.Key_Left || event.key === Qt.Key_Right) {
            root.focusDefault()
            event.accepted = true
        }
    }

    ScreenBackground { tint: "#162237" }

    GlassPanel {
        x: 96; y: 124; width: 1080; height: root.height - 288; panelRadius: 40
        Column {
            x: 56; y: 34; width: parent.width - 112; spacing: 6
            Text { text: qsTr("Connected game stores"); color: Theme.label; font.family: Theme.displayFont; font.pixelSize: 32; font.weight: Font.Black; font.letterSpacing: -0.3 }
            Text { width: parent.width; text: qsTr("Link and sync your libraries directly with GeForce NOW."); color: Theme.textMuted; font.family: Theme.bodyFont; font.pixelSize: 17; font.weight: Font.DemiBold; elide: Text.ElideRight }
        }
        ListView {
            id: accountList
            anchors.fill: parent
            anchors.leftMargin: 24; anchors.rightMargin: 24
            anchors.topMargin: 124; anchors.bottomMargin: 18
            leftMargin: 10; rightMargin: 10; topMargin: 10; bottomMargin: 10
            spacing: 2; clip: true; keyNavigationWraps: false
            highlightMoveDuration: AppController.reducedMotion ? 0 : 260
            model: ShellStore.gameAccounts
            Component.onCompleted: currentIndex = model.length ? Math.min(ShellStore.focusIndex("game-accounts"), model.length - 1) : -1
            onCountChanged: if (count > 0 && currentIndex < 0) currentIndex = Math.min(ShellStore.focusIndex("game-accounts"), count - 1)
            onCurrentIndexChanged: if (currentIndex >= 0) ShellStore.rememberFocus("game-accounts", currentIndex)
            Keys.onRightPressed: accountActions.focusFirst()
            Keys.onReturnPressed: event => { if (!event.isAutoRepeat) accountActions.focusFirst() }
            Keys.onEnterPressed: event => { if (!event.isAutoRepeat) accountActions.focusFirst() }
            delegate: ConsoleListRow {
                id: storeRow
                required property var modelData
                required property int index
                width: ListView.view.width - 20
                height: 92
                focusPolicy: Qt.NoFocus
                title: modelData.label || modelData.provider
                description: modelData.displayName || (modelData.isConnected ? qsTr("%1 synced games").arg(modelData.syncedGames) : qsTr("Not connected"))
                badge: modelData.status === "connected" ? qsTr("CONNECTED") : modelData.status === "sync_error" ? qsTr("SYNC ERROR") : modelData.status === "expired" ? qsTr("EXPIRED") : qsTr("AVAILABLE")
                badgeColor: modelData.status === "connected" ? Theme.mint : modelData.status === "not_connected" ? Theme.textMuted : Theme.coral
                currentItem: ListView.isCurrentItem
                ringVisible: ListView.isCurrentItem && accountList.activeFocus
                onClicked: {
                    accountList.currentIndex = index
                    accountList.forceActiveFocus()
                }
                ConsoleStoreMark { store: storeRow.modelData.provider; markSize: 52 }
            }
            ScrollIndicator.vertical: ScrollIndicator {}
        }
    }

    GlassPanel {
        x: 1200; y: 124; width: root.width - 1296; height: root.height - 288; panelRadius: 40
        Column {
            id: accountHeader
            x: 40; y: 40; width: parent.width - 80; spacing: 12
            Text { width: parent.width; text: root.selectedAccount ? root.selectedAccount.label : qsTr("Game accounts"); color: Theme.label; font.family: Theme.displayFont; font.pixelSize: 32; font.weight: Font.Black; wrapMode: Text.WordWrap; maximumLineCount: 2; elide: Text.ElideRight }
            Text { width: parent.width; text: ShellStore.gameAccountMessage || (root.selectedAccount && root.selectedAccount.isConnected ? qsTr("Your linked library is managed by NVIDIA.") : qsTr("Connect this store in your browser, then return to OpenNOW.")); color: Theme.textMuted; font.family: Theme.bodyFont; font.pixelSize: 17; font.weight: Font.DemiBold; wrapMode: Text.WordWrap; lineHeight: 1.2 }
        }
        ConsoleActionColumn {
            id: accountActions
            anchors.top: accountHeader.bottom; anchors.topMargin: 28
            x: 40; width: parent.width - 80
            returnTarget: accountList.count > 0 ? accountList : null
            ConsoleActionButton {
                id: actionButton; width: parent.width; primary: root.primaryAction !== "unlink"
                enabled: !ShellStore.syncOperation && root.primaryAction !== "none"
                danger: root.primaryAction === "unlink"
                text: root.primaryAction === "sync" ? qsTr("Sync library")
                    : root.primaryAction === "unlink" ? qsTr("Disconnect")
                    : root.selectedAccount && root.selectedAccount.isConnected ? qsTr("Reconnect") : qsTr("Connect account")
                onClicked: {
                    if (!root.selectedAccount)
                        return
                    if (root.primaryAction === "sync")
                        ShellStore.syncGameAccount(root.selectedAccount.provider)
                    else if (root.primaryAction === "link")
                        ShellStore.startAccountLink(root.selectedAccount.provider)
                    else if (root.primaryAction === "unlink")
                        root.requestUnlink(actionButton)
                }
            }
            ConsoleActionButton {
                id: disconnectButton
                width: parent.width; danger: true; text: qsTr("Disconnect")
                visible: root.primaryAction !== "unlink"
                enabled: Boolean(root.selectedAccount && root.selectedAccount.isConnected && root.selectedAccount.supportsLinking)
                onClicked: root.requestUnlink(disconnectButton)
            }
            ConsoleActionButton { width: parent.width; text: qsTr("Refresh status"); onClicked: ShellStore.refreshGameAccounts() }
            ConsoleActionButton { width: parent.width; visible: ShellStore.syncOperation !== null; text: qsTr("Stop waiting"); onClicked: ShellStore.cancelSyncObservation() }
            ConsoleActionButton { width: parent.width; text: qsTr("Back to settings"); onClicked: AppController.navigate("settings-account") }
        }
    }

    AppChrome {
        anchors.fill: parent; title: qsTr("Game accounts"); currentRoute: "settings"
        leftHints: [{glyph:"B", label:qsTr("Back")}]
        rightHints: [{glyph:"A", label:qsTr("Select")}]
        onRouteRequested: route => AppController.navigate(route)
    }

    ConsoleWarningSheet {
        objectName: "gameAccountUnlinkConfirmation"
        opened: root.unlinkOpen
        eyebrow: qsTr("Game accounts")
        title: qsTr("Disconnect %1?").arg(root.unlinkTarget ? root.unlinkTarget.label || root.unlinkTarget.provider : "")
        message: qsTr("GeForce NOW unlinks this store account. You can connect it again from this screen.")
        safeText: qsTr("Stay connected")
        actionText: qsTr("Disconnect")
        safeButtonObjectName: "gameAccountUnlinkSafe"
        actionButtonObjectName: "gameAccountUnlinkAction"
        onSafeRequested: root.closeUnlink()
        onActionRequested: root.confirmUnlink()
    }
}

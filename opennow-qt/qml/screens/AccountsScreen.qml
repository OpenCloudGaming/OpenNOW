import QtQuick
import QtQuick.Controls
import OpenNOW

FocusScope {
    id: root
    property string confirmKind: ""
    property var confirmTarget: null
    property bool confirmOpen: false
    readonly property string activeUserId: ShellStore.authSession ? ShellStore.authSession.user.userId : ""
    readonly property var selectedAccount: accountList.currentIndex >= 0 && accountList.currentIndex < ShellStore.savedAccounts.length
        ? ShellStore.savedAccounts[accountList.currentIndex] : null

    function openConfirm(kind) {
        confirmTarget = kind === "forget" ? selectedAccount : null
        confirmKind = kind
        confirmOpen = true
    }

    function closeConfirm() {
        const origin = confirmKind === "forget" ? forgetButton : logoutAllButton
        confirmOpen = false
        Qt.callLater(() => {
            if (root.confirmOpen)
                return
            if (origin.enabled && origin.visible) origin.forceActiveFocus()
            else root.focusDefault()
        })
    }

    function confirmAction() {
        const target = confirmTarget
        closeConfirm()
        if (confirmKind === "logout-all")
            ShellStore.logoutAll()
        else if (confirmKind === "forget" && target)
            ShellStore.removeAccount(target.userId)
    }

    function focusDefault() {
        if (accountList.count > 0) accountList.forceActiveFocus()
        else accountActions.focusFirst()
    }

    Component.onCompleted: focusDefault()
    Keys.onPressed: event => {
        if (event.key === Qt.Key_Up || event.key === Qt.Key_Down || event.key === Qt.Key_Left || event.key === Qt.Key_Right) {
            root.focusDefault()
            event.accepted = true
        }
    }

    ScreenBackground { tint: "#171E35" }

    GlassPanel {
        id: listPanel
        x: 96; y: 124; width: 1080; height: root.height - 288; panelRadius: 40
        Column {
            x: 56; y: 34; width: parent.width - 112; spacing: 6
            Text { text: qsTr("Who’s playing?"); color: Theme.label; font.family: Theme.displayFont; font.pixelSize: 32; font.weight: Font.Black; font.letterSpacing: -0.3 }
            Text { width: parent.width; text: qsTr("Saved NVIDIA sessions stay protected by your operating system."); color: Theme.textMuted; font.family: Theme.bodyFont; font.pixelSize: 17; font.weight: Font.DemiBold; elide: Text.ElideRight }
        }
        ListView {
            id: accountList
            anchors.fill: parent
            anchors.leftMargin: 24; anchors.rightMargin: 24
            anchors.topMargin: 124; anchors.bottomMargin: 18
            leftMargin: 10; rightMargin: 10; topMargin: 10; bottomMargin: 10
            spacing: 2; clip: true; keyNavigationWraps: false
            highlightMoveDuration: AppController.reducedMotion ? 0 : 260
            model: ShellStore.savedAccounts
            Component.onCompleted: currentIndex = model.length ? Math.min(ShellStore.focusIndex("accounts"), model.length - 1) : -1
            onCountChanged: if (count > 0 && currentIndex < 0) currentIndex = Math.min(ShellStore.focusIndex("accounts"), count - 1)
            onCurrentIndexChanged: if (currentIndex >= 0) ShellStore.rememberFocus("accounts", currentIndex)
            Keys.onRightPressed: accountActions.focusFirst()
            Keys.onReturnPressed: event => { if (!event.isAutoRepeat) accountActions.focusFirst() }
            Keys.onEnterPressed: event => { if (!event.isAutoRepeat) accountActions.focusFirst() }
            delegate: ConsoleListRow {
                id: accountRow
                required property var modelData
                required property int index
                readonly property bool active: modelData.userId === root.activeUserId
                width: ListView.view.width - 20
                height: 96
                focusPolicy: Qt.NoFocus
                title: modelData.displayName || qsTr("NVIDIA profile")
                description: modelData.email || modelData.providerCode || "GeForce NOW"
                badge: modelData.hasPin ? qsTr("PIN locked") : (active ? qsTr("Active") : qsTr("Saved"))
                badgeColor: active ? Theme.mint : modelData.hasPin ? Theme.violet : Theme.textMuted
                currentItem: ListView.isCurrentItem
                ringVisible: ListView.isCurrentItem && accountList.activeFocus
                onClicked: {
                    accountList.currentIndex = index
                    accountList.forceActiveFocus()
                }
                Rectangle {
                    width: 58; height: 58; radius: 29
                    color: accountRow.active ? Theme.mint : Theme.violet
                    Text {
                        anchors.centerIn: parent
                        text: String(accountRow.modelData.displayName || "P").slice(0, 1).toUpperCase()
                        color: Theme.contrastText(parent.color)
                        font.family: Theme.displayFont; font.pixelSize: 24; font.weight: Font.Black
                    }
                }
            }
        }
    }

    GlassPanel {
        x: 1200; y: 124; width: root.width - 1296; height: root.height - 288; panelRadius: 40
        Column {
            id: accountHeader
            x: 40; y: 40; width: parent.width - 80; spacing: 12
            Text {
                width: parent.width
                text: root.selectedAccount ? root.selectedAccount.displayName : qsTr("Add a profile")
                color: Theme.label; elide: Text.ElideRight
                font.family: Theme.displayFont; font.pixelSize: 32; font.weight: Font.Black
            }
            Text {
                width: parent.width; wrapMode: Text.WordWrap
                text: root.selectedAccount && root.selectedAccount.hasPin ? qsTr("A four-digit living-room PIN is required before this account can become active.") : qsTr("Profile PINs are local to this device and never sent to NVIDIA.")
                color: Theme.textMuted; font.family: Theme.bodyFont; font.pixelSize: 17; font.weight: Font.DemiBold; lineHeight: 1.2
            }
        }
        ConsoleActionColumn {
            id: accountActions
            anchors.top: accountHeader.bottom; anchors.topMargin: 28
            x: 40; width: parent.width - 80
            returnTarget: accountList.count > 0 ? accountList : null
            ConsoleActionButton {
                id: switchButton; width: parent.width; primary: true
                text: !root.selectedAccount ? qsTr("Add NVIDIA account") : (root.selectedAccount.userId === root.activeUserId ? qsTr("Currently active") : qsTr("Switch profile"))
                enabled: ShellStore.ready && ShellStore.accountSwitchRequestId === ""
                    && (!root.selectedAccount || root.selectedAccount.userId !== root.activeUserId)
                onClicked: {
                    if (!root.selectedAccount)
                        ShellStore.beginAddAccount()
                    else if (root.selectedAccount.hasPin)
                        ShellStore.openPin("unlock", root.selectedAccount)
                    else
                        ShellStore.switchAccount(root.selectedAccount.userId, "")
                }
            }
            ConsoleActionButton {
                width: parent.width
                text: root.selectedAccount && root.selectedAccount.hasPin ? qsTr("Remove profile PIN") : qsTr("Set profile PIN")
                enabled: root.selectedAccount !== null
                onClicked: ShellStore.openPin(root.selectedAccount.hasPin ? "clear" : "set", root.selectedAccount)
            }
            ConsoleActionButton {
                width: parent.width; text: qsTr("Add another account")
                onClicked: ShellStore.beginAddAccount()
            }
            ConsoleActionButton {
                id: forgetButton
                width: parent.width; text: qsTr("Forget this profile"); danger: true
                enabled: root.selectedAccount !== null
                onClicked: root.openConfirm("forget")
            }
            ConsoleActionButton {
                id: logoutAllButton
                width: parent.width; text: qsTr("Sign out all profiles"); danger: true
                enabled: ShellStore.savedAccounts.length > 0
                onClicked: root.openConfirm("logout-all")
            }
            ConsoleActionButton { width: parent.width; text: qsTr("Back to account settings"); onClicked: AppController.navigate("settings-account") }
            Text {
                objectName: "accountActionError"
                width: parent.width
                visible: text !== ""
                text: ShellStore.accountMessage
                color: Theme.coral
                font.family: Theme.bodyFont
                font.pixelSize: 17
                font.weight: Font.Bold
                wrapMode: Text.WordWrap
                Accessible.role: Accessible.AlertMessage
                Accessible.name: text
            }
        }
    }

    AppChrome {
        anchors.fill: parent; title: qsTr("Saved accounts"); currentRoute: "settings"
        leftHints: [{glyph:"B", label:qsTr("Back")}]
        rightHints: [{glyph:"A", label:qsTr("Select")}]
        onRouteRequested: route => AppController.navigate(route)
    }

    ConsoleWarningSheet {
        id: accountConfirm
        objectName: "accountsConfirmation"
        opened: root.confirmOpen
        eyebrow: root.confirmKind === "forget" ? qsTr("Saved accounts") : qsTr("Sign out")
        title: root.confirmKind === "forget"
            ? qsTr("Forget %1?").arg(root.confirmTarget ? root.confirmTarget.displayName || qsTr("NVIDIA profile") : qsTr("NVIDIA profile"))
            : qsTr("Sign out every saved profile?")
        message: root.confirmKind === "forget"
            ? qsTr("This removes the saved NVIDIA session and local PIN for this profile. Captures and settings stay on this device.")
            : qsTr("This removes all saved NVIDIA sessions and every local profile PIN. Captures and settings stay on this device.")
        safeText: root.confirmKind === "forget" ? qsTr("Keep profile") : qsTr("Keep profiles")
        actionText: root.confirmKind === "forget" ? qsTr("Forget this profile") : qsTr("Sign out all")
        safeButtonObjectName: "accountsConfirmSafe"
        actionButtonObjectName: "accountsConfirmAction"
        onSafeRequested: root.closeConfirm()
        onActionRequested: root.confirmAction()
    }
}

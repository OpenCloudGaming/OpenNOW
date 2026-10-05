import QtQuick
import OpenNOW

FocusScope {
    id: root
    property double clockMs: Date.now()
    property bool providerSheetOpen: false
    property bool signOutOpen: false
    readonly property bool connected: ShellStore.signedIn && !ShellStore.addingAccount
    readonly property var challenge: ShellStore.authChallenge
    readonly property var qrRows: challenge && challenge.qrRows ? challenge.qrRows : []
    readonly property int qrSize: qrRows.length
    readonly property int secondsLeft: challenge ? Math.max(0, Math.ceil((Number(challenge.expiresAt) - clockMs) / 1000)) : 0
    readonly property string timeLeft: Math.floor(secondsLeft / 60) + ":" + String(secondsLeft % 60).padStart(2, "0")
    readonly property bool canChooseProvider: !connected && !challenge && ShellStore.ready
    readonly property var providerOptions: ShellStore.providers.map(provider => ({label: provider.displayName || provider.idpId, value: provider.idpId}))

    function openProviderSheet() {
        if (!canChooseProvider)
            return
        providerSheetOpen = true
    }

    function closeProviderSheet() {
        providerSheetOpen = false
        Qt.callLater(() => { if (!root.providerSheetOpen) providerButton.visible ? providerButton.forceActiveFocus() : signInActions.focusFirst() })
    }

    function closeSignOut() {
        signOutOpen = false
        Qt.callLater(() => { if (!root.signOutOpen) signOutButton.visible ? signOutButton.forceActiveFocus() : signInActions.focusFirst() })
    }

    onConnectedChanged: Qt.callLater(() => { if (!root.signOutOpen && !root.providerSheetOpen) signInActions.focusFirst() })
    Keys.onPressed: event => {
        if (event.key === Qt.Key_X && root.canChooseProvider) {
            if (!event.isAutoRepeat) root.openProviderSheet()
            event.accepted = true
        } else if ((event.key === Qt.Key_Escape || event.key === Qt.Key_Back) && root.challenge) {
            if (!event.isAutoRepeat) ShellStore.cancelDeviceLogin()
            event.accepted = true
        } else if (event.key === Qt.Key_Up || event.key === Qt.Key_Down) {
            signInActions.focusFirst()
            event.accepted = true
        }
    }

    Timer { interval: 1000; repeat: true; running: root.challenge !== null; onTriggered: root.clockMs = Date.now() }
    ScreenBackground { tint: "#1B2A42" }

    Column {
        id: loginControls
        x: 120
        y: Math.max(150, Math.round((root.height - implicitHeight) / 2))
        width: Math.min(820, root.width - 120 - qrPanel.width - 200)
        spacing: 24

        Text {
            width: parent.width
            text: root.connected ? qsTr("GeForce NOW account") : qsTr("Sign in")
            color: root.connected ? Theme.mint : Theme.focus
            font.family: Theme.monoFont; font.pixelSize: 15; font.weight: Font.Bold; font.letterSpacing: 2.4
            font.capitalization: Font.AllUppercase
        }
        Text {
            width: parent.width
            text: root.connected ? qsTr("You’re ready to play.") : qsTr("Bring your games to the big screen.")
            color: Theme.label
            wrapMode: Text.WordWrap
            font.family: Theme.displayFont
            font.pixelSize: 64
            font.weight: Font.Black
            font.letterSpacing: -1.4
            lineHeight: 1.02
        }
        Text {
            width: parent.width
            wrapMode: Text.WordWrap
            text: root.connected
                  ? qsTr("Signed in as %1. Your NVIDIA password never passes through OpenNOW.").arg(ShellStore.authSession.user.displayName)
                  : ShellStore.providerDiscoveryDegraded
                      ? ShellStore.providers.length ? qsTr("Provider discovery is unavailable. Known providers are shown.")
                          : qsTr("No providers are available. Refresh to try again.")
                  : qsTr("OpenNOW connects to your GeForce NOW account without storing your NVIDIA password. Sign in from your phone, then come straight back to the controller.")
            color: Qt.rgba(Theme.label.r, Theme.label.g, Theme.label.b, 0.76)
            font.family: Theme.bodyFont
            font.pixelSize: 21
            font.weight: Font.DemiBold
            lineHeight: 1.3
        }
        Text {
            objectName: "consoleBugReportNotice"
            width: parent.width
            visible: ShellStore.bugReports.enabled
            wrapMode: Text.WordWrap
            text: qsTr("Experimental: usage & bug reports are on. Usage statistics and problem reports go to the developer with your GeForce NOW username and redacted logs. Turn this off in Settings → Account.")
            color: Theme.accentColor("amber")
            font.family: Theme.bodyFont
            font.pixelSize: 16
            font.weight: Font.DemiBold
            lineHeight: 1.3
        }
        Item { width: 1; height: 8 }
        ConsoleActionColumn {
            id: signInActions
            width: Math.min(640, parent.width)
            ConsoleActionButton {
                id: signIn
                width: parent.width
                text: root.connected ? qsTr("Continue to your games")
                      : ShellStore.authState === "starting" ? qsTr("Contacting provider…")
                      : ShellStore.authState === "completing" ? qsTr("Loading your profile…")
                      : ShellStore.authState === "error" ? qsTr("Try again")
                      : root.challenge ? qsTr("Open provider page") : qsTr("Start device sign-in")
                glyph: "A"
                primary: true
                enabled: ShellStore.ready && ShellStore.authState !== "starting" && ShellStore.authState !== "completing"
                    && (root.connected || root.challenge !== null || ShellStore.selectedProvider !== null)
                Component.onCompleted: forceActiveFocus()
                onClicked: {
                    if (root.connected)
                        AppController.navigate("library")
                    else if (root.challenge)
                        Qt.openUrlExternally(root.challenge.verificationUriComplete || root.challenge.verificationUri)
                    else
                        ShellStore.startDeviceLogin(ShellStore.selectedProvider ? ShellStore.selectedProvider.idpId : "")
                }
            }
            ConsoleActionButton {
                id: providerButton
                width: parent.width
                visible: !root.connected
                text: root.challenge ? qsTr("Cancel this sign-in")
                      : qsTr("Provider · %1").arg(ShellStore.selectedProvider ? ShellStore.selectedProvider.displayName : qsTr("Select a provider"))
                glyph: root.challenge ? "B" : "X"
                enabled: ShellStore.ready
                onClicked: {
                    if (root.challenge)
                        ShellStore.cancelDeviceLogin()
                    else
                        root.openProviderSheet()
                }
            }
            ConsoleActionButton {
                objectName: "consoleRefreshProviders"
                width: parent.width
                visible: !root.connected && ShellStore.providerDiscoveryDegraded && !root.challenge
                text: qsTr("Refresh providers")
                enabled: ShellStore.ready && ShellStore.providersRequestId === ""
                onClicked: ShellStore.refreshProviders(true)
            }
            ConsoleActionButton {
                id: signOutButton
                width: parent.width
                visible: root.connected
                text: qsTr("Sign out")
                danger: true
                onClicked: root.signOutOpen = true
            }
        }
        Row {
            spacing: 12
            Rectangle { anchors.verticalCenter: parent.verticalCenter; width: 10; height: 10; radius: 5; color: ShellStore.authState === "error" ? Theme.coral : Theme.mint }
            Text {
                width: Math.min(640, loginControls.width) - 22
                text: ShellStore.authState === "error" ? ShellStore.authMessage
                      : ShellStore.authMessage || (ShellStore.ready ? qsTr("No password is entered in OpenNOW") : qsTr("Starting the secure OpenNOW core…"))
                color: ShellStore.authState === "error" ? Theme.coral : Theme.textMuted
                elide: Text.ElideRight
                font.family: Theme.bodyFont
                font.pixelSize: 17
                font.weight: Font.DemiBold
                Accessible.role: ShellStore.authState === "error" ? Accessible.AlertMessage : Accessible.StaticText
                Accessible.name: text
            }
        }
    }

    GlassPanel {
        id: qrPanel
        x: root.width - width - 160
        anchors.verticalCenter: parent.verticalCenter
        width: 480
        height: qrColumn.implicitHeight + 80
        panelRadius: 40
        strong: true

        Column {
            id: qrColumn
            anchors.centerIn: parent
            spacing: 20

            Rectangle {
                anchors.horizontalCenter: parent.horizontalCenter
                width: 360
                height: 360
                radius: 28
                color: "#FFFFFF"

                Grid {
                    id: qrGrid
                    anchors.centerIn: parent
                    columns: root.qrSize
                    spacing: 0
                    visible: root.qrSize > 0
                    property real cellSize: root.qrSize > 0 ? Math.floor(320 / root.qrSize) : 0
                    Repeater {
                        model: root.qrSize * root.qrSize
                        Rectangle {
                            required property int index
                            width: qrGrid.cellSize
                            height: qrGrid.cellSize
                            color: root.qrRows[Math.floor(index / root.qrSize)].charAt(index % root.qrSize) === "1" ? "#000000" : "#FFFFFF"
                        }
                    }
                }
                Column {
                    anchors.centerIn: parent
                    spacing: 10
                    visible: root.qrSize === 0
                    Text { anchors.horizontalCenter: parent.horizontalCenter; text: root.connected ? "✓" : "◎"; color: "#111827"; font.pixelSize: 88; font.weight: Font.Black }
                    Text { anchors.horizontalCenter: parent.horizontalCenter; text: root.connected ? qsTr("Connected") : qsTr("Ready when you are"); color: "#111827"; font.family: Theme.bodyFont; font.pixelSize: 19; font.weight: Font.Bold }
                }
            }
            Text {
                anchors.horizontalCenter: parent.horizontalCenter
                text: root.challenge ? root.challenge.userCode : root.connected ? ShellStore.authSession.user.membershipTier : qsTr("Scan with your phone")
                color: Theme.label
                font.family: root.challenge ? Theme.monoFont : Theme.displayFont
                font.pixelSize: root.challenge ? 34 : 22
                font.weight: Font.Black
                font.letterSpacing: root.challenge ? 4 : 0
            }
            Text {
                anchors.horizontalCenter: parent.horizontalCenter
                width: 400
                horizontalAlignment: Text.AlignHCenter
                wrapMode: Text.WordWrap
                text: root.challenge ? qsTr("Expires in %1 · %2").arg(root.timeLeft).arg(root.challenge.verificationUri.replace(/^https?:\/\//, ""))
                                     : root.connected ? qsTr("GeForce NOW account") : qsTr("A real QR code appears after sign-in starts")
                color: Theme.textMuted
                font.family: Theme.bodyFont
                font.pixelSize: 16
                font.weight: Font.DemiBold
            }
        }
    }

    AppChrome { anchors.fill: parent; title: qsTr("Welcome to OpenNOW"); currentRoute: "home"; bottomVisible: false }

    ConsoleChoiceSheet {
        id: providerSheet
        objectName: "consoleProviderSheet"
        opened: root.providerSheetOpen
        eyebrow: qsTr("Sign in")
        title: qsTr("Provider")
        options: root.providerOptions
        currentIndex: root.providerOptions.findIndex(option => option.value === (ShellStore.selectedProvider ? ShellStore.selectedProvider.idpId : ""))
        onChosen: index => {
            ShellStore.selectedProviderIdpId = root.providerOptions[index].value
            root.closeProviderSheet()
        }
        onDismissed: root.closeProviderSheet()
    }

    ConsoleWarningSheet {
        objectName: "consoleSignOutConfirmation"
        opened: root.signOutOpen
        eyebrow: qsTr("Sign out")
        title: qsTr("Sign out of NVIDIA?")
        message: qsTr("OpenNOW removes the NVIDIA token from this PC. My games stay.")
        safeText: qsTr("Stay signed in")
        actionText: qsTr("Sign out")
        onSafeRequested: root.closeSignOut()
        onActionRequested: {
            root.closeSignOut()
            ShellStore.logout()
        }
    }
}

import QtQuick
import OpenNOW

FocusScope {
    id: root
    property double clockMs: Date.now()
    readonly property var challenge: ShellStore.authChallenge
    readonly property var qrRows: challenge && challenge.qrRows ? challenge.qrRows : []
    readonly property int qrSize: qrRows.length
    readonly property int secondsLeft: challenge ? Math.max(0, Math.ceil((Number(challenge.expiresAt) - clockMs) / 1000)) : 0
    readonly property string timeLeft: Math.floor(secondsLeft / 60) + ":" + String(secondsLeft % 60).padStart(2, "0")

    Timer { interval: 1000; repeat: true; running: root.challenge !== null; onTriggered: root.clockMs = Date.now() }
    ScreenBackground { tint: "#1B2A42" }

    Row {
        anchors.centerIn: parent
        spacing: 44

        GlassPanel {
            width: 720
            height: 532
            panelRadius: 40
            strong: true

            Column {
                anchors.fill: parent
                anchors.margins: 44
                spacing: 22

                Text {
                    width: parent.width
                    text: ShellStore.signedIn ? qsTr("You’re ready to play.") : qsTr("Bring your games to the big screen.")
                    color: Theme.label
                    font.family: Theme.displayFont
                    font.pixelSize: 38
                    font.weight: Font.Black
                }
                Text {
                    width: parent.width
                    wrapMode: Text.WordWrap
                    text: ShellStore.signedIn
                          ? qsTr("Signed in as %1. Your NVIDIA password never passes through OpenNOW.").arg(ShellStore.authSession.user.displayName)
                          : qsTr("OpenNOW connects to your GeForce NOW account without storing your NVIDIA password. Sign in from your phone, then come straight back to the controller.")
                    color: Theme.textMuted
                    font.family: Theme.bodyFont
                    font.pixelSize: 18
                    lineHeight: 1.4
                }
                GlassButton {
                    id: signIn
                    width: parent.width
                    text: ShellStore.signedIn ? qsTr("Continue to your games")
                          : ShellStore.authState === "starting" ? qsTr("Contacting NVIDIA…")
                          : ShellStore.authState === "completing" ? qsTr("Loading your profile…")
                          : root.challenge ? qsTr("Open NVIDIA sign-in") : qsTr("Start device sign-in")
                    glyph: "A"
                    primary: true
                    enabled: ShellStore.ready && ShellStore.authState !== "starting" && ShellStore.authState !== "completing"
                    Component.onCompleted: forceActiveFocus()
                    onClicked: {
                        if (ShellStore.signedIn)
                            AppController.navigate("library")
                        else if (root.challenge)
                            Qt.openUrlExternally(root.challenge.verificationUriComplete)
                        else
                            ShellStore.startDeviceLogin("")
                    }
                }
                GlassButton {
                    width: parent.width
                    visible: !ShellStore.signedIn
                    text: root.challenge ? qsTr("Cancel this sign-in") : ShellStore.providers.length > 1
                          ? qsTr("Provider · %1").arg(ShellStore.providers[0].displayName)
                          : qsTr("Provider · NVIDIA")
                    glyph: root.challenge ? "B" : "X"
                    enabled: ShellStore.ready
                    onClicked: {
                        if (root.challenge)
                            ShellStore.cancelDeviceLogin()
                        else
                            ShellStore.startDeviceLogin(ShellStore.providers.length ? ShellStore.providers[0].idpId : "")
                    }
                }
                GlassButton {
                    width: parent.width
                    visible: ShellStore.signedIn
                    text: qsTr("Sign out")
                    glyph: "B"
                    danger: true
                    onClicked: ShellStore.logout()
                }
                Row {
                    spacing: 12
                    Rectangle { width: 9; height: 9; radius: 5; color: ShellStore.authState === "error" ? Theme.coral : Theme.mint }
                    Text {
                        width: 590
                        text: ShellStore.authState === "error" ? ShellStore.authMessage
                              : ShellStore.authMessage || (ShellStore.ready ? qsTr("No password is entered in OpenNOW") : qsTr("Starting the secure OpenNOW core…"))
                        color: ShellStore.authState === "error" ? Theme.coral : Theme.textMuted
                        elide: Text.ElideRight
                        font.family: Theme.bodyFont
                        font.pixelSize: 14
                    }
                }
            }
        }

        GlassPanel {
            width: 360
            height: 412
            panelRadius: 36
            strong: true

            Column {
                anchors.centerIn: parent
                spacing: 18

                Rectangle {
                    width: 280
                    height: 280
                    radius: 24
                    color: "#FFFFFF"

                    Grid {
                        id: qrGrid
                        anchors.centerIn: parent
                        columns: root.qrSize
                        spacing: 0
                        visible: root.qrSize > 0
                        property real cellSize: root.qrSize > 0 ? Math.floor(248 / root.qrSize) : 0
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
                        Text { anchors.horizontalCenter: parent.horizontalCenter; text: ShellStore.signedIn ? "✓" : "◎"; color: "#111827"; font.pixelSize: 72; font.weight: Font.Black }
                        Text { anchors.horizontalCenter: parent.horizontalCenter; text: ShellStore.signedIn ? qsTr("Connected") : qsTr("Ready when you are"); color: "#111827"; font.family: Theme.bodyFont; font.pixelSize: 16; font.weight: Font.Bold }
                    }
                }
                Text {
                    anchors.horizontalCenter: parent.horizontalCenter
                    text: root.challenge ? root.challenge.userCode : ShellStore.signedIn ? ShellStore.authSession.user.membershipTier : qsTr("Scan with your phone")
                    color: Theme.label
                    font.family: Theme.displayFont
                    font.pixelSize: root.challenge ? 24 : 18
                    font.weight: Font.Black
                    font.letterSpacing: root.challenge ? 3 : 0
                }
                Text {
                    anchors.horizontalCenter: parent.horizontalCenter
                    text: root.challenge ? qsTr("Expires in %1 · %2").arg(root.timeLeft).arg(root.challenge.verificationUri.replace(/^https?:\/\//, ""))
                                         : ShellStore.signedIn ? qsTr("GeForce NOW account") : qsTr("A real QR code appears after sign-in starts")
                    color: Theme.textMuted
                    font.family: Theme.bodyFont
                    font.pixelSize: 13
                }
            }
        }
    }
    AppChrome { anchors.fill: parent; title: qsTr("Welcome to OpenNOW"); currentRoute: "home"; bottomVisible: false }
}

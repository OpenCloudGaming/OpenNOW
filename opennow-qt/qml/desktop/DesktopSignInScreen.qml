pragma ComponentBehavior: Bound
import QtQuick
import QtQuick.Controls
import QtQuick.Effects
import OpenNOW

FocusScope {
    id: root
    property bool providerOpen: false
    property bool qrRequested: false
    property bool staySignedIn: true
    property double clockMs: Date.now()
    readonly property var challenge: ShellStore.authChallenge
    readonly property var providers: ShellStore.providers && ShellStore.providers.length
        ? ShellStore.providers : [{displayName:"NVIDIA · GeForce NOW", idpId:"", region:"GLOBAL"}]
    readonly property bool waiting: ShellStore.authState === "starting" || ShellStore.authState === "waiting" || ShellStore.authState === "completing"
    readonly property bool failed: ShellStore.authState === "error"
    signal signedIn()
    signal offlineRequested()

    focus: true
    Timer { interval: 1000; repeat: true; running: root.challenge !== null; onTriggered: root.clockMs = Date.now() }

    DesktopBackdrop { anchors.fill: parent; signIn: true }

    Column {
        anchors.centerIn: parent
        width: 472
        spacing: 24

        Row {
            anchors.horizontalCenter: parent.horizontalCenter
            spacing: 11
            Image {
                anchors.verticalCenter: parent.verticalCenter
                width: 26
                height: 14
                source: "qrc:/qt/qml/OpenNOW/res/brand/opennow-mark.png"
                fillMode: Image.PreserveAspectFit
                smooth: true
                layer.enabled: true
                layer.effect: MultiEffect {
                    shadowEnabled: true
                    shadowColor: "#735EE23C"
                    shadowBlur: 0.75
                    shadowHorizontalOffset: 0
                    shadowVerticalOffset: 0
                }
            }
            Text {
                anchors.verticalCenter: parent.verticalCenter
                text: "OpenNOW"
                color: DesktopTokens.text
                font.family: DesktopTokens.displayFont
                font.pixelSize: 18
                font.weight: Font.Black
                font.letterSpacing: -0.18
            }
            Rectangle {
                anchors.verticalCenter: parent.verticalCenter
                width: versionLabel.implicitWidth + 16
                height: 20
                radius: 6
                color: "#14FFFFFF"
                Text {
                    id: versionLabel
                    anchors.centerIn: parent
                    text: Qt.application.version || qsTr("unknown")
                    color: "#A3FFFFFF"
                    font.family: DesktopTokens.monoFont
                    font.pixelSize: 9
                    font.weight: Font.Bold
                    font.letterSpacing: 0.54
                }
            }
        }

        Rectangle {
            id: card
            width: 472
            height: idleColumn.implicitHeight + 64
            radius: 18
            color: "#B80B0F1A"
            border.width: 1
            border.color: "#14FFFFFF"
            layer.enabled: true
            layer.effect: MultiEffect {
                shadowEnabled: true
                shadowColor: "#A6000000"
                shadowBlur: 0.85
                shadowVerticalOffset: 18
                shadowHorizontalOffset: 0
            }

            Column {
                id: idleColumn
                x: 32
                y: 32
                width: 408
                spacing: 22

                Column {
                    width: parent.width
                    spacing: 9
                    Rectangle {
                        width: statusRow.implicitWidth + 20
                        height: 24
                        radius: 12
                        color: root.failed ? "#14FF8A80" : root.waiting ? "#147FD4FF" : "#1FFFD166"
                        Row {
                            id: statusRow
                            anchors.centerIn: parent
                            spacing: 8
                            DesktopGlyph {
                                visible: !root.failed && !root.waiting
                                width: 10
                                height: 12
                                icon: "desktop-lock.svg"
                            }
                            Text {
                                text: root.failed ? qsTr("SIGN IN FAILED")
                                    : root.waiting ? qsTr("WAITING FOR APPROVAL")
                                    : qsTr("NOT SIGNED IN")
                                color: root.failed ? DesktopTokens.danger : root.waiting ? DesktopTokens.focus : DesktopTokens.amber
                                font.family: DesktopTokens.monoFont
                                font.pixelSize: 9
                                font.weight: Font.Bold
                                font.letterSpacing: 0.9
                            }
                        }
                    }
                    Text {
                        width: parent.width
                        text: root.failed ? qsTr("We could not finish sign in")
                            : root.waiting ? (root.qrRequested ? qsTr("Scan to sign in") : qsTr("Finish in your browser"))
                            : qsTr("Sign in to continue")
                        color: DesktopTokens.text
                        font.family: DesktopTokens.displayFont
                        font.pixelSize: 28
                        font.weight: Font.Black
                        font.letterSpacing: -0.7
                    }
                    Text {
                        width: parent.width
                        wrapMode: Text.WordWrap
                        text: root.failed ? (ShellStore.authMessage || qsTr("Your provider returned without a usable session. Nothing was saved."))
                            : root.waiting ? (root.qrRequested ? qsTr("Scan with your phone and approve the request on your provider.") : qsTr("A secure provider page is open. Approve it there and return to OpenNOW."))
                            : qsTr("Nothing works until your provider tells us who you are.")
                        color: "#A3FFFFFF"
                        font.family: DesktopTokens.bodyFont
                        font.pixelSize: 13
                        font.weight: Font.Medium
                        lineHeight: 19
                        lineHeightMode: Text.FixedHeight
                    }
                }

                Column {
                    width: parent.width
                    spacing: 10
                    visible: !root.waiting && !root.failed
                    Text {
                        text: qsTr("PROVIDER")
                        color: "#8AFFFFFF"
                        font.family: DesktopTokens.monoFont
                        font.pixelSize: 10
                        font.weight: Font.Bold
                        font.letterSpacing: 1.4
                    }
                    ItemDelegate {
                        id: providerButton
                        width: parent.width
                        height: 60
                        padding: 0
                        background: Rectangle {
                            radius: 12
                            color: providerButton.hovered || providerButton.activeFocus ? "#14FFFFFF" : "#0FFFFFFF"
                            border.width: 1
                            border.color: "#1FFFFFFF"
                        }
                        contentItem: Item {
                            Rectangle {
                                x: 12
                                anchors.verticalCenter: parent.verticalCenter
                                width: 36
                                height: 36
                                radius: 10
                                color: DesktopTokens.text
                                Text {
                                    anchors.centerIn: parent
                                    text: "NV"
                                    color: "#0A0D14"
                                    font.family: DesktopTokens.monoFont
                                    font.pixelSize: 12
                                    font.weight: Font.Bold
                                }
                            }
                            Column {
                                x: 61
                                anchors.verticalCenter: parent.verticalCenter
                                spacing: 3
                                Text {
                                    text: String(root.providers[0].displayName || "NVIDIA · GeForce NOW")
                                    color: DesktopTokens.text
                                    font.family: DesktopTokens.bodyFont
                                    font.pixelSize: 14
                                    font.weight: Font.ExtraBold
                                }
                                Text {
                                    text: String(root.providers[0].region || "GLOBAL").toUpperCase() + qsTr("  ·  DEFAULT PROVIDER")
                                    color: "#8AFFFFFF"
                                    font.family: DesktopTokens.monoFont
                                    font.pixelSize: 10
                                    font.weight: Font.Bold
                                    font.letterSpacing: 0.6
                                }
                            }
                            Row {
                                anchors.right: parent.right
                                anchors.rightMargin: 14
                                anchors.verticalCenter: parent.verticalCenter
                                spacing: 9
                                Text {
                                    visible: root.providers.length > 1
                                    text: qsTr("%1 MORE").arg(root.providers.length - 1)
                                    color: "#6BFFFFFF"
                                    font.family: DesktopTokens.monoFont
                                    font.pixelSize: 10
                                    font.weight: Font.Bold
                                    font.letterSpacing: 0.6
                                }
                                DesktopGlyph {
                                    anchors.verticalCenter: parent.verticalCenter
                                    width: 10
                                    height: 6
                                    icon: "desktop-chevron-down.svg"
                                    rotation: root.providerOpen ? 180 : 0
                                }
                            }
                        }
                        Component.onCompleted: if (root.visible) forceActiveFocus()
                        onClicked: root.providerOpen = !root.providerOpen
                    }
                    Text {
                        width: parent.width
                        wrapMode: Text.WordWrap
                        text: qsTr("Alliance partners like LG U+, Taiwan Mobile and bro.game run their own rigs.")
                        color: "#8AFFFFFF"
                        font.family: DesktopTokens.bodyFont
                        font.pixelSize: 11
                        font.weight: Font.Medium
                        lineHeight: 16
                        lineHeightMode: Text.FixedHeight
                    }
                    Rectangle { width: parent.width; height: 1; color: "#14FFFFFF" }
                    Rectangle {
                        width: parent.width
                        height: persistWarning.implicitHeight + 20
                        visible: ShellStore.sessionPersistenceMessage !== ""
                        radius: 10
                        color: "#14FF8A80"
                        border.width: 1
                        border.color: "#28FF8A80"
                        Text {
                            id: persistWarning
                            anchors.fill: parent
                            anchors.margins: 10
                            wrapMode: Text.WordWrap
                            text: ShellStore.sessionPersistenceMessage
                            color: DesktopTokens.textBody
                            font.family: DesktopTokens.bodyFont
                            font.pixelSize: 11
                            lineHeight: 1.35
                        }
                    }
                    ItemDelegate {
                        width: parent.width
                        height: 44
                        padding: 0
                        background: Item {}
                        contentItem: Item {
                            Rectangle {
                                anchors.verticalCenter: parent.verticalCenter
                                width: 17
                                height: 17
                                radius: 5
                                color: root.staySignedIn ? DesktopTokens.text : "transparent"
                                border.width: root.staySignedIn ? 0 : 1
                                border.color: DesktopTokens.textMuted
                                DesktopGlyph {
                                    anchors.centerIn: parent
                                    visible: root.staySignedIn
                                    width: 10
                                    height: 8
                                    icon: "desktop-check.svg"
                                }
                            }
                            Column {
                                x: 30
                                anchors.verticalCenter: parent.verticalCenter
                                spacing: 1
                                Text {
                                    text: qsTr("Stay signed in on this PC")
                                    color: "#E0FFFFFF"
                                    font.family: DesktopTokens.bodyFont
                                    font.pixelSize: 13
                                    font.weight: Font.Bold
                                }
                                Text {
                                    text: qsTr("The refresh token is encrypted with the OS keychain.")
                                    color: "#6BFFFFFF"
                                    font.family: DesktopTokens.bodyFont
                                    font.pixelSize: 11
                                    font.weight: Font.Medium
                                }
                            }
                        }
                        onClicked: root.staySignedIn = !root.staySignedIn
                    }
                }

                Column {
                    width: parent.width
                    spacing: 10
                    visible: !root.waiting && !root.failed
                    DesktopButton {
                        width: parent.width
                        height: 46
                        cornerRadius: 11
                        primary: true
                        glyph: "desktop-shield.svg"
                        glyphSize: 16
                        font.pixelSize: 14
                        text: qsTr("Continue with NVIDIA")
                        enabled: ShellStore.ready
                        onClicked: ShellStore.startDeviceLogin(root.providers[0].idpId || "", root.staySignedIn)
                    }
                    DesktopButton {
                        width: parent.width
                        height: 42
                        cornerRadius: 11
                        glyph: "desktop-qr.svg"
                        glyphSize: 15
                        font.pixelSize: 13
                        font.weight: Font.Bold
                        text: qsTr("Sign in with a QR code")
                        enabled: ShellStore.ready
                        onClicked: { root.qrRequested = true; ShellStore.startDeviceLogin(root.providers[0].idpId || "", root.staySignedIn) }
                    }
                }

                Row {
                    width: parent.width
                    spacing: 10
                    visible: !root.waiting && !root.failed
                    DesktopGlyph {
                        y: 2
                        width: 14
                        height: 14
                        icon: "desktop-info.svg"
                    }
                    Text {
                        width: parent.width - 24
                        wrapMode: Text.WordWrap
                        text: qsTr("OpenNOW never sees your password. Sign-in happens on your provider's own page.")
                        color: "#8AFFFFFF"
                        font.family: DesktopTokens.bodyFont
                        font.pixelSize: 11
                        font.weight: Font.Medium
                        lineHeight: 16
                        lineHeightMode: Text.FixedHeight
                    }
                }

                Column {
                    width: parent.width
                    spacing: 14
                    visible: root.waiting
                    Rectangle {
                        visible: root.qrRequested
                        anchors.horizontalCenter: parent.horizontalCenter
                        width: 148
                        height: 148
                        radius: 10
                        color: DesktopTokens.text
                        Grid {
                            id: qrGrid
                            anchors.centerIn: parent
                            columns: root.challenge && root.challenge.qrRows ? root.challenge.qrRows.length : 0
                            visible: columns > 0
                            property var qrRows: root.challenge && root.challenge.qrRows ? root.challenge.qrRows : []
                            property real cell: columns > 0 ? Math.floor(132 / columns) : 0
                            Repeater {
                                model: qrGrid.columns * qrGrid.columns
                                Rectangle {
                                    required property int index
                                    width: qrGrid.cell
                                    height: qrGrid.cell
                                    color: qrGrid.qrRows[Math.floor(index / qrGrid.columns)].charAt(index % qrGrid.columns) === "1" ? "#050608" : "#FFFFFF"
                                }
                            }
                        }
                        Text {
                            anchors.centerIn: parent
                            visible: !qrGrid.visible
                            text: "QR"
                            color: DesktopTokens.shell
                            font.family: DesktopTokens.monoFont
                            font.pixelSize: 30
                            font.weight: Font.Black
                        }
                    }
                    Rectangle {
                        width: parent.width
                        height: 82
                        radius: 12
                        color: "#0AFFFFFF"
                        border.width: 1
                        border.color: DesktopTokens.seamSoft
                        Column {
                            x: 16
                            anchors.verticalCenter: parent.verticalCenter
                            spacing: 7
                            Text {
                                text: root.challenge ? String(root.challenge.userCode || "").toUpperCase() : qsTr("CREATING SECURE CODE…")
                                color: DesktopTokens.text
                                font.family: DesktopTokens.monoFont
                                font.pixelSize: 20
                                font.weight: Font.Bold
                                font.letterSpacing: 2
                            }
                            Text {
                                text: root.challenge ? String(root.challenge.verificationUri || "").replace(/^https?:\/\//, "") : qsTr("Contacting provider")
                                color: DesktopTokens.focus
                                font.family: DesktopTokens.monoFont
                                font.pixelSize: 10
                            }
                        }
                    }
                    Row {
                        width: parent.width
                        spacing: 8
                        Rectangle {
                            anchors.verticalCenter: parent.verticalCenter
                            width: 7
                            height: 7
                            radius: 4
                            color: DesktopTokens.green
                            SequentialAnimation on opacity {
                                running: root.waiting
                                loops: Animation.Infinite
                                NumberAnimation { to: .35; duration: 700 }
                                NumberAnimation { to: 1; duration: 700 }
                            }
                        }
                        Text {
                            text: ShellStore.authMessage || qsTr("Waiting for approval…")
                            color: DesktopTokens.textBody
                            font.family: DesktopTokens.bodyFont
                            font.pixelSize: 12
                        }
                    }
                    DesktopButton {
                        width: parent.width
                        height: 42
                        cornerRadius: 11
                        primary: !root.qrRequested
                        text: root.qrRequested ? qsTr("Open sign-in page") : qsTr("Open provider page")
                        onClicked: if (root.challenge) Qt.openUrlExternally(root.challenge.verificationUriComplete || root.challenge.verificationUri)
                    }
                    DesktopButton {
                        width: parent.width
                        height: 40
                        cornerRadius: 11
                        text: qsTr("Cancel")
                        onClicked: { ShellStore.cancelDeviceLogin(); root.qrRequested = false }
                    }
                }

                Column {
                    width: parent.width
                    spacing: 16
                    visible: root.failed
                    Rectangle {
                        width: parent.width
                        height: 79
                        radius: 12
                        color: "#08FF8A80"
                        border.width: 1
                        border.color: "#24FF8A80"
                        Rectangle { x: 14; y: 20; width: 3; height: 36; radius: 2; color: DesktopTokens.danger }
                        Text {
                            x: 29
                            y: 16
                            width: 280
                            text: qsTr("The authorization was cancelled or expired. Your previous account state is unchanged.")
                            wrapMode: Text.WordWrap
                            color: DesktopTokens.textBody
                            font.family: DesktopTokens.bodyFont
                            font.pixelSize: 11
                            lineHeight: 1.35
                        }
                    }
                    Row {
                        width: parent.width
                        spacing: 10
                        DesktopButton {
                            width: 217
                            height: 44
                            cornerRadius: 11
                            primary: true
                            text: qsTr("Try again")
                            onClicked: { ShellStore.authState = "idle"; ShellStore.startDeviceLogin(root.providers[0].idpId || "", root.staySignedIn) }
                        }
                        DesktopButton {
                            width: 179
                            height: 44
                            cornerRadius: 11
                            text: qsTr("Choose provider")
                            onClicked: { ShellStore.authState = "idle"; root.providerOpen = true }
                        }
                    }
                    Rectangle {
                        width: parent.width
                        height: 140
                        radius: 12
                        color: "#06FFFFFF"
                        border.width: 1
                        border.color: DesktopTokens.seamSoft
                        Column {
                            anchors.fill: parent
                            anchors.margins: 12
                            spacing: 8
                            Text { text: qsTr("BEFORE YOU TRY AGAIN"); color: DesktopTokens.textFaint; font.family: DesktopTokens.monoFont; font.pixelSize: 9; font.weight: Font.Bold; font.letterSpacing: .8 }
                            Row { spacing: 8; DesktopGlyph { width: 10; height: 8; icon: "desktop-check-light.svg"; anchors.verticalCenter: parent.verticalCenter } Text { text: qsTr("Your internet connection is available"); color: DesktopTokens.textBody; font.family: DesktopTokens.bodyFont; font.pixelSize: 11 } }
                            Row { spacing: 8; DesktopGlyph { width: 10; height: 8; icon: "desktop-check-light.svg"; anchors.verticalCenter: parent.verticalCenter } Text { text: qsTr("Pop-ups are allowed in your browser"); color: DesktopTokens.textBody; font.family: DesktopTokens.bodyFont; font.pixelSize: 11 } }
                            Row { spacing: 8; DesktopGlyph { width: 10; height: 8; icon: "desktop-check-light.svg"; anchors.verticalCenter: parent.verticalCenter } Text { text: qsTr("You selected the correct alliance provider"); color: DesktopTokens.textBody; font.family: DesktopTokens.bodyFont; font.pixelSize: 11 } }
                        }
                    }
                }
            }

            Rectangle {
                id: providerMenu
                x: 32
                y: 202
                width: 408
                height: Math.min(260, root.providers.length * 52 + 16)
                visible: root.providerOpen && !root.waiting && !root.failed
                z: 20
                radius: 12
                color: "#FA111722"
                border.width: 1
                border.color: "#29FFFFFF"
                ListView {
                    anchors.fill: parent
                    anchors.margins: 8
                    clip: true
                    model: root.providers
                    delegate: ItemDelegate {
                        id: providerOption
                        required property var modelData
                        width: ListView.view.width
                        height: 52
                        background: Rectangle { radius: 8; color: providerOption.hovered || providerOption.activeFocus ? "#14FFFFFF" : "transparent" }
                        contentItem: Column {
                            spacing: 2
                            Text { text: modelData.displayName || qsTr("Provider"); color: DesktopTokens.textHigh; font.family: DesktopTokens.bodyFont; font.pixelSize: 12; font.weight: Font.Bold }
                            Text { text: String(modelData.region || "GLOBAL").toUpperCase(); color: DesktopTokens.textFaint; font.family: DesktopTokens.monoFont; font.pixelSize: 9 }
                        }
                        onClicked: { root.providerOpen = false; ShellStore.startDeviceLogin(modelData.idpId || "", root.staySignedIn) }
                    }
                }
                opacity: visible ? 1 : 0
                scale: visible ? 1 : .97
                Behavior on opacity { NumberAnimation { duration: DesktopTokens.quickDuration } }
                Behavior on scale { NumberAnimation { duration: DesktopTokens.quickDuration; easing.type: Easing.OutCubic } }
            }
        }

        Row {
            anchors.horizontalCenter: parent.horizontalCenter
            spacing: 16
            Text { text: qsTr("Why an account?"); color: DesktopTokens.focus; font.family: DesktopTokens.bodyFont; font.pixelSize: 11; font.weight: Font.Bold }
            Rectangle { anchors.verticalCenter: parent.verticalCenter; width: 3; height: 3; radius: 2; color: "#33FFFFFF" }
            Text { text: qsTr("Source"); color: DesktopTokens.focus; font.family: DesktopTokens.bodyFont; font.pixelSize: 11; font.weight: Font.Bold }
            Rectangle { anchors.verticalCenter: parent.verticalCenter; width: 3; height: 3; radius: 2; color: "#33FFFFFF" }
            Text { text: qsTr("Privacy"); color: DesktopTokens.focus; font.family: DesktopTokens.bodyFont; font.pixelSize: 11; font.weight: Font.Bold }
            Rectangle { anchors.verticalCenter: parent.verticalCenter; width: 3; height: 3; radius: 2; color: "#33FFFFFF" }
            Text {
                text: root.waiting ? qsTr("WAITING") : root.failed ? qsTr("NOT SIGNED IN") : qsTr("OFFLINE MODE")
                color: "#6BFFFFFF"
                font.family: DesktopTokens.monoFont
                font.pixelSize: 10
                font.weight: Font.Bold
                font.letterSpacing: 0.6
            }
        }
    }

    Connections {
        target: ShellStore
        function onSignedInChanged() { if (ShellStore.signedIn) root.signedIn() }
        function onAuthChallengeChanged() {
            if (ShellStore.authChallenge && !root.qrRequested)
                Qt.openUrlExternally(ShellStore.authChallenge.verificationUriComplete || ShellStore.authChallenge.verificationUri)
        }
    }
}

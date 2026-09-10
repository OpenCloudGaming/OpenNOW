pragma ComponentBehavior: Bound
import QtQuick
import QtQuick.Controls
import QtQuick.Effects
import QtQuick.Shapes
import QtQuick.Window
import OpenNOW

FocusScope {
    id: root
    objectName: "desktopSignInScreen"
    property bool providerOpen: false
    property bool qrRequested: false
    property bool staySignedIn: true
    property double clockMs: Date.now()
    property double challengeReceivedAt: Date.now()
    readonly property var challenge: ShellStore.authChallenge
    readonly property var providers: ShellStore.providers && ShellStore.providers.length
        ? ShellStore.providers : [{displayName:"NVIDIA · GeForce NOW", idpId:"", region:"GLOBAL"}]
    readonly property bool waiting: ShellStore.authState === "starting" || ShellStore.authState === "waiting" || ShellStore.authState === "completing"
    readonly property bool failed: ShellStore.authState === "error"
    readonly property bool wideLayout: width >= DesktopTokens.px(1240)
    readonly property color mutedInk: Theme.lightMode ? Theme.textMuted : "#8AFFFFFF"
    readonly property color faintInk: Theme.lightMode ? Theme.textMuted : "#6BFFFFFF"
    readonly property color bodyInk: Theme.lightMode ? Theme.textMuted : "#A3FFFFFF"
    readonly property color mint: "#6EE7B7"
    readonly property color cardSeam: Theme.lightMode ? Theme.seam : "#24FFFFFF"
    readonly property bool hasExpiry: challenge !== null && Number.isFinite(Number(challenge.expiresAt))
    readonly property int secondsLeft: hasExpiry ? Math.max(0, Math.ceil((Number(challenge.expiresAt) - clockMs) / 1000)) : 0
    readonly property string timeLeft: Math.floor(secondsLeft / 60) + ":" + String(secondsLeft % 60).padStart(2, "0")
    signal signedIn()
    signal offlineRequested()

    focus: true
    Timer { interval: 1000; repeat: true; running: root.challenge !== null; onTriggered: root.clockMs = Date.now() }

    function revealFocusedControl() {
        const win = root.Window.window
        const focused = win ? win.activeFocusItem : null
        if (!focused)
            return
        let ancestor = focused
        while (ancestor && ancestor !== body)
            ancestor = ancestor.parent
        if (!ancestor)
            return
        const point = focused.mapToItem(viewport.contentItem, 0, 0)
        const margin = DesktopTokens.px(16)
        let offset = viewport.contentY
        if (point.y < offset + margin)
            offset = point.y - margin
        else if (point.y + focused.height > offset + viewport.height - margin)
            offset = point.y + focused.height - viewport.height + margin
        viewport.contentY = Math.max(0, Math.min(Math.max(0, viewport.contentHeight - viewport.height), offset))
    }

    Connections {
        target: root.Window.window
        function onActiveFocusItemChanged() { Qt.callLater(root.revealFocusedControl) }
    }

    component BodyText: Text {
        color: root.bodyInk
        font.family: DesktopTokens.bodyFont
        font.pixelSize: DesktopTokens.px(13)
        font.weight: Font.Medium
        wrapMode: Text.WordWrap
        lineHeight: DesktopTokens.px(18)
        lineHeightMode: Text.FixedHeight
        height: text.length > 0 ? Math.max(1, lineCount) * lineHeight : 0
    }

    component MonoText: Text {
        color: root.faintInk
        font.family: DesktopTokens.monoFont
        font.pixelSize: DesktopTokens.px(10)
        font.weight: Font.Bold
        font.letterSpacing: 1.2 * DesktopTokens.uiScale
        lineHeight: DesktopTokens.px(12)
        lineHeightMode: Text.FixedHeight
        height: text.length > 0 ? Math.max(1, lineCount) * lineHeight : 0
    }

    component AuthButton: DesktopButton {
        id: action
        property bool quiet: false
        property bool external: false
        height: DesktopTokens.px(44)
        implicitWidth: Math.max(DesktopTokens.px(68), contentItem.implicitWidth + leftPadding + rightPadding)
        cornerRadius: DesktopTokens.px(10)
        font.pixelSize: DesktopTokens.px(13)
        contentItem: Item {
            implicitWidth: actionContents.implicitWidth
            implicitHeight: actionContents.implicitHeight
            Row {
                id: actionContents
                anchors.centerIn: parent
                spacing: DesktopTokens.px(10)
                DesktopGlyph { visible: action.glyph !== ""; anchors.verticalCenter: parent.verticalCenter; width: action.glyphSize; height: action.glyphSize; icon: action.glyph }
                BodyText { anchors.verticalCenter: parent.verticalCenter; text: action.text; color: action.primary ? "#0B0F1A" : action.quiet ? root.bodyInk : DesktopTokens.text; font: action.font }
                BodyText { visible: action.external; anchors.verticalCenter: parent.verticalCenter; text: "↗"; color: action.primary ? "#0B0F1A" : DesktopTokens.text; font: action.font }
            }
        }
        background: Rectangle {
            radius: action.cornerRadius
            color: action.primary ? (action.down ? Qt.darker(root.mint, 1.1) : root.mint)
                : action.hovered || action.activeFocus ? DesktopTokens.raised : "transparent"
            border.width: action.activeFocus ? 2 : action.primary || action.quiet ? 0 : 1
            border.color: action.activeFocus ? DesktopTokens.focus : root.cardSeam
            opacity: action.enabled ? 1 : 0.5
            layer.enabled: action.primary
            layer.effect: MultiEffect {
                shadowEnabled: true
                shadowColor: "#2E6EE7B7"
                shadowBlur: 0.6
                shadowVerticalOffset: DesktopTokens.px(8)
                shadowHorizontalOffset: 0
            }
        }
    }

    component HeaderLink: AbstractButton {
        id: link
        property string destination: ""
        property string explanation: ""
        implicitWidth: linkText.implicitWidth
        implicitHeight: DesktopTokens.px(24)
        focusPolicy: Qt.StrongFocus
        hoverEnabled: true
        contentItem: BodyText {
            id: linkText
            text: link.text; color: DesktopTokens.focus
            font.weight: Font.Bold; lineHeight: DesktopTokens.px(16)
            verticalAlignment: Text.AlignVCenter
        }
        background: Rectangle {
            radius: DesktopTokens.px(4)
            color: link.hovered ? DesktopTokens.raised : "transparent"
            border.width: link.activeFocus ? 2 : 0; border.color: Theme.focus
        }
        onClicked: {
            if (destination !== "")
                Qt.openUrlExternally(destination)
            else
                explanationPopup.open()
        }
        Popup {
            id: explanationPopup
            parent: Overlay.overlay
            x: Math.max(DesktopTokens.px(24), root.width - width - DesktopTokens.px(24))
            y: DesktopTokens.px(64)
            width: Math.min(DesktopTokens.px(360), root.width - DesktopTokens.px(48))
            padding: DesktopTokens.px(20)
            focus: true
            closePolicy: Popup.CloseOnEscape | Popup.CloseOnPressOutside
            background: Rectangle { radius: DesktopTokens.px(12); color: Theme.shell; border.color: Theme.seam }
            contentItem: BodyText { text: link.explanation }
        }
    }

    DesktopOnboardingBackdrop { anchors.fill: parent }

    Item {
        id: topStrip
        width: parent.width
        height: DesktopTokens.px(72)
        Row {
            x: DesktopTokens.px(root.wideLayout ? 40 : 24)
            anchors.verticalCenter: parent.verticalCenter
            spacing: DesktopTokens.px(10)
            DesktopOnboardingMark { anchors.verticalCenter: parent.verticalCenter; width: DesktopTokens.px(26); height: DesktopTokens.px(16) }
            BodyText {
                anchors.verticalCenter: parent.verticalCenter
                text: "OpenNOW"
                color: DesktopTokens.text
                font.pixelSize: DesktopTokens.px(18)
                font.weight: Font.Black
                font.letterSpacing: -0.36 * DesktopTokens.uiScale
                lineHeight: DesktopTokens.px(22)
            }
            Row {
                anchors.verticalCenter: parent.verticalCenter
                spacing: DesktopTokens.px(6)
                leftPadding: DesktopTokens.px(4)
                Rectangle {
                    width: versionLabel.implicitWidth + DesktopTokens.px(16)
                    height: DesktopTokens.px(24)
                    radius: DesktopTokens.px(6)
                    color: DesktopTokens.seamSoft
                    border.width: 1
                    border.color: DesktopTokens.seamSoft
                    MonoText {
                        id: versionLabel
                        anchors.centerIn: parent
                        text: Qt.application.version || qsTr("unknown")
                        color: root.bodyInk
                        font.pixelSize: DesktopTokens.px(11)
                        font.letterSpacing: 0
                        lineHeight: DesktopTokens.px(14)
                    }
                }
                Rectangle {
                    width: betaLabel.implicitWidth + DesktopTokens.px(14)
                    height: DesktopTokens.px(24)
                    radius: DesktopTokens.px(6)
                    color: "#1FFFD166"
                    MonoText { id: betaLabel; anchors.centerIn: parent; text: qsTr("BETA"); color: Theme.accentColor("amber") }
                }
            }
        }
        Row {
            visible: root.width >= DesktopTokens.px(700)
            anchors.right: parent.right
            anchors.rightMargin: DesktopTokens.px(root.wideLayout ? 40 : 24)
            anchors.verticalCenter: parent.verticalCenter
            spacing: DesktopTokens.px(22)
            HeaderLink {
                text: qsTr("Why an account?")
                explanation: qsTr("OpenNOW is a native client for GeForce NOW and its alliance partners. Sign in with your provider to access your library, browse the stores and connect with friends.")
            }
            HeaderLink { text: qsTr("Source"); destination: "https://github.com/OpenCloudGaming/OpenNOW" }
            HeaderLink {
                text: qsTr("Privacy")
                explanation: qsTr("OpenNOW never sees your password. Sign-in happens on your provider's own page and only a session token comes back.")
                    + "\n\n" + qsTr("The refresh token is encrypted with the OS keychain.")
            }
        }
    }

    Flickable {
        id: viewport
        objectName: "signInScroll"
        anchors.top: topStrip.bottom
        anchors.bottom: footer.top
        width: parent.width
        contentWidth: width
        contentHeight: Math.max(height, body.height + DesktopTokens.px(48))
        flickableDirection: Flickable.VerticalFlick
        boundsBehavior: Flickable.StopAtBounds
        clip: true
        ScrollBar.vertical: ScrollBar { policy: ScrollBar.AsNeeded }

        Item {
            id: body
            x: root.wideLayout ? DesktopTokens.px(80) : (viewport.width - width) / 2
            y: Math.max(DesktopTokens.px(24), (viewport.height - height) / 2)
            width: root.wideLayout ? viewport.width - DesktopTokens.px(176) : Math.max(0, Math.min(DesktopTokens.px(440), viewport.width - DesktopTokens.px(48)))
            height: Math.max(card.height, root.wideLayout ? hero.implicitHeight : 0)

            Column {
                id: hero
                visible: root.wideLayout
                width: DesktopTokens.px(560)
                anchors.verticalCenter: parent.verticalCenter
                spacing: DesktopTokens.px(24)
                Rectangle {
                    width: DesktopTokens.px(72)
                    height: DesktopTokens.px(72)
                    radius: DesktopTokens.px(20)
                    color: Theme.lightMode ? Theme.shell : "#0B0F1A"
                    border.width: 1
                    border.color: root.cardSeam
                    layer.enabled: true
                    layer.effect: MultiEffect {
                        shadowEnabled: true
                        shadowColor: "#186EE7B7"
                        shadowBlur: 1
                        shadowVerticalOffset: 0
                        shadowHorizontalOffset: 0
                    }
                    DesktopOnboardingMark { anchors.centerIn: parent; width: DesktopTokens.px(40); height: DesktopTokens.px(26) }
                }
                Column {
                    width: parent.width
                    spacing: DesktopTokens.px(14)
                    BodyText {
                        width: parent.width
                        text: qsTr("Your GeForce NOW,\nwithout the browser.")
                        color: DesktopTokens.text
                        font.family: DesktopTokens.displayFont
                        font.pixelSize: DesktopTokens.px(56)
                        font.weight: Font.Black
                        font.letterSpacing: -1.68 * DesktopTokens.uiScale
                        lineHeight: DesktopTokens.px(60)
                        topPadding: -DesktopTokens.px(9)
                    }
                    BodyText {
                        width: DesktopTokens.px(500)
                        text: qsTr("OpenNOW is a native client for GeForce NOW and its alliance partners. Sign in with your provider to access your library, browse the stores and connect with friends.")
                        font.pixelSize: DesktopTokens.px(16)
                        lineHeight: DesktopTokens.px(25)
                    }
                }
                Row {
                    width: parent.width
                    spacing: DesktopTokens.px(36)
                    topPadding: DesktopTokens.px(8)
                    Column {
                        width: DesktopTokens.px(130)
                        spacing: DesktopTokens.px(4)
                        MonoText { text: qsTr("Native Qt"); color: DesktopTokens.text; font.pixelSize: DesktopTokens.px(24); font.letterSpacing: 0; lineHeight: DesktopTokens.px(28) }
                        BodyText { width: parent.width; text: qsTr("Native Qt shell, no Chromium"); color: root.mutedInk; font.pixelSize: DesktopTokens.px(12); lineHeight: DesktopTokens.px(16) }
                    }
                    Rectangle { width: 1; height: DesktopTokens.px(44); color: root.cardSeam }
                    Column {
                        width: DesktopTokens.px(108)
                        spacing: DesktopTokens.px(4)
                        MonoText { text: qsTr("Library"); color: DesktopTokens.text; font.pixelSize: DesktopTokens.px(24); font.letterSpacing: 0; lineHeight: DesktopTokens.px(28) }
                        BodyText { width: parent.width; text: qsTr("Your games, in one place"); color: root.mutedInk; font.pixelSize: DesktopTokens.px(12); lineHeight: DesktopTokens.px(16) }
                    }
                    Rectangle { width: 1; height: DesktopTokens.px(44); color: root.cardSeam }
                    Column {
                        width: DesktopTokens.px(176)
                        spacing: DesktopTokens.px(4)
                        MonoText { text: qsTr("0 passwords"); color: DesktopTokens.text; font.pixelSize: DesktopTokens.px(24); font.letterSpacing: 0; lineHeight: DesktopTokens.px(28) }
                        BodyText { width: parent.width; text: qsTr("Sign-in happens on your provider's page"); color: root.mutedInk; font.pixelSize: DesktopTokens.px(12); lineHeight: DesktopTokens.px(16) }
                    }
                }
                Row {
                    width: parent.width
                    spacing: DesktopTokens.px(10)
                    topPadding: DesktopTokens.px(4)
                    Rectangle { anchors.verticalCenter: parent.verticalCenter; width: DesktopTokens.px(8); height: width; radius: width / 2; color: DesktopTokens.amber }
                    BodyText {
                        width: parent.width - DesktopTokens.px(18)
                        text: qsTr("%1 is a beta. We will say more about that right after you sign in.").arg(Qt.application.version || qsTr("OpenNOW"))
                        color: root.mutedInk
                    }
                }
            }

            Rectangle {
                id: card
                x: parent.width - width
                anchors.verticalCenter: parent.verticalCenter
                width: Math.min(DesktopTokens.px(440), parent.width)
                height: cardColumn.implicitHeight + 2
                radius: DesktopTokens.px(22)
                color: Theme.lightMode ? Qt.rgba(Theme.shell.r, Theme.shell.g, Theme.shell.b, 0.92) : "#D10B0F1A"
                border.width: 1
                border.color: root.cardSeam
                layer.enabled: true
                layer.effect: MultiEffect {
                    shadowEnabled: true
                    shadowColor: "#8C000000"
                    shadowBlur: 1
                    shadowVerticalOffset: DesktopTokens.px(24)
                    shadowHorizontalOffset: 0
                }

                Column {
                    id: cardColumn
                    x: 1
                    y: 1
                    width: parent.width - 2

                    Column {
                        x: DesktopTokens.px(26)
                        width: parent.width - DesktopTokens.px(52)
                        topPadding: DesktopTokens.px(26)
                        bottomPadding: DesktopTokens.px(18)
                        spacing: DesktopTokens.px(8)
                        Row {
                            spacing: DesktopTokens.px(8)
                            Rectangle {
                                anchors.verticalCenter: parent.verticalCenter
                                width: DesktopTokens.px(8)
                                height: width
                                radius: width / 2
                                color: root.failed ? DesktopTokens.danger : root.waiting ? DesktopTokens.amber : root.faintInk
                            }
                            MonoText {
                                text: root.failed ? qsTr("SIGN IN FAILED") : root.waiting ? qsTr("WAITING FOR APPROVAL") : qsTr("NOT SIGNED IN")
                                color: root.failed ? DesktopTokens.danger : root.waiting ? Theme.accentColor("amber") : root.faintInk
                            }
                        }
                        BodyText {
                            width: parent.width
                            text: root.failed ? qsTr("We could not finish sign in")
                                : root.waiting ? (root.qrRequested ? qsTr("Scan to sign in") : qsTr("Finish in your browser")) : qsTr("Sign in to continue")
                            color: DesktopTokens.text
                            font.family: DesktopTokens.displayFont
                            font.pixelSize: DesktopTokens.px(26)
                            font.weight: Font.Black
                            font.letterSpacing: -0.52 * DesktopTokens.uiScale
                            lineHeight: DesktopTokens.px(30)
                            topPadding: -DesktopTokens.px(3)
                        }
                        BodyText {
                            width: parent.width
                            text: root.failed ? (ShellStore.authMessage || qsTr("Your provider returned without a usable session. Nothing was saved."))
                                : root.waiting ? (root.qrRequested ? qsTr("Scan with your phone and approve the request on your provider. This screen moves on by itself.") : qsTr("A secure provider page is open. Approve it there and return to OpenNOW."))
                                : qsTr("Nothing works until your provider tells us who you are.")
                        }
                    }

                    Column {
                        id: providerColumn
                        x: DesktopTokens.px(26)
                        width: parent.width - DesktopTokens.px(52)
                        spacing: DesktopTokens.px(8)
                        bottomPadding: DesktopTokens.px(18)
                        visible: !root.waiting && !root.failed
                        MonoText { text: qsTr("PROVIDER") }
                        ItemDelegate {
                            id: providerButton
                            focusPolicy: Qt.StrongFocus
                            width: parent.width
                            height: DesktopTokens.px(56)
                            padding: 0
                            Accessible.name: String(root.providers[0].displayName || "NVIDIA · GeForce NOW")
                            background: Rectangle {
                                radius: DesktopTokens.px(12)
                                color: providerButton.hovered || providerButton.activeFocus ? DesktopTokens.raisedStrong : DesktopTokens.raised
                                border.width: 1
                                border.color: providerButton.activeFocus ? DesktopTokens.focus : root.cardSeam
                            }
                            contentItem: Item {
                                Rectangle {
                                    x: DesktopTokens.px(14)
                                    anchors.verticalCenter: parent.verticalCenter
                                    width: DesktopTokens.px(32)
                                    height: width
                                    radius: DesktopTokens.px(9)
                                    color: "#76B900"
                                    BodyText { anchors.centerIn: parent; text: "N"; color: "#0B0F1A"; font.pixelSize: DesktopTokens.px(14); font.weight: Font.Black }
                                }
                                Column {
                                    x: DesktopTokens.px(58)
                                    width: parent.width - DesktopTokens.px(100)
                                    anchors.verticalCenter: parent.verticalCenter
                                    spacing: DesktopTokens.px(2)
                                    BodyText { objectName: "signInProviderName"; width: parent.width; text: String(root.providers[0].displayName || "NVIDIA · GeForce NOW"); color: DesktopTokens.text; font.pixelSize: DesktopTokens.px(14); font.weight: Font.ExtraBold; maximumLineCount: 1; elide: Text.ElideRight }
                                    MonoText { objectName: "signInProviderRegion"; width: parent.width; text: String(root.providers[0].region || "GLOBAL").toUpperCase() + qsTr("  ·  DEFAULT PROVIDER"); font.letterSpacing: 0.8 * DesktopTokens.uiScale; elide: Text.ElideRight }
                                }
                                DesktopGlyph {
                                    anchors.right: parent.right
                                    anchors.rightMargin: DesktopTokens.px(14)
                                    anchors.verticalCenter: parent.verticalCenter
                                    width: DesktopTokens.px(14)
                                    height: DesktopTokens.px(14)
                                    icon: "desktop-chevron-down.svg"
                                    rotation: root.providerOpen ? 180 : 0
                                }
                            }
                            Component.onCompleted: if (root.visible) forceActiveFocus()
                            onClicked: root.providerOpen = !root.providerOpen
                        }
                        Row {
                            width: parent.width
                            spacing: DesktopTokens.px(12)
                            leftPadding: DesktopTokens.px(4)
                            rightPadding: DesktopTokens.px(4)
                            BodyText {
                                width: parent.width - DesktopTokens.px(8) - (moreProviders.visible ? moreProviders.width + parent.spacing : 0)
                                text: qsTr("Alliance partners like LG U+, Taiwan Mobile and bro.game run their own rigs.")
                                color: root.mutedInk
                                font.pixelSize: DesktopTokens.px(12)
                                lineHeight: DesktopTokens.px(16)
                            }
                            MonoText { id: moreProviders; anchors.verticalCenter: parent.verticalCenter; visible: root.providers.length > 1; text: qsTr("%1 MORE").arg(root.providers.length - 1); color: DesktopTokens.focus; font.letterSpacing: 0.8 * DesktopTokens.uiScale }
                        }
                        Rectangle {
                            width: parent.width
                            height: persistWarning.height + DesktopTokens.px(20)
                            visible: ShellStore.sessionPersistenceMessage !== ""
                            radius: DesktopTokens.px(10)
                            color: "#14FF8A80"
                            border.width: 1
                            border.color: "#28FF8A80"
                            BodyText { id: persistWarning; x: DesktopTokens.px(10); y: DesktopTokens.px(10); width: parent.width - DesktopTokens.px(20); text: ShellStore.sessionPersistenceMessage; font.pixelSize: DesktopTokens.px(11); lineHeight: DesktopTokens.px(16) }
                        }
                    }

                    ItemDelegate {
                        id: persistenceButton
                        focusPolicy: Qt.StrongFocus
                        visible: !root.waiting && !root.failed
                        width: parent.width
                        height: Math.max(DesktopTokens.px(65), persistenceText.implicitHeight + DesktopTokens.px(30))
                        padding: 0
                        Accessible.name: qsTr("Stay signed in on this PC")
                        Accessible.role: Accessible.CheckBox
                        Accessible.checkable: true
                        Accessible.checked: root.staySignedIn
                        background: Rectangle {
                            color: persistenceButton.hovered || persistenceButton.activeFocus ? DesktopTokens.raised : "transparent"
                            Rectangle { width: parent.width; height: 1; color: DesktopTokens.seamSoft }
                            Rectangle { anchors.bottom: parent.bottom; width: parent.width; height: 1; color: DesktopTokens.seamSoft }
                        }
                        contentItem: Item {
                            Column {
                                id: persistenceText
                                x: DesktopTokens.px(26)
                                width: parent.width - DesktopTokens.px(112)
                                anchors.verticalCenter: parent.verticalCenter
                                spacing: DesktopTokens.px(2)
                                BodyText { width: parent.width; text: qsTr("Stay signed in on this PC"); color: DesktopTokens.text; font.weight: Font.ExtraBold; lineHeight: DesktopTokens.px(17) }
                                BodyText { width: parent.width; text: qsTr("The refresh token is encrypted with the OS keychain."); color: root.mutedInk; font.pixelSize: DesktopTokens.px(12); lineHeight: DesktopTokens.px(16) }
                            }
                            Rectangle {
                                anchors.right: parent.right
                                anchors.rightMargin: DesktopTokens.px(26)
                                anchors.verticalCenter: parent.verticalCenter
                                width: DesktopTokens.px(44)
                                height: DesktopTokens.px(26)
                                radius: height / 2
                                color: root.staySignedIn ? root.mint : DesktopTokens.raisedStrong
                                border.width: persistenceButton.activeFocus ? 2 : 0
                                border.color: DesktopTokens.focus
                                Rectangle { x: root.staySignedIn ? parent.width - width - DesktopTokens.px(3) : DesktopTokens.px(3); y: DesktopTokens.px(3); width: DesktopTokens.px(20); height: width; radius: width / 2; color: root.staySignedIn ? "#0B0F1A" : DesktopTokens.text }
                            }
                        }
                        onClicked: root.staySignedIn = !root.staySignedIn
                    }

                    Column {
                        x: DesktopTokens.px(26)
                        width: parent.width - DesktopTokens.px(52)
                        spacing: DesktopTokens.px(10)
                        topPadding: DesktopTokens.px(20)
                        visible: !root.waiting && !root.failed
                        AuthButton {
                            width: parent.width
                            height: DesktopTokens.px(48)
                            primary: true
                            external: true
                            font.pixelSize: DesktopTokens.px(14)
                            text: qsTr("Continue with NVIDIA")
                            enabled: ShellStore.ready
                            onClicked: ShellStore.startDeviceLogin(root.providers[0].idpId || "", root.staySignedIn)
                        }
                        AuthButton {
                            width: parent.width
                            glyph: "desktop-qr.svg"
                            glyphSize: DesktopTokens.px(14)
                            text: qsTr("Sign in with a QR code")
                            enabled: ShellStore.ready
                            onClicked: { root.qrRequested = true; ShellStore.startDeviceLogin(root.providers[0].idpId || "", root.staySignedIn) }
                        }
                    }

                    Row {
                        x: DesktopTokens.px(26)
                        width: parent.width - DesktopTokens.px(52)
                        spacing: DesktopTokens.px(8)
                        topPadding: DesktopTokens.px(16)
                        bottomPadding: DesktopTokens.px(24)
                        visible: !root.waiting && !root.failed
                        Item {
                            width: DesktopTokens.px(12)
                            height: DesktopTokens.px(14)
                            Shape {
                                width: 12
                                height: 14
                                transform: Scale { xScale: DesktopTokens.uiScale; yScale: DesktopTokens.uiScale }
                                ShapePath {
                                    strokeColor: root.faintInk
                                    strokeWidth: 1.4
                                    fillColor: "transparent"
                                    joinStyle: ShapePath.RoundJoin
                                    PathSvg { path: "M6 1l4.5 1.8v3.6c0 3-2 5.2-4.5 6.1C3.5 11.6 1.5 9.4 1.5 6.4V2.8L6 1Z" }
                                }
                            }
                        }
                        BodyText {
                            width: parent.width - DesktopTokens.px(20)
                            text: qsTr("OpenNOW never sees your password. Sign-in happens on your provider's own page and only a session token comes back.")
                            color: root.faintInk
                            font.pixelSize: DesktopTokens.px(12)
                            lineHeight: DesktopTokens.px(16)
                        }
                    }

                    Column {
                        width: parent.width
                        visible: root.waiting
                        Item {
                            id: codeBlock
                            x: DesktopTokens.px(26)
                            width: parent.width - DesktopTokens.px(52)
                            readonly property bool stacked: width < DesktopTokens.px(340)
                            height: (stacked && qrBox.visible ? qrBox.height + DesktopTokens.px(22) + codeText.implicitHeight : Math.max(qrBox.visible ? qrBox.height : 0, codeText.implicitHeight)) + DesktopTokens.px(20)
                            Rectangle {
                                id: qrBox
                                visible: root.qrRequested
                                width: DesktopTokens.px(150)
                                height: width
                                x: codeBlock.stacked ? (parent.width - width) / 2 : 0
                                radius: DesktopTokens.px(12)
                                color: "#FFFFFF"
                                Grid {
                                    id: qrGrid
                                    anchors.centerIn: parent
                                    columns: root.challenge && root.challenge.qrRows ? root.challenge.qrRows.length : 0
                                    visible: columns > 0
                                    property var qrRows: root.challenge && root.challenge.qrRows ? root.challenge.qrRows : []
                                    property real cell: columns > 0 ? Math.floor(DesktopTokens.px(126) / columns) : 0
                                    Repeater {
                                        model: qrGrid.columns * qrGrid.columns
                                        Rectangle {
                                            required property int index
                                            width: qrGrid.cell
                                            height: qrGrid.cell
                                            color: qrGrid.qrRows[Math.floor(index / qrGrid.columns)].charAt(index % qrGrid.columns) === "1" ? "#0B0F1A" : "#FFFFFF"
                                        }
                                    }
                                }
                                MonoText { anchors.centerIn: parent; visible: !qrGrid.visible; text: "QR"; color: "#0B0F1A"; font.pixelSize: DesktopTokens.px(30); lineHeight: DesktopTokens.px(36) }
                            }
                            Column {
                                id: codeText
                                x: qrBox.visible && !codeBlock.stacked ? qrBox.width + DesktopTokens.px(22) : 0
                                y: codeBlock.stacked && qrBox.visible ? qrBox.height + DesktopTokens.px(22) : Math.max(0, (qrBox.height - implicitHeight) / 2)
                                width: parent.width - x
                                spacing: DesktopTokens.px(10)
                                MonoText { width: parent.width; text: qsTr("OR ENTER THIS CODE"); wrapMode: Text.WordWrap }
                                MonoText {
                                    width: parent.width
                                    text: root.challenge ? String(root.challenge.userCode || "").toUpperCase() : qsTr("CREATING SECURE CODE…")
                                    color: DesktopTokens.text
                                    font.pixelSize: DesktopTokens.px(root.challenge ? 28 : 13)
                                    font.letterSpacing: 1.68 * DesktopTokens.uiScale
                                    lineHeight: DesktopTokens.px(32)
                                    wrapMode: Text.WrapAnywhere
                                }
                                MonoText {
                                    width: parent.width
                                    text: root.challenge ? String(root.challenge.verificationUri || "").replace(/^https?:\/\//, "") : qsTr("Contacting provider")
                                    color: DesktopTokens.focus
                                    font.pixelSize: DesktopTokens.px(12)
                                    font.weight: Font.Medium
                                    font.letterSpacing: 0
                                    lineHeight: DesktopTokens.px(16)
                                    wrapMode: Text.WrapAnywhere
                                }
                                Column {
                                    width: parent.width
                                    visible: root.hasExpiry
                                    topPadding: DesktopTokens.px(4)
                                    spacing: DesktopTokens.px(6)
                                    Rectangle {
                                        width: parent.width
                                        height: DesktopTokens.px(3)
                                        radius: DesktopTokens.px(2)
                                        color: DesktopTokens.raised
                                        Rectangle { width: parent.width * Math.max(0, Math.min(1, (Number(root.challenge ? root.challenge.expiresAt : 0) - root.clockMs) / Math.max(1, Number(root.challenge ? root.challenge.expiresAt : 0) - root.challengeReceivedAt))); height: parent.height; radius: parent.radius; color: DesktopTokens.amber }
                                    }
                                    MonoText { width: parent.width; text: qsTr("Code expires in %1").arg(root.timeLeft); color: root.mutedInk; font.pixelSize: DesktopTokens.px(11); font.weight: Font.Medium; font.letterSpacing: 0; lineHeight: DesktopTokens.px(14); wrapMode: Text.WordWrap }
                                }
                            }
                        }
                        Rectangle {
                            width: parent.width
                            height: Math.max(DesktopTokens.px(66), approvalStatus.height + DesktopTokens.px(30))
                            color: Theme.lightMode ? DesktopTokens.seamSoft : "#08FFFFFF"
                            Rectangle { width: parent.width; height: 1; color: DesktopTokens.seamSoft }
                            Rectangle { anchors.bottom: parent.bottom; width: parent.width; height: 1; color: DesktopTokens.seamSoft }
                            Item {
                                x: DesktopTokens.px(26)
                                anchors.verticalCenter: parent.verticalCenter
                                width: DesktopTokens.px(16)
                                height: width
                                Shape {
                                    width: 16
                                    height: 16
                                    transform: Scale { xScale: DesktopTokens.uiScale; yScale: DesktopTokens.uiScale }
                                    ShapePath { strokeColor: DesktopTokens.amber; strokeWidth: 1.8; fillColor: "transparent"; capStyle: ShapePath.RoundCap; PathSvg { path: "M8 2a6 6 0 1 1-6 6" } }
                                }
                                RotationAnimator on rotation { from: 0; to: 360; duration: 1200; running: root.waiting && !AppController.reducedMotion; loops: Animation.Infinite }
                            }
                            BodyText { id: approvalStatus; x: DesktopTokens.px(52); anchors.verticalCenter: parent.verticalCenter; width: parent.width - DesktopTokens.px(78); text: ShellStore.authMessage || qsTr("Waiting for approval…") }
                        }
                        Row {
                            x: DesktopTokens.px(26)
                            width: parent.width - DesktopTokens.px(52)
                            spacing: DesktopTokens.px(10)
                            topPadding: DesktopTokens.px(20)
                            bottomPadding: DesktopTokens.px(24)
                            AuthButton {
                                width: parent.width - cancelButton.width - parent.spacing
                                external: true
                                text: root.qrRequested ? qsTr("Open sign-in page") : qsTr("Open provider page")
                                onClicked: if (root.challenge) Qt.openUrlExternally(root.challenge.verificationUriComplete || root.challenge.verificationUri)
                            }
                            AuthButton { id: cancelButton; width: Math.max(DesktopTokens.px(78), implicitWidth); quiet: true; text: qsTr("Cancel"); onClicked: { ShellStore.cancelDeviceLogin(); root.qrRequested = false } }
                        }
                    }

                    Column {
                        x: DesktopTokens.px(26)
                        width: parent.width - DesktopTokens.px(52)
                        spacing: DesktopTokens.px(16)
                        bottomPadding: DesktopTokens.px(24)
                        visible: root.failed
                        Rectangle {
                            width: parent.width
                            height: failureText.height + DesktopTokens.px(32)
                            radius: DesktopTokens.px(12)
                            color: "#08FF8A80"
                            border.width: 1
                            border.color: "#24FF8A80"
                            Rectangle { x: DesktopTokens.px(14); y: DesktopTokens.px(20); width: DesktopTokens.px(3); height: parent.height - DesktopTokens.px(40); radius: DesktopTokens.px(2); color: DesktopTokens.danger }
                            BodyText { id: failureText; x: DesktopTokens.px(29); y: DesktopTokens.px(16); width: parent.width - DesktopTokens.px(45); text: qsTr("The authorization was cancelled or expired. Your previous account state is unchanged."); font.pixelSize: DesktopTokens.px(11); lineHeight: DesktopTokens.px(16) }
                        }
                        Row {
                            width: parent.width
                            spacing: DesktopTokens.px(10)
                            AuthButton { width: (parent.width - parent.spacing) * 0.55; primary: true; text: qsTr("Try again"); onClicked: { ShellStore.authState = "idle"; ShellStore.startDeviceLogin(root.providers[0].idpId || "", root.staySignedIn) } }
                            AuthButton { width: (parent.width - parent.spacing) * 0.45; text: qsTr("Choose provider"); onClicked: { ShellStore.authState = "idle"; root.providerOpen = true } }
                        }
                        Rectangle {
                            width: parent.width
                            height: retryNotes.implicitHeight + DesktopTokens.px(24)
                            radius: DesktopTokens.px(12)
                            color: DesktopTokens.seamSoft
                            border.width: 1
                            border.color: DesktopTokens.seamSoft
                            Column {
                                id: retryNotes
                                x: DesktopTokens.px(12)
                                y: DesktopTokens.px(12)
                                width: parent.width - DesktopTokens.px(24)
                                spacing: DesktopTokens.px(8)
                                MonoText { width: parent.width; text: qsTr("BEFORE YOU TRY AGAIN"); wrapMode: Text.WordWrap }
                                BodyText { width: parent.width; text: qsTr("Your internet connection is available"); font.pixelSize: DesktopTokens.px(11); lineHeight: DesktopTokens.px(16) }
                                BodyText { width: parent.width; text: qsTr("Pop-ups are allowed in your browser"); font.pixelSize: DesktopTokens.px(11); lineHeight: DesktopTokens.px(16) }
                                BodyText { width: parent.width; text: qsTr("You selected the correct alliance provider"); font.pixelSize: DesktopTokens.px(11); lineHeight: DesktopTokens.px(16) }
                            }
                        }
                    }
                }

                Rectangle {
                    id: providerMenu
                    x: cardColumn.x + providerColumn.x
                    y: cardColumn.y + providerColumn.y + providerButton.y + providerButton.height + DesktopTokens.px(6)
                    width: providerColumn.width
                    height: Math.min(DesktopTokens.px(260), root.providers.length * DesktopTokens.px(52) + DesktopTokens.px(16))
                    MotionProgress { id: providerMotion; shown: root.providerOpen && !root.waiting && !root.failed; enterDuration: 120; exitDuration: 120 }
                    visible: providerMotion.present
                    enabled: root.providerOpen && !root.waiting && !root.failed
                    z: 20
                    radius: DesktopTokens.px(12)
                    color: Theme.lightMode ? Theme.shell : "#FA111722"
                    border.width: 1
                    border.color: root.cardSeam
                    ListView {
                        anchors.fill: parent
                        anchors.margins: DesktopTokens.px(8)
                        clip: true
                        model: root.providers
                        delegate: ItemDelegate {
                            focusPolicy: Qt.StrongFocus
                            id: providerOption
                            required property var modelData
                            width: ListView.view.width
                            height: DesktopTokens.px(52)
                            Accessible.name: modelData.displayName || qsTr("Provider")
                            background: Rectangle { radius: DesktopTokens.px(8); color: providerOption.hovered || providerOption.activeFocus ? DesktopTokens.raised : "transparent" }
                            contentItem: Column {
                                spacing: DesktopTokens.px(2)
                                BodyText { width: parent.width; text: providerOption.modelData.displayName || qsTr("Provider"); color: DesktopTokens.text; font.pixelSize: DesktopTokens.px(12); font.weight: Font.Bold; maximumLineCount: 1; elide: Text.ElideRight }
                                MonoText { text: String(providerOption.modelData.region || "GLOBAL").toUpperCase(); font.pixelSize: DesktopTokens.px(9) }
                            }
                            onClicked: { root.providerOpen = false; ShellStore.startDeviceLogin(modelData.idpId || "", root.staySignedIn) }
                        }
                    }
                    opacity: providerMotion.progress
                    scale: providerMotion.zoom
                    transformOrigin: Item.TopLeft
                }
            }
        }
    }

    Item {
        id: footer
        anchors.bottom: parent.bottom
        width: parent.width
        height: DesktopTokens.px(72)
        Rectangle { width: parent.width; height: 1; color: DesktopTokens.seamSoft }
        Row {
            x: DesktopTokens.px(root.wideLayout ? 40 : 24)
            anchors.verticalCenter: parent.verticalCenter
            spacing: DesktopTokens.px(18)
            Row {
                spacing: DesktopTokens.px(8)
                Rectangle {
                    anchors.verticalCenter: parent.verticalCenter
                    width: enterLabel.implicitWidth + DesktopTokens.px(12)
                    height: DesktopTokens.px(22)
                    radius: DesktopTokens.px(6)
                    color: DesktopTokens.raised
                    MonoText { id: enterLabel; anchors.centerIn: parent; text: qsTr("Enter"); color: root.bodyInk; font.letterSpacing: 0 }
                }
                BodyText { anchors.verticalCenter: parent.verticalCenter; text: qsTr("Activate focused action"); color: root.mutedInk; font.pixelSize: DesktopTokens.px(12); lineHeight: DesktopTokens.px(16) }
            }
            Row {
                spacing: DesktopTokens.px(8)
                Rectangle {
                    anchors.verticalCenter: parent.verticalCenter
                    width: tabLabel.implicitWidth + DesktopTokens.px(12)
                    height: DesktopTokens.px(22)
                    radius: DesktopTokens.px(6)
                    color: DesktopTokens.raised
                    MonoText { id: tabLabel; anchors.centerIn: parent; text: qsTr("Tab"); color: root.bodyInk; font.letterSpacing: 0 }
                }
                BodyText { anchors.verticalCenter: parent.verticalCenter; text: qsTr("Move focus"); color: root.mutedInk; font.pixelSize: DesktopTokens.px(12); lineHeight: DesktopTokens.px(16) }
            }
        }
        Row {
            visible: root.wideLayout
            anchors.right: parent.right
            anchors.rightMargin: DesktopTokens.px(40)
            anchors.verticalCenter: parent.verticalCenter
            spacing: DesktopTokens.px(8)
            Rectangle { anchors.verticalCenter: parent.verticalCenter; width: DesktopTokens.px(8); height: width; radius: width / 2; color: root.waiting ? DesktopTokens.amber : root.faintInk }
            MonoText { text: root.waiting ? qsTr("WAITING · APPROVE ON YOUR PHONE OR IN THE BROWSER") : qsTr("OFFLINE MODE · NOT SIGNED IN") }
        }
    }

    Connections {
        target: ShellStore
        function onSignedInChanged() { if (ShellStore.signedIn) root.signedIn() }
        function onAuthChallengeChanged() {
            root.challengeReceivedAt = Date.now()
            root.clockMs = root.challengeReceivedAt
            if (ShellStore.authChallenge && !root.qrRequested)
                Qt.openUrlExternally(ShellStore.authChallenge.verificationUriComplete || ShellStore.authChallenge.verificationUri)
        }
    }
}

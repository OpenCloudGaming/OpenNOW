import QtQuick
import QtQuick.Controls
import OpenNOW

Rectangle {
    id: root
    objectName: "desktopSourceSignIn"
    required property string sourceId
    readonly property var store: ShellStore.sourceOwnerState
    readonly property var source: store.sourceById(sourceId)
    readonly property var authState: store.authState(sourceId)
    readonly property var challenge: authState && authState.state === "pending" ? authState.challenge : null
    readonly property bool active: store.authSourceId === sourceId
    readonly property var kinds: store.authKinds(sourceId)
    readonly property string error: store.authErrors[sourceId] || ""

    implicitHeight: body.implicitHeight + DesktopTokens.px(48)
    radius: DesktopTokens.px(16)
    color: Theme.lightMode ? Theme.glass : "#C70B0F1A"
    border.color: Theme.seam

    function kindLabel(kind) {
        return kind === "device-code" ? qsTr("Sign in with a code")
            : kind === "browser" ? qsTr("Sign in with your browser")
            : kind === "pairing" ? qsTr("Pair this device") : kind
    }

    Column {
        id: body
        x: DesktopTokens.px(24); y: DesktopTokens.px(24)
        width: parent.width - DesktopTokens.px(48)
        spacing: DesktopTokens.px(14)

        Text {
            width: parent.width
            text: root.source ? qsTr("Sign in to %1").arg(String(root.source.name || root.sourceId)) : ""
            textFormat: Text.PlainText
            color: Theme.label; font.family: Theme.displayFont
            font.pixelSize: DesktopTokens.px(22); font.weight: Font.Black
            wrapMode: Text.WordWrap
        }
        Text {
            width: parent.width
            text: root.challenge
                ? qsTr("Finish signing in, then come back. OpenNOW checks automatically.")
                : qsTr("Your library and sessions for this service need an account. OpenNOW never asks for your password; sign-in happens on the service's own page.")
            color: Theme.textMuted; font.family: Theme.bodyFont
            font.pixelSize: DesktopTokens.bodySize; wrapMode: Text.WordWrap
        }

        Column {
            objectName: "desktopSourceDeviceCode"
            width: parent.width
            spacing: DesktopTokens.px(10)
            visible: root.challenge !== null && root.challenge.kind === "device-code"
            Text {
                text: qsTr("Go to")
                color: Theme.textMuted; font.family: Theme.bodyFont; font.pixelSize: DesktopTokens.captionSize
            }
            DesktopButton {
                objectName: "desktopSourceVerificationLink"
                text: root.challenge && root.challenge.verificationUri ? String(root.challenge.verificationUri) : ""
                font.pixelSize: DesktopTokens.smallSize
                onClicked: {
                    const url = String(root.challenge.verificationUri || "")
                    if (url.indexOf("https://") === 0)
                        Qt.openUrlExternally(url)
                }
            }
            Text {
                text: qsTr("and enter this code")
                color: Theme.textMuted; font.family: Theme.bodyFont; font.pixelSize: DesktopTokens.captionSize
            }
            Text {
                objectName: "desktopSourceUserCode"
                text: root.challenge && root.challenge.userCode ? String(root.challenge.userCode) : ""
                textFormat: Text.PlainText
                color: Theme.label; font.family: Theme.monoFont
                font.pixelSize: DesktopTokens.px(30); font.weight: Font.Bold; font.letterSpacing: 2
            }
        }

        Column {
            width: parent.width
            spacing: DesktopTokens.px(10)
            visible: root.challenge !== null && root.challenge.kind === "browser"
            Text {
                objectName: "desktopSourceBrowserWaiting"
                width: parent.width
                text: qsTr("Finish signing in on the page that opened in your browser. OpenNOW continues automatically.")
                color: Theme.textMuted; font.family: Theme.bodyFont; font.pixelSize: DesktopTokens.captionSize
                wrapMode: Text.WordWrap
            }
            DesktopButton {
                objectName: "desktopSourceBrowserOpen"
                enabled: root.active && root.store.canOpenBrowser
                text: qsTr("Open sign-in page")
                onClicked: root.store.openBrowser()
            }
        }

        Column {
            width: parent.width
            spacing: DesktopTokens.px(10)
            visible: root.challenge !== null && root.challenge.kind === "pairing"
            Text {
                text: root.challenge && root.challenge.code ? qsTr("Enter this code on the other device") : qsTr("Confirm the pairing on the other device")
                color: Theme.textMuted; font.family: Theme.bodyFont; font.pixelSize: DesktopTokens.captionSize
            }
            Text {
                visible: text !== ""
                text: root.challenge && root.challenge.code ? String(root.challenge.code) : ""
                textFormat: Text.PlainText
                color: Theme.label; font.family: Theme.monoFont
                font.pixelSize: DesktopTokens.px(30); font.weight: Font.Bold; font.letterSpacing: 2
            }
        }

        Row {
            spacing: DesktopTokens.px(10)
            visible: root.challenge === null
            Repeater {
                model: root.kinds
                delegate: DesktopButton {
                    required property string modelData
                    required property int index
                    objectName: "desktopSourceSignIn-" + modelData
                    primary: index === 0
                    enabled: ShellStore.ready && !root.store.authBusy
                    text: root.kindLabel(modelData)
                    onClicked: root.store.startSignIn(root.sourceId, modelData)
                }
            }
        }

        Row {
            spacing: DesktopTokens.px(12)
            visible: root.challenge !== null || (root.active && root.store.authBusy)
            BusyIndicator {
                width: DesktopTokens.px(24); height: width
                running: parent.visible && !AppController.reducedMotion
            }
            Text {
                anchors.verticalCenter: parent.verticalCenter
                text: root.authState && root.authState.state === "authorized" ? qsTr("Finishing sign-in…") : qsTr("Waiting for approval…")
                color: Theme.textMuted; font.family: Theme.bodyFont; font.pixelSize: DesktopTokens.captionSize
            }
            DesktopButton {
                objectName: "desktopSourceSignInCancel"
                text: qsTr("Cancel")
                onClicked: root.store.cancelSignIn()
            }
        }

        Text {
            objectName: "desktopSourceSignInError"
            visible: root.error !== ""
            width: parent.width
            text: root.error
            textFormat: Text.PlainText
            color: Theme.coral; font.family: Theme.bodyFont; font.pixelSize: DesktopTokens.captionSize
            wrapMode: Text.WordWrap
        }
    }
}

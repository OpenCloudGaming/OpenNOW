import QtQuick
import QtQuick.Controls
import OpenNOW

FocusScope {
    id: root
    objectName: "desktopSourceLibrary"
    property string searchQuery: ""
    readonly property var store: ShellStore.sourceOwnerState
    readonly property var library: ShellStore.sourceLibraryOwnerState
    readonly property var source: store.selectedSource
    readonly property var authState: source ? store.authState(source.id) : null
    property bool signInRequested: false
    readonly property bool canSignIn: source !== null && authState !== null && authState.state === "signed-out"
        && store.authKinds(source.id).length > 0
    onSourceChanged: signInRequested = false
    readonly property var session: ShellStore.sourceSession
    signal detailsRequested(var item)

    onSearchQueryChanged: searchTimer.restart()
    Timer { id: searchTimer; interval: 300; onTriggered: root.library.search(root.searchQuery) }

    function accountText() {
        if (!root.authState)
            return ""
        if (root.authState.state === "not-required")
            return qsTr("No account needed")
        if (root.authState.state === "signed-in")
            return qsTr("Signed in as %1").arg(String(root.authState.account && root.authState.account.name || ""))
        return qsTr("Not signed in")
    }

    Item {
        id: header
        x: DesktopTokens.px(24); y: DesktopTokens.px(18)
        width: parent.width - DesktopTokens.px(48)
        height: DesktopTokens.px(44)
        Column {
            anchors.verticalCenter: parent.verticalCenter
            Text {
                objectName: "desktopSourceName"
                text: root.source ? String(root.source.name || root.source.id) : ""
                textFormat: Text.PlainText
                color: Theme.label; font.family: Theme.displayFont
                font.pixelSize: DesktopTokens.px(20); font.weight: Font.Black
            }
            Text {
                text: root.accountText() + (root.library.mode === "public" ? " · " + qsTr("Public catalog") : "")
                textFormat: Text.PlainText
                color: Theme.textMuted; font.family: Theme.bodyFont; font.pixelSize: DesktopTokens.captionSize
            }
        }
        Row {
            anchors.right: parent.right
            anchors.verticalCenter: parent.verticalCenter
            spacing: DesktopTokens.px(10)
            Text {
                objectName: "desktopSourceSessionStatus"
                anchors.verticalCenter: parent.verticalCenter
                visible: root.session.active
                text: root.session.title !== "" ? qsTr("%1 · %2").arg(root.session.title).arg(root.session.message)
                    : root.session.message
                textFormat: Text.PlainText
                color: root.session.phase === "failed" ? Theme.coral : Theme.label
                font.family: Theme.bodyFont; font.pixelSize: DesktopTokens.captionSize
                elide: Text.ElideRight
                width: Math.min(implicitWidth, DesktopTokens.px(420))
            }
            DesktopButton {
                objectName: "desktopSourceSessionResume"
                visible: root.session.phase === "ready"
                primary: true
                text: qsTr("Resume")
                onClicked: root.session.resume()
            }
            DesktopButton {
                objectName: "desktopSourceSessionEnd"
                visible: root.session.active && !root.session.mediaActive
                text: root.session.phase === "failed" && root.session.session === null ? qsTr("Dismiss") : qsTr("End session")
                onClicked: root.session.stop()
            }
            DesktopButton {
                objectName: "desktopSourceSignInButton"
                visible: root.canSignIn && !signIn.visible
                primary: true
                text: qsTr("Sign in")
                onClicked: root.signInRequested = true
            }
            DesktopButton {
                objectName: "desktopSourceSignOut"
                visible: root.authState !== null && root.authState.state === "signed-in"
                text: qsTr("Sign out")
                onClicked: root.store.signOut(root.source.id)
            }
            DesktopButton {
                text: qsTr("Service settings")
                visible: root.store.has(root.source, "settings.v2")
                onClicked: AppController.navigate("settings-plugins")
            }
        }
    }

    DesktopSourceSignInPanel {
        id: signIn
        visible: root.source !== null && root.authState !== null && root.authState.state !== "signed-in"
            && root.authState.state !== "not-required" && root.store.authKinds(root.source.id).length > 0
            && (root.library.needsSignIn || root.signInRequested || root.store.authSourceId === root.source.id)
        x: header.x
        y: header.y + header.height + DesktopTokens.px(16)
        width: Math.min(header.width, DesktopTokens.px(640))
        sourceId: root.source ? root.source.id : ""
    }

    Text {
        id: statusLine
        objectName: "desktopSourceStatus"
        x: header.x
        y: (signIn.visible ? signIn.y + signIn.height : header.y + header.height) + DesktopTokens.px(12)
        width: header.width
        visible: text !== ""
        text: root.library.error !== "" ? root.library.error
            : root.library.loading && root.library.items.length === 0 ? qsTr("Loading games…")
            : root.library.mode !== "" && !root.library.loading && root.library.items.length === 0 ? qsTr("No games match.")
            : root.library.mode === "" && !signIn.visible && root.source !== null ? qsTr("This service has no catalog you can browse yet.") : ""
        textFormat: Text.PlainText
        color: root.library.error !== "" ? Theme.coral : Theme.textMuted
        font.family: Theme.bodyFont; font.pixelSize: DesktopTokens.bodySize
        wrapMode: Text.WordWrap
    }

    DesktopPosterGrid {
        id: grid
        objectName: "desktopSourceGrid"
        x: header.x - 6
        y: (statusLine.visible ? statusLine.y + statusLine.height : signIn.visible ? signIn.y + signIn.height : header.y + header.height) + DesktopTokens.px(16)
        width: parent.width - 2 * x
        height: parent.height - y
        model: root.library.items
        focus: true
        playHints: root.library.playbackSupported() && ShellStore.sourcePlaybackAvailable
        noteForGame: item => item.availability === "available" ? "" : DesktopTokens.sourceAvailabilityText(item.availability)
        onAtYEndChanged: if (atYEnd && count > 0) root.library.loadMore()
        onGameActivated: item => root.detailsRequested(item)
    }
}

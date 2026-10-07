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
    readonly property bool detailsOpen: library.details !== null || library.detailsRequestId !== "" || library.detailsError !== ""
    readonly property var session: ShellStore.sourceSession
    readonly property bool sessionHere: session.active && source !== null && session.sourceId === source.id
    readonly property bool playEnabled: library.playbackSupported() && ShellStore.sourcePlaybackAvailable
        && library.defaultVariant() !== null && !library.playRequested && !ShellStore.streamBusy
        && ShellStore.activeSession === null
    readonly property string playNote: {
        if (!library.details)
            return ""
        if (!library.playbackSupported())
            return qsTr("This service only lists games. They can't be played from OpenNOW.")
        if (!ShellStore.sourcePlaybackAvailable)
            return qsTr("This device can't play games from this service.")
        if (library.defaultVariant() === null)
            return qsTr("This game isn't available to play right now.")
        if (library.launchDecision && library.launchDecision.state === "blocked")
            return String(library.launchDecision.message || qsTr("The service can't start this game right now."))
        if (session.active || ShellStore.activeSession !== null)
            return qsTr("Finish your current session before starting another.")
        return ""
    }

    onSearchQueryChanged: searchTimer.restart()
    Timer { id: searchTimer; interval: 300; onTriggered: root.library.search(root.searchQuery) }

    function availabilityText(value) {
        return value === "maintenance" ? qsTr("Maintenance")
            : value === "patching" ? qsTr("Updating")
            : value === "subscription-required" ? qsTr("Subscription required")
            : value === "ownership-required" ? qsTr("Not owned")
            : value === "account-link-required" ? qsTr("Link an account")
            : value === "unavailable" ? qsTr("Unavailable") : ""
    }

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

    GridView {
        id: grid
        objectName: "desktopSourceGrid"
        x: header.x
        y: (statusLine.visible ? statusLine.y + statusLine.height : signIn.visible ? signIn.y + signIn.height : header.y + header.height) + DesktopTokens.px(16)
        width: (root.detailsOpen ? parent.width - details.width - DesktopTokens.px(24) : parent.width) - 2 * header.x
        height: parent.height - y - DesktopTokens.px(16)
        clip: true
        cellWidth: DesktopTokens.px(188)
        cellHeight: DesktopTokens.px(292)
        model: root.library.items
        boundsBehavior: Flickable.StopAtBounds
        ScrollBar.vertical: ScrollBar {}
        onAtYEndChanged: if (atYEnd && count > 0) root.library.loadMore()
        delegate: ItemDelegate {
            required property var modelData
            required property int index
            objectName: "desktopSourceTile-" + modelData.ref.localId
            width: grid.cellWidth - DesktopTokens.px(14)
            height: grid.cellHeight - DesktopTokens.px(14)
            padding: 0
            Accessible.name: modelData.title
            background: Rectangle {
                radius: DesktopTokens.px(14)
                color: parent.hovered ? DesktopTokens.raisedStrong : DesktopTokens.raised
                border.width: parent.activeFocus ? 2 : 1
                border.color: parent.activeFocus ? Theme.focus : Theme.seam
            }
            contentItem: Column {
                spacing: DesktopTokens.px(8)
                Rectangle {
                    width: parent.width; height: DesktopTokens.px(210)
                    radius: DesktopTokens.px(14)
                    color: DesktopTokens.seamSoft
                    clip: true
                    Image {
                        id: artwork
                        anchors.fill: parent
                        visible: artwork.status === Image.Ready
                        source: modelData.imageUrl
                        fillMode: Image.PreserveAspectCrop
                        asynchronous: true
                    }
                    Text {
                        anchors.centerIn: parent
                        visible: modelData.imageUrl === ""
                        text: modelData.title.charAt(0).toUpperCase()
                        color: Theme.textMuted; font.family: Theme.displayFont
                        font.pixelSize: DesktopTokens.px(48); font.weight: Font.Black
                    }
                }
                Text {
                    x: DesktopTokens.px(10); width: parent.width - DesktopTokens.px(20)
                    text: modelData.title
                    textFormat: Text.PlainText
                    elide: Text.ElideRight
                    color: Theme.label; font.family: Theme.bodyFont
                    font.pixelSize: DesktopTokens.bodySize; font.weight: Font.Bold
                }
                Text {
                    x: DesktopTokens.px(10); width: parent.width - DesktopTokens.px(20)
                    text: root.availabilityText(modelData.availability) || modelData.subtitle
                    textFormat: Text.PlainText
                    elide: Text.ElideRight
                    color: modelData.availability === "available" ? Theme.textMuted : Theme.yellow
                    font.family: Theme.bodyFont; font.pixelSize: DesktopTokens.captionSize
                }
            }
            onClicked: root.library.openDetails(modelData.ref.localId)
        }
    }

    Rectangle {
        id: details
        objectName: "desktopSourceDetails"
        visible: root.detailsOpen
        anchors.right: parent.right; anchors.rightMargin: DesktopTokens.px(24)
        y: grid.y
        width: Math.min(DesktopTokens.px(380), parent.width * 0.4)
        height: Math.min(parent.height - y - DesktopTokens.px(16), detailsColumn.implicitHeight + DesktopTokens.px(48))
        radius: DesktopTokens.px(16)
        color: Theme.lightMode ? Theme.glass : "#C70B0F1A"
        border.color: Theme.seam
        Column {
            id: detailsColumn
            x: DesktopTokens.px(24); y: DesktopTokens.px(24)
            width: parent.width - DesktopTokens.px(48)
            spacing: DesktopTokens.px(12)
            Text {
                width: parent.width
                text: root.library.details ? root.library.details.game.title
                    : root.library.detailsError !== "" ? root.library.detailsError : qsTr("Loading…")
                textFormat: Text.PlainText
                color: root.library.detailsError !== "" ? Theme.coral : Theme.label
                font.family: Theme.displayFont; font.pixelSize: DesktopTokens.px(22); font.weight: Font.Black
                wrapMode: Text.WordWrap
            }
            Text {
                objectName: "desktopSourceDescription"
                width: parent.width
                visible: text !== ""
                text: root.library.details ? root.library.details.description : ""
                textFormat: Text.PlainText
                color: Theme.textMuted; font.family: Theme.bodyFont; font.pixelSize: DesktopTokens.bodySize
                wrapMode: Text.WordWrap
            }
            DesktopButton {
                objectName: "desktopSourcePlay"
                width: parent.width
                primary: enabled
                visible: root.library.details !== null
                enabled: root.playEnabled
                text: root.library.playRequested ? qsTr("Checking…") : qsTr("Play")
                onClicked: root.library.play()
            }
            Text {
                objectName: "desktopSourcePlayNote"
                width: parent.width
                visible: text !== ""
                text: root.playNote
                textFormat: Text.PlainText
                color: Theme.textMuted; font.family: Theme.bodyFont; font.pixelSize: DesktopTokens.captionSize
                wrapMode: Text.WordWrap
            }
            DesktopButton {
                text: qsTr("Close")
                onClicked: root.library.closeDetails()
            }
        }
    }
}

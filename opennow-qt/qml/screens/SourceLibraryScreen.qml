import QtQuick
import QtQuick.Controls
import OpenNOW

FocusScope {
    id: root
    objectName: "consoleSourceLibrary"
    property string currentRoute: "library"
    readonly property var store: ShellStore.sourceOwnerState
    readonly property var library: ShellStore.sourceLibraryOwnerState
    readonly property var source: store.selectedSource
    readonly property var authState: source ? store.authState(source.id) : null
    readonly property var challenge: authState && authState.state === "pending" ? authState.challenge : null
    readonly property bool signInShown: source !== null && authState !== null && authState.state !== "signed-in"
        && authState.state !== "not-required" && store.authKinds(source.id).length > 0
        && (library.needsSignIn || signInRequested || store.authSourceId === source.id)
    readonly property var selectedItem: grid.currentIndex >= 0 ? library.items[grid.currentIndex] || null : null
    property bool signInRequested: false
    readonly property bool canSignIn: source !== null && authState !== null && authState.state === "signed-out"
        && store.authKinds(source.id).length > 0
    onSourceChanged: signInRequested = false
    property bool chooserOpen: false
    property bool detailsOpen: false
    readonly property var session: ShellStore.sourceSession
    readonly property bool sessionHere: session.active && source !== null && session.sourceId === source.id
    readonly property bool playEnabled: library.playbackSupported() && ShellStore.sourcePlaybackAvailable
        && library.defaultVariant() !== null && !library.playRequested && !ShellStore.streamBusy
        && ShellStore.activeSession === null
    focus: true

    function accountText() {
        if (!authState)
            return ""
        if (authState.state === "not-required")
            return qsTr("No account needed")
        if (authState.state === "signed-in")
            return qsTr("Signed in as %1").arg(String(authState.account && authState.account.name || ""))
        return qsTr("Not signed in")
    }

    function chooserOptions() {
        return store.playableSources.map(item => ({label: String(item.name || item.id), value: item.id,
            detail: item.id === store.gfnId ? "" : (store.signedIn(item.id) ? qsTr("Signed in") : "")}))
    }

    function openChooser() {
        sourceSheet.options = chooserOptions()
        sourceSheet.currentIndex = store.playableSources.findIndex(item => item.id === store.selectedSourceId)
        sourceSheet.focusedIndex = Math.max(0, sourceSheet.currentIndex)
        chooserOpen = true
        sourceSheet.syncFocus()
        sourceSheet.forceActiveFocus()
    }

    function detailOptions() {
        const options = []
        if (sessionHere && session.phase === "ready")
            options.push({label: qsTr("Resume"), value: "resume", detail: session.title})
        if (sessionHere && !session.mediaActive)
            options.push({label: qsTr("End session"), value: "end", detail: session.message})
        if (library.details)
            options.push({label: library.playRequested ? qsTr("Checking…") : qsTr("Play"), value: "play", disabled: !playEnabled,
                detail: !library.playbackSupported() ? qsTr("This service only lists games")
                    : !ShellStore.sourcePlaybackAvailable ? qsTr("This device can't play games from this service.")
                    : library.defaultVariant() === null ? qsTr("This game isn't available to play right now.")
                    : library.launchDecision && library.launchDecision.state === "blocked"
                        ? String(library.launchDecision.message || qsTr("The service can't start this game right now."))
                    : session.active || ShellStore.activeSession !== null
                        ? qsTr("Finish your current session before starting another.") : ""})
        options.push({label: qsTr("Back"), value: "back", detail: ""})
        return options
    }

    function openDetails() {
        if (!selectedItem)
            return
        library.openDetails(selectedItem.ref.localId)
        detailsOpen = true
        detailSheet.focusedIndex = 0
        detailSheet.syncFocus()
        detailSheet.forceActiveFocus()
    }

    function closeSheets() {
        chooserOpen = false
        detailsOpen = false
        library.closeDetails()
        Qt.callLater(() => (signInShown ? signInActions : grid).forceActiveFocus())
    }

    Keys.onPressed: event => {
        if (event.key === Qt.Key_Y && !event.isAutoRepeat && !chooserOpen && !detailsOpen) {
            openChooser()
            event.accepted = true
        } else if (event.key === Qt.Key_X && !event.isAutoRepeat && !chooserOpen && !detailsOpen && canSignIn) {
            signInRequested = !signInRequested
            Qt.callLater(() => (signInShown ? signInActions : grid).forceActiveFocus())
            event.accepted = true
        }
    }

    Connections {
        target: root.library
        function onDetailsChanged() {
            if (!root.detailsOpen || root.library.details === null)
                return
            Qt.callLater(() => {
                detailSheet.focusedIndex = 0
                detailSheet.syncFocus()
            })
        }
    }

    ScreenBackground { tint: "#17233B" }

    GlassPanel {
        objectName: "consoleSourceHeader"
        x: 96; y: 124; width: root.width - 192; height: 96; panelRadius: 32
        Column {
            x: 34; anchors.verticalCenter: parent.verticalCenter
            spacing: 4
            Text {
                text: root.source ? String(root.source.name || root.source.id) : ""
                textFormat: Text.PlainText
                color: Theme.label; font.family: Theme.displayFont; font.pixelSize: 32; font.weight: Font.Black
            }
            Text {
                text: root.accountText() + (root.library.mode === "public" ? " · " + qsTr("Public catalog") : "")
                textFormat: Text.PlainText
                color: Theme.textMuted; font.family: Theme.bodyFont; font.pixelSize: 17; font.weight: Font.Bold
            }
        }
        Text {
            objectName: "consoleSourceSessionStatus"
            anchors.right: parent.right; anchors.rightMargin: 34
            anchors.verticalCenter: parent.verticalCenter
            width: Math.min(implicitWidth, parent.width * 0.5)
            visible: root.session.active
            text: root.session.title !== "" ? qsTr("%1 · %2").arg(root.session.title).arg(root.session.message)
                : root.session.message
            textFormat: Text.PlainText
            horizontalAlignment: Text.AlignRight
            elide: Text.ElideRight
            color: root.session.phase === "failed" ? Theme.coral : Theme.label
            font.family: Theme.bodyFont; font.pixelSize: 17; font.weight: Font.Bold
        }
    }

    GlassPanel {
        id: signInPanel
        objectName: "consoleSourceSignIn"
        visible: root.signInShown
        x: 96; y: 244; width: Math.min(root.width - 192, 1000); height: signInColumn.implicitHeight + 72; panelRadius: 40
        Column {
            id: signInColumn
            x: 40; y: 36; width: parent.width - 80
            spacing: 16
            Text {
                width: parent.width
                text: root.source ? qsTr("Sign in to %1").arg(String(root.source.name || root.source.id)) : ""
                textFormat: Text.PlainText
                color: Theme.label; font.family: Theme.displayFont; font.pixelSize: 40; font.weight: Font.Black
                wrapMode: Text.WordWrap
            }
            Text {
                width: parent.width
                visible: root.challenge !== null && root.challenge.kind === "device-code"
                text: root.challenge && root.challenge.kind === "device-code"
                    ? qsTr("On your phone or computer, go to %1 and enter this code:").arg(String(root.challenge.verificationUri || "")) : ""
                textFormat: Text.PlainText
                color: Theme.textMuted; font.family: Theme.bodyFont; font.pixelSize: 21; font.weight: Font.DemiBold
                wrapMode: Text.WordWrap
            }
            Text {
                objectName: "consoleSourceUserCode"
                visible: text !== ""
                text: root.challenge && (root.challenge.userCode || root.challenge.code)
                    ? String(root.challenge.userCode || root.challenge.code) : ""
                textFormat: Text.PlainText
                color: Theme.label; font.family: Theme.monoFont; font.pixelSize: 48; font.weight: Font.Bold; font.letterSpacing: 3
            }
            Text {
                width: parent.width
                visible: root.challenge !== null && root.challenge.kind === "browser"
                text: qsTr("Finish signing in on the page that opened in your browser. OpenNOW continues automatically.")
                color: Theme.textMuted; font.family: Theme.bodyFont; font.pixelSize: 19; font.weight: Font.Bold
                wrapMode: Text.WordWrap
            }
            Text {
                width: parent.width
                visible: text !== ""
                text: root.source ? root.store.authErrors[root.source.id] || "" : ""
                textFormat: Text.PlainText
                color: Theme.coral; font.family: Theme.bodyFont; font.pixelSize: 19; font.weight: Font.Bold
                wrapMode: Text.WordWrap
            }
            ConsoleActionColumn {
                id: signInActions
                width: Math.min(640, parent.width)
                focus: root.signInShown
                Repeater {
                    model: root.challenge === null && root.source ? root.store.authKinds(root.source.id) : []
                    delegate: ConsoleActionButton {
                        required property string modelData
                        required property int index
                        width: parent.width
                        primary: index === 0
                        glyph: index === 0 ? "A" : ""
                        enabled: ShellStore.ready && !root.store.authBusy
                        text: modelData === "device-code" ? qsTr("Sign in with a code")
                            : modelData === "browser" ? qsTr("Sign in with your browser") : qsTr("Pair this device")
                        onClicked: root.store.startSignIn(root.source.id, modelData)
                    }
                }
                ConsoleActionButton {
                    objectName: "consoleSourceBrowserOpen"
                    width: parent.width
                    visible: root.challenge !== null && root.challenge.kind === "browser"
                    enabled: root.store.canOpenBrowser
                    glyph: "A"
                    primary: true
                    text: qsTr("Open sign-in page")
                    onClicked: root.store.openBrowser()
                }
                ConsoleActionButton {
                    objectName: "consoleSourceSignInCancel"
                    width: parent.width
                    visible: root.challenge !== null || root.store.authBusy
                    glyph: "B"
                    text: root.authState && root.authState.state === "authorized" ? qsTr("Finishing sign-in…") : qsTr("Cancel sign-in")
                    onClicked: root.store.cancelSignIn()
                }
            }
        }
    }

    Item {
        id: gridFrame
        visible: !root.signInShown
        x: 77; y: 226
        width: root.width - 154
        height: root.height - y - 140
        clip: true
        GridView {
            id: grid
            objectName: "consoleSourceGrid"
            visible: !root.signInShown
            x: 19; y: 18
            width: parent.width - 38
            height: parent.height - 18
            cellWidth: 172; cellHeight: 269
            clip: false
            focus: !root.signInShown
            model: root.library.items
            keyNavigationWraps: false
            onCurrentIndexChanged: {
                positionViewAtIndex(currentIndex, GridView.Contain)
                if (currentIndex >= count - 10)
                    root.library.loadMore()
            }
            Keys.onPressed: event => {
                if ((event.key === Qt.Key_Return || event.key === Qt.Key_Enter) && !event.isAutoRepeat) {
                    root.openDetails()
                    event.accepted = true
                }
            }
            delegate: Item {
                id: tile
                required property var modelData
                required property int index
                width: grid.cellWidth; height: grid.cellHeight
                PosterTile {
                    width: 156; height: 234
                    title: tile.modelData.title
                    artwork: tile.modelData.imageUrl
                    showLabel: false
                    focusPolicy: Qt.NoFocus
                    currentItem: grid.activeFocus && tile.GridView.isCurrentItem
                    parked: !grid.activeFocus && tile.GridView.isCurrentItem
                    onClicked: {
                        grid.currentIndex = tile.index
                        grid.forceActiveFocus()
                        root.openDetails()
                    }
                }
            }
        }
    }

    Text {
        objectName: "consoleSourceStatus"
        visible: !root.signInShown && text !== ""
        x: 96; y: gridFrame.y + 38; width: root.width - 192
        horizontalAlignment: Text.AlignHCenter
        text: root.library.error !== "" ? root.library.error
            : root.library.loading && root.library.items.length === 0 ? qsTr("Loading games…")
            : root.library.mode !== "" && !root.library.loading && root.library.items.length === 0 ? qsTr("No games match.")
            : root.library.mode === "" && root.source !== null ? qsTr("This service has no catalog you can browse yet.") : ""
        textFormat: Text.PlainText
        color: root.library.error !== "" ? Theme.coral : Theme.label
        font.family: Theme.displayFont; font.pixelSize: 30; font.weight: Font.Black
        wrapMode: Text.WordWrap
    }

    AppChrome {
        anchors.fill: parent
        title: root.selectedItem && !root.signInShown ? root.selectedItem.title : qsTr("Library")
        currentRoute: root.currentRoute
        leftHints: root.store.playableSources.length > 1 ? [{glyph: "Y", label: qsTr("Change service")}] : []
        rightHints: (root.canSignIn ? [{glyph: "X", label: root.signInShown ? qsTr("Browse") : qsTr("Sign in")}] : [])
            .concat(root.signInShown ? [{glyph: "A", label: qsTr("Select")}] : [{glyph: "A", label: qsTr("Details")}])
        onRouteRequested: route => AppController.navigate(route)
    }

    ConsoleChoiceSheet {
        id: sourceSheet
        objectName: "consoleSourceSheet"
        opened: root.chooserOpen
        textFormat: Text.PlainText
        eyebrow: qsTr("Browse with")
        title: qsTr("Choose a service")
        chooseText: qsTr("Choose")
        dismissText: qsTr("Cancel")
        onChosen: index => {
            const option = sourceSheet.options[index]
            root.closeSheets()
            if (option)
                root.store.select(option.value)
        }
        onDismissed: root.closeSheets()
    }

    ConsoleChoiceSheet {
        id: detailSheet
        objectName: "consoleSourceDetails"
        opened: root.detailsOpen
        textFormat: Text.PlainText
        eyebrow: root.source ? String(root.source.name || "") : ""
        title: root.library.details ? root.library.details.game.title : root.selectedItem ? root.selectedItem.title : ""
        description: root.library.detailsError !== "" ? root.library.detailsError
            : root.library.details ? root.library.details.description : qsTr("Loading…")
        options: root.detailOptions()
        currentIndex: -1
        chooseText: qsTr("Select")
        dismissText: qsTr("Back")
        onChosen: index => {
            const option = detailSheet.options[index]
            if (!option || option.disabled)
                return
            if (option.value === "back")
                root.closeSheets()
            else if (option.value === "play")
                root.library.play()
            else if (option.value === "resume")
                root.session.resume()
            else if (option.value === "end")
                root.session.stop()
        }
        onDismissed: root.closeSheets()
    }
}

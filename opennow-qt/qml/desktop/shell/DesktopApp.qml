import QtQuick
import QtQuick.Controls
import QtQuick.Window
import OpenNOW

FocusScope {
    id: root
    anchors.fill: parent
    focus: true
    property bool commandOpen: false
    property bool modeErrorVisible: false
    property string searchText: ""
    property string settingsSubtitle: qsTr("Profile")
    readonly property string route: AppController.route
    // Game details are modal on desktop. Keep the route beneath the modal so
    // opening a Home or Store tile does not silently swap in the Library page.
    property string contentRoute: "home"
    readonly property string requestedContentRoute: route === "game-detail"
        ? (AppController.backRoute || "library") : route
    onRequestedContentRouteChanged: {
        if (requestedContentRoute !== "inserting" && requestedContentRoute !== "stream")
            contentRoute = requestedContentRoute
    }
    readonly property bool signInVisible: !ShellStore.authRestorePending && (!ShellStore.signedIn || route === "sign-in")
    readonly property bool sessionStartingVisible: !signInVisible
        && (route === "inserting" || (streamVisible && !desktopStream.videoReady))
    readonly property bool streamVisible: !signInVisible && route === "stream"
    readonly property bool streamPointerLocked: streamVisible && desktopStream.streamPointerLocked
    readonly property var frameGenerationStats: streamVisible ? desktopStream.frameGenerationStats : ({})
    readonly property bool shellVisible: !signInVisible && !sessionStartingVisible && !streamVisible

    function titleForRoute(value) {
        if (value === "updates") return qsTr("Updates")
        if (value === "library" || value === "game-detail") return qsTr("Library")
        if (value === "store") return qsTr("Store")
        if (value === "friends") return qsTr("Friends")
        if (value.indexOf("settings") === 0 || value === "controllers") return qsTr("Settings")
        return qsTr("Home")
    }
    function subtitleForRoute(value) {
        if (value === "updates") return ""
        if (value === "library" || value === "game-detail") return qsTr("%1 games").arg(ShellStore.catalogTotalCount || ShellStore.catalogGames.length)
        if (value === "store") return qsTr("%1 in catalog").arg(ShellStore.storeTotalCount || ShellStore.storeGames.length)
        if (value === "friends") return qsTr("Coming soon")
        if (value.indexOf("settings") === 0 || value === "controllers") return settingsSubtitle
        return ShellStore.catalogTotalCount
            ? qsTr("%1 games").arg(ShellStore.catalogTotalCount)
            : qsTr("Your library")
    }
    function settingsSection(value) {
        if (value === "settings-subscription") return 1
        if (value === "settings-stores" || value === "game-accounts") return 2
        if (value === "settings-streaming" || value === "settings-video" || value === "settings-video-dropdown") return 3
        if (value === "settings-audio") return 4
        if (value === "controllers" || value === "settings-input") return 5
        if (value === "settings-network") return 6
        if (value === "settings-themes") return 8
        if (value === "settings-console") return 9
        if (value === "settings-shortcuts") return 10
        if (value === "settings-advanced" || value === "settings-advanced-dropdown") return 11
        if (value === "settings-account") return 0
        return 3
    }
    function contentForRoute(value) {
        if (value === "updates") return updatesComponent
        if (value === "store") return storeComponent
        if (value === "friends") return friendsComponent
        if (value.indexOf("settings") === 0 || value === "controllers") return settingsComponent
        if (value === "library" || value === "game-detail") return libraryComponent
        return homeComponent
    }
    function toggleConsoleMode() {
        ShellStore.requestConsoleSurface(!DesktopTokens.consoleModeTargetOn(Window.window))
    }
    // Fit the design within bounded readable sizes, then apply the user's
    // interface scale. Qt handles device-pixel-ratio changes independently.
    function updateUiScale() {
        if (root.width <= 0 || root.height <= 0)
            return
        const fitted = DesktopTokens.scaleForWindow(root.width, root.height)
        const preference = Number(ShellStore.settings.desktopUiScale || 1)
        DesktopTokens.uiScale = Math.min(1.4, Math.max(0.9, fitted * preference))
    }
    onWidthChanged: root.updateUiScale()
    onHeightChanged: root.updateUiScale()
    Connections {
        target: ShellStore
        function onSettingsChanged() { root.updateUiScale() }
    }
    function resynchronizeStreamInput() {
        if (root.sessionStartingVisible)
            sessionStarting.restoreFocus()
        else if (root.streamVisible)
            desktopStream.resynchronizeStreamInput()
    }
    function restoreShellFocus() {
        if (root.shellVisible && root.route !== "game-detail" && !root.commandOpen
                && AppController.overlay === "" && pageLoader.item)
            pageLoader.item.forceActiveFocus()
    }

    DesktopSignInScreen {
        id: desktopSignIn
        anchors.fill: parent
        visible: root.signInVisible
        z: 50
        // Restoring an existing session must not discard a requested deep link
        // (including screenshot/smoke-test routes). A completed interactive
        // sign-in still returns to Home as expected.
        onSignedIn: if (root.route === "sign-in") AppController.navigate("home")
    }

    DesktopShell {
        id: shell
        anchors.fill: parent
        visible: root.shellVisible || (root.route === "inserting" && launchReveal.progress < 1)
        enabled: root.shellVisible
        route: root.contentRoute
        title: root.titleForRoute(root.contentRoute)
        subtitle: root.subtitleForRoute(root.contentRoute)
        searchText: root.searchText
        onSearchTextChanged: root.searchText = searchText
        onRouteRequested: route => AppController.navigate(route)
        onConsoleModeRequested: root.toggleConsoleMode()
        onCommandPaletteRequested: root.commandOpen = true

        Loader {
            id: pageLoader
            objectName: "desktopPageLoader"
            anchors.fill: parent
            sourceComponent: root.contentForRoute(root.contentRoute)
            opacity: pageEntrance.pageOpacity
            transform: Translate { y: pageEntrance.offset }
            PageEntrance { id: pageEntrance; objectName: "desktopPageEntrance" }
            onLoaded: {
                pageEntrance.restart()
                Qt.callLater(root.restoreShellFocus)
            }
        }
    }

    MotionProgress {
        id: launchReveal
        objectName: "sessionLaunchMotion"
        shown: root.sessionStartingVisible
        enterDuration: 280
        exitDuration: 420
    }

    DesktopSessionStarting {
        id: sessionStarting
        layer.enabled: HdrOutput.chromeRequired && root.streamVisible
        layer.effect: HdrChromeEffect {}
        anchors.fill: parent
        visible: launchReveal.present && !root.signInVisible
            && (root.route === "inserting" || root.streamVisible)
        opacity: launchReveal.progress
        enabled: root.sessionStartingVisible
            && !ShellStore.streamOverlayBlocksGameplayInput(AppController.overlay)
        focus: enabled
        z: 90
        onCancelRequested: ShellStore.requestStreamExitConfirmation()
        onRetryRequested: root.route === "stream"
            ? ShellStore.retryNativeStreamer() : ShellStore.retrySessionLaunch()
    }

    DesktopStreamScreen {
        id: desktopStream
        anchors.fill: parent
        visible: root.streamVisible
        launchCovered: launchReveal.present
        focus: visible && !launchCovered
        z: 85
        onStopRequested: ShellStore.requestStreamExitConfirmation()
    }

    DesktopGameModal {
        anchors.fill: parent
        game: SmokeTestMode ? SmokeTestGame : ShellStore.selectedGame
        opened: root.shellVisible && root.route === "game-detail"
        z: 100
        onCloseRequested: AppController.goBack()
        onPlayRequested: ShellStore.launchSelectedGame(false)
    }
    DesktopCommandPalette {
        opened: root.commandOpen && root.shellVisible
        z: 120
        onCloseRequested: root.commandOpen = false
        onRouteRequested: route => AppController.navigate(route)
        onGameRequested: game => { ShellStore.selectedGame = game; ShellStore.launchSelectedGame(false) }
    }

    Rectangle {
        anchors.top: parent.top
        layer.enabled: HdrOutput.chromeRequired && root.streamVisible
        layer.effect: HdrChromeEffect {}
        anchors.topMargin: 76
        anchors.horizontalCenter: parent.horizontalCenter
        width: Math.min(640, errorText.implicitWidth + 40)
        height: 48
        radius: 12
        color: "#F02B1D24"
        border.width: 1
        border.color: DesktopTokens.danger
        visible: root.modeErrorVisible
        z: 220
        Text {
            id: errorText
            anchors.centerIn: parent
            width: parent.width - 28
            text: ShellStore.consoleSurfaceError
            color: "#FFFFDAD6"
            font.family: DesktopTokens.bodyFont
            font.pixelSize: 12
            font.weight: Font.Bold
            horizontalAlignment: Text.AlignHCenter
            elide: Text.ElideRight
        }
    }
    Timer {
        id: modeErrorTimer
        interval: 6000
        onTriggered: root.modeErrorVisible = false
    }

    Component {
        id: homeComponent
        DesktopHomeScreen {
            active: root.route === "home"
            onRouteRequested: route => AppController.navigate(route)
            onGameRequested: game => ShellStore.openGame(game)
        }
    }
    Component {
        id: libraryComponent
        DesktopLibraryScreen {
            searchQuery: root.searchText
            onDetailsRequested: game => ShellStore.openGame(game)
            onPlayRequested: game => { ShellStore.selectedGame = game; ShellStore.launchSelectedGame(false) }
        }
    }
    Component {
        id: storeComponent
        DesktopStoreContent {
            searchText: root.searchText
            onGameSelected: game => ShellStore.openGame(game)
            onPlayRequested: game => { ShellStore.selectedGame = game; ShellStore.launchSelectedGame(false) }
            onRouteRequested: route => AppController.navigate(route)
            onClaimRequested: games => ShellStore.accessibilityMessage = qsTr("Claim request prepared for %1 games").arg(games.length)
            onSearchRequested: shell.forceActiveFocus()
            onMessageRequested: message => ShellStore.accessibilityMessage = message
        }
    }
    Component { id: friendsComponent; DesktopFriendsScreen {} }
    Component { id: updatesComponent; DesktopUpdateScreen {} }
    Component {
        id: settingsComponent
        DesktopSettingsScreen {
            selectedSection: root.settingsSection(root.route)
            onSelectedSectionChanged: root.settingsSubtitle = pageTitles[selectedSection]
            onRequestConsoleMode: enabled => ShellStore.requestConsoleSurface(enabled)
            Component.onCompleted: root.settingsSubtitle = pageTitles[selectedSection]
        }
    }

    Connections {
        target: AppController
        function onRouteChanged() {
            root.commandOpen = false
            root.searchText = ""
            Qt.callLater(root.restoreShellFocus)
        }
    }
    Connections {
        target: ShellStore
        function onConsoleSurfaceErrorChanged() {
            if (ShellStore.consoleSurfaceError === "")
                return
            root.modeErrorVisible = true
            modeErrorTimer.restart()
        }
    }
    onSignInVisibleChanged: Qt.callLater(() => {
        if (root.signInVisible)
            desktopSignIn.forceActiveFocus()
        else
            root.restoreShellFocus()
    })
    onCommandOpenChanged: if (!commandOpen) Qt.callLater(root.restoreShellFocus)
    Component.onCompleted: {
        // Synchronous: onCompleted always runs on a live instance, while a
        // deferred call may fire after a surface switch destroyed it (method
        // calls on dead wrappers throw; property reads just yield undefined).
        root.updateUiScale()
        if (root.requestedContentRoute !== "inserting" && root.requestedContentRoute !== "stream")
            root.contentRoute = root.requestedContentRoute
        Qt.callLater(() => {
            if (root.signInVisible)
                desktopSignIn.forceActiveFocus()
        })
    }
    Keys.onPressed: event => {
        if ((event.modifiers & Qt.ControlModifier) && event.key === Qt.Key_K) {
            root.commandOpen = !root.commandOpen
            event.accepted = true
        } else if (event.key === Qt.Key_Escape && root.commandOpen) {
            root.commandOpen = false
            event.accepted = true
        }
    }
}

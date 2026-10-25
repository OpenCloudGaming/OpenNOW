pragma ComponentBehavior: Bound

import QtQuick
import QtQuick.Controls
import QtQuick.Window
import OpenNOW

FocusScope {
    id: root
    width: 1440
    height: 900
    property bool opened: false
    readonly property bool present: reveal.present
    visible: present
    enabled: opened
    focus: opened

    signal resumeRequested()
    signal inviteRequested()
    signal consoleModeRequested(bool enabled)
    signal fullscreenRequested()
    signal endSessionRequested()
    signal statsRequested()

    property string page: "main"
    property string pendingAction: ""
    property bool closing: false
    property double nowMs: Date.now()
    readonly property var game: ShellStore.selectedGame || ({})
    readonly property var session: ShellStore.activeSession || ({})
    readonly property var profile: session.negotiatedStreamProfile || session.streamProfile || ({})
    readonly property var live: ShellStore.streamer || ({})
    readonly property bool liveTelemetryAvailable: ["starting", "negotiating", "connecting", "streaming", "error"]
        .indexOf(String(live.status || "")) >= 0
    readonly property bool modePending: DesktopTokens.consoleModePending(Window.window)
    readonly property bool modeOn: DesktopTokens.consoleModeOn(Window.window)
    readonly property bool fullscreen: Window.window
        && Window.window.visibility === Window.FullScreen
    readonly property int outputHeight: Number(profile.height || 0)
    readonly property bool invitesAvailable: Boolean(ShellStore.socialCapabilities && ShellStore.socialCapabilities.invitesAvailable)
    readonly property bool controllerHints: AppController.inputMode === "controller"
    readonly property var pads: ControllerInput.availableControllers || []
    readonly property var flightDevices: ControllerInput.flightDevices || []
    readonly property bool flightVisible: ControllerInput.forwardFlightControls && flightDevices.length > 0
    readonly property int activeFlightCount: flightDevices.filter(device => device.status === "active").length
    readonly property int declinedFlightCount: flightDevices.filter(device => device.status === "declined").length
    readonly property color flightColor: activeFlightCount === flightDevices.length
        ? (Theme.lightMode ? DesktopTokens.flightReady : DesktopTokens.flightMint)
        : (Theme.lightMode ? DesktopTokens.flightWarning : DesktopTokens.ledAmber)
    readonly property int drawerMargin: 16
    readonly property int drawerWidth: Math.min(456, root.width - 2 * drawerMargin)

    function liveNumber(value) {
        return value === undefined || value === null || isNaN(Number(value)) ? null : Number(value)
    }
    function firstAvailable(primary, fallback) {
        return primary === undefined || primary === null ? fallback : primary
    }
    function liveValue(value) {
        return root.liveTelemetryAvailable ? value : undefined
    }
    function liveText(value, suffix, fallback) {
        const number = root.liveNumber(value)
        return number === null ? (fallback || qsTr("Measuring"))
            : String(Math.round(number)) + (suffix || "")
    }
    function shortcut(key, fallback) {
        const value = ShellStore.settings[key]
        return String(value === undefined || value === null ? fallback : value)
    }
    readonly property string fpsText: root.liveText(root.liveValue(root.firstAvailable(live.framesPerSecond, live.fps)), "", qsTr("Measuring"))
    readonly property string bitrateText: root.liveText(root.liveValue(root.firstAvailable(live.bitrateMbps, live.receiveBitrateMbps)), " Mb", qsTr("Measuring"))
    readonly property string codecText: String(root.liveValue(live.codec) || profile.codec
        || ShellStore.runtimeStreamProfile.codec || ShellStore.settings.codec || qsTr("Automatic")).toUpperCase()
    readonly property string transportText: String(root.liveValue(live.transport) || "NVST").toUpperCase()
    readonly property string backendText: String(root.liveValue(live.mediaBackend) || qsTr("Pending")).toUpperCase()
    readonly property string streamerStatus: String(live.status || qsTr("Starting")).toUpperCase()
    readonly property string resolution: outputHeight >= 2160 ? "2160P"
        : outputHeight >= 1440 ? "1440P"
        : outputHeight >= 1080 ? "1080P"
        : outputHeight > 0 ? outputHeight + "P" : qsTr("Pending")

    function runAction(action) {
        if (closing) return
        if (action === "microphone") {
            ShellStore.toggleMicrophone()
            return
        }
        if (action === "antiAfk") {
            ShellStore.applyStreamShortcutAction("toggle-anti-afk")
            return
        }
        if (action === "fullscreen") {
            fullscreenRequested()
            return
        }
        if (action === "controllers") {
            openPage("controllers")
            return
        }
        pendingAction = action
        closing = true
        if (reveal.progress === 0) Qt.callLater(root.finishAction)
    }
    function finishAction() {
        // A hidden callback or rapid toggle must never dispatch the action twice.
        const action = pendingAction
        pendingAction = ""
        if (action === "resume") resumeRequested()
        else if (action === "invite") inviteRequested()
        else if (action === "console") consoleModeRequested(!root.modeOn)
        else if (action === "end") endSessionRequested()
        else if (action === "stats") statsRequested()
    }
    function openPage(name) {
        page = name
        if (name === "controllers") controllersPage.focusFirst()
        else controllersRow.forceActiveFocus()
    }
    function clockText() {
        if (ShellStore.streamStartedAtMs <= 0)
            return "—"
        const elapsed = Math.max(0, nowMs - ShellStore.streamStartedAtMs)
        const total = Math.floor(elapsed / 1000)
        const hours = Math.floor(total / 3600)
        const minutes = Math.floor((total % 3600) / 60)
        const seconds = total % 60
        return (hours > 0 ? hours + ":" : "") + String(minutes).padStart(2, "0") + ":" + String(seconds).padStart(2, "0")
    }

    readonly property var mainItems: [backButton, microphoneTile, antiAfkTile, statsTile, fullscreenTile,
        controllersRow, inviteRow, consoleRow, pointerLockRow, endRow]
    // Index tables for the 2×2 quick-toggle grid: [up, down, left, right]; -1 keeps focus.
    readonly property var gridMoves: [
        [-1, 1, -1, -1], [0, 3, -1, 2], [0, 4, 1, -1], [1, 5, -1, 4], [2, 5, 3, -1],
        [3, 6, -1, -1], [5, 7, -1, -1], [6, 8, -1, -1], [7, 9, -1, -1], [8, -1, -1, -1]
    ]
    function usable(item) { return item && item.visible && item.enabled }
    function focusMain(index) {
        const item = mainItems[index]
        item.forceActiveFocus()
        mainScroll.reveal(item)
    }
    function followMainFocus(item) {
        if (!item) return
        item.activeFocusChanged.connect(() => {
            if (item.activeFocus) mainScroll.reveal(item)
        })
    }
    function stepMain(from, step) {
        for (let offset = 1; offset <= mainItems.length; ++offset) {
            const index = (from + step * offset + mainItems.length * offset) % mainItems.length
            if (usable(mainItems[index])) return index
        }
        return from
    }
    function moveMain(direction) {
        const from = Math.max(0, mainItems.findIndex(item => item.activeFocus))
        let index = gridMoves[from][direction]
        while (index >= 0 && !usable(mainItems[index])) {
            const sibling = gridMoves[index][3] >= 0 ? gridMoves[index][3] : gridMoves[index][2]
            index = direction < 2 && sibling >= 0 && usable(mainItems[sibling]) ? sibling : gridMoves[index][direction]
        }
        if (index >= 0) focusMain(index)
    }

    component QuickTile: DesktopStreamDrawerButton {
        id: tile
        property bool switchVisible: true
        property string title: ""
        property string stateText: ""
        property string glyph: ""
        property string shortcutText: ""
        signal activated()
        width: parent ? (parent.width - 12) / 2 : 0
        height: 116
        radius: 14
        text: title
        fill: checked ? "#147FD4FF" : "#0FFFFFFF"
        hoverFill: checked ? "#1F7FD4FF" : "#1FFFFFFF"
        edge: checked ? "#4D7FD4FF" : "#17FFFFFF"
        Accessible.checkable: switchVisible
        Accessible.checked: checked
        onClicked: activated()
        DesktopGlyph { x: 16; y: 16; width: 18; height: 18; icon: tile.glyph }
        DesktopSettingsToggle {
            visible: tile.switchVisible
            anchors.right: parent.right; anchors.rightMargin: 14; y: 12
            scale: 0.85
            focusPolicy: Qt.NoFocus
            checked: tile.checked
            onValueChangedByUser: tile.activated()
        }
        Text {
            x: 16; y: 46; width: parent.width - 32
            text: tile.title
            color: DesktopTokens.textHigh; font.family: DesktopTokens.bodyFont
            font.pixelSize: 14; font.weight: Font.Bold; elide: Text.ElideRight
        }
        Text {
            x: 16; y: 66; width: parent.width - 32
            text: tile.stateText
            color: tile.checked ? DesktopTokens.focus : DesktopTokens.textMuted
            font.family: DesktopTokens.bodyFont; font.pixelSize: 11; font.weight: Font.DemiBold
            elide: Text.ElideRight
        }
        KeyboardGlyph {
            x: 16; y: 88
            visible: tile.shortcutText !== ""
            shortcut: tile.shortcutText
            keySize: 16
            ink: DesktopTokens.textMuted
        }
    }

    component MenuRow: DesktopStreamDrawerButton {
        id: row
        property string title: ""
        property string subtitle: ""
        property string detail: ""
        property color detailColor: DesktopTokens.textFaint
        property string glyph: ""
        property bool danger: false
        property bool drillIn: false
        property string shortcutText: ""
        width: parent ? parent.width : 0
        height: subtitle !== "" ? 58 : 50
        text: title
        fill: danger ? "#14FF8A80" : "#0FFFFFFF"
        hoverFill: danger ? "#29FF8A80" : "#1FFFFFFF"
        edge: danger ? "#40FF8A80" : "#17FFFFFF"
        Rectangle {
            x: 12; anchors.verticalCenter: parent.verticalCenter
            width: 30; height: 30; radius: 8
            color: row.danger ? "#1FFF8A80" : "#12FFFFFF"
            DesktopGlyph { anchors.centerIn: parent; width: 16; height: 16; icon: row.glyph }
        }
        Column {
            x: 54; anchors.verticalCenter: parent.verticalCenter
            width: parent.width - x - trailing.width - 26
            spacing: 2
            Text {
                width: parent.width
                text: row.title
                color: row.danger ? (Theme.lightMode ? "#9F1239" : "#FFB4AE") : DesktopTokens.textHigh
                font.family: DesktopTokens.bodyFont; font.pixelSize: 13; font.weight: Font.Bold
                elide: Text.ElideRight
            }
            Text {
                width: parent.width
                visible: row.subtitle !== ""
                text: row.subtitle
                color: DesktopTokens.textMuted
                font.family: DesktopTokens.bodyFont; font.pixelSize: 11; font.weight: Font.DemiBold
                elide: Text.ElideRight
            }
        }
        Row {
            id: trailing
            anchors.right: parent.right; anchors.rightMargin: 14
            anchors.verticalCenter: parent.verticalCenter
            spacing: 8
            Text {
                anchors.verticalCenter: parent.verticalCenter
                visible: row.detail !== ""
                text: row.detail
                color: row.detailColor
                font.family: DesktopTokens.monoFont; font.pixelSize: 9
                font.weight: Font.Bold; font.letterSpacing: 0.8
            }
            KeyboardGlyph {
                anchors.verticalCenter: parent.verticalCenter
                visible: row.shortcutText !== ""
                shortcut: row.shortcutText
                keySize: 18
                ink: DesktopTokens.textMuted
            }
            Text {
                anchors.verticalCenter: parent.verticalCenter
                visible: row.drillIn
                text: "›"
                color: DesktopTokens.textMuted
                font.family: DesktopTokens.bodyFont; font.pixelSize: 20; font.weight: Font.Bold
            }
        }
    }

    MotionProgress {
        id: reveal
        objectName: "streamMenuMotion"
        shown: root.opened && !root.closing
        exitDuration: 140
        onHidden: {
            root.page = "main"
            if (root.closing && root.pendingAction !== "") root.finishAction()
        }
    }
    MotionProgress {
        id: pageMotion
        shown: root.page === "controllers"
        enterDuration: 220
        exitDuration: 180
    }

    Item {
        objectName: "streamMenuGameArea"
        anchors.fill: parent
        TapHandler { onTapped: root.runAction("resume") }
    }
    Rectangle {
        objectName: "streamMenuScrim"
        x: 0
        width: Math.min(root.width, drawer.x + drawer.width + 280)
        height: root.height
        opacity: reveal.progress
        gradient: Gradient {
            orientation: Gradient.Horizontal
            GradientStop { position: 0; color: "#B304060A" }
            GradientStop { position: 0.62; color: "#5904060A" }
            GradientStop { position: 1; color: "#0004060A" }
        }
    }
    Rectangle {
        objectName: "streamMenuEdgeShadow"
        x: drawer.x + drawer.width - 6
        y: drawer.y + 18
        width: 34
        height: drawer.height - 36
        opacity: reveal.progress
        gradient: Gradient {
            orientation: Gradient.Horizontal
            GradientStop { position: 0; color: "#80000000" }
            GradientStop { position: 1; color: "#00000000" }
        }
    }

    Rectangle {
        id: drawer
        objectName: "streamMenuDrawer"
        x: root.drawerMargin - (1 - reveal.progress) * (root.drawerWidth + 2 * root.drawerMargin)
        y: root.drawerMargin
        width: root.drawerWidth
        height: root.height - 2 * root.drawerMargin
        radius: 20
        color: Theme.lightMode ? Theme.shell : "#FC0A0E15"
        border.width: 1
        border.color: "#24FFFFFF"
        opacity: Math.min(1, reveal.progress * 1.6)
        clip: true
        MouseArea { anchors.fill: parent; acceptedButtons: Qt.AllButtons }

        Rectangle {
            objectName: "streamMenuEdgeSeam"
            anchors.right: parent.right
            anchors.rightMargin: 0
            y: 20
            width: 1
            height: parent.height - 40
            color: "#47FFFFFF"
        }

        FocusScope {
            id: mainPage
            objectName: "streamMenuMainPage"
            x: -pageMotion.progress * drawer.width
            width: drawer.width
            height: drawer.height
            visible: pageMotion.progress < 1
            enabled: root.page === "main"
            focus: root.page === "main"

            Flickable {
                id: mainScroll
                objectName: "streamMenuMainScroll"
                x: 22
                y: 20
                width: parent.width - 44
                height: mainFooter.y - y - 10
                contentWidth: width
                contentHeight: mainColumn.height
                interactive: contentHeight > height
                boundsBehavior: Flickable.StopAtBounds
                clip: true
                ScrollIndicator.vertical: ScrollIndicator {}
                function reveal(item) {
                    const top = item.mapToItem(mainColumn, 0, 0).y - 8
                    const bottom = top + item.height + 16
                    if (top < contentY) contentY = Math.max(0, top)
                    else if (bottom > contentY + height) contentY = Math.min(contentHeight - height, bottom - height)
                }

                Column {
                    id: mainColumn
                    width: mainScroll.width
                    spacing: 14

                    Item {
                        width: parent.width
                        height: 52
                        RoundedArtwork {
                            width: 48; height: 48
                            artwork: String(root.game.imageUrl || root.game.heroImageUrl || "")
                            cornerRadius: 10
                            scrimStart: 1
                            fallbackColor: "#1A2030"
                        }
                        Text {
                            x: 62; y: 2
                            width: parent.width - x - closeChip.width - 12
                            text: String(root.game.title || qsTr("GeForce NOW"))
                            color: DesktopTokens.text
                            font.family: DesktopTokens.displayFont
                            font.pixelSize: 20
                            font.weight: Font.Black
                            elide: Text.ElideRight
                        }
                        Row {
                            x: 62; y: 32
                            spacing: 7
                            Rectangle { width: 6; height: 6; radius: 3; anchors.verticalCenter: parent.verticalCenter; color: DesktopTokens.green }
                            Text {
                                text: qsTr("LIVE · %1").arg(root.clockText())
                                color: DesktopTokens.textMuted
                                font.family: DesktopTokens.monoFont
                                font.pixelSize: 10
                                font.weight: Font.Bold
                                font.letterSpacing: 0.7
                            }
                        }
                        Item {
                            id: closeChip
                            anchors.right: parent.right
                            y: 4
                            width: closeGlyph.visible ? closeGlyph.implicitWidth : closePad.width
                            height: 22
                            KeyboardGlyph { id: closeGlyph; visible: !root.controllerHints; shortcut: "Esc"; keySize: 20; ink: DesktopTokens.textMuted }
                            ControllerGlyph { id: closePad; visible: root.controllerHints; glyph: "B"; label: ""; glyphSize: 20; spacing: 0 }
                            TapHandler { onTapped: root.runAction("resume") }
                        }
                    }

                    Rectangle {
                        objectName: "streamMenuConnection"
                        width: parent.width
                        height: 86
                        radius: 14
                        color: "#0AFFFFFF"
                        border.width: 1
                        border.color: "#17FFFFFF"
                        Rectangle {
                            id: statusBadge
                            anchors.right: parent.right
                            anchors.rightMargin: 14
                            y: 12
                            width: statusText.implicitWidth + 20
                            height: 20
                            radius: 10
                            color: root.streamerStatus === "STREAMING" ? "#1756E6A5" : "#17FFFFFF"
                            border.width: 1
                            border.color: root.streamerStatus === "STREAMING" ? "#3856E6A5" : "#29FFFFFF"
                            Text {
                                id: statusText
                                anchors.centerIn: parent
                                text: root.streamerStatus
                                color: root.streamerStatus === "STREAMING" ? DesktopTokens.green : DesktopTokens.textMuted
                                font.family: DesktopTokens.monoFont
                                font.pixelSize: 8
                                font.weight: Font.Bold
                                font.letterSpacing: 0.8
                            }
                        }
                        Row {
                            id: metricRow
                            x: 16; y: 12
                            width: Math.max(0, statusBadge.x - x - 12)
                            spacing: 12
                            Repeater {
                                id: headerMetrics
                                model: {
                                    const metrics = [
                                        { label: "FPS", value: root.fpsText },
                                        { label: qsTr("OUTPUT"), value: root.resolution },
                                        { label: qsTr("BITRATE"), value: root.bitrateText }
                                    ]
                                    if (root.flightVisible)
                                        metrics.push({ label: qsTr("FLIGHT"), value: root.activeFlightCount === root.flightDevices.length
                                            ? String(root.activeFlightCount) : root.activeFlightCount + "/" + root.flightDevices.length, flight: true })
                                    return metrics
                                }
                                delegate: Column {
                                    id: headerMetric
                                    required property var modelData
                                    width: Math.max(0, (metricRow.width - metricRow.spacing * (headerMetrics.count - 1)) / headerMetrics.count)
                                    spacing: 4
                                    Text {
                                        width: headerMetric.width
                                        text: headerMetric.modelData.label
                                        color: DesktopTokens.textFaint
                                        font.family: DesktopTokens.monoFont
                                        font.pixelSize: 8
                                        font.weight: Font.Bold
                                        font.letterSpacing: 1
                                        elide: Text.ElideRight
                                    }
                                    Row {
                                        width: headerMetric.width
                                        spacing: 4
                                        Rectangle {
                                            visible: headerMetric.modelData.flight === true
                                            anchors.verticalCenter: parent.verticalCenter
                                            width: 6; height: 6; radius: 3; color: root.flightColor
                                        }
                                        Text {
                                            width: Math.max(0, headerMetric.width - (headerMetric.modelData.flight ? 10 : 0))
                                            text: headerMetric.modelData.value
                                            color: headerMetric.modelData.flight ? root.flightColor : DesktopTokens.textHigh
                                            font.family: DesktopTokens.monoFont
                                            font.pixelSize: 14
                                            font.weight: Font.Bold
                                            elide: Text.ElideRight
                                        }
                                    }
                                }
                            }
                        }
                        Text {
                            id: transportLine
                            x: 16
                            y: 46
                            width: parent.width - 32
                            text: [root.transportText, root.codecText, root.backendText].join(" · ")
                            color: DesktopTokens.textMuted
                            font.family: DesktopTokens.monoFont
                            font.pixelSize: 9
                            font.weight: Font.Bold
                            font.letterSpacing: 0.6
                            elide: Text.ElideRight
                        }
                        Text {
                            x: 16; y: 62
                            width: parent.width - 32
                            text: qsTr("Quality, bitrate and codec change from the next session.")
                            color: DesktopTokens.textFaint
                            font.family: DesktopTokens.bodyFont
                            font.pixelSize: 10
                            font.weight: Font.DemiBold
                            elide: Text.ElideRight
                        }
                    }

                    DesktopStreamDrawerButton {
                        id: backButton
                        objectName: "streamMenuBackToGame"
                        width: parent.width
                        height: 50
                        text: qsTr("Back to game")
                        fill: "#F2FFFFFF"
                        hoverFill: "#FFFFFFFF"
                        edge: "transparent"
                        onClicked: root.runAction("resume")
                        DesktopGlyph { x: 16; anchors.verticalCenter: parent.verticalCenter; width: 16; height: 16; icon: "desktop-play.svg" }
                        Text {
                            x: 42; anchors.verticalCenter: parent.verticalCenter
                            text: backButton.text
                            color: "#0B0F1A"
                            font.family: DesktopTokens.bodyFont; font.pixelSize: 14; font.weight: Font.Bold
                        }
                        KeyboardGlyph {
                            visible: !root.controllerHints
                            anchors.right: parent.right; anchors.rightMargin: 14; anchors.verticalCenter: parent.verticalCenter
                            shortcut: "Esc"; keySize: 18; ink: "#0B0F1A"
                        }
                        ControllerGlyph {
                            visible: root.controllerHints
                            anchors.right: parent.right; anchors.rightMargin: 14; anchors.verticalCenter: parent.verticalCenter
                            glyph: "B"; label: ""; glyphSize: 18; spacing: 0; glyphColor: "#0B0F1A"
                        }
                    }

                    Text {
                        text: qsTr("QUICK TOGGLES · MENU STAYS OPEN")
                        color: DesktopTokens.textFaint
                        font.family: DesktopTokens.monoFont
                        font.pixelSize: 9
                        font.weight: Font.Bold
                        font.letterSpacing: 1.2
                    }

                    Grid {
                        id: quickGrid
                        width: parent.width
                        columns: 2
                        columnSpacing: 12
                        rowSpacing: 12
                        QuickTile {
                            id: microphoneTile
                            objectName: "streamMicrophoneControl"
                            enabled: ShellStore.microphoneCanToggle
                            checked: ShellStore.microphoneEnabled
                            glyph: ShellStore.microphoneEnabled ? "desktop-mic.svg" : "desktop-mic-off.svg"
                            title: qsTr("Microphone")
                            text: ShellStore.microphoneActionLabel
                            stateText: ShellStore.microphoneLabel
                            description: ShellStore.microphoneDescription
                            shortcutText: root.shortcut("shortcutToggleMicrophone", "Ctrl+Shift+M")
                            onActivated: root.runAction("microphone")
                        }
                        QuickTile {
                            id: antiAfkTile
                            objectName: "streamAntiAfkControl"
                            checked: ShellStore.antiAfkEnabled
                            glyph: "desktop-clock.svg"
                            title: qsTr("Anti-AFK")
                            stateText: ShellStore.antiAfkEnabled ? qsTr("On") : qsTr("Off")
                            description: qsTr("Sends a short keypress every 4 minutes so GeForce NOW doesn't end the session while you're idle.")
                            shortcutText: root.shortcut("shortcutToggleAntiAfk", "Ctrl+Shift+K")
                            onActivated: root.runAction("antiAfk")
                        }
                        QuickTile {
                            id: statsTile
                            objectName: "streamStatsControl"
                            switchVisible: false
                            glyph: "desktop-sliders.svg"
                            title: qsTr("Stats overlay")
                            stateText: qsTr("Opens over the game")
                            description: qsTr("See frame rate, latency and bitrate without leaving the game.")
                            shortcutText: root.shortcut("shortcutToggleStats", "Ctrl+N")
                            onActivated: root.runAction("stats")
                        }
                        QuickTile {
                            id: fullscreenTile
                            objectName: "streamFullscreenControl"
                            checked: root.fullscreen
                            glyph: root.fullscreen ? "desktop-collapse.svg" : "desktop-expand.svg"
                            title: qsTr("Fullscreen")
                            text: root.fullscreen ? qsTr("Exit fullscreen") : qsTr("Go fullscreen")
                            stateText: root.fullscreen ? qsTr("On") : qsTr("Off")
                            shortcutText: root.shortcut("shortcutToggleFullscreen", "F11")
                            onActivated: root.runAction("fullscreen")
                        }
                    }

                    MenuRow {
                        id: controllersRow
                        objectName: "streamControllersRow"
                        glyph: "desktop-gamepad.svg"
                        title: qsTr("Controllers")
                        subtitle: root.flightVisible
                            ? (root.pads.length === 0 ? qsTr("No pad") : qsTr("%1 connected").arg(root.pads.length))
                                + " · " + qsTr("%1 of %2 flight devices forwarded").arg(root.activeFlightCount).arg(root.flightDevices.length)
                            : root.pads.length === 0 ? qsTr("No controller connected")
                            : root.pads.length === 1 ? qsTr("1 connected · dead zones and vibration")
                            : qsTr("%1 connected · dead zones and vibration").arg(root.pads.length)
                        detail: !root.flightVisible || root.activeFlightCount === root.flightDevices.length ? ""
                            : "● " + (root.declinedFlightCount > 0 ? qsTr("%1 DECLINED").arg(root.declinedFlightCount)
                                : qsTr("%1 NOT SENT").arg(root.flightDevices.length - root.activeFlightCount))
                        detailColor: root.flightColor
                        drillIn: true
                        onClicked: root.runAction("controllers")
                    }
                    MenuRow {
                        id: inviteRow
                        objectName: "streamInviteRow"
                        enabled: root.invitesAvailable
                        glyph: "desktop-user-plus.svg"
                        title: qsTr("Invite a friend")
                        detail: root.invitesAvailable ? "" : qsTr("UNAVAILABLE")
                        onClicked: root.runAction("invite")
                    }
                    MenuRow {
                        id: consoleRow
                        objectName: "streamConsoleModeRow"
                        glyph: "desktop-gamepad.svg"
                        title: root.modeOn ? qsTr("Switch to desktop mode") : qsTr("Switch to console mode")
                        detail: root.modePending ? qsTr("SWITCHING…") : root.modeOn ? qsTr("DESKTOP READY") : qsTr("GAMEPAD READY")
                        onClicked: root.runAction("console")
                    }
                    MenuRow {
                        id: pointerLockRow
                        objectName: "streamPointerLockRow"
                        visible: Qt.platform.os === "osx"
                        glyph: "desktop-lock.svg"
                        title: qsTr("Pointer lock")
                        detail: ShellStore.settings.macPointerLockEnabled !== false ? qsTr("ON") : qsTr("OFF")
                        shortcutText: root.shortcut("shortcutTogglePointerLock", "F8")
                        Accessible.checkable: true
                        Accessible.checked: ShellStore.settings.macPointerLockEnabled !== false
                        onClicked: ShellStore.pointerLockToggleRequested()
                    }
                    Item { width: 1; height: 4 }
                    MenuRow {
                        id: endRow
                        objectName: "streamEndSessionRow"
                        danger: true
                        glyph: "desktop-logout.svg"
                        title: qsTr("End session")
                        subtitle: qsTr("Asks first, the stream keeps running")
                        shortcutText: root.shortcut("shortcutStopStream", "Ctrl+Shift+Q")
                        onClicked: root.runAction("end")
                    }
                }
            }

            Row {
                id: mainFooter
                x: 22
                y: parent.height - height - 18
                spacing: 16
                DesktopKeyHint { visible: !root.controllerHints; keyText: "↑ ↓"; shortcut: "Up+Down"; label: qsTr("Move"); compact: true }
                DesktopKeyHint { visible: !root.controllerHints; keyText: "Enter"; label: qsTr("Select"); compact: true }
                DesktopKeyHint { visible: !root.controllerHints; keyText: "Esc"; label: qsTr("Back"); compact: true }
                ControllerGlyph { visible: root.controllerHints; glyph: "A"; label: qsTr("Select"); glyphSize: 18; labelPixelSize: 11; labelWeight: Font.DemiBold; glyphColor: DesktopTokens.mint }
                ControllerGlyph { visible: root.controllerHints; glyph: "B"; label: qsTr("Back"); glyphSize: 18; labelPixelSize: 11; labelWeight: Font.DemiBold; glyphColor: DesktopTokens.danger }
                DesktopKeyHint { keyText: "Ctrl K"; shortcut: "Ctrl+K"; label: qsTr("Commands"); compact: true }
            }

            Keys.onPressed: event => {
                const modifiers = event.modifiers & (Qt.ControlModifier | Qt.ShiftModifier
                    | Qt.AltModifier | Qt.MetaModifier)
                if (event.key === Qt.Key_Escape || event.key === Qt.Key_Back) {
                    root.runAction("resume")
                } else if (event.key === Qt.Key_Q && modifiers === (Qt.ControlModifier | Qt.ShiftModifier)) {
                    root.runAction("end")
                } else if (event.key === Qt.Key_Up) {
                    root.moveMain(0)
                } else if (event.key === Qt.Key_Down) {
                    root.moveMain(1)
                } else if (event.key === Qt.Key_Left) {
                    root.moveMain(2)
                } else if (event.key === Qt.Key_Right) {
                    root.moveMain(3)
                } else if (event.key === Qt.Key_Tab || event.key === Qt.Key_Backtab) {
                    const from = Math.max(0, root.mainItems.findIndex(item => item.activeFocus))
                    root.focusMain(root.stepMain(from, event.key === Qt.Key_Tab ? 1 : -1))
                } else {
                    return
                }
                event.accepted = true
            }
        }

        DesktopStreamControllersPage {
            id: controllersPage
            x: (1 - pageMotion.progress) * drawer.width
            width: drawer.width
            height: drawer.height
            visible: pageMotion.progress > 0
            active: root.opened && root.page === "controllers"
            enabled: active
            focus: active
            controllerHints: root.controllerHints
            onBackRequested: root.openPage("main")
            onResumeRequested: root.runAction("resume")
        }
    }

    Timer { interval: 1000; repeat: true; running: root.visible; onTriggered: root.nowMs = Date.now() }
    onOpenedChanged: if (opened) {
        closing = false
        pendingAction = ""
        page = "main"
        mainScroll.contentY = 0
        forceActiveFocus()
        backButton.forceActiveFocus()
    }
    Component.onCompleted: mainItems.forEach(root.followMainFocus)
}

import QtQuick
import QtQuick.Controls
import QtQuick.Effects
import QtQuick.Window
import OpenNOW

FocusScope {
    id: root
    objectName: "consoleSessionGuide"
    property string page: "guide-session"
    property real revealProgress: 1
    property double nowMs: Date.now()
    property string toastTitle: ""
    property string toastDetail: ""
    readonly property var game: ShellStore.selectedGame || ({title: qsTr("GeForce NOW")})
    readonly property var session: ShellStore.activeSession || ({})
    readonly property var profile: session.negotiatedStreamProfile || ({})
    readonly property var streamer: ShellStore.streamer || ({})
    readonly property bool mediaLive: streamer.status === "streaming"
        && streamer.firstFrameLatencyMs !== undefined && streamer.firstFrameLatencyMs !== null
    readonly property string panelPage: page === "guide-controls" || page === "guide-shortcuts" ? page : "guide-session"
    readonly property bool fullscreen: Window.window !== null && Window.window.visibility === Window.FullScreen
    readonly property var variant: game.variants && Number(game.selectedVariantIndex) >= 0
        ? (game.variants[Number(game.selectedVariantIndex)] || null) : null
    readonly property string subtitle: [variant && variant.store ? DesktopTokens.storeLabel(variant.store) : "",
        String(session.serverLocation || ShellStore.selectedRegion || "")].filter(part => part !== "").join(" · ")
    readonly property string profileLine: [
        profile.width && profile.height ? profile.width + " × " + profile.height : String(profile.resolution || ""),
        profile.fps ? qsTr("%1 FPS").arg(profile.fps) : "",
        profile.codec ? String(profile.codec).toUpperCase() : "",
        session.gpuType ? String(session.gpuType) : ""
    ].filter(part => part !== "").join(" · ")
    readonly property var helpRows: [
        {label: qsTr("Session menu"), value: "Ctrl+G"},
        {label: qsTr("Stats overlay"), value: shortcutHint("shortcutToggleStats", "Ctrl+N")},
        {label: qsTr("Pointer lock"), value: shortcutHint("shortcutTogglePointerLock", "F8")},
        {label: qsTr("Fullscreen"), value: shortcutHint("shortcutToggleFullscreen", "F11")},
        {label: qsTr("Screenshot"), value: shortcutHint("shortcutScreenshot", "Ctrl+F11")},
        {label: qsTr("Toggle recording"), value: shortcutHint("shortcutToggleRecording", "F12")},
        {label: qsTr("Save clip"), value: shortcutHint("shortcutSaveClip", "Ctrl+F12")},
        {label: qsTr("Toggle Anti-AFK"), value: shortcutHint("shortcutToggleAntiAfk", "Ctrl+Shift+K")},
        {label: qsTr("Microphone"), value: ShellStore.microphoneToggleAvailable
            ? shortcutHint("shortcutToggleMicrophone", "Ctrl+Shift+M") : ShellStore.microphoneLabel},
        {label: qsTr("End session"), value: shortcutHint("shortcutStopStream", "Ctrl+Shift+Q")}
    ]

    anchors.fill: parent
    focus: visible
    Accessible.role: Accessible.Dialog
    Accessible.name: qsTr("OpenNOW session guide")

    function shortcutHint(key, fallback) {
        const value = ShellStore.settings[key] ?? fallback
        return value === "" ? qsTr("Not set") : String(value)
    }

    function elapsedLabel() {
        if (!ShellStore.streamStartedAtMs)
            return ""
        const total = Math.max(0, Math.floor((root.nowMs - ShellStore.streamStartedAtMs) / 1000))
        const hours = Math.floor(total / 3600)
        return (hours > 0 ? hours + ":" : "") + String(Math.floor((total % 3600) / 60)).padStart(hours > 0 ? 2 : 1, "0")
            + ":" + String(total % 60).padStart(2, "0")
    }

    function focusDefault() {
        if (!root.visible)
            return
        if (root.panelPage !== "guide-session")
            pagePanel.forceActiveFocus()
        else if (root.page === "guide-media" && screenshotTile.enabled)
            screenshotTile.forceActiveFocus()
        else
            resumeButton.forceActiveFocus()
    }

    function showToast(title, detail) {
        root.toastTitle = title
        root.toastDetail = detail
        toastTimer.restart()
    }

    function activate(action) {
        if (action === "resume") {
            AppController.showOverlay("")
        } else if (action === "end") {
            ShellStore.requestStreamExitConfirmation()
        } else if (action === "stats") {
            ShellStore.applyStreamShortcutAction("toggle-stats")
        } else if (action === "fullscreen") {
            ShellStore.applyStreamShortcutAction("toggle-fullscreen")
        } else if (action === "anti-afk") {
            ShellStore.applyStreamShortcutAction("toggle-anti-afk")
        } else if (action === "screenshot") {
            ShellStore.captureStreamScreenshot()
        } else if (action === "recording") {
            ShellStore.toggleStreamRecording()
        } else if (action === "microphone") {
            ShellStore.toggleMicrophone()
        } else if (action === "controllers") {
            AppController.showOverlay("guide-controls")
        } else if (action === "help") {
            AppController.showOverlay("guide-shortcuts")
        } else if (action === "root") {
            AppController.showOverlay("guide-session")
        }
    }

    onPageChanged: {
        if (!visible)
            return
        ShellStore.recordGuidePage(page)
        pushMotion.restart()
        Qt.callLater(focusDefault)
    }
    onVisibleChanged: if (visible) {
        ShellStore.recordGuidePage(page)
        Qt.callLater(focusDefault)
    }
    Component.onCompleted: if (visible) {
        ShellStore.recordGuidePage(page)
        Qt.callLater(focusDefault)
    }

    Keys.onPressed: event => {
        const modifiers = event.modifiers & (Qt.ControlModifier | Qt.ShiftModifier | Qt.AltModifier | Qt.MetaModifier)
        if ((event.key === Qt.Key_G && modifiers === Qt.ControlModifier)
                || (event.key === Qt.Key_F1 && AppController.inputMode === "controller")) {
            event.accepted = true
            if (!event.isAutoRepeat)
                root.activate("resume")
        } else if (event.key === Qt.Key_Escape || event.key === Qt.Key_Back) {
            event.accepted = true
            if (!event.isAutoRepeat)
                root.activate(root.panelPage === "guide-session" ? "resume" : "root")
        }
    }

    Connections {
        target: ShellStore
        function onStreamControlMessageChanged() {
            if (ShellStore.streamControlMessage !== "")
                root.showToast(I18n.source(ShellStore.streamControlMessage, I18n.revision), "")
        }
        function onMediaMessageChanged() {
            if (ShellStore.mediaMessage === "")
                return
            const saved = ShellStore.mediaMessage === qsTr("Screenshot saved")
            root.showToast(I18n.source(ShellStore.mediaMessage, I18n.revision),
                saved ? qsTr("Find it in Media after your session") : "")
        }
    }

    Timer { id: toastTimer; interval: 4000; onTriggered: root.toastTitle = "" }
    Timer { interval: 1000; repeat: true; running: root.visible; onTriggered: root.nowMs = Date.now() }
    SessionGlyphs { id: glyphs }

    component GuideTile: Button {
        id: tile
        property string caption: ""
        property color captionColor: Theme.textMuted
        property Component glyphIcon: null
        width: 176
        height: 118
        focusPolicy: enabled ? Qt.StrongFocus : Qt.NoFocus
        Accessible.role: Accessible.Button
        Accessible.name: text + (caption !== "" ? ", " + caption : "")
        Keys.onPressed: event => {
            if (event.key !== Qt.Key_Return && event.key !== Qt.Key_Enter && event.key !== Qt.Key_Space)
                return
            event.accepted = true
            if (!event.isAutoRepeat && tile.enabled)
                tile.clicked()
        }
        background: Item {
            Rectangle {
                anchors.fill: parent
                anchors.margins: -8
                radius: 32
                color: "transparent"
                border.width: 4
                border.color: Qt.rgba(Theme.focus.r, Theme.focus.g, Theme.focus.b, 0.5)
                visible: tile.activeFocus
            }
            Rectangle {
                anchors.fill: parent
                radius: 24
                color: Qt.rgba(Theme.face.r, Theme.face.g, Theme.face.b, tile.activeFocus ? 0.12 : 0.05)
                border.width: tile.activeFocus ? 3 : 1
                border.color: tile.activeFocus ? Theme.face : Theme.seam
            }
        }
        contentItem: Item {
            opacity: tile.enabled ? 1 : 0.46
            Loader {
                x: 6; y: 6
                width: 28; height: 28
                sourceComponent: tile.glyphIcon
            }
            Column {
                x: 6
                anchors.bottom: parent.bottom
                anchors.bottomMargin: 4
                width: parent.width - 12
                Text {
                    width: parent.width
                    text: tile.text
                    elide: Text.ElideRight
                    color: Theme.label
                    font.family: Theme.displayFont; font.pixelSize: 19; font.weight: Font.Black
                }
                Text {
                    width: parent.width
                    text: tile.caption
                    elide: Text.ElideRight
                    color: tile.captionColor
                    font.family: Theme.bodyFont; font.pixelSize: 15; font.weight: Font.DemiBold
                }
            }
        }
        padding: 14
    }

    component GuideRow: Button {
        id: row
        property string value: ""
        property bool danger: false
        property bool chevron: true
        width: 554
        height: 64
        focusPolicy: Qt.StrongFocus
        Accessible.role: Accessible.Button
        Accessible.name: text + (value !== "" ? ", " + value : "")
        Keys.onPressed: event => {
            if (event.key !== Qt.Key_Return && event.key !== Qt.Key_Enter && event.key !== Qt.Key_Space)
                return
            event.accepted = true
            if (!event.isAutoRepeat && row.enabled)
                row.clicked()
        }
        background: Item {
            Rectangle {
                anchors.fill: parent
                anchors.margins: -8
                radius: 28
                color: "transparent"
                border.width: 4
                border.color: Qt.rgba(Theme.focus.r, Theme.focus.g, Theme.focus.b, 0.5)
                visible: row.activeFocus
            }
            Rectangle {
                anchors.fill: parent
                radius: 20
                color: row.danger ? Qt.rgba(Theme.coral.r, Theme.coral.g, Theme.coral.b, row.activeFocus ? 0.18 : 0.08)
                    : Qt.rgba(Theme.face.r, Theme.face.g, Theme.face.b, row.activeFocus ? 0.12 : 0.05)
                border.width: row.activeFocus ? 3 : 1
                border.color: row.activeFocus ? Theme.face
                    : row.danger ? Qt.rgba(Theme.coral.r, Theme.coral.g, Theme.coral.b, 0.5) : "transparent"
            }
        }
        contentItem: Item {
            Text {
                anchors.verticalCenter: parent.verticalCenter
                x: 6
                text: row.text
                color: row.danger ? Theme.coral : Theme.label
                font.family: Theme.displayFont; font.pixelSize: 20; font.weight: Font.Black
            }
            Text {
                anchors.verticalCenter: parent.verticalCenter
                anchors.right: parent.right
                anchors.rightMargin: row.chevron ? 30 : 6
                text: row.value
                color: Theme.textMuted
                font.family: Theme.bodyFont; font.pixelSize: 17; font.weight: Font.DemiBold
            }
            Text {
                visible: row.chevron
                anchors.verticalCenter: parent.verticalCenter
                anchors.right: parent.right
                anchors.rightMargin: 4
                text: "›"
                color: Theme.label
                font.family: Theme.displayFont; font.pixelSize: 28; font.weight: Font.Black
            }
        }
        leftPadding: 14; rightPadding: 14
    }

    component SvgIcon: Image {
        property url icon
        anchors.fill: parent
        source: icon
        sourceSize: Qt.size(56, 56)
        fillMode: Image.PreserveAspectFit
        layer.enabled: true
        layer.effect: MultiEffect { colorization: 1; colorizationColor: Theme.label }
    }

    Rectangle {
        anchors.fill: parent
        gradient: Gradient {
            orientation: Gradient.Horizontal
            GradientStop { position: 0; color: Qt.rgba(0.01, 0.02, 0.05, 0.55) }
            GradientStop { position: 0.5; color: Qt.rgba(0.01, 0.02, 0.05, 0.32) }
            GradientStop { position: 1; color: Qt.rgba(0.01, 0.02, 0.05, 0.22) }
        }
    }

    Rectangle {
        id: panel
        x: 50 - 32 * (1 - root.revealProgress)
        y: 48
        width: 618
        height: parent.height - 96
        radius: 40
        color: Qt.rgba(Theme.shell.r, Theme.shell.g, Theme.shell.b, 0.96)
        border.width: 1
        border.color: Theme.seam
        clip: true

        Item {
            id: pageHost
            anchors.fill: parent
            anchors.margins: 32
            anchors.bottomMargin: 96

            SequentialAnimation {
                id: pushMotion
                PropertyAction { target: pageHost; property: "anchors.leftMargin"; value: AppController.reducedMotion ? 32 : 56 }
                PropertyAction { target: pageHost; property: "opacity"; value: AppController.reducedMotion ? 1 : 0.35 }
                ParallelAnimation {
                    NumberAnimation { target: pageHost; property: "anchors.leftMargin"; to: 32; duration: AppController.reducedMotion ? 0 : 200; easing.type: Easing.OutCubic }
                    NumberAnimation { target: pageHost; property: "opacity"; to: 1; duration: AppController.reducedMotion ? 0 : 200; easing.type: Easing.OutCubic }
                }
            }

            Item {
                id: rootPage
                anchors.fill: parent
                visible: root.panelPage === "guide-session"

                Row {
                    id: header
                    width: parent.width
                    spacing: 16
                    Rectangle {
                        width: 64; height: 64; radius: 14
                        color: Theme.glassStrong
                        clip: true
                        ArtworkSource {
                            id: guideArt
                            sourceUrl: DesktopTokens.decodeArtworkUrl(root.game.boxArtUrl || root.game.imageUrl || "")
                            active: rootPage.visible && sourceUrl !== ""
                        }
                        Image {
                            anchors.fill: parent
                            source: guideArt.resolvedUrl
                            fillMode: Image.PreserveAspectCrop
                            sourceSize: Qt.size(128, 128)
                            asynchronous: true
                        }
                    }
                    Column {
                        anchors.verticalCenter: parent.verticalCenter
                        width: parent.width - 80 - livePill.width - 16
                        Text {
                            width: parent.width
                            text: String(root.game.title || qsTr("GeForce NOW"))
                            elide: Text.ElideRight
                            color: Theme.label
                            font.family: Theme.displayFont; font.pixelSize: 30; font.weight: Font.Black
                        }
                        Text {
                            visible: root.subtitle !== ""
                            width: parent.width
                            text: root.subtitle
                            elide: Text.ElideRight
                            color: Theme.textMuted
                            font.family: Theme.bodyFont; font.pixelSize: 17; font.weight: Font.DemiBold
                        }
                    }
                    Rectangle {
                        id: livePill
                        anchors.verticalCenter: parent.verticalCenter
                        visible: root.elapsedLabel() !== ""
                        width: visible ? liveRow.implicitWidth + 28 : 0
                        height: 36; radius: 18
                        color: Qt.rgba(Theme.mint.r, Theme.mint.g, Theme.mint.b, 0.12)
                        Accessible.role: Accessible.StaticText
                        Accessible.name: qsTr("Session time %1").arg(root.elapsedLabel())
                        Row {
                            id: liveRow
                            anchors.centerIn: parent
                            spacing: 8
                            Rectangle { anchors.verticalCenter: parent.verticalCenter; width: 8; height: 8; radius: 4; color: Theme.mint }
                            Text {
                                text: root.elapsedLabel()
                                color: Theme.mint
                                font.family: Theme.monoFont; font.pixelSize: 15; font.weight: Font.Bold
                            }
                        }
                    }
                }

                Text {
                    id: profileText
                    anchors.top: header.bottom
                    anchors.topMargin: 18
                    width: parent.width
                    visible: root.profileLine !== ""
                    text: root.profileLine
                    elide: Text.ElideRight
                    color: Theme.textMuted
                    font.family: Theme.monoFont; font.pixelSize: 15; font.weight: Font.Medium; font.letterSpacing: 1
                }

                Column {
                    anchors.top: profileText.visible ? profileText.bottom : header.bottom
                    anchors.topMargin: 22
                    width: parent.width
                    spacing: 14

                    ConsoleActionButton {
                        id: resumeButton
                        objectName: "guideResumeButton"
                        width: parent.width
                        implicitWidth: 554
                        height: 80
                        primary: true
                        text: qsTr("Resume game")
                        KeyNavigation.down: screenshotTile
                        onClicked: root.activate("resume")
                        contentItem: Item {
                            Text {
                                anchors.verticalCenter: parent.verticalCenter
                                x: 10
                                text: "▶"
                                color: Theme.faceText
                                font.pixelSize: 22
                            }
                            Text {
                                anchors.verticalCenter: parent.verticalCenter
                                x: 46
                                text: resumeButton.text
                                color: Theme.faceText
                                font.family: Theme.displayFont; font.pixelSize: 24; font.weight: Font.Black
                            }
                            ControllerGlyph {
                                anchors.verticalCenter: parent.verticalCenter
                                anchors.right: parent.right
                                glyph: glyphs.button("B")
                                keyboard: glyphs.keyboard
                                label: ""
                                glyphSize: 30
                                glyphColor: Theme.faceText
                            }
                        }
                    }

                    Grid {
                        columns: 3
                        columnSpacing: 13
                        rowSpacing: 13

                        GuideTile {
                            id: screenshotTile
                            objectName: "guideScreenshotTile"
                            text: qsTr("Screenshot")
                            caption: glyphs.keyboard ? root.shortcutHint("shortcutScreenshot", "Ctrl+F11") : qsTr("Saves to Media")
                            enabled: root.mediaLive
                            glyphIcon: Component {
                                Item {
                                    Rectangle { anchors.centerIn: parent; width: 26; height: 20; radius: 5; color: "transparent"; border.width: 2.5; border.color: Theme.label }
                                    Rectangle { anchors.centerIn: parent; width: 10; height: 10; radius: 5; color: "transparent"; border.width: 2.5; border.color: Theme.label }
                                }
                            }
                            KeyNavigation.up: resumeButton
                            KeyNavigation.right: recordTile
                            KeyNavigation.down: statsTile
                            onClicked: root.activate("screenshot")
                        }
                        GuideTile {
                            id: recordTile
                            objectName: "guideRecordTile"
                            text: qsTr("Record")
                            caption: ShellStore.streamRecordingActive ? qsTr("Recording") : qsTr("Off")
                            captionColor: ShellStore.streamRecordingActive ? Theme.coral : Theme.textMuted
                            enabled: root.mediaLive || ShellStore.streamRecordingActive
                            glyphIcon: Component {
                                Item {
                                    Rectangle { anchors.centerIn: parent; width: 26; height: 26; radius: 13; color: "transparent"; border.width: 2.5; border.color: Theme.label }
                                    Rectangle { anchors.centerIn: parent; width: 12; height: 12; radius: 6; color: ShellStore.streamRecordingActive ? Theme.coral : Theme.label }
                                }
                            }
                            KeyNavigation.up: resumeButton
                            KeyNavigation.left: screenshotTile
                            KeyNavigation.right: microphoneTile
                            KeyNavigation.down: fullscreenTile
                            onClicked: root.activate("recording")
                        }
                        GuideTile {
                            id: microphoneTile
                            objectName: "guideMicrophoneTile"
                            text: qsTr("Microphone")
                            caption: I18n.source(ShellStore.microphoneLabel, I18n.revision)
                            captionColor: ShellStore.microphoneEnabled ? Theme.mint
                                : ShellStore.microphoneState === "muted" ? Theme.yellow : Theme.textMuted
                            enabled: ShellStore.microphoneCanToggle
                            glyphIcon: Component { SvgIcon { icon: ShellStore.microphoneEnabled ? "qrc:/qt/qml/OpenNOW/res/icons/desktop-mic.svg" : "qrc:/qt/qml/OpenNOW/res/icons/desktop-mic-off.svg" } }
                            KeyNavigation.up: resumeButton
                            KeyNavigation.left: recordTile
                            KeyNavigation.down: antiAfkTile
                            onClicked: root.activate("microphone")
                        }
                        GuideTile {
                            id: statsTile
                            objectName: "guideStatsTile"
                            text: qsTr("Stats overlay")
                            caption: glyphs.keyboard ? root.shortcutHint("shortcutToggleStats", "Ctrl+N") : qsTr("Show on stream")
                            glyphIcon: Component {
                                Row {
                                    anchors.centerIn: parent
                                    spacing: 4
                                    Repeater {
                                        model: [10, 18, 24, 14]
                                        Rectangle {
                                            required property int modelData
                                            anchors.bottom: parent.bottom
                                            width: 3.5; height: modelData; radius: 2
                                            color: Theme.label
                                        }
                                    }
                                }
                            }
                            KeyNavigation.up: screenshotTile
                            KeyNavigation.right: fullscreenTile
                            KeyNavigation.down: controllersRow
                            onClicked: root.activate("stats")
                        }
                        GuideTile {
                            id: fullscreenTile
                            objectName: "guideFullscreenTile"
                            text: qsTr("Fullscreen")
                            caption: root.fullscreen ? qsTr("On") : qsTr("Off")
                            captionColor: root.fullscreen ? Theme.mint : Theme.textMuted
                            glyphIcon: Component { SvgIcon { icon: root.fullscreen ? "qrc:/qt/qml/OpenNOW/res/icons/desktop-collapse.svg" : "qrc:/qt/qml/OpenNOW/res/icons/desktop-expand.svg" } }
                            KeyNavigation.up: recordTile
                            KeyNavigation.left: statsTile
                            KeyNavigation.right: antiAfkTile
                            KeyNavigation.down: controllersRow
                            onClicked: root.activate("fullscreen")
                        }
                        GuideTile {
                            id: antiAfkTile
                            objectName: "guideAntiAfkTile"
                            text: qsTr("Anti-AFK")
                            caption: ShellStore.antiAfkEnabled ? qsTr("On") : qsTr("Off")
                            captionColor: ShellStore.antiAfkEnabled ? Theme.mint : Theme.textMuted
                            glyphIcon: Component { SvgIcon { icon: "qrc:/qt/qml/OpenNOW/res/icons/desktop-clock.svg" } }
                            KeyNavigation.up: microphoneTile
                            KeyNavigation.left: fullscreenTile
                            KeyNavigation.down: controllersRow
                            onClicked: root.activate("anti-afk")
                        }
                    }

                    Item { width: 1; height: 6 }

                    GuideRow {
                        id: controllersRow
                        objectName: "guideControllersRow"
                        width: parent.width
                        text: qsTr("Controllers & co-op")
                        value: qsTr("%1 connected").arg(ControllerInput.controllers.length)
                        KeyNavigation.up: statsTile
                        KeyNavigation.down: helpRow
                        onClicked: root.activate("controllers")
                    }
                    GuideRow {
                        id: helpRow
                        objectName: "guideHelpRow"
                        width: parent.width
                        text: qsTr("Shortcuts & help")
                        value: qsTr("Read-only")
                        KeyNavigation.up: controllersRow
                        KeyNavigation.down: endRow
                        onClicked: root.activate("help")
                    }
                }

                GuideRow {
                    id: endRow
                    objectName: "guideEndSessionRow"
                    anchors.bottom: parent.bottom
                    width: parent.width
                    danger: true
                    chevron: false
                    text: qsTr("End session…")
                    KeyNavigation.up: helpRow
                    onClicked: root.activate("end")
                }
            }

            FocusScope {
                id: pagePanel
                anchors.fill: parent
                visible: root.panelPage !== "guide-session"
                focus: visible
                activeFocusOnTab: false

                Column {
                    id: pageHeader
                    width: parent.width
                    spacing: 10
                    Text {
                        width: parent.width
                        text: (qsTr("Guide") + "  ›  " + String(root.game.title || qsTr("GeForce NOW"))).toUpperCase()
                        elide: Text.ElideRight
                        color: Theme.textMuted
                        font.family: Theme.monoFont; font.pixelSize: 15; font.weight: Font.Bold; font.letterSpacing: 2.4
                    }
                    Text {
                        text: root.panelPage === "guide-controls" ? qsTr("Controllers & co-op") : qsTr("Shortcuts & help")
                        color: Theme.label
                        font.family: Theme.displayFont; font.pixelSize: 38; font.weight: Font.Black
                        Accessible.role: Accessible.Heading
                        Accessible.name: text
                    }
                    Text {
                        width: parent.width
                        wrapMode: Text.WordWrap
                        text: root.panelPage === "guide-controls"
                            ? qsTr("OpenNOW forwards up to four controllers to the game. Connect or wake one and it joins the next free slot.")
                            : glyphs.keyboard ? qsTr("Your current keyboard bindings for stream controls. Change them in Settings, outside the session.")
                            : qsTr("Keyboard bindings for stream controls, for reference only. On a controller, use the actions in this guide. Change bindings in Settings, outside the session.")
                        color: Theme.textMuted
                        font.family: Theme.bodyFont; font.pixelSize: 18
                    }
                }

                Column {
                    visible: root.panelPage === "guide-controls"
                    anchors.top: pageHeader.bottom
                    anchors.topMargin: 24
                    width: parent.width
                    spacing: 12
                    Accessible.role: Accessible.List
                    Accessible.name: qsTr("Controller slots")
                    Repeater {
                        model: 4
                        Rectangle {
                            id: slotCard
                            required property int index
                            readonly property var controller: ControllerInput.controllers.length > index ? ControllerInput.controllers[index] : null
                            readonly property int battery: controller ? Number(controller.batteryPercent) : -1
                            width: parent.width
                            height: 84
                            radius: 22
                            color: controller ? Qt.rgba(Theme.face.r, Theme.face.g, Theme.face.b, 0.06) : "transparent"
                            border.width: 1
                            border.color: Theme.seam
                            Accessible.role: Accessible.ListItem
                            Accessible.name: controller ? qsTr("Player %1, %2").arg(index + 1).arg(controller.name)
                                : qsTr("Player %1, waiting for controller").arg(index + 1)
                            Rectangle {
                                x: 20
                                anchors.verticalCenter: parent.verticalCenter
                                width: 48; height: 48; radius: 24
                                color: slotCard.controller ? (slotCard.index === 0 ? Theme.mint : Theme.focus) : "transparent"
                                border.width: slotCard.controller ? 0 : 2
                                border.color: Theme.seam
                                Text {
                                    anchors.centerIn: parent
                                    text: "P" + (slotCard.index + 1)
                                    color: slotCard.controller ? Theme.contrastText(parent.color) : Theme.textMuted
                                    font.family: Theme.displayFont; font.pixelSize: 17; font.weight: Font.Black
                                }
                            }
                            Column {
                                x: 84
                                anchors.verticalCenter: parent.verticalCenter
                                width: parent.width - 84 - batteryText.width - 28
                                Text {
                                    width: parent.width
                                    text: slotCard.controller ? String(slotCard.controller.name) : qsTr("Waiting for controller")
                                    elide: Text.ElideRight
                                    color: slotCard.controller ? Theme.label : Theme.textMuted
                                    font.family: Theme.displayFont; font.pixelSize: 20; font.weight: Font.Black
                                }
                                Text {
                                    width: parent.width
                                    elide: Text.ElideRight
                                    text: !slotCard.controller ? qsTr("Connect or press a button to join")
                                        : slotCard.index === 0 ? qsTr("Player 1 · Guide button opens OpenNOW")
                                        : qsTr("Player %1").arg(slotCard.index + 1)
                                    color: Theme.textMuted
                                    font.family: Theme.bodyFont; font.pixelSize: 15; font.weight: Font.DemiBold
                                }
                            }
                            Text {
                                id: batteryText
                                anchors.right: parent.right
                                anchors.rightMargin: 20
                                anchors.verticalCenter: parent.verticalCenter
                                visible: slotCard.battery >= 0
                                text: slotCard.controller && slotCard.controller.charging
                                    ? qsTr("%1% · charging").arg(slotCard.battery) : qsTr("%1%").arg(slotCard.battery)
                                color: slotCard.battery >= 0 && slotCard.battery <= 20 ? Theme.yellow : Theme.label
                                font.family: Theme.monoFont; font.pixelSize: 15; font.weight: Font.Bold
                            }
                        }
                    }
                }

                Column {
                    visible: root.panelPage === "guide-shortcuts"
                    anchors.top: pageHeader.bottom
                    anchors.topMargin: 20
                    width: parent.width
                    spacing: 4
                    Accessible.role: Accessible.List
                    Accessible.name: qsTr("Keyboard shortcuts")
                    Repeater {
                        model: root.helpRows
                        Item {
                            required property var modelData
                            readonly property bool drawable: InputPromptIcons.keysFor(modelData.value)
                                .every(key => InputPromptIcons.keyboardAsset(key) !== "")
                            width: parent.width
                            height: 46
                            Accessible.role: Accessible.ListItem
                            Accessible.name: modelData.label + ", " + modelData.value
                            Text {
                                anchors.verticalCenter: parent.verticalCenter
                                text: modelData.label
                                color: Theme.label
                                font.family: Theme.bodyFont; font.pixelSize: 18; font.weight: Font.Bold
                            }
                            KeyboardGlyph {
                                visible: parent.drawable
                                anchors.right: parent.right
                                anchors.verticalCenter: parent.verticalCenter
                                shortcut: parent.modelData.value
                                keySize: 28
                            }
                            Text {
                                visible: !parent.drawable
                                anchors.right: parent.right
                                anchors.verticalCenter: parent.verticalCenter
                                text: parent.modelData.value
                                color: Theme.textMuted
                                font.family: Theme.monoFont; font.pixelSize: 15; font.weight: Font.Bold
                            }
                        }
                    }
                }

                Rectangle {
                    visible: root.panelPage === "guide-controls"
                    anchors.bottom: parent.bottom
                    width: parent.width
                    height: infoColumn.implicitHeight + 40
                    radius: 22
                    color: Qt.rgba(Theme.face.r, Theme.face.g, Theme.face.b, 0.05)
                    Column {
                        id: infoColumn
                        x: 20; y: 20
                        width: parent.width - 40
                        spacing: 8
                        Text {
                            text: qsTr("Information only").toUpperCase()
                            color: Theme.textMuted
                            font.family: Theme.monoFont; font.pixelSize: 14; font.weight: Font.Bold; font.letterSpacing: 2.4
                        }
                        Text {
                            width: parent.width
                            wrapMode: Text.WordWrap
                            text: qsTr("Remapping and controller settings live in Settings → Input & controllers, outside the session.")
                            color: Theme.label
                            font.family: Theme.bodyFont; font.pixelSize: 17
                        }
                    }
                }
            }
        }

        Row {
            x: 32
            anchors.bottom: parent.bottom
            anchors.bottomMargin: 34
            spacing: 22
            SessionGlyphs.Hint {
                visible: root.panelPage === "guide-session"
                prompts: glyphs
                button: "A"
                glyphColor: Theme.mint
                label: qsTr("Select")
            }
            SessionGlyphs.Hint {
                prompts: glyphs
                button: "B"
                glyphColor: Theme.coral
                label: root.panelPage === "guide-session" ? qsTr("Resume") : qsTr("Back to guide")
            }
            SessionGlyphs.Hint {
                objectName: "guideCloseHint"
                prompts: glyphs
                button: "GUIDE"
                label: qsTr("Close")
            }
        }
    }

    Rectangle {
        visible: root.toastTitle !== ""
        anchors.right: parent.right
        anchors.rightMargin: 48
        anchors.bottom: parent.bottom
        anchors.bottomMargin: 64
        width: Math.max(420, toastColumn.implicitWidth + 60)
        height: toastColumn.implicitHeight + 36
        radius: 22
        color: Qt.rgba(Theme.shell.r, Theme.shell.g, Theme.shell.b, 0.96)
        border.width: 1
        border.color: Theme.seam
        Accessible.role: Accessible.AlertMessage
        Accessible.name: root.toastTitle
        Rectangle { x: 22; anchors.verticalCenter: parent.verticalCenter; width: 4; height: parent.height - 32; radius: 2; color: Theme.mint }
        Column {
            id: toastColumn
            x: 40
            anchors.verticalCenter: parent.verticalCenter
            Text {
                text: root.toastTitle
                color: Theme.label
                font.family: Theme.displayFont; font.pixelSize: 19; font.weight: Font.Black
            }
            Text {
                visible: root.toastDetail !== ""
                text: root.toastDetail
                color: Theme.textMuted
                font.family: Theme.bodyFont; font.pixelSize: 15
            }
        }
    }
}

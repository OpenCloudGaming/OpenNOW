import QtQuick
import QtQuick.Controls
import QtQuick.Window
import OpenNOW

FocusScope {
    id: root
    property string page: "guide-session"
    property double nowMs: Date.now()
    readonly property var game: ShellStore.selectedGame || ({title: qsTr("GeForce NOW")})
    readonly property var session: ShellStore.activeSession || ({})
    readonly property var profile: session.negotiatedStreamProfile || ({})
    readonly property var actions: root.page === "guide-session" ? [
        {label:qsTr("Resume game"), action:"resume", glyph:"B"},
        {label:qsTr("Toggle stats overlay"), action:"stats", value:ShellStore.settings.shortcutToggleStats || "Ctrl+N"},
        {label:qsTr("Toggle pointer lock"), action:"none", value:ShellStore.settings.shortcutTogglePointerLock || "F8"},
        {label:qsTr("Toggle fullscreen"), action:"fullscreen", value:ShellStore.settings.shortcutToggleFullscreen || "F11"},
        {label:qsTr("Take screenshot"), action:"screenshot", value:ShellStore.settings.shortcutScreenshot || "Ctrl+F11"},
        {label:ShellStore.streamRecordingActive ? "Stop recording" : "Start recording", action:"recording", value:ShellStore.settings.shortcutToggleRecording || "F12"},
        {label:qsTr("Screenshots & recordings"), action:"media", value:qsTr("Open library")},
        {label:qsTr("Microphone"), action:"none", value:ShellStore.microphoneLabel},
        {label:qsTr("End session"), action:"end", danger:true}
    ] : root.page === "guide-controls" ? [
        {label:qsTr("Controller layout"), action:"controllers"},
        {label:qsTr("Keyboard & mouse"), action:"input-settings"},
        {label:qsTr("Input capture"), action:"none", value:qsTr("Paused while guide is open")},
        {label:qsTr("Controller order"), action:"joining", value:qsTr("%1 connected").arg(AppController.controllerCount)}
    ] : root.page === "guide-media" ? [
        {label:qsTr("Recent captures"), action:"media", value:qsTr("%1 items").arg(ShellStore.mediaItems.length)},
        {label:qsTr("Reveal capture folder"), action:"reveal", value:ShellStore.mediaRootPath ? qsTr("Open") : qsTr("Load library")},
        {label:qsTr("Take screenshot"), action:"screenshot", value:ShellStore.settings.shortcutScreenshot || "Ctrl+F11"},
        {label:ShellStore.streamRecordingActive ? "Stop recording" : "Start recording", action:"recording", value:ShellStore.settings.shortcutToggleRecording || "F12"}
    ] : [
        {label:qsTr("Guide overlay"), action:"none", value:qsTr("Guide / Ctrl+G")},
        {label:qsTr("Toggle fullscreen"), action:"fullscreen", value:ShellStore.settings.shortcutToggleFullscreen || "F11"},
        {label:qsTr("Stats overlay"), action:"stats", value:ShellStore.settings.shortcutToggleStats || "Ctrl+N"},
        {label:qsTr("Pointer lock"), action:"none", value:ShellStore.settings.shortcutTogglePointerLock || "F8"},
        {label:qsTr("Screenshot"), action:"screenshot", value:ShellStore.settings.shortcutScreenshot || "Ctrl+F11"},
        {label:qsTr("Toggle recording"), action:"recording", value:ShellStore.settings.shortcutToggleRecording || "F12"},
        {label:qsTr("Toggle Anti-AFK"), action:"none", value:ShellStore.settings.shortcutToggleAntiAfk || "Ctrl+Shift+K"},
        {label:qsTr("Microphone upstream"), action:"none", value:ShellStore.microphoneLabel},
        {label:qsTr("End session"), action:"none", value:ShellStore.settings.shortcutStopStream || "Ctrl+Shift+Q"}
    ]
    anchors.fill: parent
    focus: visible
    Accessible.name: qsTr("OpenNOW session guide")
    onPageChanged: if (visible) ShellStore.recordGuidePage(page)
    onVisibleChanged: if (visible) {
        ShellStore.recordGuidePage(page)
        actionList.forceActiveFocus()
    }
    Component.onCompleted: if (visible) {
        ShellStore.recordGuidePage(page)
        Qt.callLater(actionList.forceActiveFocus)
    }

    function pad(value) { return value < 10 ? "0" + value : String(value) }

    function toggleFullscreen() {
        const targetWindow = Window.window
        if (!targetWindow)
            return
        const enteringFullscreen = targetWindow.visibility !== Window.FullScreen
        if (enteringFullscreen)
            targetWindow.showFullScreen()
        else
            targetWindow.showNormal()
        ShellStore.streamControlMessage = enteringFullscreen
            ? qsTr("Fullscreen on") : qsTr("Fullscreen off")
        ShellStore.accessibilityMessage = ShellStore.streamControlMessage
    }

    function elapsedLabel() {
        if (!ShellStore.streamStartedAtMs)
            return "Starting"
        const total = Math.max(0, Math.floor((root.nowMs - ShellStore.streamStartedAtMs) / 1000))
        const hours = Math.floor(total / 3600)
        const minutes = Math.floor((total % 3600) / 60)
        const seconds = total % 60
        return (hours > 0 ? root.pad(hours) + ":" : "")
             + root.pad(minutes) + ":" + root.pad(seconds)
    }

    function activate(action) {
        if (action === "resume") {
            AppController.showOverlay("")
        } else if (action === "end") {
            AppController.showOverlay("")
            ShellStore.stopStreamingSession()
        } else if (action === "stats") {
            AppController.showOverlay("stream-stats")
        } else if (action === "fullscreen") {
            root.toggleFullscreen()
        } else if (action === "screenshot") {
            ShellStore.captureStreamScreenshot()
        } else if (action === "recording") {
            ShellStore.toggleStreamRecording()
        } else if (action === "media") {
            AppController.showOverlay("")
            AppController.navigate("media")
        } else if (action === "reveal") {
            if (ShellStore.mediaRootPath)
                AppController.openLocalPath(ShellStore.mediaRootPath, false)
            else
                ShellStore.refreshMedia()
        } else if (action === "controllers" || action === "joining" || action === "input-settings") {
            AppController.showOverlay("")
            AppController.navigate(action === "controllers" ? "controllers" : action === "joining" ? "joining" : "settings-input")
        }
    }

    Timer { interval: 1000; repeat: true; running: root.visible; onTriggered: root.nowMs = Date.now() }
    Rectangle { anchors.fill: parent; color: Qt.rgba(0.01, 0.02, 0.05, 0.76) }
    GlassPanel {
        anchors.horizontalCenter: parent.horizontalCenter; y: 234; width: 526; height: 60; panelRadius: 30; strong: true
        Row {
            anchors.centerIn: parent; spacing: 8
            ControllerGlyph { glyph: "LB"; label: ""; glyphSize: 24 }
            Repeater {
                model: [{id:"guide-session",n:"Session"},{id:"guide-controls",n:"Controls"},{id:"guide-media",n:"Media"},{id:"guide-shortcuts",n:"Shortcuts"}]
                GlassButton { required property var modelData; height: 40; width: Math.max(86, implicitWidth); text: modelData.n; glyph: ""; primary: root.page === modelData.id; onClicked: AppController.showOverlay(modelData.id) }
            }
            ControllerGlyph { glyph: "RB"; label: ""; glyphSize: 24 }
        }
    }
    GlassPanel {
        anchors.centerIn: parent; width: 1240; height: 458; panelRadius: 38; strong: true
        Row {
            anchors.fill: parent; anchors.margins: 28; spacing: 22
            Column {
                width: 420; spacing: 14
                PosterTile { width: 200; height: 200; title: root.game.title; artwork: root.game.heroImageUrl || root.game.imageUrl || "" }
                Text { width: parent.width; elide: Text.ElideRight; text: root.game.title; color: Theme.label; font.family: Theme.displayFont; font.pixelSize: 30; font.weight: Font.Black }
                Text { text: (root.profile.resolution || ShellStore.settings.resolution || "—") + " · " + (root.profile.fps || ShellStore.settings.fps ? qsTr("%1 FPS").arg(root.profile.fps || ShellStore.settings.fps) : "—") + " · " + String(root.profile.codec || ShellStore.settings.codec || "—").toUpperCase(); color: Theme.textMuted; font.family: Theme.bodyFont; font.pixelSize: 14 }
                Row {
                    spacing: 8
                    Repeater {
                        model: [root.elapsedLabel() + "\nSESSION", (root.session.zone || root.session.serverLocation || "—") + "\nREGION", (ShellStore.settings.maxBitrateMbps ? ShellStore.settings.maxBitrateMbps : "—") + "\nMBPS"]
                        GlassPanel { required property string modelData; width: 128; height: 66; panelRadius: 18; strong: true; Text { anchors.centerIn: parent; text: I18n.source(modelData, I18n.revision); color: Theme.label; font.family: Theme.monoFont; font.pixelSize: 13; font.weight: Font.Bold; horizontalAlignment: Text.AlignHCenter } }
                    }
                }
            }
            Item {
                width: parent.width - 442; height: parent.height
                ListView {
                    id: actionList
                    anchors.fill: parent; spacing: 8; clip: true; model: root.actions; keyNavigationWraps: true
                    delegate: GlassButton {
                        required property var modelData
                        required property int index
                        width: ListView.view.width
                        text: I18n.source(modelData.label, I18n.revision) + (modelData.value ? "  ·  " + I18n.source(modelData.value, I18n.revision) : "")
                        glyph: modelData.glyph || (index === 0 ? "A" : "")
                        primary: index === 0
                        danger: Boolean(modelData.danger)
                        enabled: modelData.action !== "unavailable" && modelData.action !== "none"
                        currentItem: ListView.isCurrentItem
                        onClicked: root.activate(modelData.action)
                    }
                    Keys.onReturnPressed: if (currentItem) currentItem.clicked()
                    Keys.onEnterPressed: if (currentItem) currentItem.clicked()
                }
                Text {
                    anchors.left: parent.left
                    anchors.right: parent.right
                    anchors.bottom: parent.bottom
                    anchors.bottomMargin: 4
                    visible: ShellStore.streamControlMessage !== ""
                    text: I18n.source(ShellStore.streamControlMessage, I18n.revision)
                    color: Theme.textMuted
                    font.family: Theme.bodyFont
                    font.pixelSize: 12
                    horizontalAlignment: Text.AlignHCenter
                }
            }
        }
    }
    HintBar { anchors.horizontalCenter: parent.horizontalCenter; y: parent.height - height - 90; hints: [{glyph:"A",label:qsTr("Select")},{glyph:"B",label:qsTr("Resume")},{glyph:"GUIDE",label:qsTr("Close")}] }
}

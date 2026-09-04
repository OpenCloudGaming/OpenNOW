import QtQuick
import OpenNOW

FocusScope {
    id: root
    width: 1440
    height: 900
    property string overlay: ""
    property bool inputBlocking: false
    focus: visible && inputBlocking

    readonly property bool menuVisible: overlay === "desktop-stream-menu"
    readonly property bool exitVisible: overlay === "desktop-stream-exit-confirm"
    readonly property bool statsVisible: overlay === "desktop-stream-stats"
        || overlay === "desktop-stream-stats-expanded"
        || overlay === "stream-stats"
        || overlay === "stream-stats-expanded"

    function closeOverlay() {
        AppController.showOverlay("")
    }
    function cycleStats() {
        const prefix = overlay.startsWith("desktop-") ? "desktop-stream-stats" : "stream-stats"
        if (overlay === prefix)
            AppController.showOverlay(prefix + "-expanded")
        else if (overlay === prefix + "-expanded")
            AppController.showOverlay("")
        else
            AppController.showOverlay(prefix)
    }
    function liveNumber(value) {
        return value === undefined || value === null || isNaN(Number(value)) ? null : Number(value)
    }
    function firstAvailable(primary, fallback) {
        return primary === undefined || primary === null ? fallback : primary
    }
    function liveValue(live, value) {
        return ["starting", "negotiating", "connecting", "streaming", "error"]
            .indexOf(String(live.status || "")) >= 0 ? value : undefined
    }
    function liveText(value) {
        const number = root.liveNumber(value)
        return number === null ? qsTr("N/A") : String(Math.round(number))
    }
    function copyStatsSummary() {
        const live = ShellStore.streamer || ({})
        const profile = ShellStore.negotiatedStreamProfile || ({})
        const fps = root.liveText(root.liveValue(live,
            root.firstAvailable(live.framesPerSecond, live.fps)))
        const bitrate = root.liveText(root.liveValue(live,
            root.firstAvailable(live.bitrateMbps, live.receiveBitrateMbps)))
        const output = Number(profile.height || live.outputHeight || 0) > 0
            ? Number(profile.height || live.outputHeight) + "p" : qsTr("pending")
        return qsTr("Stream stats: %1 FPS, %2 Mbps, %3 output")
            .arg(fps).arg(bitrate).arg(output)
    }
    function copyStatsToClipboard() {
        const summary = root.copyStatsSummary()
        if (AppController.writeClipboardText(summary))
            ShellStore.accessibilityMessage = summary
    }

    DesktopInStreamMenu {
        anchors.fill: parent
        visible: root.menuVisible
        focus: visible
        onResumeRequested: root.closeOverlay()
        onInviteRequested: if (ShellStore.socialCapabilities
                && ShellStore.socialCapabilities.invitesAvailable)
            AppController.showOverlay("friends")
        onConsoleModeRequested: enabled => {
            ShellStore.requestConsoleSurface(enabled)
            ShellStore.accessibilityMessage = enabled
                ? qsTr("Switching to console mode") : qsTr("Switching to computer mode")
            root.closeOverlay()
        }
        onEndSessionRequested: {
            ShellStore.requestStreamExitConfirmation()
        }
        onFullscreenRequested: ShellStore.fullscreenToggleRequested()
        onStatsRequested: AppController.showOverlay("desktop-stream-stats-expanded")
    }

    DesktopStreamExitConfirm {
        anchors.fill: parent
        visible: root.exitVisible
        focus: visible
        onCancelRequested: root.closeOverlay()
        onConfirmRequested: ShellStore.confirmStreamExit()
    }

    DesktopStreamStats {
        anchors.fill: parent
        visible: root.statsVisible
        focus: false
        expanded: root.overlay === "desktop-stream-stats-expanded"
            || root.overlay === "stream-stats-expanded"
        onCycleRequested: root.cycleStats()
        onCloseRequested: root.closeOverlay()
        onCopyRequested: root.copyStatsToClipboard()
    }

    Keys.onPressed: event => {
        const modifiers = event.modifiers & (Qt.ControlModifier | Qt.ShiftModifier
            | Qt.AltModifier | Qt.MetaModifier)
        if (!root.exitVisible && event.key === Qt.Key_Q
                && modifiers === (Qt.ControlModifier | Qt.ShiftModifier)) {
            ShellStore.requestStreamExitConfirmation()
            event.accepted = true
        }
    }
}

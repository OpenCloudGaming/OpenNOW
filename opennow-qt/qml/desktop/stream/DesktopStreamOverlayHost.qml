import QtQuick
import OpenNOW

FocusScope {
    id: root
    objectName: "desktopStreamOverlayHost"
    width: 1440
    height: 900
    property string overlay: ""
    property bool inputBlocking: false
    property bool pointerLocked: false
    property var frameGenerationStats: ({})
    property bool notificationsEnabled: false
    focus: visible && inputBlocking
    readonly property bool present: menuView.present || exitView.present || statsVisible || notificationsEnabled

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
    function copyStatsSummary() { return statsView.report() }
    function copyStatsToClipboard() {
        const summary = root.copyStatsSummary()
        if (AppController.writeClipboardText(summary))
            ShellStore.accessibilityMessage = summary
    }

    DesktopInStreamMenu {
        id: menuView
        anchors.fill: parent
        opened: root.menuVisible
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
        id: exitView
        anchors.fill: parent
        opened: root.exitVisible
        onCancelRequested: root.closeOverlay()
        onConfirmRequested: ShellStore.confirmStreamExit()
    }

    DesktopStreamStats {
        id: statsView
        anchors.fill: parent
        visible: root.statsVisible
        frameGenerationStats: root.frameGenerationStats
        pointerLocked: root.pointerLocked
        focus: false
        expanded: root.overlay === "desktop-stream-stats-expanded"
            || root.overlay === "stream-stats-expanded"
        onCycleRequested: root.cycleStats()
        onCloseRequested: root.closeOverlay()
        onCopyRequested: root.copyStatsToClipboard()
    }

    DesktopStreamToasts {
        controllers: ControllerInput.controllers
        anchors.right: parent.right
        anchors.rightMargin: 24
        y: statsView.topRightInset
        visible: root.notificationsEnabled && !root.inputBlocking && y + 148 <= root.height - 24
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

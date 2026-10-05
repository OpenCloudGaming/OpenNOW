import QtQuick
import OpenNOW

FocusScope {
    id: root
    property string overlay: ""
    objectName: "fallbackOverlayHost"
    readonly property bool supportedOverlay: overlay.startsWith("guide-")
        || ["friends", "friend-actions", "quick-settings", "session-conflict", "session-report", "queue-ad"].indexOf(overlay) >= 0
    readonly property bool retainingQueueAd: presentedOverlay === "queue-ad"
        && ["desktop-stream-exit-confirm", "application-quit-confirm"].indexOf(overlay) >= 0
    readonly property bool requested: supportedOverlay || retainingQueueAd
    property string presentedOverlay: ""
    readonly property bool present: reveal.present
    onOverlayChanged: if (supportedOverlay) presentedOverlay = overlay
    onRequestedChanged: if (supportedOverlay) presentedOverlay = overlay
    Component.onCompleted: if (supportedOverlay) presentedOverlay = overlay
    MotionProgress {
        id: reveal
        shown: root.requested
        onHidden: if (!root.requested) root.presentedOverlay = ""
    }
    visible: present
    enabled: supportedOverlay
    opacity: reveal.progress
    focus: supportedOverlay
    onVisibleChanged: if (visible && supportedOverlay) forceActiveFocus()
    Keys.onTabPressed: event => event.accepted = true
    Keys.onBacktabPressed: event => event.accepted = true

    Keys.onPressed: event => {
        if (event.key === Qt.Key_Escape || event.key === Qt.Key_Back) {
            if (root.presentedOverlay === "session-conflict") {
                ShellStore.resolveSessionConflict("cancel")
                event.accepted = true
            } else {
                event.accepted = AppController.goBack()
            }
        } else if (root.presentedOverlay.startsWith("guide-") && event.key === Qt.Key_PageUp) {
            event.accepted = AppController.cycleGuidePage(-1)
        } else if (root.presentedOverlay.startsWith("guide-") && event.key === Qt.Key_PageDown) {
            event.accepted = AppController.cycleGuidePage(1)
        }
    }

    Rectangle {
        anchors.fill: parent
        // Modal dim follows the theme: dark shells need only a faint veil
        // (a heavy dim crushes them to unreadable black), light shells need
        // a stronger one to separate the popup.
        color: root.presentedOverlay.startsWith("guide-") ? "transparent"
            : Qt.rgba(0, 0, 0, Theme.lightMode ? 0.28 : 0.12)
    }

    Loader {
        anchors.fill: parent
        scale: ShellStore.desktopUiActive && !root.presentedOverlay.startsWith("guide-") ? reveal.zoom : 1
        sourceComponent: root.presentedOverlay.startsWith("guide-") ? guideComponent
                       : root.presentedOverlay === "friends" || root.presentedOverlay === "friend-actions"
                            ? (ShellStore.desktopUiActive ? friendsComponent : consoleFriendsComponent)
                       : root.presentedOverlay === "quick-settings" ? quickSettingsComponent
                       : root.presentedOverlay === "session-conflict"
                            ? (ShellStore.desktopUiActive ? sessionConflictComponent : consoleSessionConflictComponent)
                       : root.presentedOverlay === "session-report"
                            ? (ShellStore.desktopUiActive ? sessionReportComponent : consoleSessionReportComponent)
                       : root.presentedOverlay === "queue-ad"
                            ? (ShellStore.desktopUiActive ? queueAdComponent : consoleQueueAdComponent)
                       : undefined
    }

    Component { id: guideComponent; GuideOverlay { page: root.presentedOverlay; revealProgress: reveal.progress } }
    Component { id: consoleFriendsComponent; ConsoleFriendsOverlay {} }
    Component {
        id: friendsComponent
        Item {
            FriendsOverlay {
                x: 40; y: 98
                actionsOpen: root.presentedOverlay === "friend-actions"
            }
        }
    }
    Component {
        id: quickSettingsComponent
        Item { QuickSettingsOverlay { x: parent.width - width - 40; y: 98 } }
    }
    Component { id: sessionConflictComponent; SessionConflictOverlay {} }
    Component { id: sessionReportComponent; SessionReportOverlay {} }
    Component { id: queueAdComponent; QueueAdOverlay {} }
    Component { id: consoleSessionConflictComponent; ConsoleSessionConflict {} }
    Component { id: consoleSessionReportComponent; ConsoleSessionReport {} }
    Component { id: consoleQueueAdComponent; ConsoleQueueAd {} }

}

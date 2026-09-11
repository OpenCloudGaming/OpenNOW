import QtQuick
import OpenNOW

FocusScope {
    id: root
    property string overlay: ""
    objectName: "fallbackOverlayHost"
    readonly property bool requested: overlay.startsWith("guide-")
        || ["friends", "friend-actions", "quick-settings", "session-conflict", "session-report", "queue-ad"].indexOf(overlay) >= 0
    property string presentedOverlay: ""
    readonly property bool present: reveal.present
    onOverlayChanged: if (requested) presentedOverlay = overlay
    onRequestedChanged: if (requested) presentedOverlay = overlay
    Component.onCompleted: if (requested) presentedOverlay = overlay
    MotionProgress {
        id: reveal
        shown: root.requested
        onHidden: if (!root.requested) root.presentedOverlay = ""
    }
    visible: present
    enabled: requested
    opacity: reveal.progress
    focus: requested
    onVisibleChanged: if (visible && requested) forceActiveFocus()
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
        scale: root.presentedOverlay.startsWith("guide-") ? 1 : reveal.zoom
        sourceComponent: root.presentedOverlay.startsWith("guide-") ? guideComponent
                       : root.presentedOverlay === "friends" || root.presentedOverlay === "friend-actions" ? friendsComponent
                       : root.presentedOverlay === "quick-settings" ? quickSettingsComponent
                       : root.presentedOverlay === "session-conflict" ? sessionConflictComponent
                       : root.presentedOverlay === "session-report" ? sessionReportComponent
                       : root.presentedOverlay === "queue-ad" ? queueAdComponent
                       : undefined
    }

    Component { id: guideComponent; GuideOverlay { page: root.presentedOverlay } }
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

}

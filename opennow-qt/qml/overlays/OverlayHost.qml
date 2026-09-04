import QtQuick
import OpenNOW

FocusScope {
    id: root
    property string overlay: ""
    visible: overlay.length > 0
    opacity: visible ? 1 : 0
    focus: visible
    onVisibleChanged: if (visible) forceActiveFocus()
    Keys.onTabPressed: event => event.accepted = true
    Keys.onBacktabPressed: event => event.accepted = true

    Keys.onPressed: event => {
        if (event.key === Qt.Key_Escape || event.key === Qt.Key_Back) {
            event.accepted = AppController.goBack()
        } else if (root.overlay.startsWith("guide-") && event.key === Qt.Key_PageUp) {
            event.accepted = AppController.cycleGuidePage(-1)
        } else if (root.overlay.startsWith("guide-") && event.key === Qt.Key_PageDown) {
            event.accepted = AppController.cycleGuidePage(1)
        }
    }

    Rectangle {
        anchors.fill: parent
        // Modal dim follows the theme: dark shells need only a faint veil
        // (a heavy dim crushes them to unreadable black), light shells need
        // a stronger one to separate the popup.
        color: root.overlay.startsWith("guide-") ? "transparent"
            : Qt.rgba(0, 0, 0, Theme.lightMode ? 0.28 : 0.12)
    }

    Loader {
        anchors.fill: parent
        sourceComponent: root.overlay.startsWith("guide-") ? guideComponent
                       : root.overlay === "friends" || root.overlay === "friend-actions" ? friendsComponent
                       : root.overlay === "quick-settings" ? quickSettingsComponent
                       : root.overlay === "session-conflict" ? sessionConflictComponent
                       : root.overlay === "session-report" ? sessionReportComponent
                       : root.overlay === "queue-ad" ? queueAdComponent
                       : undefined
    }

    Component { id: guideComponent; GuideOverlay { page: root.overlay } }
    Component {
        id: friendsComponent
        Item {
            FriendsOverlay {
                x: 40; y: 98
                actionsOpen: root.overlay === "friend-actions"
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

    Behavior on opacity { NumberAnimation { duration: Theme.overlayDuration; easing.type: Easing.OutCubic } }
}

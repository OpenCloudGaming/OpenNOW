import QtQuick
import OpenNOW

// Co-op toasts and notices over the shell and the stream: invite toast, ask-to-join toasts,
// host notices and ask outcomes. Artboards GLT-1, H3H-1, MFZ-1. Nothing here takes keyboard
// focus; Ctrl+Shift+Y / Ctrl+Shift+N answer the oldest prompt from anywhere.
Item {
    id: root
    objectName: "coopNotices"
    readonly property var coop: ShellStore.coop
    readonly property var friends: ShellStore.friends
    readonly property bool timed: coop.inviteToast !== null || coop.joinRequests.length > 0
    property real now: Date.now()

    Timer {
        interval: 1000
        repeat: true
        running: root.visible && root.timed
        triggeredOnStart: true
        onTriggered: root.now = Date.now()
    }
    Timer {
        id: hostNoticeTimeout
        objectName: "coopHostNoticeTimeout"
        interval: 8000
        running: root.coop.hostNotice !== null
        onTriggered: root.coop.dismissHostNotice()
    }

    Column {
        x: parent.width - width - 24
        y: 24
        width: 384
        spacing: 10

        Loader {
            active: root.coop.inviteToast !== null
            visible: active
            sourceComponent: DesktopCoopInviteToast {
                invite: root.coop.inviteToast
                now: root.now
            }
        }

        Loader {
            active: root.coop.joinRequests.length > 0
            visible: active
            sourceComponent: DesktopCoopAskToast { now: root.now }
        }

        DesktopCoopNoticeCard {
            objectName: visible ? "coopHostNotice" : ""
            readonly property var notice: root.coop.hostNotice
            readonly property bool joining: notice !== null && notice.kind === "joining"
            visible: notice !== null
            user: joining ? notice.user : null
            online: joining
            title: joining ? qsTr("%1 is joining · P%2").arg(root.friends.displayName(notice.user)).arg(Number(notice.slot) + 1)
                : qsTr("Co-op closed")
            body: joining ? qsTr("Connecting… %1 will appear in your lobby").arg(root.friends.displayName(notice.user))
                : qsTr("No one joined before the code expired.")
            trailingIcon: joining ? "qrc:/qt/qml/OpenNOW/res/icons/desktop-check-mint.svg" : ""
            dismissible: !joining
            onDismissed: root.coop.dismissHostNotice()
        }

        DesktopCoopNoticeCard {
            objectName: visible ? "coopAskNotice" : ""
            readonly property var notice: root.coop.askNotice
            visible: notice !== null && notice.kind === "declined"
            user: visible ? notice.user : null
            title: visible ? qsTr("%1 can’t take guests right now").arg(root.friends.displayName(notice.user)) : ""
            body: qsTr("No hard feelings. You can ask again in %1 min.").arg(Math.round(root.coop.askAgainDelayMs / 60000))
            dismissible: true
            onDismissed: root.coop.dismissAskNotice()
        }

        Loader {
            active: root.coop.askNotice !== null && (root.coop.askNotice.kind === "full" || root.coop.askNotice.kind === "room")
            visible: active
            sourceComponent: DesktopCoopAskNotice { notice: root.coop.askNotice }
        }
    }
}

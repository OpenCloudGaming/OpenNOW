import QtQuick
import OpenNOW

// Co-op guest flow (route coop-guest): join code, connecting, join errors, waiting room, end states.
// Variation C artboards 5GF-0, 5JO-0, H2M-1, 5KK-0, 5LZ-0, 62P-0. The views are laid out on the
// 1600x900 artboard and scaled to the window so the desktop and console surfaces share them.
FocusScope {
    id: root
    objectName: "coopGuestScreen"
    readonly property var coop: ShellStore.coop
    readonly property string phase: coop.guestPhase
    readonly property Component view: phase === "joining" || phase === "connecting" ? connectingView
        : phase === "error" ? errorView
        : phase === "ended" ? endedView
        : phase === "waiting" || phase === "starting" || phase === "reconnecting" || phase === "playing" ? waitingView
        : joinCodeView

    function restoreFocus() {
        if (visible && viewLoader.item)
            viewLoader.item.forceActiveFocus()
    }
    onVisibleChanged: if (visible) Qt.callLater(restoreFocus)
    onActiveFocusChanged: if (activeFocus && viewLoader.item && !viewLoader.item.activeFocus) Qt.callLater(restoreFocus)

    Rectangle {
        anchors.fill: parent
        color: "#07090E"
    }

    Item {
        id: stage
        width: 1600
        height: 900
        scale: Math.min(root.width / width, root.height / height)
        x: (root.width - width * scale) / 2
        y: (root.height - height * scale) / 2
        transformOrigin: Item.TopLeft

        DesktopBrandLockup {
            x: 32
            y: 41 - height / 2
            markHeight: 13
            fontPixelSize: 16
            spacing: 10
            ink: "#FFFFFF"
        }

        Loader {
            id: viewLoader
            anchors.fill: parent
            focus: true
            sourceComponent: root.view
            onLoaded: if (root.visible) Qt.callLater(root.restoreFocus)
        }
    }

    Component { id: joinCodeView; DesktopCoopGuestJoinCode { coop: root.coop } }
    Component { id: connectingView; DesktopCoopGuestConnecting { coop: root.coop } }
    Component { id: errorView; DesktopCoopGuestError { coop: root.coop } }
    Component { id: waitingView; DesktopCoopGuestWaitingRoom { coop: root.coop } }
    Component { id: endedView; DesktopCoopGuestEnded { coop: root.coop } }
}

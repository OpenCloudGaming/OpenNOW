import QtQuick
import QtQuick.Controls
import OpenNOW

FocusScope {
    id: root
    property var resetTarget: null
    property bool resetOpen: false
    readonly property var selectedLocation: locationList.currentIndex >= 0 && locationList.currentIndex < ShellStore.storageLocations.length
        ? ShellStore.storageLocations[locationList.currentIndex] : null
    readonly property bool storeLaunchReady: ShellStore.storeLaunchTarget !== null
    readonly property bool storeLaunchBusy: ShellStore.pendingLaunchParams !== null
        && ShellStore.pendingLaunchParams.storeLaunch === true && ShellStore.streamBusy

    function requestReset() {
        if (!selectedLocation || !selectedLocation.isAvailable)
            return
        resetTarget = selectedLocation
        resetOpen = true
    }

    function closeReset() {
        resetOpen = false
        Qt.callLater(() => {
            if (root.resetOpen)
                return
            if (resetButton.enabled) resetButton.forceActiveFocus()
            else root.focusDefault()
        })
    }

    function confirmReset() {
        const target = resetTarget
        closeReset()
        if (target)
            ShellStore.resetPersistentStorage(target.code)
    }

    function focusDefault() {
        if (locationList.count > 0) locationList.forceActiveFocus()
        else storageActions.focusFirst()
    }

    Keys.onPressed: event => {
        if (event.key === Qt.Key_Up || event.key === Qt.Key_Down || event.key === Qt.Key_Left || event.key === Qt.Key_Right) {
            root.focusDefault()
            event.accepted = true
        }
    }

    ScreenBackground { tint: "#1B2338" }

    GlassPanel {
        x: 96; y: 124; width: 1080; height: root.height - 288; panelRadius: 40
        Column {
            x: 56; y: 34; width: parent.width - 112; spacing: 6
            Text { text: qsTr("Persistent storage"); color: Theme.label; font.family: Theme.displayFont; font.pixelSize: 32; font.weight: Font.Black; font.letterSpacing: -0.3 }
            Text { width: parent.width; text: qsTr("Choose the NVIDIA storage region to inspect or reset."); color: Theme.textMuted; font.family: Theme.bodyFont; font.pixelSize: 17; font.weight: Font.DemiBold; elide: Text.ElideRight }
        }
        ListView {
            id: locationList
            anchors.fill: parent
            anchors.leftMargin: 24; anchors.rightMargin: 24
            anchors.topMargin: 124; anchors.bottomMargin: 18
            leftMargin: 10; rightMargin: 10; topMargin: 10; bottomMargin: 10
            spacing: 2; clip: true; keyNavigationWraps: false
            highlightMoveDuration: AppController.reducedMotion ? 0 : 260
            model: ShellStore.storageLocations
            currentIndex: {
                for (let index = 0; index < model.length; ++index)
                    if (model[index].isCurrent) return index
                return model.length ? 0 : -1
            }
            Component.onCompleted: {
                const remembered = ShellStore.focusIndex("persistent-storage")
                if (remembered > 0 && remembered < model.length)
                    currentIndex = remembered
            }
            onCurrentIndexChanged: if (currentIndex >= 0) ShellStore.rememberFocus("persistent-storage", currentIndex)
            Keys.onRightPressed: storageActions.focusFirst()
            Keys.onReturnPressed: event => { if (!event.isAutoRepeat) storageActions.focusFirst() }
            Keys.onEnterPressed: event => { if (!event.isAutoRepeat) storageActions.focusFirst() }
            delegate: ConsoleListRow {
                id: locationRow
                required property var modelData
                required property int index
                width: ListView.view.width - 20
                height: 80
                focusPolicy: Qt.NoFocus
                title: modelData.name
                badge: modelData.isCurrent ? qsTr("CURRENT") : modelData.isRecommended ? qsTr("RECOMMENDED") : modelData.code
                badgeColor: modelData.isCurrent ? Theme.mint : Theme.textMuted
                currentItem: ListView.isCurrentItem
                ringVisible: ListView.isCurrentItem && locationList.activeFocus
                onClicked: {
                    locationList.currentIndex = index
                    locationList.forceActiveFocus()
                }
                Rectangle {
                    width: 14; height: 14; radius: 7
                    color: locationRow.modelData.isCurrent ? Theme.mint : locationRow.modelData.isAvailable ? Theme.focus : Theme.coral
                }
            }
            ScrollIndicator.vertical: ScrollIndicator {}
        }
    }

    GlassPanel {
        x: 1200; y: 124; width: root.width - 1296; height: root.height - 288; panelRadius: 40
        Column {
            id: storageHeader
            x: 40; y: 40; width: parent.width - 80; spacing: 12
            Text { width: parent.width; text: root.selectedLocation ? root.selectedLocation.name : qsTr("Cloud storage"); wrapMode: Text.WordWrap; maximumLineCount: 2; elide: Text.ElideRight; color: Theme.label; font.family: Theme.displayFont; font.pixelSize: 32; font.weight: Font.Black }
            Text { width: parent.width; text: ShellStore.storageMessage || qsTr("Reset deletes game settings and files stored by GeForce NOW in the selected region. This cannot be undone."); wrapMode: Text.WordWrap; color: Theme.textMuted; font.family: Theme.bodyFont; font.pixelSize: 17; font.weight: Font.DemiBold; lineHeight: 1.2 }
        }
        ConsoleActionColumn {
            id: storageActions
            anchors.top: storageHeader.bottom; anchors.topMargin: 28
            x: 40; width: parent.width - 80
            returnTarget: locationList.count > 0 ? locationList : null
            ConsoleActionButton {
                id: resetButton; width: parent.width; danger: true
                text: qsTr("Reset this storage")
                enabled: Boolean(root.selectedLocation && root.selectedLocation.isAvailable)
                onClicked: root.requestReset()
            }
            ConsoleActionButton { width: parent.width; text: qsTr("Refresh locations"); onClicked: ShellStore.refreshStorageLocations() }
            ConsoleActionButton {
                objectName: "persistentStorageStoreLaunch"
                width: parent.width
                text: ShellStore.storeLaunchFailed ? qsTr("Retry") : qsTr("Launch Steam")
                enabled: ShellStore.storeLaunchFailed
                    || (root.storeLaunchReady && ShellStore.storeLaunchRequestId === ""
                        && !root.storeLaunchBusy)
                onClicked: {
                    if (ShellStore.storeLaunchFailed) ShellStore.inspectStoreLaunch()
                    else ShellStore.launchStoreGame()
                }
            }
            Text {
                objectName: "persistentStorageStoreLaunchStatus"
                width: parent.width
                text: ShellStore.storeLaunchRequestId !== ""
                    ? qsTr("Checking the Steam store launch…")
                    : root.storeLaunchBusy
                        ? qsTr("Starting the Steam store session…")
                        : root.storeLaunchReady
                            ? qsTr("Opens the %1 store with your persistent storage.").arg(ShellStore.storeLaunchTarget.title)
                            : ShellStore.storeLaunchDecision.message
                wrapMode: Text.WordWrap
                color: root.storeLaunchReady ? Theme.textMuted : Theme.coral
                font.family: Theme.bodyFont; font.pixelSize: 16; font.weight: Font.DemiBold; lineHeight: 1.2
            }
            ConsoleActionButton { width: parent.width; text: qsTr("Back to settings"); onClicked: AppController.navigate("settings-account") }
        }
    }

    property Connections storeLaunchLifecycle: Connections {
        target: ShellStore
        function onReadyChanged() {
            if (ShellStore.ready && ShellStore.signedIn)
                ShellStore.inspectStoreLaunch()
        }
        function onSignedInChanged() {
            if (ShellStore.ready && ShellStore.signedIn)
                ShellStore.inspectStoreLaunch()
        }
        function onAuthGenerationChanged() {
            if (ShellStore.ready && ShellStore.signedIn)
                ShellStore.inspectStoreLaunch()
        }
    }

    Component.onCompleted: {
        ShellStore.refreshStorageLocations()
        if (!ShellStore.storeLaunchFailed)
            ShellStore.inspectStoreLaunch()
        focusDefault()
    }

    AppChrome {
        anchors.fill: parent; title: qsTr("Persistent storage"); currentRoute: "settings"
        leftHints: [{glyph:"B", label:qsTr("Back")}]
        rightHints: [{glyph:"A", label:qsTr("Select")}]
        onRouteRequested: route => AppController.navigate(route)
    }

    ConsoleWarningSheet {
        objectName: "persistentStorageResetConfirmation"
        opened: root.resetOpen
        eyebrow: qsTr("Persistent storage")
        title: qsTr("Reset %1?").arg(root.resetTarget ? root.resetTarget.name : "")
        message: qsTr("Reset deletes game settings and files stored by GeForce NOW in the selected region. This cannot be undone.")
        safeText: qsTr("Keep storage")
        actionText: qsTr("Reset this storage")
        safeButtonObjectName: "persistentStorageResetSafe"
        actionButtonObjectName: "persistentStorageResetAction"
        onSafeRequested: root.closeReset()
        onActionRequested: root.confirmReset()
    }
}

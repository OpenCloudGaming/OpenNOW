import QtQuick
import QtQuick.Controls
import OpenNOW

FocusScope {
    id: root
    property bool confirmReset: false
    readonly property var selectedLocation: locationList.currentIndex >= 0 && locationList.currentIndex < ShellStore.storageLocations.length
        ? ShellStore.storageLocations[locationList.currentIndex] : null

    ScreenBackground { tint: "#1B2338" }

    GlassPanel {
        x: 96; y: 128; width: root.width * 0.57; height: root.height - 264; panelRadius: 40
        Column {
            anchors.fill: parent; anchors.margins: 34; spacing: 14
            Text { text: qsTr("Persistent storage"); color: Theme.label; font.family: Theme.displayFont; font.pixelSize: 36; font.weight: Font.Black }
            Text { text: qsTr("Choose the NVIDIA storage region to inspect or reset."); color: Theme.textMuted; font.family: Theme.bodyFont; font.pixelSize: 16 }
            ListView {
                id: locationList
                width: parent.width; height: parent.height - 112; spacing: 7; clip: true
                model: ShellStore.storageLocations
                currentIndex: {
                    for (let index = 0; index < model.length; ++index)
                        if (model[index].isCurrent) return index
                    return model.length ? 0 : -1
                }
                focus: true; KeyNavigation.right: resetButton
                Component.onCompleted: {
                    const remembered = ShellStore.focusIndex("persistent-storage")
                    if (remembered > 0 && remembered < model.length)
                        currentIndex = remembered
                }
                onCurrentIndexChanged: {
                    root.confirmReset = false
                    if (currentIndex >= 0)
                        ShellStore.rememberFocus("persistent-storage", currentIndex)
                }
                delegate: ItemDelegate {
                    required property var modelData
                    required property int index
                    width: locationList.width; height: 66; focusPolicy: Qt.StrongFocus
                    highlighted: ListView.isCurrentItem
                    onClicked: locationList.currentIndex = index
                    background: Rectangle { radius: 21; color: parent.highlighted ? Theme.glassStrong : Theme.glass; border.color: parent.highlighted ? Theme.focus : Theme.seam; border.width: parent.highlighted ? 3 : 1 }
                    contentItem: Row {
                        spacing: 14
                        Rectangle { width: 14; height: 14; radius: 7; anchors.verticalCenter: parent.verticalCenter; color: modelData.isCurrent ? Theme.mint : modelData.isAvailable ? Theme.focus : Theme.coral }
                        Text { width: parent.width - 170; anchors.verticalCenter: parent.verticalCenter; text: modelData.name; color: Theme.label; font.family: Theme.bodyFont; font.pixelSize: 17; font.weight: Font.Bold; elide: Text.ElideRight }
                        Text { anchors.verticalCenter: parent.verticalCenter; text: modelData.isCurrent ? qsTr("CURRENT") : modelData.isRecommended ? qsTr("RECOMMENDED") : modelData.code; color: modelData.isCurrent ? Theme.mint : Theme.textMuted; font.family: Theme.monoFont; font.pixelSize: 11; font.weight: Font.Black }
                    }
                }
                ScrollIndicator.vertical: ScrollIndicator {}
            }
        }
    }

    GlassPanel {
        x: root.width * 0.7; y: 175; width: root.width * 0.23; height: 460; panelRadius: 36; strong: true
        Column {
            anchors.fill: parent; anchors.margins: 26; spacing: 14
            Text { width: parent.width; text: root.selectedLocation ? root.selectedLocation.name : qsTr("Cloud storage"); wrapMode: Text.WordWrap; color: Theme.label; font.family: Theme.displayFont; font.pixelSize: 27; font.weight: Font.Black }
            Text { width: parent.width; text: ShellStore.storageMessage || qsTr("Reset deletes game settings and files stored by GeForce NOW in the selected region. This cannot be undone."); wrapMode: Text.WordWrap; color: root.confirmReset ? Theme.coral : Theme.textMuted; font.family: Theme.bodyFont; font.pixelSize: 15; lineHeight: 1.2 }
            GlassButton {
                id: resetButton; width: parent.width; glyph: root.confirmReset ? "A" : "X"; danger: true
                text: root.confirmReset ? qsTr("Confirm reset") : qsTr("Reset this storage")
                enabled: root.selectedLocation && root.selectedLocation.isAvailable
                onClicked: {
                    if (!root.confirmReset) {
                        root.confirmReset = true
                        return
                    }
                    root.confirmReset = false
                    ShellStore.resetPersistentStorage(root.selectedLocation.code)
                }
                Component.onCompleted: forceActiveFocus()
            }
            GlassButton { width: parent.width; glyph: "↻"; text: qsTr("Refresh locations"); onClicked: { root.confirmReset = false; ShellStore.refreshStorageLocations() } }
            GlassButton { width: parent.width; glyph: "B"; text: qsTr("Back to settings"); onClicked: AppController.navigate("settings-account") }
        }
    }

    Component.onCompleted: ShellStore.refreshStorageLocations()
    AppChrome { anchors.fill: parent; title: qsTr("Persistent storage"); currentRoute: "settings"; onRouteRequested: route => AppController.navigate(route) }
}

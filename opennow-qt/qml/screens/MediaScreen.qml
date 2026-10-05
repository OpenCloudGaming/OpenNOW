import QtQuick
import QtQuick.Controls
import QtQuick.Dialogs
import OpenNOW

FocusScope {
    id: root
    property var pendingDelete: null
    property var pendingExport: null
    property bool deleteOpen: false
    readonly property var currentCapture: mediaGrid.currentIndex >= 0 && mediaGrid.currentIndex < ShellStore.mediaItems.length
        ? ShellStore.mediaItems[mediaGrid.currentIndex] : null

    function requestDelete(item) {
        pendingDelete = item
        deleteOpen = true
    }

    function closeDelete() {
        deleteOpen = false
        Qt.callLater(() => { if (!root.deleteOpen) mediaGrid.forceActiveFocus() })
    }

    function confirmDelete() {
        const item = pendingDelete
        closeDelete()
        if (item)
            ShellStore.deleteMedia(item)
    }

    Component.onCompleted: ShellStore.refreshMedia()

    ScreenBackground { tint: "#17243A" }

    Column {
        x: 96; y: 124; width: parent.width - 192; spacing: 8
        Text {
            text: qsTr("Captures")
            color: Theme.label; font.family: Theme.displayFont; font.pixelSize: 40; font.weight: Font.Black; font.letterSpacing: -0.5
        }
        Text {
            width: parent.width
            text: ShellStore.mediaMessage || qsTr("Screenshots and recordings from your streams")
            color: Theme.textMuted; font.family: Theme.bodyFont; font.pixelSize: 18; font.weight: Font.DemiBold; elide: Text.ElideRight
        }
    }

    GridView {
        id: mediaGrid
        x: 84; y: 214; width: parent.width - 168; height: parent.height - 214 - 176
        leftMargin: 12; rightMargin: 12; topMargin: 14; bottomMargin: 14
        cellWidth: Math.floor((width - 24) / Math.max(1, Math.floor((width - 24) / 300))); cellHeight: 248; clip: true; focus: true
        keyNavigationWraps: true
        highlightMoveDuration: AppController.reducedMotion ? 0 : 260
        model: ShellStore.mediaItems
        Component.onCompleted: currentIndex = model.length ? Math.min(ShellStore.focusIndex("media"), model.length - 1) : -1
        onCountChanged: if (count > 0 && currentIndex < 0) currentIndex = Math.min(ShellStore.focusIndex("media"), count - 1)
        onCurrentIndexChanged: if (currentIndex >= 0) ShellStore.rememberFocus("media", currentIndex)
        delegate: ItemDelegate {
            id: tile
            required property var modelData
            required property int index
            readonly property bool focused: GridView.isCurrentItem && mediaGrid.activeFocus
            readonly property bool parked: GridView.isCurrentItem && root.deleteOpen
            width: mediaGrid.cellWidth - 24; height: mediaGrid.cellHeight - 24
            padding: 10
            focusPolicy: Qt.NoFocus
            scale: tile.focused && !AppController.reducedMotion ? 1.04 : 1
            z: tile.focused ? 2 : 1
            Behavior on scale { NumberAnimation { duration: AppController.reducedMotion ? 0 : 90; easing.type: Easing.OutCubic } }
            Accessible.role: Accessible.Button
            Accessible.name: modelData.fileName
            Accessible.description: modelData.kind === "recording" ? qsTr("RECORDING") : qsTr("SCREENSHOT")
            onClicked: {
                mediaGrid.currentIndex = index
                mediaGrid.forceActiveFocus()
                AppController.openLocalPath(modelData.filePath, false)
            }
            background: Item {
                Rectangle {
                    anchors.fill: parent; anchors.margins: -9; radius: 34
                    color: "transparent"; border.width: 5
                    border.color: Qt.rgba(Theme.focus.r, Theme.focus.g, Theme.focus.b, 0.5)
                    visible: tile.focused
                }
                Rectangle {
                    anchors.fill: parent; radius: 26
                    color: tile.focused ? Qt.rgba(Theme.face.r, Theme.face.g, Theme.face.b, 0.10) : Theme.glass
                    border.width: tile.focused ? 3 : tile.parked ? 2 : 1
                    border.color: tile.focused || tile.parked ? Theme.face : Theme.seam
                }
            }
            contentItem: Column {
                spacing: 10
                Rectangle {
                    width: parent.width; height: tile.availableHeight - 34; radius: 18; color: Theme.glassStrong; clip: true
                    Image {
                        anchors.fill: parent; source: tile.modelData.thumbnailUrl || ""
                        fillMode: Image.PreserveAspectCrop; asynchronous: true
                    }
                    Rectangle {
                        visible: tile.modelData.kind === "recording"
                        anchors.centerIn: parent; width: 54; height: 54; radius: 27; color: Qt.rgba(0.02,0.04,0.08,0.74)
                        Text { anchors.centerIn: parent; text: qsTr("▶"); color: Theme.label; font.pixelSize: 21 }
                    }
                    Rectangle {
                        x: 10; y: 10; width: kindLabel.implicitWidth + 20; height: 26; radius: 13
                        color: tile.modelData.kind === "recording" ? Theme.coral : Theme.violet
                        Text { id: kindLabel; anchors.centerIn: parent; text: tile.modelData.kind === "recording" ? qsTr("RECORDING") : qsTr("SCREENSHOT"); color: Theme.contrastText(parent.color); font.family: Theme.monoFont; font.pixelSize: 11; font.weight: Font.Bold; font.letterSpacing: 1 }
                    }
                }
                Text {
                    width: parent.width; text: tile.modelData.fileName; elide: Text.ElideMiddle
                    color: Theme.label; font.family: Theme.bodyFont; font.pixelSize: 16; font.weight: tile.focused ? Font.Black : Font.Bold
                }
            }
            Keys.onDeletePressed: root.requestDelete(modelData)
            Keys.onPressed: event => {
                if (event.key === Qt.Key_Y) {
                    root.requestDelete(modelData)
                    event.accepted = true
                } else if (event.key === Qt.Key_X && modelData.kind === "screenshot") {
                    root.pendingExport = modelData
                    exportDialog.open()
                    event.accepted = true
                } else if (event.key === Qt.Key_R && modelData.kind === "recording") {
                    ShellStore.mediaMessage = ThumbnailGenerator.regenerate(modelData.filePath)
                        ? qsTr("Regenerating recording thumbnail…")
                        : qsTr("Thumbnail regeneration is unavailable")
                    event.accepted = true
                }
            }
        }
        Keys.onPressed: event => {
            if (event.key === Qt.Key_Return || event.key === Qt.Key_Enter) {
                if (!event.isAutoRepeat && root.currentCapture)
                    AppController.openLocalPath(root.currentCapture.filePath, false)
                event.accepted = true
            }
        }
    }

    FileDialog {
        id: exportDialog
        title: qsTr("Save screenshot as")
        fileMode: FileDialog.SaveFile
        nameFilters: [qsTr("PNG image (*.png)"), qsTr("JPEG image (*.jpg *.jpeg)"), qsTr("WebP image (*.webp)"), qsTr("All files (*)")]
        defaultSuffix: root.pendingExport
            ? String(root.pendingExport.fileName).split(".").pop() : "png"
        onAccepted: {
            const saved = root.pendingExport
                && AppController.copyScreenshotTo(root.pendingExport.filePath, selectedFile)
            ShellStore.mediaMessage = saved ? qsTr("Screenshot exported") : qsTr("Could not export screenshot")
            root.pendingExport = null
            mediaGrid.forceActiveFocus()
        }
        onRejected: {
            root.pendingExport = null
            mediaGrid.forceActiveFocus()
        }
    }

    Connections {
        target: ThumbnailGenerator
        function onFinished(sourcePath, thumbnailUrl, ok, message) {
            ShellStore.mediaMessage = message
            ShellStore.refreshMedia()
            if (!root.deleteOpen)
                mediaGrid.forceActiveFocus()
        }
    }

    GlassPanel {
        visible: ShellStore.mediaState === "loading" || ShellStore.mediaItems.length === 0
        anchors.centerIn: mediaGrid; width: 620; height: 168; panelRadius: 40; strong: true
        Column {
            anchors.centerIn: parent; spacing: 10
            Text { anchors.horizontalCenter: parent.horizontalCenter; text: ShellStore.mediaState === "loading" ? qsTr("Loading captures…") : qsTr("No captures yet"); color: Theme.label; font.family: Theme.displayFont; font.pixelSize: 28; font.weight: Font.Black }
            Text { anchors.horizontalCenter: parent.horizontalCenter; text: qsTr("Screenshots and recordings will appear here."); color: Theme.textMuted; font.family: Theme.bodyFont; font.pixelSize: 17; font.weight: Font.DemiBold }
        }
    }

    AppChrome {
        anchors.fill: parent; title: qsTr("Captures"); currentRoute: "media"
        leftHints: root.currentCapture ? [{glyph:"B", label:qsTr("Back")}, {glyph:"Y", label:qsTr("Delete")}] : [{glyph:"B", label:qsTr("Back")}]
        rightHints: !root.currentCapture ? []
            : root.currentCapture.kind === "screenshot" ? [{glyph:"X", label:qsTr("Export screenshot")}, {glyph:"A", label:qsTr("Open")}]
            : [{glyph:"R", keyboard:true, label:qsTr("Refresh thumbnail")}, {glyph:"A", label:qsTr("Open")}]
        onRouteRequested: route => AppController.navigate(route)
    }

    ConsoleWarningSheet {
        id: deleteSheet
        objectName: "mediaDeleteConfirmation"
        opened: root.deleteOpen
        eyebrow: qsTr("Captures")
        title: qsTr("Delete this capture?")
        message: qsTr("This permanently removes the file from this device.")
        detail: root.pendingDelete ? root.pendingDelete.fileName : ""
        safeText: qsTr("Keep capture")
        actionText: qsTr("Delete permanently")
        safeButtonObjectName: "mediaDeleteKeep"
        actionButtonObjectName: "mediaDeleteConfirm"
        onSafeRequested: root.closeDelete()
        onActionRequested: root.confirmDelete()
        onPresentChanged: if (!present && !root.deleteOpen) root.pendingDelete = null
    }
}

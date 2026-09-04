import QtQuick
import QtQuick.Controls
import QtQuick.Dialogs
import OpenNOW

FocusScope {
    id: root
    property var pendingDelete: null
    property var pendingExport: null
    Component.onCompleted: ShellStore.refreshMedia()

    ScreenBackground { tint: "#17243A" }

    Column {
        x: 92; y: 116; width: parent.width - 184; spacing: 10
        Text {
            text: qsTr("Captures")
            color: Theme.label; font.family: Theme.displayFont; font.pixelSize: 42; font.weight: Font.Black
        }
        Text {
            text: ShellStore.mediaMessage || qsTr("Screenshots and recordings from your streams")
            color: Theme.textMuted; font.family: Theme.bodyFont; font.pixelSize: 16
        }
    }

    GridView {
        id: mediaGrid
        x: 92; y: 208; width: parent.width - 184; height: parent.height - 338
        cellWidth: 306; cellHeight: 232; clip: true; focus: true
        keyNavigationWraps: true
        model: ShellStore.mediaItems
        Component.onCompleted: currentIndex = model.length ? Math.min(ShellStore.focusIndex("media"), model.length - 1) : -1
        onCountChanged: if (count > 0 && currentIndex < 0) currentIndex = Math.min(ShellStore.focusIndex("media"), count - 1)
        onCurrentIndexChanged: if (currentIndex >= 0) ShellStore.rememberFocus("media", currentIndex)
        delegate: ItemDelegate {
            id: tile
            required property var modelData
            required property int index
            width: 286; height: 212; focusPolicy: Qt.StrongFocus
            highlighted: GridView.isCurrentItem
            onClicked: {
                if (modelData.kind === "recording")
                    AppController.openLocalPath(modelData.filePath, false)
                else
                    AppController.openLocalPath(modelData.filePath, false)
            }
            background: GlassPanel {
                panelRadius: 24; strong: tile.highlighted
                border.color: tile.highlighted ? Theme.focus : Theme.seam
                border.width: tile.highlighted ? 3 : 1
            }
            contentItem: Column {
                spacing: 9
                Rectangle {
                    width: parent.width; height: 148; radius: 17; color: Theme.glassStrong; clip: true
                    Image {
                        anchors.fill: parent; source: modelData.thumbnailUrl || ""
                        fillMode: Image.PreserveAspectCrop; asynchronous: true
                    }
                    Rectangle {
                        visible: modelData.kind === "recording"
                        anchors.centerIn: parent; width: 54; height: 54; radius: 27; color: Qt.rgba(0.02,0.04,0.08,0.74)
                        Text { anchors.centerIn: parent; text: qsTr("▶"); color: Theme.label; font.pixelSize: 21 }
                    }
                    Rectangle {
                        x: 10; y: 10; width: 94; height: 27; radius: 13
                        color: modelData.kind === "recording" ? Theme.coral : Theme.violet
                        Text { anchors.centerIn: parent; text: modelData.kind === "recording" ? qsTr("RECORDING") : qsTr("SCREENSHOT"); color: Theme.contrastText(modelData.kind === "recording" ? Theme.coral : Theme.violet); font.family: Theme.monoFont; font.pixelSize: 10; font.weight: Font.Black }
                    }
                }
                Text {
                    width: parent.width; text: modelData.fileName; elide: Text.ElideMiddle
                    color: tile.highlighted ? Theme.focus : Theme.label; font.family: Theme.bodyFont; font.pixelSize: 14; font.weight: Font.Bold
                }
            }
            Keys.onDeletePressed: {
                root.pendingDelete = modelData
                deleteDialog.forceActiveFocus()
            }
            Keys.onPressed: event => {
                if (event.key === Qt.Key_Y) {
                    root.pendingDelete = modelData
                    deleteDialog.forceActiveFocus()
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
        Keys.onReturnPressed: if (currentItem) currentItem.clicked()
        Keys.onEnterPressed: if (currentItem) currentItem.clicked()
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
            mediaGrid.forceActiveFocus()
        }
    }

    GlassPanel {
        visible: ShellStore.mediaState === "loading" || ShellStore.mediaItems.length === 0
        anchors.centerIn: mediaGrid; width: 560; height: 160; panelRadius: 30; strong: true
        Column {
            anchors.centerIn: parent; spacing: 10
            Text { anchors.horizontalCenter: parent.horizontalCenter; text: ShellStore.mediaState === "loading" ? qsTr("Loading captures…") : qsTr("No captures yet"); color: Theme.label; font.family: Theme.displayFont; font.pixelSize: 25; font.weight: Font.Black }
            Text { anchors.horizontalCenter: parent.horizontalCenter; text: qsTr("Screenshots and recordings will appear here."); color: Theme.textMuted; font.family: Theme.bodyFont; font.pixelSize: 15 }
        }
    }

    GlassPanel {
        id: deleteDialog
        visible: root.pendingDelete !== null
        focus: visible
        anchors.centerIn: parent; width: 570; height: 230; z: 20; panelRadius: 34; strong: true
        onVisibleChanged: if (visible) Qt.callLater(keepButton.forceActiveFocus)
        Keys.onEscapePressed: { root.pendingDelete = null; mediaGrid.forceActiveFocus() }
        Column {
            anchors.fill: parent; anchors.margins: 26; spacing: 14
            Text { text: qsTr("Delete this capture?"); color: Theme.label; font.family: Theme.displayFont; font.pixelSize: 26; font.weight: Font.Black }
            Text { width: parent.width; text: root.pendingDelete ? root.pendingDelete.fileName : ""; elide: Text.ElideMiddle; color: Theme.textMuted; font.family: Theme.bodyFont; font.pixelSize: 14 }
            Row {
                spacing: 10
                GlassButton { id: keepButton; width: 240; text: qsTr("Keep capture"); primary: true; KeyNavigation.right: deleteButton; onClicked: { root.pendingDelete = null; mediaGrid.forceActiveFocus() } }
                GlassButton { id: deleteButton; width: 240; text: qsTr("Delete permanently"); danger: true; KeyNavigation.left: keepButton; onClicked: { ShellStore.deleteMedia(root.pendingDelete); root.pendingDelete = null; mediaGrid.forceActiveFocus() } }
            }
        }
    }

    HintBar { anchors.horizontalCenter: parent.horizontalCenter; y: parent.height - height - 84; hints: [{glyph:"A",label:qsTr("Open")},{glyph:"X",label:qsTr("Export screenshot")},{glyph:"R",label:qsTr("Refresh thumbnail")},{glyph:"Y",label:qsTr("Delete")},{glyph:"B",label:qsTr("Back")}] }
    AppChrome { anchors.fill: parent; title: qsTr("Captures"); currentRoute: "media"; onRouteRequested: route => AppController.navigate(route) }
}

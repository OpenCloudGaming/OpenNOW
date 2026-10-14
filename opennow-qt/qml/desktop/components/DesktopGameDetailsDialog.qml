import QtQuick
import QtQuick.Controls
import QtQuick.Window
import OpenNOW

FocusScope {
    id: root
    property var game: null
    property bool opened: false
    property string partName: "gameDetails"
    property string badgeText: ""
    property string metaText: ""
    property Item initialFocusItem: null
    default property alias body: bodyContent.data
    signal closeRequested()
    visible: reveal.present
    enabled: opened
    focus: opened
    onOpenedChanged: if (opened) Qt.callLater(() => { if (root.opened) root.focusInitial() })
    MotionProgress { id: reveal; objectName: root.partName + "Motion"; shown: root.opened }

    readonly property int gutter: DesktopTokens.px(32)
    readonly property int dialogWidth: Math.min(DesktopTokens.px(760), Math.max(0, width - gutter * 2))
    readonly property int maximumDialogHeight: Math.max(0, height - gutter * 2)
    readonly property int dialogHeight: Math.min(maximumDialogHeight, Math.ceil(detailsColumn.implicitHeight))

    function focusInitial() {
        if (root.initialFocusItem)
            root.initialFocusItem.forceActiveFocus()
        else
            root.forceActiveFocus()
    }
    function revealFocusedControl() {
        if (!root.opened || !root.Window.window)
            return
        const item = root.Window.window.activeFocusItem
        let ancestor = item
        while (ancestor && ancestor !== detailsColumn)
            ancestor = ancestor.parent
        if (!ancestor)
            return
        const top = item.mapToItem(detailsFlick.contentItem, 0, 0).y
        const margin = DesktopTokens.px(8)
        const maximumY = Math.max(0, detailsFlick.contentHeight - detailsFlick.height)
        if (top < detailsFlick.contentY + margin)
            detailsFlick.contentY = Math.max(0, top - margin)
        else if (top + item.height > detailsFlick.contentY + detailsFlick.height - margin)
            detailsFlick.contentY = Math.min(maximumY, top + item.height + margin - detailsFlick.height)
    }
    Connections {
        target: root.Window.window
        function onActiveFocusItemChanged() { Qt.callLater(root.revealFocusedControl) }
    }

    Rectangle {
        anchors.fill: parent; color: "#A6040D10"; opacity: reveal.progress
        MouseArea {
            anchors.fill: parent; acceptedButtons: Qt.AllButtons
            hoverEnabled: true; preventStealing: true
            onClicked: root.closeRequested()
            onWheel: wheel => wheel.accepted = true
        }
    }
    Rectangle {
        id: dialog
        objectName: root.partName + "Dialog"
        opacity: reveal.progress
        scale: reveal.zoom
        transformOrigin: Item.Center
        anchors.centerIn: parent
        width: root.dialogWidth; height: root.dialogHeight
        radius: DesktopTokens.px(24); color: Theme.shell; border.width: 1; border.color: Theme.seam
        // Swallow blank-space clicks inside the modal, never activate its scrim.
        MouseArea { anchors.fill: parent; acceptedButtons: Qt.AllButtons; onWheel: wheel => wheel.accepted = true }
        Flickable {
            id: detailsFlick
            objectName: root.partName + "Scroll"
            anchors.fill: parent
            contentWidth: width; contentHeight: detailsColumn.implicitHeight
            clip: true; boundsBehavior: Flickable.StopAtBounds
            flickableDirection: Flickable.VerticalFlick
            onHeightChanged: Qt.callLater(root.revealFocusedControl)
            onContentHeightChanged: Qt.callLater(root.revealFocusedControl)
            ScrollBar.vertical: ScrollBar {
                policy: detailsFlick.contentHeight > detailsFlick.height ? ScrollBar.AlwaysOn : ScrollBar.AlwaysOff
            }
            Column {
                id: detailsColumn
                width: parent.width
                Item {
                    width: parent.width
                    height: Math.max(headerInfo.implicitHeight + DesktopTokens.px(48),
                        Math.min(DesktopTokens.px(280), root.maximumDialogHeight * 0.38))
                    RoundedArtwork {
                        anchors.fill: parent; artwork: DesktopTokens.artworkUrl(root.game, true)
                        cornerRadius: DesktopTokens.px(24); scrimStart: 0.1; fallbackColor: Theme.shell
                    }
                    Rectangle {
                        anchors.fill: parent
                        gradient: Gradient {
                            GradientStop { position: 0.25; color: "transparent" }
                            GradientStop { position: 1; color: Theme.shell }
                        }
                    }
                    Column {
                        id: headerInfo
                        x: DesktopTokens.px(24); anchors.bottom: parent.bottom; anchors.bottomMargin: DesktopTokens.px(24)
                        width: parent.width - DesktopTokens.px(48); spacing: DesktopTokens.px(8)
                        Text {
                            objectName: root.partName + "Title"
                            width: parent.width; text: root.game ? String(root.game.title || qsTr("Game")) : qsTr("Game")
                            textFormat: Text.PlainText
                            color: Theme.label; font.family: Theme.displayFont
                            font.pixelSize: DesktopTokens.px(34); font.weight: Font.Black
                            wrapMode: Text.WordWrap; maximumLineCount: 2; elide: Text.ElideRight
                        }
                        Flow {
                            width: parent.width; spacing: DesktopTokens.px(10)
                            Rectangle {
                                objectName: root.partName + "Badge"
                                visible: root.badgeText !== ""
                                width: Math.min(parent.width, badgeLabel.implicitWidth + DesktopTokens.px(20))
                                height: DesktopTokens.px(26); radius: DesktopTokens.px(13); color: DesktopTokens.raisedStrong
                                Text { id: badgeLabel; anchors.centerIn: parent; width: parent.width - DesktopTokens.px(20); elide: Text.ElideRight; text: root.badgeText; textFormat: Text.PlainText; color: Theme.label; font.family: Theme.bodyFont; font.pixelSize: DesktopTokens.captionSize; font.weight: Font.Bold }
                            }
                            Text {
                                objectName: root.partName + "Meta"
                                width: Math.min(implicitWidth, parent.width)
                                wrapMode: Text.WordWrap; maximumLineCount: 2; elide: Text.ElideRight
                                text: root.metaText
                                textFormat: Text.PlainText
                                color: Theme.textMuted; font.family: Theme.bodyFont; font.pixelSize: DesktopTokens.captionSize
                            }
                        }
                    }
                }
                Item {
                    width: parent.width
                    height: bodyContent.implicitHeight + DesktopTokens.px(24)
                    Column {
                        id: bodyContent
                        objectName: root.partName + "Body"
                        x: DesktopTokens.px(24)
                        width: parent.width - DesktopTokens.px(48)
                        spacing: DesktopTokens.px(20)
                    }
                }
            }
        }
        DesktopButton {
            objectName: root.partName + "Close"
            anchors.right: parent.right; anchors.rightMargin: -width / 2
            anchors.top: parent.top; anchors.topMargin: -height / 2
            width: DesktopTokens.px(36); height: DesktopTokens.px(36); themedGlyph: "close"; leftPadding: 0; rightPadding: 0
            cornerRadius: width / 2
            Accessible.name: qsTr("Close details")
            onClicked: root.closeRequested()
        }
    }
    Keys.onEscapePressed: root.closeRequested()
}

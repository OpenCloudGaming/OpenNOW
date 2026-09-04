import QtQuick
import QtQuick.Controls
import OpenNOW

Item {
    id: root
    anchors.fill: parent
    visible: presented
    z: 10000
    property bool opened: false
    property bool presented: false
    property Item anchorItem: null
    property var items: []
    property var selectedValue: ""
    property string footer: ""
    signal chosen(var value)

    readonly property int rowHeight: DesktopTokens.px(32)
    readonly property int headingHeight: DesktopTokens.px(24)
    readonly property int footerHeight: DesktopTokens.px(36)
    readonly property int menuWidth: Math.max(DesktopTokens.px(320), Math.min(root.width - DesktopTokens.px(24), DesktopTokens.px(400)))
    readonly property int menuMaxHeight: DesktopTokens.px(640)
    readonly property int menuMargin: DesktopTokens.px(8)
    readonly property int menuRadius: DesktopTokens.px(14)
    readonly property int rowRadius: DesktopTokens.px(8)

    readonly property int menuContentHeight: {
        let height = 0
        const list = items || []
        for (let i = 0; i < list.length; ++i)
            height += list[i] && list[i].kind === "heading" ? headingHeight : rowHeight
        if (footer !== "")
            height += footerHeight
        return height
    }

    function placeMenu() {
        if (!anchorItem)
            return
        const gap = DesktopTokens.px(6)
        const margin = DesktopTokens.px(12)
        const point = anchorItem.mapToItem(root, 0, anchorItem.height + gap)
        menu.x = Math.max(margin, Math.min(root.width - menu.width - margin, point.x + anchorItem.width - menu.width))
        menu.y = Math.max(margin, Math.min(root.height - menu.height - margin, point.y))
    }

    function showFor(anchor, choices, current, footerText) {
        closeTimer.stop()
        anchorItem = anchor
        items = choices || []
        selectedValue = current
        footer = footerText || ""
        presented = true
        opened = true
        placeMenu()
        Qt.callLater(() => {
            placeMenu()
            choiceList.forceActiveFocus()
        })
    }

    function dismiss() {
        const wasOpen = root.opened
        opened = false
        closeTimer.restart()
        if (wasOpen && anchorItem) {
            const anchor = anchorItem
            Qt.callLater(() => { if (anchor) anchor.forceActiveFocus() })
        }
    }

    Timer {
        id: closeTimer
        interval: AppController.reducedMotion ? 0 : 150
        onTriggered: root.presented = false
    }

    Rectangle {
        anchors.fill: parent
        color: Qt.rgba(0, 0, 0, root.opened ? 0.12 : 0)
        Behavior on color { ColorAnimation { duration: AppController.reducedMotion ? 0 : 150 } }
        MouseArea { anchors.fill: parent; onClicked: root.dismiss() }
    }

    Rectangle {
        id: menu
        width: root.menuWidth
        height: Math.min(Math.min(root.menuMaxHeight, root.height - DesktopTokens.px(24)), Math.max(DesktopTokens.px(48), root.menuContentHeight + root.menuMargin * 2))
        radius: root.menuRadius
        color: "#FA141924"
        border.width: 1
        border.color: "#2EFFFFFF"
        opacity: root.opened ? 1 : 0
        scale: root.opened ? 1 : 0.97
        transformOrigin: Item.TopRight
        Behavior on opacity { NumberAnimation { duration: AppController.reducedMotion ? 0 : 150; easing.type: Easing.OutCubic } }
        Behavior on scale { NumberAnimation { duration: AppController.reducedMotion ? 0 : 170; easing.type: Easing.OutBack; easing.overshoot: 0.6 } }

        Flickable {
            id: choiceList
            anchors.fill: parent
            anchors.margins: root.menuMargin
            contentWidth: width
            contentHeight: Math.max(choiceColumn.implicitHeight, root.menuContentHeight)
            clip: true
            boundsBehavior: Flickable.StopAtBounds
            Keys.onEscapePressed: event => { root.dismiss(); event.accepted = true }

            Column {
                id: choiceColumn
                width: choiceList.width
                spacing: 0

                Repeater {
                    model: root.items
                    delegate: Item {
                        required property int index
                        required property var modelData
                        readonly property bool heading: modelData.kind === "heading"
                        readonly property bool on: !heading && modelData.value === root.selectedValue
                        readonly property bool unavailable: !heading && Boolean(modelData.disabled)
                        width: choiceColumn.width
                        height: heading ? root.headingHeight : root.rowHeight

                        Text {
                            visible: heading
                            anchors.left: parent.left
                            anchors.leftMargin: DesktopTokens.px(12)
                            anchors.verticalCenter: parent.verticalCenter
                            text: modelData.label
                            color: "#6BFFFFFF"
                            font.family: DesktopTokens.monoFont
                            font.pixelSize: DesktopTokens.tinySize
                            font.weight: Font.DemiBold
                            font.letterSpacing: 0.8
                        }

                        Rectangle {
                            visible: !heading
                            opacity: parent.unavailable ? 0.42 : 1
                            anchors.fill: parent
                            radius: root.rowRadius
                            color: rowHover.hovered ? "#0FFFFFFF" : (on ? "#17FFFFFF" : "transparent")
                            border.width: on ? 1 : 0
                            border.color: DesktopTokens.focus

                            Row {
                                anchors.fill: parent
                                anchors.leftMargin: DesktopTokens.px(10)
                                anchors.rightMargin: DesktopTokens.px(12)
                                spacing: DesktopTokens.px(8)

                                Item {
                                    width: DesktopTokens.px(12)
                                    height: parent.height
                                    DesktopGlyph {
                                        anchors.centerIn: parent
                                        width: DesktopTokens.px(12)
                                        height: DesktopTokens.px(10)
                                        visible: on
                                        icon: "desktop-check-focus.svg"
                                    }
                                }

                                Text {
                                    width: parent.width - DesktopTokens.px(12) - DesktopTokens.px(8) - detailLabel.implicitWidth - DesktopTokens.px(8)
                                    height: parent.height
                                    text: modelData.label
                                    color: DesktopTokens.text
                                    font.family: DesktopTokens.bodyFont
                                    font.pixelSize: DesktopTokens.captionSize
                                    font.weight: Font.DemiBold
                                    elide: Text.ElideRight
                                    verticalAlignment: Text.AlignVCenter
                                }

                                Text {
                                    id: detailLabel
                                    height: parent.height
                                    text: modelData.detail || ""
                                    color: modelData.detailColor || DesktopTokens.textMuted
                                    font.family: DesktopTokens.monoFont
                                    font.pixelSize: DesktopTokens.tinySize
                                    font.weight: Font.DemiBold
                                    verticalAlignment: Text.AlignVCenter
                                }
                            }

                            HoverHandler {
                                id: rowHover
                                enabled: !parent.parent.unavailable
                                cursorShape: Qt.PointingHandCursor
                            }
                            TapHandler {
                                enabled: !parent.parent.unavailable
                                onTapped: { root.chosen(modelData.value); root.dismiss() }
                            }
                        }
                    }
                }

                Item {
                    visible: root.footer !== ""
                    width: parent.width
                    height: visible ? root.footerHeight : 0

                    Rectangle {
                        width: parent.width
                        height: 1
                        color: "#14FFFFFF"
                    }

                    Text {
                        anchors.left: parent.left
                        anchors.leftMargin: DesktopTokens.px(12)
                        anchors.verticalCenter: parent.verticalCenter
                        text: root.footer
                        color: "#75FFFFFF"
                        font.family: DesktopTokens.monoFont
                        font.pixelSize: DesktopTokens.tinySize
                        font.weight: Font.DemiBold
                    }

                    Row {
                        anchors.right: parent.right
                        anchors.rightMargin: DesktopTokens.px(12)
                        anchors.verticalCenter: parent.verticalCenter
                        spacing: DesktopTokens.px(12)
                        Text {
                            text: "ENTER  Pick"
                            color: "#66FFFFFF"
                            font.family: DesktopTokens.monoFont
                            font.pixelSize: DesktopTokens.tinySize
                            font.weight: Font.DemiBold
                        }
                        Text {
                            text: "ESC  Cancel"
                            color: "#66FFFFFF"
                            font.family: DesktopTokens.monoFont
                            font.pixelSize: DesktopTokens.tinySize
                            font.weight: Font.DemiBold
                        }
                    }
                }
            }
        }
    }

    Keys.onEscapePressed: event => { root.dismiss(); event.accepted = true }
}

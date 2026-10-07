import QtQuick
import QtQuick.Controls
import QtQuick.Layouts
import OpenNOW

Dialog {
    id: root
    objectName: "desktopQueueSelector"
    title: qsTr("Choose a server")
    header: null
    required property var selector
    required property var settingsStore
    property string selectedZoneId: ""
    readonly property bool compact: height < DesktopTokens.px(600)
    readonly property var selectedLocation: selector.zone(selectedZoneId)
    readonly property bool higherLatency: {
        if (!selectedLocation || selectedLocation.pingMs === null) return false
        return selector.locations.some(item => item.pingMs !== null && item.pingMs < selectedLocation.pingMs)
    }

    function choose(zoneId) {
        const hide = dontShow.checked
        selector.choose(zoneId)
        if (hide && !selector.opened) settingsStore.setSetting("hideQueueSelector", true)
    }

    function waitLabel(etaMs) {
        if (etaMs === null || etaMs === undefined) return qsTr("Wait unavailable")
        const minutes = Math.max(1, Math.ceil(etaMs / 60000))
        return minutes < 60 ? qsTr("~%1 min").arg(minutes)
            : qsTr("~%1 hr %2 min").arg(Math.floor(minutes / 60)).arg(minutes % 60)
    }

    parent: Overlay.overlay
    anchors.centerIn: parent
    width: Math.min(DesktopTokens.px(760), parent.width - DesktopTokens.px(32))
    height: Math.min(DesktopTokens.px(660), parent.height - DesktopTokens.px(32))
    padding: DesktopTokens.px(compact ? 16 : 24)
    modal: true
    focus: true
    closePolicy: Popup.CloseOnEscape
    visible: selector.opened
    onOpened: {
        dontShow.checked = false
        selectedZoneId = ""
        defaultButton.forceActiveFocus()
    }
    onClosed: if (selector.opened) selector.dismiss()
    background: Rectangle {
        radius: DesktopTokens.px(18)
        color: Theme.shell
        border.color: DesktopTokens.seam
    }

    Connections {
        target: root.selector
        function onLocationsChanged() { root.selectedZoneId = "" }
        function onRecommendedZoneIdChanged() { root.selectedZoneId = root.selector.recommendedZoneId }
    }

    contentItem: ColumnLayout {
        spacing: DesktopTokens.px(root.compact ? 8 : 16)
        RowLayout {
            Layout.fillWidth: true
            ColumnLayout {
                Layout.fillWidth: true
                spacing: DesktopTokens.px(4)
                Text {
                    text: root.title
                    color: DesktopTokens.textHigh
                    font.family: DesktopTokens.displayFont
                    font.pixelSize: DesktopTokens.titleSize
                    font.weight: Font.Bold
                }
                Text {
                    Layout.fillWidth: true
                    text: root.selector.gameTitle
                    textFormat: Text.PlainText
                    elide: Text.ElideRight
                    color: DesktopTokens.textMuted
                    font.family: DesktopTokens.bodyFont
                    font.pixelSize: DesktopTokens.bodySize
                }
            }
            DesktopSettingsButton {
                objectName: "queueSelectorCancel"
                text: qsTr("Cancel")
                onClicked: root.selector.dismiss()
            }
        }
        Text {
            Layout.fillWidth: true
            visible: !root.compact
            text: qsTr("Compare free-tier queues before you play. A shorter queue can mean higher latency.")
            color: DesktopTokens.textBody
            font.family: DesktopTokens.bodyFont
            font.pixelSize: DesktopTokens.captionSize
            wrapMode: Text.WordWrap
        }
        Rectangle {
            Layout.fillWidth: true
            visible: !root.compact
            height: 1
            color: DesktopTokens.seam
        }
        RowLayout {
            Layout.fillWidth: true
            Text {
                Layout.fillWidth: true
                text: root.selector.loading ? qsTr("Checking queues and latency…")
                    : qsTr("SERVER LOCATIONS")
                color: DesktopTokens.textMuted
                font.family: DesktopTokens.bodyFont
                font.pixelSize: DesktopTokens.smallSize
                font.weight: Font.Bold
                font.letterSpacing: 1
            }
            DesktopSettingsButton {
                objectName: "queueSelectorRefresh"
                text: qsTr("Refresh")
                compact: true
                enabled: !root.selector.loading
                onClicked: root.selector.refresh()
            }
        }
        Item {
            Layout.fillWidth: true
            Layout.fillHeight: true
            Layout.minimumHeight: DesktopTokens.px(80)
            Column {
                anchors.centerIn: parent
                width: parent.width
                spacing: DesktopTokens.px(12)
                visible: root.selector.loading || root.selector.error !== ""
                BusyIndicator {
                    anchors.horizontalCenter: parent.horizontalCenter
                    visible: root.selector.loading
                    running: visible
                }
                Text {
                    width: parent.width
                    text: root.selector.loading ? qsTr("Finding a balance between wait time and connection quality…")
                        : root.selector.error
                    color: DesktopTokens.textMuted
                    font.family: DesktopTokens.bodyFont
                    font.pixelSize: DesktopTokens.bodySize
                    wrapMode: Text.WordWrap
                    horizontalAlignment: Text.AlignHCenter
                }
            }
            ListView {
                id: locations
                objectName: "queueSelectorLocations"
                anchors.fill: parent
                clip: true
                spacing: DesktopTokens.px(8)
                visible: !root.selector.loading && root.selector.error === ""
                model: root.selector.locations
                ScrollBar.vertical: ScrollBar {}
                delegate: ItemDelegate {
                    id: option
                    required property var modelData
                    required property int index
                    objectName: "queueLocation_" + modelData.zoneId
                    width: ListView.view.width - DesktopTokens.px(12)
                    implicitHeight: Math.max(DesktopTokens.px(82), contentItem.implicitHeight + padding * 2)
                    padding: DesktopTokens.px(14)
                    readonly property var zones: modelData.zones || []
                    highlighted: root.selectedZoneId === modelData.zoneId
                        || zones.some(zone => zone.zoneId === root.selectedZoneId)
                    Accessible.role: Accessible.RadioButton
                    Accessible.checked: highlighted
                    Accessible.name: modelData.title + ", " + qsTr("%1 in queue").arg(modelData.queuePosition)
                        + ", " + (modelData.pingMs === null ? qsTr("Latency unavailable") : qsTr("%1 ms").arg(modelData.pingMs))
                    onClicked: if (!highlighted) root.selectedZoneId = modelData.zoneId
                    background: Rectangle {
                        radius: DesktopTokens.px(12)
                        color: option.highlighted ? DesktopTokens.raisedStrong : option.hovered ? DesktopTokens.raised : "transparent"
                        border.color: option.highlighted || option.activeFocus ? DesktopTokens.focus : DesktopTokens.seam
                        border.width: option.activeFocus ? 2 : 1
                    }
                    contentItem: RowLayout {
                        spacing: DesktopTokens.px(16)
                        ColumnLayout {
                            Layout.fillWidth: true
                            spacing: DesktopTokens.px(4)
                            Text {
                                Layout.fillWidth: true
                                text: option.modelData.title
                                textFormat: Text.PlainText
                                elide: Text.ElideRight
                                color: DesktopTokens.textHigh
                                font.family: DesktopTokens.bodyFont
                                font.pixelSize: DesktopTokens.bodySize
                                font.weight: Font.Bold
                            }
                            Text {
                                Layout.fillWidth: true
                                text: option.modelData.region + " · " + option.modelData.zoneId
                                    + (option.modelData.alternateCount > 0
                                        ? qsTr(" · %1 servers").arg(option.modelData.alternateCount + 1) : "")
                                textFormat: Text.PlainText
                                elide: Text.ElideRight
                                color: DesktopTokens.textMuted
                                font.family: DesktopTokens.bodyFont
                                font.pixelSize: DesktopTokens.smallSize
                            }
                            Text {
                                visible: option.modelData.zoneId === root.selector.recommendedZoneId
                                text: qsTr("Recommended")
                                color: DesktopTokens.focus
                                font.family: DesktopTokens.bodyFont
                                font.pixelSize: DesktopTokens.smallSize
                                font.weight: Font.Bold
                            }
                            Flow {
                                objectName: "queueLocationZones_" + option.modelData.zoneId
                                Layout.fillWidth: true
                                Layout.topMargin: DesktopTokens.px(6)
                                visible: option.highlighted && option.zones.length > 1
                                spacing: DesktopTokens.px(6)
                                Repeater {
                                    model: option.zones
                                    delegate: DesktopSettingsButton {
                                        required property var modelData
                                        objectName: "queueZone_" + modelData.zoneId
                                        compact: true
                                        primary: root.selectedZoneId === modelData.zoneId
                                        text: modelData.zoneId + " · " + qsTr("%1 in queue").arg(modelData.queuePosition)
                                            + " · " + (modelData.pingMs === null ? qsTr("No ping") : qsTr("%1 ms").arg(modelData.pingMs))
                                        onClicked: root.selectedZoneId = modelData.zoneId
                                    }
                                }
                            }
                        }
                        ColumnLayout {
                            spacing: DesktopTokens.px(4)
                            Text {
                                Layout.alignment: Qt.AlignRight
                                text: qsTr("%1 in queue").arg(option.modelData.queuePosition)
                                color: DesktopTokens.textHigh
                                font.family: DesktopTokens.bodyFont
                                font.pixelSize: DesktopTokens.captionSize
                                font.weight: Font.DemiBold
                            }
                            Text {
                                Layout.alignment: Qt.AlignRight
                                text: root.waitLabel(option.modelData.etaMs)
                                color: DesktopTokens.textMuted
                                font.family: DesktopTokens.bodyFont
                                font.pixelSize: DesktopTokens.smallSize
                            }
                        }
                        Text {
                            Layout.preferredWidth: DesktopTokens.px(90)
                            horizontalAlignment: Text.AlignRight
                            text: option.modelData.pingMs === null ? qsTr("No ping") : qsTr("%1 ms").arg(option.modelData.pingMs)
                            color: DesktopTokens.textHigh
                            font.family: DesktopTokens.monoFont
                            font.pixelSize: DesktopTokens.monoSize
                        }
                    }
                }
            }
        }
        Text {
            objectName: "queueSelectorLatencyWarning"
            Layout.fillWidth: true
            visible: root.higherLatency
            text: qsTr("This server has higher latency than your closest location. Gameplay may feel less responsive.")
            color: Theme.lightMode ? Theme.textMuted : DesktopTokens.amber
            font.family: DesktopTokens.bodyFont
            font.pixelSize: DesktopTokens.captionSize
            wrapMode: Text.WordWrap
        }
        Text {
            Layout.fillWidth: true
            visible: !root.compact
            text: qsTr("Queue estimates can change. Your selection applies to this launch only.")
            color: DesktopTokens.textMuted
            font.family: DesktopTokens.bodyFont
            font.pixelSize: DesktopTokens.smallSize
            wrapMode: Text.WordWrap
        }
        CheckBox {
            id: dontShow
            objectName: "queueSelectorDontShow"
            Layout.fillWidth: true
            text: qsTr("Don't show again")
            spacing: DesktopTokens.px(10)
            padding: DesktopTokens.px(4)
            indicator: Rectangle {
                x: dontShow.leftPadding
                y: (dontShow.height - height) / 2
                width: DesktopTokens.px(18)
                height: width
                radius: DesktopTokens.px(4)
                color: dontShow.checked ? Theme.focus : "transparent"
                border.color: dontShow.activeFocus ? Theme.focus : Theme.textMuted
                border.width: dontShow.activeFocus ? 2 : 1
                Text {
                    anchors.centerIn: parent
                    visible: dontShow.checked
                    text: "✓"
                    color: Theme.focusText
                    font.pixelSize: DesktopTokens.captionSize
                }
            }
            contentItem: Text {
                leftPadding: dontShow.indicator.width + dontShow.spacing
                text: dontShow.text
                color: DesktopTokens.textBody
                font.family: DesktopTokens.bodyFont
                font.pixelSize: DesktopTokens.captionSize
                verticalAlignment: Text.AlignVCenter
            }
        }
        RowLayout {
            Layout.fillWidth: true
            spacing: DesktopTokens.px(12)
            Text {
                text: qsTr("Powered by")
                color: DesktopTokens.textMuted
                font.family: DesktopTokens.bodyFont
                font.pixelSize: DesktopTokens.microSize
            }
            Button {
                objectName: "queueSelectorCredit"
                text: "PrintedWaste"
                Accessible.name: qsTr("Powered by PrintedWaste")
                padding: DesktopTokens.px(3)
                background: Rectangle {
                    color: "transparent"
                    border.color: parent.activeFocus ? DesktopTokens.focus : "transparent"
                    radius: DesktopTokens.px(3)
                }
                contentItem: Text {
                    text: parent.text
                    color: DesktopTokens.textMuted
                    font.family: DesktopTokens.bodyFont
                    font.pixelSize: DesktopTokens.microSize
                    font.underline: true
                }
                onClicked: Qt.openUrlExternally("https://printedwaste.com/gfn")
            }
            Item { Layout.fillWidth: true }
            DesktopSettingsButton {
                id: defaultButton
                objectName: "queueSelectorDefault"
                text: qsTr("Use default region")
                onClicked: root.choose("")
            }
            DesktopSettingsButton {
                objectName: "queueSelectorPlay"
                text: qsTr("Play here")
                primary: true
                enabled: root.selectedLocation !== null && !root.selector.loading
                onClicked: root.choose(root.selectedZoneId)
            }
        }
    }
}

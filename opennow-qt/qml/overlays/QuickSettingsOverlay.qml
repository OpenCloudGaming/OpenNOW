pragma ComponentBehavior: Bound

import QtQuick
import QtQuick.Controls
import OpenNOW

FocusScope {
    id: root
    width: 520
    height: 687
    focus: true
    Accessible.role: Accessible.Pane
    Accessible.name: qsTr("Quick settings")

    readonly property var bitrates: [25, 50, 75, 100, 150, 200]
    readonly property var controllers: ControllerInput.controllers || []
    readonly property string tier: String(ShellStore.subscription
        && ShellStore.subscription.membershipTier || qsTr("Ready"))

    function nextBitrate() {
        const current = Number(ShellStore.settings.maxBitrateMbps || 75)
        const index = root.bitrates.indexOf(current)
        ShellStore.setSetting("maxBitrateMbps",
            root.bitrates[(index + 1) % root.bitrates.length])
    }
    function openAllSettings() {
        AppController.showOverlay("")
        AppController.navigate("settings-streaming")
    }

    component QuickRow: Rectangle {
        id: row
        required property string title
        property string value: ""
        property bool statusDot: false
        property bool toggleVisible: false
        property bool checked: false
        property bool sliderVisible: false
        property real sliderProgress: 0
        signal triggered()

        width: 476
        height: 62
        radius: 24
        color: activeFocus ? "#FFFFFF" : "#14FFFFFF"
        activeFocusOnTab: enabled
        opacity: enabled ? 1 : 0.56
        Accessible.role: Accessible.Button
        Accessible.name: title

        Rectangle {
            anchors.fill: parent
            anchors.margins: -4
            radius: parent.radius + 4
            color: "transparent"
            border.width: 4
            border.color: DesktopTokens.focus
            visible: row.activeFocus
        }
        Rectangle {
            x: 18
            anchors.verticalCenter: parent.verticalCenter
            width: 8; height: 8; radius: 4
            visible: row.statusDot
            color: DesktopTokens.green
        }
        Text {
            x: row.statusDot ? 36 : 18
            width: 180
            anchors.verticalCenter: parent.verticalCenter
            text: row.title
            color: row.activeFocus ? DesktopTokens.shell : DesktopTokens.textHigh
            font.family: DesktopTokens.bodyFont
            font.pixelSize: 17
            font.weight: Font.ExtraBold
            elide: Text.ElideRight
        }
        Item {
            anchors.right: parent.right
            anchors.rightMargin: 18
            anchors.verticalCenter: parent.verticalCenter
            width: 230
            height: 36

            Row {
                anchors.right: parent.right
                anchors.verticalCenter: parent.verticalCenter
                spacing: 14
                visible: row.sliderVisible
                Rectangle {
                    width: 150; height: 10; radius: 5
                    anchors.verticalCenter: parent.verticalCenter
                    color: row.activeFocus ? "#240B0F1A" : "#1FFFFFFF"
                    Rectangle {
                        width: Math.max(10, parent.width * Math.max(0, Math.min(1, row.sliderProgress)))
                        height: parent.height; radius: parent.radius
                        color: DesktopTokens.focus
                    }
                }
                Text {
                    anchors.verticalCenter: parent.verticalCenter
                    text: row.value
                    color: row.activeFocus ? "#990B0F1A" : DesktopTokens.textMuted
                    font.family: DesktopTokens.bodyFont
                    font.pixelSize: 14
                    font.weight: Font.Bold
                }
            }
            Rectangle {
                anchors.right: parent.right
                anchors.verticalCenter: parent.verticalCenter
                width: 56; height: 32; radius: 16
                visible: row.toggleVisible
                color: row.checked ? DesktopTokens.mint
                    : row.activeFocus ? "#2E0B0F1A" : "#2EFFFFFF"
                Rectangle {
                    x: row.checked ? parent.width - width - 4 : 4
                    anchors.verticalCenter: parent.verticalCenter
                    width: 24; height: 24; radius: 12
                    color: row.checked || row.activeFocus ? "#FFFFFF" : "#B8FFFFFF"
                    Behavior on x { NumberAnimation { duration: AppController.reducedMotion ? 0 : 120; easing.type: Easing.OutCubic } }
                }
            }
            Text {
                anchors.right: parent.right
                anchors.verticalCenter: parent.verticalCenter
                visible: !row.sliderVisible && !row.toggleVisible
                width: parent.width
                horizontalAlignment: Text.AlignRight
                text: row.value
                color: row.activeFocus ? "#990B0F1A" : DesktopTokens.textMuted
                font.family: DesktopTokens.bodyFont
                font.pixelSize: 14
                font.weight: Font.Bold
                elide: Text.ElideRight
            }
        }
        HoverHandler { cursorShape: row.enabled ? Qt.PointingHandCursor : Qt.ForbiddenCursor }
        TapHandler { enabled: row.enabled; onTapped: { row.forceActiveFocus(); row.triggered() } }
        Keys.onPressed: event => {
            if ((event.key === Qt.Key_Return || event.key === Qt.Key_Enter
                    || event.key === Qt.Key_Space) && row.enabled) {
                row.triggered()
                event.accepted = true
            }
        }
    }

    component ControllerRow: Rectangle {
        id: controllerRow
        required property var controller
        width: 476
        height: 52
        radius: 20
        color: "#14FFFFFF"
        readonly property int battery: Number(controller.batteryPercent === undefined
            ? -1 : controller.batteryPercent)

        Rectangle {
            x: 14; anchors.verticalCenter: parent.verticalCenter
            width: 30; height: 30; radius: 15; color: "#FFFFFF"
            Text { anchors.centerIn: parent; text: "P" + Number(controllerRow.controller.slot || 1); color: DesktopTokens.shell; font.family: DesktopTokens.monoFont; font.pixelSize: 11; font.weight: Font.Black }
        }
        Text {
            x: 56; width: 265; anchors.verticalCenter: parent.verticalCenter
            text: String(controllerRow.controller.name || qsTr("Game controller"))
            color: DesktopTokens.textHigh
            font.family: DesktopTokens.bodyFont
            font.pixelSize: 15
            font.weight: Font.ExtraBold
            elide: Text.ElideRight
        }
        Text {
            anchors.right: parent.right; anchors.rightMargin: 48
            anchors.verticalCenter: parent.verticalCenter
            text: controllerRow.battery >= 0 ? controllerRow.battery + "%" : qsTr("Ready")
            color: DesktopTokens.textMuted
            font.family: DesktopTokens.bodyFont
            font.pixelSize: 13
            font.weight: Font.Bold
        }
        Rectangle {
            anchors.right: parent.right; anchors.rightMargin: 14
            anchors.verticalCenter: parent.verticalCenter
            width: 26; height: 14; radius: 4
            color: "transparent"; border.width: 2; border.color: DesktopTokens.textHigh
            Rectangle {
                x: 3; anchors.verticalCenter: parent.verticalCenter
                width: controllerRow.battery < 0 ? 14
                    : Math.max(2, Math.round(16 * controllerRow.battery / 100))
                height: 6; radius: 1
                color: controllerRow.battery >= 0 && controllerRow.battery < 30
                    ? DesktopTokens.yellow : DesktopTokens.mint
            }
        }
    }

    Rectangle {
        anchors.fill: parent
        radius: 36
        color: "#F50E1018"
        border.width: 1
        border.color: DesktopTokens.seam

        Column {
            x: 22; y: 22; width: 476; spacing: 14

            Item {
                width: parent.width; height: 46
                Column {
                    anchors.left: parent.left; anchors.verticalCenter: parent.verticalCenter
                    spacing: 2
                    Text { text: qsTr("Quick settings"); color: DesktopTokens.textHigh; font.family: DesktopTokens.displayFont; font.pixelSize: 24; font.weight: Font.Black }
                    Text { text: qsTr("Applies to your next launch"); color: DesktopTokens.textMuted; font.family: DesktopTokens.bodyFont; font.pixelSize: 13; font.weight: Font.Bold }
                }
                Rectangle {
                    anchors.right: parent.right; anchors.verticalCenter: parent.verticalCenter
                    width: tierText.implicitWidth + 28; height: 32; radius: 16
                    color: "#14FFFFFF"; border.width: 1; border.color: DesktopTokens.seam
                    Text { id: tierText; anchors.centerIn: parent; text: root.tier; color: DesktopTokens.textHigh; font.family: DesktopTokens.bodyFont; font.pixelSize: 13; font.weight: Font.ExtraBold }
                }
            }

            QuickRow {
                id: regionRow
                title: qsTr("Region")
                value: String(ShellStore.settings.region || qsTr("Automatic"))
                statusDot: true
                KeyNavigation.down: bitrateRow
                onTriggered: { AppController.showOverlay(""); AppController.navigate("settings-network") }
            }
            QuickRow {
                id: bitrateRow
                title: qsTr("Max bitrate")
                value: Number(ShellStore.settings.maxBitrateMbps || 75) + " Mbps"
                sliderVisible: true
                sliderProgress: Number(ShellStore.settings.maxBitrateMbps || 75) / 200
                KeyNavigation.up: regionRow; KeyNavigation.down: statsRow
                onTriggered: root.nextBitrate()
            }
            QuickRow {
                id: statsRow
                title: qsTr("Stats overlay")
                toggleVisible: true
                checked: Boolean(ShellStore.settings.showNativeStreamerStats)
                KeyNavigation.up: bitrateRow; KeyNavigation.down: syncRow
                onTriggered: ShellStore.setSetting("showNativeStreamerStats", !checked)
            }
            QuickRow {
                id: syncRow
                title: qsTr("Cloud G-Sync")
                toggleVisible: true
                checked: Boolean(ShellStore.settings.enableCloudGsync)
                KeyNavigation.up: statsRow; KeyNavigation.down: micRow
                onTriggered: ShellStore.setSetting("enableCloudGsync", !checked)
            }
            QuickRow {
                id: micRow
                title: qsTr("Microphone")
                value: ShellStore.microphoneLabel
                enabled: false
                KeyNavigation.up: syncRow
            }

            Item {
                width: parent.width; height: 20
                Text {
                    x: 14; anchors.verticalCenter: parent.verticalCenter
                    text: qsTr("CONTROLLERS · %1 CONNECTED").arg(AppController.controllerCount)
                    color: DesktopTokens.textMuted
                    font.family: DesktopTokens.monoFont
                    font.pixelSize: 11
                    font.weight: Font.Bold
                    font.letterSpacing: 0.8
                }
            }
            Repeater {
                model: root.controllers.slice(0, 2)
                delegate: ControllerRow { required property var modelData; controller: modelData }
            }
            Rectangle {
                id: controllerButton
                width: parent.width; height: 52; radius: 20
                visible: root.controllers.length === 0
                color: activeFocus ? "#FFFFFF" : "#14FFFFFF"
                activeFocusOnTab: visible
                Rectangle { x: 14; anchors.verticalCenter: parent.verticalCenter; width: 30; height: 30; radius: 15; color: controllerButton.activeFocus ? DesktopTokens.shell : "#FFFFFF"; Text { anchors.centerIn: parent; text: "A"; color: controllerButton.activeFocus ? "#FFFFFF" : DesktopTokens.shell; font.family: DesktopTokens.monoFont; font.pixelSize: 11; font.weight: Font.Black } }
                Text { x: 56; anchors.verticalCenter: parent.verticalCenter; text: qsTr("Connect player two"); color: controllerButton.activeFocus ? DesktopTokens.shell : DesktopTokens.textHigh; font.family: DesktopTokens.bodyFont; font.pixelSize: 15; font.weight: Font.ExtraBold }
                TapHandler { onTapped: { AppController.showOverlay(""); AppController.navigate("joining") } }
            }

            Item {
                width: parent.width; height: 35
                Rectangle { width: parent.width; height: 1; color: DesktopTokens.seam }
                Row {
                    y: 9; spacing: 18
                    Repeater {
                        model: [
                            { key: "A", label: qsTr("Adjust") },
                            { key: "Y", label: qsTr("All settings") },
                            { key: "RT", label: qsTr("Close") }
                        ]
                        delegate: Row {
                            required property var modelData
                            spacing: 8
                            Rectangle {
                                width: modelData.key === "RT" ? 31 : 26; height: 26
                                radius: modelData.key === "RT" ? 8 : 13
                                color: "#FFFFFF"
                                Text { anchors.centerIn: parent; text: modelData.key; color: DesktopTokens.shell; font.family: DesktopTokens.monoFont; font.pixelSize: 11; font.weight: Font.Black }
                            }
                            Text { anchors.verticalCenter: parent.verticalCenter; text: modelData.label; color: DesktopTokens.textMuted; font.family: DesktopTokens.bodyFont; font.pixelSize: 14; font.weight: Font.Bold }
                        }
                    }
                }
            }
        }
    }

    onVisibleChanged: if (visible) Qt.callLater(regionRow.forceActiveFocus)
    Keys.onPressed: event => {
        if (event.key === Qt.Key_Y) {
            root.openAllSettings()
            event.accepted = true
        }
    }
}

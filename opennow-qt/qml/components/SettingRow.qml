import QtQuick
import QtQuick.Controls
import OpenNOW

ItemDelegate {
    id: root
    property var rowData: ({})
    property string title: String(rowData.t || qsTr("Setting"))
    property string description: String(rowData.d || "")
    property string value: String(rowData.v || "")
    property bool currentItem: false
    readonly property string controlType: rowData.control || (rowData.info ? "info" : (rowData.toggle ? "toggle" : rowData.values ? "dropdown" : "button"))
    readonly property int selectedChoice: rowData.selectedIndex !== undefined
                                          ? Number(rowData.selectedIndex)
                                          : rowData.values ? rowData.values.indexOf(ShellStore.settings[rowData.key]) : -1
    highlighted: activeFocus || currentItem

    implicitHeight: Number(rowData.height || 70)
    focusPolicy: root.controlType === "info" ? Qt.NoFocus : Qt.StrongFocus
    Accessible.name: I18n.source(title, I18n.revision)
    Accessible.description: I18n.source(description, I18n.revision)
        + (value.length > 0 ? qsTr(". Current value: ") + I18n.source(value, I18n.revision) : "")
    Accessible.role: Accessible.Button
    padding: 0

    background: Rectangle {
        color: "transparent"
        radius: 22
        border.color: root.highlighted ? Theme.focus : "transparent"
        border.width: root.highlighted ? 3 : 0
        Behavior on border.color { ColorAnimation { duration: Theme.focusDuration } }
        Rectangle {
            visible: !root.highlighted
            x: 18; anchors.bottom: parent.bottom
            width: parent.width - 36; height: 1
            color: Theme.seam
        }
    }

    contentItem: Item {
        Column {
            visible: root.controlType !== "profile" && root.controlType !== "controllers" && root.controlType !== "region"
            anchors.left: parent.left
            anchors.leftMargin: 18
            anchors.right: trailing.left
            anchors.rightMargin: 24
            anchors.verticalCenter: parent.verticalCenter
            Text {
                width: parent.width
                text: I18n.source(root.title, I18n.revision)
                color: Theme.label
                font.family: Theme.bodyFont
                font.pixelSize: 18
                font.weight: Font.ExtraBold
                elide: Text.ElideRight
            }
            Text {
                width: parent.width
                visible: root.description.length > 0
                text: I18n.source(root.description, I18n.revision)
                color: Theme.textMuted
                font.family: Theme.bodyFont
                font.pixelSize: 13
                font.weight: Font.DemiBold
                elide: Text.ElideRight
            }
        }
        Item {
            id: trailing
            visible: root.controlType !== "profile" && root.controlType !== "controllers" && root.controlType !== "region"
            anchors.right: parent.right
            anchors.rightMargin: 18
            anchors.verticalCenter: parent.verticalCenter
            width: segments.visible ? segments.implicitWidth
                 : colors.visible ? colors.implicitWidth
                 : sliderVisual.visible ? 338
                 : toggleVisual.visible ? 60
                 : infoValue.visible ? infoValue.implicitWidth
                 : valuePill.width
            height: 42

            Text {
                id: infoValue
                visible: root.controlType === "info"
                anchors.right: parent.right
                anchors.verticalCenter: parent.verticalCenter
                width: Math.min(420, implicitWidth)
                text: I18n.source(root.value, I18n.revision)
                color: Theme.textMuted
                font.family: Theme.bodyFont
                font.pixelSize: 15
                font.weight: Font.Bold
                horizontalAlignment: Text.AlignRight
                elide: Text.ElideRight
            }

            Row {
                id: segments
                anchors.centerIn: parent
                spacing: 6
                visible: root.controlType === "segments"
                Repeater {
                    model: root.rowData.segmentLabels || root.rowData.labels || []
                    Rectangle {
                        required property string modelData
                        required property int index
                        readonly property bool available: (root.rowData.disabledValues || []).indexOf(
                            root.rowData.values ? root.rowData.values[index] : undefined) < 0
                        height: 38
                        width: segmentLabel.implicitWidth + 28
                        radius: 19
                        color: index === root.selectedChoice ? Theme.face : "transparent"
                        border.color: index === root.selectedChoice ? "transparent" : Theme.seam
                        border.width: index === root.selectedChoice ? 0 : 1
                        opacity: available ? 1 : 0.38
                        Text {
                            id: segmentLabel
                            anchors.centerIn: parent
                            text: I18n.source(modelData, I18n.revision)
                            color: index === root.selectedChoice ? Theme.faceText : Theme.label
                            font.family: Theme.bodyFont
                            font.pixelSize: 14
                            font.weight: Font.ExtraBold
                        }
                    }
                }
            }

            Row {
                id: colors
                anchors.centerIn: parent
                spacing: 8
                visible: root.controlType === "colors"
                Repeater {
                    model: root.rowData.colors || []
                    Rectangle {
                        required property color modelData
                        required property int index
                        width: 36; height: 36; radius: 18
                        color: modelData
                        border.color: index === root.selectedChoice ? Theme.face : "transparent"
                        border.width: index === root.selectedChoice ? 4 : 0
                        Rectangle { anchors.fill: parent; anchors.margins: -3; radius: width / 2; color: "transparent"; border.color: index === root.selectedChoice ? Theme.focus : "transparent"; border.width: 2 }
                    }
                }
            }

            Row {
                id: sliderVisual
                visible: root.controlType === "slider"
                anchors.centerIn: parent
                spacing: 14
                Rectangle {
                    anchors.verticalCenter: parent.verticalCenter
                    width: 240; height: 10; radius: 5
                    color: Qt.rgba(1, 1, 1, 0.14)
                    Rectangle {
                        width: parent.width * Math.max(0, Math.min(1, Number(root.rowData.sliderPercent || 0)))
                        height: parent.height; radius: parent.radius; color: Theme.focus
                        Behavior on width { NumberAnimation { duration: Theme.focusDuration; easing.type: Easing.OutCubic } }
                    }
                }
                Text {
                    width: 84
                    text: I18n.source(root.value, I18n.revision)
                    color: Theme.label
                    font.family: Theme.bodyFont
                    font.pixelSize: 15
                    font.weight: Font.ExtraBold
                }
            }

            Rectangle {
                id: toggleVisual
                visible: root.controlType === "toggle"
                anchors.centerIn: parent
                width: 60; height: 34; radius: 17
                readonly property bool toggleOn: root.rowData.toggleState !== undefined ? Boolean(root.rowData.toggleState) : Boolean(ShellStore.settings[root.rowData.key])
                color: toggleOn ? Theme.mint : Qt.rgba(1, 1, 1, 0.18)
                Rectangle {
                    width: 26; height: 26; radius: 13
                    x: toggleVisual.toggleOn ? 30 : 4
                    anchors.verticalCenter: parent.verticalCenter
                    color: Theme.face
                    Behavior on x { NumberAnimation { duration: Theme.focusDuration; easing.type: Easing.OutCubic } }
                }
            }

            Rectangle {
                id: valuePill
                visible: root.controlType === "dropdown" || root.controlType === "button"
                anchors.right: parent.right
                anchors.verticalCenter: parent.verticalCenter
                width: Math.min(420, valueLabel.implicitWidth + (root.controlType === "dropdown" ? 58 : 38))
                height: 42
                radius: 21
                color: root.rowData.danger ? Qt.rgba(1, 0.3, 0.3, 0.12) : Theme.glassStrong
                border.color: root.rowData.danger ? Theme.coral : Theme.seam
                border.width: 1
                Text {
                    id: valueLabel
                    anchors.left: parent.left
                    anchors.leftMargin: 19
                    anchors.right: chevron.visible ? chevron.left : parent.right
                    anchors.rightMargin: chevron.visible ? 10 : 19
                    anchors.verticalCenter: parent.verticalCenter
                    text: I18n.source(root.value, I18n.revision)
                    color: root.rowData.danger ? Theme.coral : Theme.label
                    font.family: Theme.bodyFont
                    font.pixelSize: 15
                    font.weight: Font.ExtraBold
                    elide: Text.ElideRight
                }
                Text {
                    id: chevron
                    visible: root.controlType === "dropdown"
                    anchors.right: parent.right
                    anchors.rightMargin: 17
                    anchors.verticalCenter: parent.verticalCenter
                    text: "⌄"
                    color: Theme.textMuted
                    font.family: Theme.bodyFont
                    font.pixelSize: 18
                    font.weight: Font.Black
                }
            }
        }

        Item {
            visible: root.controlType === "profile"
            anchors.fill: parent
            anchors.leftMargin: 8
            anchors.rightMargin: 8
            Rectangle {
                x: 0; anchors.verticalCenter: parent.verticalCenter
                width: 88; height: 88; radius: 44
                color: Theme.violet
                border.color: Theme.face; border.width: 3
                Text { anchors.centerIn: parent; text: root.rowData.initial || "O"; color: Theme.faceText; font.family: Theme.displayFont; font.pixelSize: 34; font.weight: Font.Black }
            }
            Column {
                x: 112; anchors.verticalCenter: parent.verticalCenter; width: parent.width - 340; spacing: 5
                Row {
                    spacing: 12
                    Text { text: root.rowData.name || qsTr("OpenNOW profile"); color: Theme.label; font.family: Theme.bodyFont; font.pixelSize: 24; font.weight: Font.Black }
                    Rectangle { anchors.verticalCenter: parent.verticalCenter; width: tierText.implicitWidth + 18; height: 28; radius: 14; color: Theme.yellow
                        Text { id: tierText; anchors.centerIn: parent; text: root.rowData.tier || "—"; color: Theme.faceText; font.family: Theme.bodyFont; font.pixelSize: 12; font.weight: Font.Black }
                    }
                }
                Text { width: parent.width; text: root.rowData.subtitle || qsTr("NVIDIA account"); color: Theme.textMuted; font.family: Theme.bodyFont; font.pixelSize: 15; font.weight: Font.DemiBold; elide: Text.ElideRight }
                Text { width: parent.width; text: root.rowData.meta || qsTr("This PC"); color: Theme.textMuted; font.family: Theme.bodyFont; font.pixelSize: 13; font.weight: Font.DemiBold; elide: Text.ElideRight }
            }
            Rectangle {
                anchors.right: parent.right; anchors.rightMargin: 10; anchors.verticalCenter: parent.verticalCenter
                width: 194; height: 42; radius: 21; color: Theme.glassStrong; border.color: Theme.seam; border.width: 1
                Text { anchors.centerIn: parent; text: root.rowData.v || qsTr("Manage account"); color: Theme.label; font.family: Theme.bodyFont; font.pixelSize: 15; font.weight: Font.ExtraBold }
            }
        }

        Row {
            visible: root.controlType === "controllers"
            anchors.fill: parent
            spacing: 12
            Repeater {
                model: root.rowData.controllers || []
                Rectangle {
                    required property var modelData
                    width: (root.width - 12) / 2; height: 84; anchors.verticalCenter: parent.verticalCenter
                    radius: 26; color: Theme.glassStrong; border.color: Theme.seam; border.width: 1
                    Rectangle { x: 16; anchors.verticalCenter: parent.verticalCenter; width: 48; height: 48; radius: 24; color: modelData.connected ? Theme.face : Theme.glass
                        Text { anchors.centerIn: parent; text: String(modelData.slot || "2"); color: modelData.connected ? Theme.faceText : Theme.textMuted; font.family: Theme.bodyFont; font.pixelSize: 18; font.weight: Font.Black }
                    }
                    Image { x: 76; anchors.verticalCenter: parent.verticalCenter; width: 36; height: 36; source: "qrc:/qt/qml/OpenNOW/res/icons/nav-controller.svg"; opacity: modelData.connected ? 1 : 0.35 }
                    Column { x: 128; anchors.verticalCenter: parent.verticalCenter; width: parent.width - 220
                        Text { width: parent.width; text: modelData.name; color: modelData.connected ? Theme.label : Theme.textMuted; font.family: Theme.bodyFont; font.pixelSize: 17; font.weight: Font.ExtraBold; elide: Text.ElideRight }
                        Text { width: parent.width; text: modelData.connected ? qsTr("Connected") : qsTr("Press a button to join"); color: Theme.textMuted; font.family: Theme.bodyFont; font.pixelSize: 13; font.weight: Font.DemiBold; elide: Text.ElideRight }
                    }
                    Rectangle { anchors.right: parent.right; anchors.rightMargin: 16; anchors.verticalCenter: parent.verticalCenter; width: batteryLabel.implicitWidth + 18; height: 30; radius: 15; color: modelData.connected ? Theme.glass : "transparent"; border.color: Theme.seam; border.width: modelData.connected ? 1 : 0
                        Text { id: batteryLabel; anchors.centerIn: parent; text: modelData.battery; color: Theme.label; font.family: Theme.bodyFont; font.pixelSize: 13; font.weight: Font.ExtraBold }
                    }
                }
            }
        }
    }
}

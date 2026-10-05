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
    property bool ringVisible: activeFocus || currentItem
    property bool parked: false
    readonly property string controlType: rowData.control || (rowData.info ? "info" : (rowData.toggle ? "toggle" : rowData.values ? "dropdown" : "button"))
    readonly property int selectedChoice: rowData.selectedIndex !== undefined
                                          ? Number(rowData.selectedIndex)
                                          : rowData.values ? rowData.values.indexOf(ShellStore.settings[rowData.key]) : -1
    readonly property bool cardRow: controlType === "profile" || controlType === "controllers" || controlType === "region"
    readonly property bool showRing: ringVisible && !parked
    readonly property int cardGap: controlType === "profile" ? 8 : 0
    readonly property int cardRadius: controlType === "profile" ? 28 : 24
    highlighted: showRing

    implicitHeight: cardRow ? Number(rowData.height || 120)
        : Math.max(Number(rowData.height || 84), textColumn.implicitHeight + 24)
    focusPolicy: root.controlType === "info" ? Qt.NoFocus : Qt.StrongFocus
    Accessible.name: I18n.source(title, I18n.revision)
    Accessible.description: I18n.source(description, I18n.revision)
        + (value.length > 0 ? qsTr(". Current value: ") + I18n.source(value, I18n.revision) : "")
    Accessible.role: Accessible.Button
    padding: 0

    function choiceDisabled(index) {
        return (root.rowData.disabledValues || []).indexOf(
            root.rowData.values ? root.rowData.values[index] : undefined) >= 0
    }

    background: Item {
        Rectangle {
            anchors.fill: parent
            anchors.margins: -5
            anchors.bottomMargin: root.cardGap - 5
            radius: root.cardRadius + 5
            color: "transparent"
            border.width: 5
            border.color: Qt.rgba(Theme.focus.r, Theme.focus.g, Theme.focus.b, 0.4)
            opacity: root.showRing ? 1 : 0
            Behavior on opacity { NumberAnimation { duration: Theme.focusDuration; easing.type: Easing.OutCubic } }
        }
        Rectangle {
            anchors.fill: parent
            anchors.bottomMargin: root.cardGap
            radius: root.cardRadius
            color: root.showRing ? Qt.rgba(Theme.face.r, Theme.face.g, Theme.face.b, 0.10)
                : root.parked ? Qt.rgba(Theme.face.r, Theme.face.g, Theme.face.b, 0.08)
                : root.controlType === "profile" ? Qt.rgba(Theme.face.r, Theme.face.g, Theme.face.b, 0.06)
                : "transparent"
            border.width: root.showRing ? 3 : root.parked ? 2 : 0
            border.color: root.showRing ? Theme.face : Qt.rgba(Theme.face.r, Theme.face.g, Theme.face.b, 0.7)
            Behavior on color { ColorAnimation { duration: Theme.focusDuration } }
        }
        Rectangle {
            visible: !root.showRing && !root.parked && !root.cardRow
            x: 18; anchors.bottom: parent.bottom
            width: parent.width - 36; height: 1
            color: Qt.rgba(Theme.face.r, Theme.face.g, Theme.face.b, 0.09)
        }
    }

    contentItem: Item {
        Item {
            id: textColumn
            visible: !root.cardRow
            anchors.left: parent.left
            anchors.leftMargin: 25
            anchors.right: trailing.left
            anchors.rightMargin: 24
            anchors.verticalCenter: parent.verticalCenter
            anchors.verticalCenterOffset: 1
            implicitHeight: descriptionText.visible ? 30 + Math.max(1, descriptionText.lineCount) * 20 : 26
            height: implicitHeight
            Text {
                objectName: "consoleSettingTitle"
                width: parent.width
                height: 26
                verticalAlignment: Text.AlignVCenter
                text: I18n.source(root.title, I18n.revision)
                color: root.controlType === "info" ? Theme.textMuted : Theme.label
                font.family: Theme.displayFont
                font.pixelSize: 21
                font.weight: Font.Black
                elide: Text.ElideRight
            }
            Text {
                id: descriptionText
                objectName: "consoleSettingDescription"
                y: 29
                width: parent.width
                visible: root.description.length > 0
                text: I18n.source(root.description, I18n.revision)
                color: Theme.textMuted
                font.family: Theme.bodyFont
                font.pixelSize: 16
                font.weight: Font.DemiBold
                lineHeightMode: Text.FixedHeight
                lineHeight: 20
                wrapMode: Text.WordWrap
                maximumLineCount: 2
                elide: Text.ElideRight
            }
        }

        Item {
            id: trailing
            visible: !root.cardRow
            anchors.right: parent.right
            anchors.rightMargin: 25
            anchors.verticalCenter: parent.verticalCenter
            anchors.verticalCenterOffset: 1
            width: segments.visible ? segments.implicitWidth
                 : colors.visible ? colors.implicitWidth
                 : sliderVisual.visible ? sliderVisual.implicitWidth
                 : toggleVisual.visible ? 64
                 : cycler.visible ? cycler.implicitWidth
                 : infoValue.visible ? infoValue.width
                 : valuePill.width
            height: 48

            Text {
                id: infoValue
                visible: root.controlType === "info" && root.value.length > 0
                anchors.right: parent.right
                anchors.verticalCenter: parent.verticalCenter
                width: Math.min(460, implicitWidth)
                text: I18n.source(root.value, I18n.revision)
                color: Theme.textMuted
                font.family: Theme.displayFont
                font.pixelSize: 18
                font.weight: Font.ExtraBold
                horizontalAlignment: Text.AlignRight
                elide: Text.ElideRight
            }

            Row {
                id: segments
                anchors.right: parent.right
                anchors.verticalCenter: parent.verticalCenter
                spacing: 6
                visible: root.controlType === "segments"
                Repeater {
                    model: root.rowData.segmentLabels || root.rowData.labels || []
                    Rectangle {
                        id: segment
                        required property string modelData
                        required property int index
                        readonly property bool selected: index === root.selectedChoice
                        readonly property bool available: !root.choiceDisabled(index)
                        height: 44
                        width: segmentRow.implicitWidth + 36
                        radius: 22
                        color: selected ? Theme.face : "transparent"
                        border.color: selected ? "transparent" : Qt.rgba(Theme.face.r, Theme.face.g, Theme.face.b, 0.18)
                        border.width: selected ? 0 : 1
                        Row {
                            id: segmentRow
                            anchors.centerIn: parent
                            spacing: 6
                            LockGlyph {
                                visible: !segment.available
                                anchors.verticalCenter: parent.verticalCenter
                                ink: Theme.textMuted
                            }
                            Text {
                                anchors.verticalCenter: parent.verticalCenter
                                text: I18n.source(segment.modelData, I18n.revision)
                                color: segment.selected ? Theme.faceText
                                    : segment.available ? Theme.label : Theme.textMuted
                                font.family: Theme.displayFont
                                font.pixelSize: 16
                                font.weight: Font.Black
                            }
                        }
                    }
                }
            }

            Row {
                id: colors
                anchors.right: parent.right
                anchors.verticalCenter: parent.verticalCenter
                spacing: 10
                visible: root.controlType === "colors"
                Repeater {
                    model: root.rowData.colors || []
                    Item {
                        id: swatch
                        required property color modelData
                        required property int index
                        width: 36; height: 36
                        Rectangle {
                            anchors.fill: parent
                            anchors.margins: -6
                            radius: width / 2
                            color: Theme.face
                            visible: swatch.index === root.selectedChoice
                            Rectangle { anchors.fill: parent; anchors.margins: 3; radius: width / 2; color: Theme.shell }
                        }
                        Rectangle { anchors.fill: parent; radius: 18; color: swatch.modelData }
                    }
                }
            }

            Row {
                id: sliderVisual
                visible: root.controlType === "slider"
                anchors.right: parent.right
                anchors.verticalCenter: parent.verticalCenter
                spacing: 16
                Item {
                    id: sliderTrack
                    anchors.verticalCenter: parent.verticalCenter
                    width: 280; height: 22
                    readonly property real fraction: Math.max(0, Math.min(1, Number(root.rowData.sliderPercent || 0)))
                    Rectangle {
                        anchors.verticalCenter: parent.verticalCenter
                        width: parent.width; height: 10; radius: 5
                        color: Qt.rgba(Theme.face.r, Theme.face.g, Theme.face.b, 0.16)
                        Rectangle {
                            width: parent.width * sliderTrack.fraction
                            height: parent.height; radius: parent.radius
                            color: Theme.focus
                            Behavior on width { NumberAnimation { duration: Theme.focusDuration; easing.type: Easing.OutCubic } }
                        }
                    }
                    Rectangle {
                        x: Math.max(0, parent.width * sliderTrack.fraction - width / 2)
                        anchors.verticalCenter: parent.verticalCenter
                        width: 22; height: 22; radius: 11
                        color: Theme.face
                        Behavior on x { NumberAnimation { duration: Theme.focusDuration; easing.type: Easing.OutCubic } }
                    }
                }
                Text {
                    anchors.verticalCenter: parent.verticalCenter
                    width: 96
                    text: I18n.source(root.value, I18n.revision)
                    color: Theme.label
                    font.family: Theme.displayFont
                    font.pixelSize: 19
                    font.weight: Font.Black
                    elide: Text.ElideRight
                }
            }

            Rectangle {
                id: toggleVisual
                visible: root.controlType === "toggle"
                anchors.right: parent.right
                anchors.verticalCenter: parent.verticalCenter
                width: 64; height: 36; radius: 18
                readonly property bool toggleOn: root.rowData.toggleState !== undefined ? Boolean(root.rowData.toggleState) : Boolean(ShellStore.settings[root.rowData.key])
                color: toggleOn ? Theme.mint : Qt.rgba(Theme.face.r, Theme.face.g, Theme.face.b, 0.18)
                Behavior on color { ColorAnimation { duration: Theme.focusDuration } }
                Rectangle {
                    width: 28; height: 28; radius: 14
                    x: toggleVisual.toggleOn ? 32 : 4
                    anchors.verticalCenter: parent.verticalCenter
                    color: Theme.face
                    Behavior on x { NumberAnimation { duration: Theme.focusDuration; easing.type: Easing.OutCubic } }
                }
            }

            Row {
                id: cycler
                visible: root.controlType === "cycler"
                anchors.right: parent.right
                anchors.verticalCenter: parent.verticalCenter
                spacing: 6
                readonly property int count: (root.rowData.values || []).length
                Rectangle {
                    anchors.verticalCenter: parent.verticalCenter
                    width: 44; height: 44; radius: 22
                    color: root.showRing ? Theme.face : "transparent"
                    border.width: root.showRing ? 0 : 1
                    border.color: Qt.rgba(Theme.face.r, Theme.face.g, Theme.face.b, 0.22)
                    Text {
                        anchors.centerIn: parent
                        text: "‹"
                        color: root.showRing ? Theme.faceText : Theme.label
                        font.family: Theme.displayFont
                        font.pixelSize: 28
                        font.weight: Font.Black
                    }
                }
                Column {
                    anchors.verticalCenter: parent.verticalCenter
                    width: 170
                    spacing: 4
                    Text {
                        width: parent.width
                        height: 24
                        verticalAlignment: Text.AlignVCenter
                        horizontalAlignment: Text.AlignHCenter
                        text: root.value !== "" ? I18n.source(root.value, I18n.revision) : "—"
                        color: Theme.label
                        elide: Text.ElideRight
                        font.family: Theme.displayFont
                        font.pixelSize: 20
                        font.weight: Font.Black
                    }
                    Row {
                        anchors.horizontalCenter: parent.horizontalCenter
                        spacing: 5
                        Repeater {
                            model: cycler.count
                            Rectangle {
                                required property int index
                                width: index === root.selectedChoice ? 18 : 6
                                height: 6; radius: 3
                                color: index === root.selectedChoice ? Theme.face
                                    : Qt.rgba(Theme.face.r, Theme.face.g, Theme.face.b, 0.3)
                            }
                        }
                    }
                }
                Rectangle {
                    anchors.verticalCenter: parent.verticalCenter
                    width: 44; height: 44; radius: 22
                    color: root.showRing ? Theme.face : "transparent"
                    border.width: root.showRing ? 0 : 1
                    border.color: Qt.rgba(Theme.face.r, Theme.face.g, Theme.face.b, 0.22)
                    Text {
                        anchors.centerIn: parent
                        text: "›"
                        color: root.showRing ? Theme.faceText : Theme.label
                        font.family: Theme.displayFont
                        font.pixelSize: 28
                        font.weight: Font.Black
                    }
                }
            }

            Rectangle {
                id: valuePill
                visible: (root.controlType === "dropdown" || root.controlType === "button")
                    && (root.value !== "" || Boolean(root.rowData.shortcut))
                anchors.right: parent.right
                anchors.verticalCenter: parent.verticalCenter
                width: Math.min(460, (root.rowData.shortcut ? shortcutValue.implicitWidth : valueLabel.implicitWidth) + 66)
                height: 48
                radius: 24
                color: root.rowData.danger ? Qt.rgba(Theme.coral.r, Theme.coral.g, Theme.coral.b, 0.10)
                    : Qt.rgba(Theme.face.r, Theme.face.g, Theme.face.b, 0.10)
                border.color: root.rowData.danger ? Qt.rgba(Theme.coral.r, Theme.coral.g, Theme.coral.b, 0.6)
                    : Qt.rgba(Theme.face.r, Theme.face.g, Theme.face.b, 0.18)
                border.width: root.rowData.danger ? 1.5 : 1
                Text {
                    id: valueLabel
                    visible: !root.rowData.shortcut
                    anchors.left: parent.left
                    anchors.leftMargin: 20
                    anchors.right: chevron.left
                    anchors.rightMargin: 12
                    anchors.verticalCenter: parent.verticalCenter
                    text: root.value !== "" ? I18n.source(root.value, I18n.revision) : "—"
                    color: root.rowData.danger ? Theme.coral : Theme.label
                    font.family: Theme.displayFont
                    font.pixelSize: 18
                    font.weight: Font.Black
                    elide: Text.ElideRight
                }
                KeyboardGlyph {
                    id: shortcutValue
                    visible: Boolean(root.rowData.shortcut)
                    anchors.left: parent.left
                    anchors.leftMargin: 16
                    anchors.verticalCenter: parent.verticalCenter
                    shortcut: visible ? root.value : ""
                    keySize: 26
                }
                Text {
                    id: chevron
                    width: 18
                    horizontalAlignment: Text.AlignHCenter
                    anchors.right: parent.right
                    anchors.rightMargin: 16
                    anchors.verticalCenter: parent.verticalCenter
                    text: "›"
                    color: root.rowData.danger ? Theme.coral : Theme.label
                    font.family: Theme.displayFont
                    font.pixelSize: 26
                    font.weight: Font.Black
                }
            }
        }

        Item {
            visible: root.controlType === "profile"
            anchors.fill: parent
            anchors.leftMargin: 22
            anchors.rightMargin: 22
            anchors.bottomMargin: root.cardGap
            Rectangle {
                id: avatar
                anchors.verticalCenter: parent.verticalCenter
                width: 76; height: 76; radius: 38
                color: Theme.violet
                border.color: Theme.face; border.width: 3
                Text { anchors.centerIn: parent; text: root.rowData.initial || "O"; color: Theme.faceText; font.family: Theme.displayFont; font.pixelSize: 32; font.weight: Font.Black }
            }
            Column {
                anchors.left: avatar.right; anchors.leftMargin: 22
                anchors.right: profileAction.left; anchors.rightMargin: 22
                anchors.verticalCenter: parent.verticalCenter
                spacing: 4
                Row {
                    height: 34
                    spacing: 12
                    Text { height: 34; verticalAlignment: Text.AlignVCenter; text: root.rowData.name || qsTr("OpenNOW profile"); color: Theme.label; font.family: Theme.displayFont; font.pixelSize: 28; font.weight: Font.Black }
                    Rectangle {
                        anchors.verticalCenter: parent.verticalCenter
                        width: tierText.implicitWidth + 20; height: 24; radius: 10
                        color: Qt.rgba(Theme.violet.r, Theme.violet.g, Theme.violet.b, 0.16)
                        Text { id: tierText; anchors.centerIn: parent; text: root.rowData.tier || "—"; color: Theme.violet; font.family: Theme.monoFont; font.pixelSize: 13; font.weight: Font.Bold; font.letterSpacing: 1 }
                    }
                }
                Text { width: parent.width; height: 22; verticalAlignment: Text.AlignVCenter; text: root.rowData.subtitle || qsTr("NVIDIA account"); color: Theme.textMuted; font.family: Theme.bodyFont; font.pixelSize: 17; font.weight: Font.Bold; elide: Text.ElideRight }
                Text { width: parent.width; height: 18; verticalAlignment: Text.AlignVCenter; text: root.rowData.meta || qsTr("This PC"); color: Theme.textMuted; font.family: Theme.bodyFont; font.pixelSize: 14; font.weight: Font.DemiBold; elide: Text.ElideRight }
            }
            Rectangle {
                id: profileAction
                anchors.right: parent.right; anchors.verticalCenter: parent.verticalCenter
                width: profileActionText.implicitWidth + 66; height: 48; radius: 24
                color: Qt.rgba(Theme.face.r, Theme.face.g, Theme.face.b, 0.10)
                border.color: Qt.rgba(Theme.face.r, Theme.face.g, Theme.face.b, 0.18); border.width: 1
                Text { id: profileActionText; x: 20; anchors.verticalCenter: parent.verticalCenter; text: root.rowData.v || qsTr("Manage account"); color: Theme.label; font.family: Theme.displayFont; font.pixelSize: 18; font.weight: Font.Black }
                Text { width: 18; horizontalAlignment: Text.AlignHCenter; anchors.right: parent.right; anchors.rightMargin: 16; anchors.verticalCenter: parent.verticalCenter; text: "›"; color: Theme.label; font.family: Theme.displayFont; font.pixelSize: 26; font.weight: Font.Black }
            }
        }

        Row {
            visible: root.controlType === "controllers"
            anchors.fill: parent
            anchors.topMargin: 6
            anchors.bottomMargin: 14
            spacing: 12
            Repeater {
                model: root.rowData.controllers || []
                Rectangle {
                    id: controllerCard
                    required property var modelData
                    readonly property int cardCount: Math.max(1, (root.rowData.controllers || []).length)
                    readonly property int inset: modelData.connected ? 18 : 19
                    width: (parent.width - 12 * (cardCount - 1)) / cardCount
                    height: parent.height
                    radius: 24
                    color: modelData.connected ? Qt.rgba(Theme.face.r, Theme.face.g, Theme.face.b, 0.07) : "transparent"
                    border.color: Qt.rgba(Theme.face.r, Theme.face.g, Theme.face.b, 0.22)
                    border.width: modelData.connected ? 0 : 1
                    Rectangle {
                        id: controllerSlot
                        x: controllerCard.inset; y: controllerCard.inset
                        width: 40; height: 40; radius: 20
                        color: controllerCard.modelData.connected ? Theme.mint : "transparent"
                        border.width: controllerCard.modelData.connected ? 0 : 2
                        border.color: Qt.rgba(Theme.face.r, Theme.face.g, Theme.face.b, 0.3)
                        Text { anchors.centerIn: parent; text: qsTr("P%1").arg(controllerCard.modelData.slot || 1); color: controllerCard.modelData.connected ? Theme.faceText : Theme.textMuted; font.family: Theme.displayFont; font.pixelSize: 15; font.weight: Font.Black }
                    }
                    Text {
                        anchors.right: parent.right; anchors.rightMargin: controllerCard.inset
                        anchors.verticalCenter: controllerSlot.verticalCenter
                        text: controllerCard.modelData.battery || ""
                        color: Theme.label
                        font.family: Theme.monoFont; font.pixelSize: 14; font.weight: Font.Bold
                    }
                    Text {
                        x: controllerCard.inset; y: controllerCard.inset + 52
                        width: parent.width - 2 * controllerCard.inset
                        height: 21
                        verticalAlignment: Text.AlignVCenter
                        text: controllerCard.modelData.connected ? controllerCard.modelData.name : qsTr("Press a button to join")
                        color: controllerCard.modelData.connected ? Theme.label : Theme.textMuted
                        font.family: Theme.displayFont; font.pixelSize: 16; font.weight: Font.ExtraBold
                        elide: Text.ElideRight
                    }
                }
            }
        }
    }

    component LockGlyph: Item {
        property color ink: Theme.textMuted
        width: 12; height: 14
        Rectangle { x: 2; y: 0; width: 8; height: 8; radius: 4; color: "transparent"; border.width: 2; border.color: parent.ink }
        Rectangle { y: 5; width: 12; height: 9; radius: 2; color: parent.ink }
    }
}

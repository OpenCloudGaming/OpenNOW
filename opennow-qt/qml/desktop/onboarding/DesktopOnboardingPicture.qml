pragma ComponentBehavior: Bound
import QtQuick
import QtQuick.Controls
import QtQuick.Layouts
import QtQuick.Shapes
import OpenNOW

Column {
    id: root
    required property var store
    readonly property var settings: store.onboardingSettings
    readonly property string resolution: String(settings.resolution || "1920x1080")
    readonly property bool wide: width >= DesktopTokens.px(1000)
    readonly property color mint: Theme.accentColor("green")
    readonly property color panelColor: Theme.lightMode ? Theme.glass : "#C70B0F1A"
    spacing: DesktopTokens.px(24)

    component Copy: Text {
        color: Theme.textMuted
        font.family: Theme.bodyFont
        font.pixelSize: DesktopTokens.px(12)
        wrapMode: Text.WordWrap
        lineHeightMode: Text.FixedHeight
        lineHeight: DesktopTokens.px(font.pixelSize >= DesktopTokens.px(14) ? 18 : 16)
    }
    component Badge: Rectangle {
        property alias text: badgeText.text
        implicitWidth: badgeText.implicitWidth + DesktopTokens.px(16)
        implicitHeight: DesktopTokens.px(28)
        radius: DesktopTokens.px(5)
        color: DesktopTokens.raised
        border.color: DesktopTokens.seamSoft
        Text {
            id: badgeText
            anchors.centerIn: parent
            color: Theme.label
            font.family: Theme.monoFont
            font.pixelSize: DesktopTokens.px(10)
            font.weight: Font.Bold
        }
    }
    component SettingRow: Item {
        id: settingRow
        property string title
        property string description
        property string glyph
        property bool divider: true
        default property alias control: controlSlot.data
        readonly property bool stacked: width < DesktopTokens.px(600)
        implicitHeight: Math.max(DesktopTokens.px(67), labels.implicitHeight + DesktopTokens.px(28))
            + (stacked ? controlSlot.implicitHeight + DesktopTokens.px(12) : 0)
        Rectangle {
            x: DesktopTokens.px(18)
            y: DesktopTokens.px(16)
            width: DesktopTokens.px(36); height: width
            radius: DesktopTokens.px(10); color: DesktopTokens.raised
            DesktopSettingsIcon { anchors.centerIn: parent; width: DesktopTokens.px(16); height: width; glyph: settingRow.glyph; ink: Theme.label }
        }
        Column {
            id: labels
            x: DesktopTokens.px(68)
            y: settingRow.stacked ? DesktopTokens.px(14) : (parent.height - height) / 2
            width: parent.width - x - DesktopTokens.px(18) - (settingRow.stacked ? 0 : controlSlot.width + DesktopTokens.px(14))
            spacing: DesktopTokens.px(3)
            Copy { width: parent.width; text: settingRow.title; color: Theme.label; font.pixelSize: DesktopTokens.px(14); font.weight: Font.ExtraBold }
            Copy { width: parent.width; text: settingRow.description }
        }
        Row {
            id: controlSlot
            anchors.right: parent.right; anchors.rightMargin: DesktopTokens.px(18)
            y: settingRow.stacked ? labels.y + labels.height + DesktopTokens.px(12) : (parent.height - height) / 2
            spacing: DesktopTokens.px(8)
        }
        Rectangle { visible: settingRow.divider; anchors.bottom: parent.bottom; width: parent.width; height: 1; color: DesktopTokens.seamSoft }
    }
    component Segments: Item {
        id: segments
        property var options: []
        property int selectedIndex: 0
        property int optionWidth: 56
        property var disabledValues: []
        property string disabledHint: ""
        signal selected(int index, var value)
        function optionValue(option) { return typeof option === "object" ? option.value : option }
        function isDisabled(option) { return option.enabled === false || disabledValues.some(value => String(optionValue(value)) === String(optionValue(option))) }
        implicitWidth: DesktopTokens.px(6) + options.length * DesktopTokens.px(optionWidth + 2)
        implicitHeight: DesktopTokens.px(36)
        Rectangle { anchors.fill: parent; radius: height / 2; color: DesktopTokens.raised; border.color: DesktopTokens.seamSoft }
        Row {
            x: DesktopTokens.px(3); y: DesktopTokens.px(3); spacing: DesktopTokens.px(2)
            Repeater {
                model: segments.options
                AbstractButton {
                    id: segment
                    required property int index
                    required property var modelData
                    objectName: "settingsOption-" + String(segments.optionValue(modelData))
                    width: DesktopTokens.px(segments.optionWidth); height: DesktopTokens.px(30)
                    enabled: !segments.isDisabled(modelData); opacity: enabled ? 1 : 0.45
                    checkable: true; checked: index === segments.selectedIndex
                    Accessible.name: label.text
                    onClicked: segments.selected(index, modelData)
                    background: Rectangle { radius: height / 2; color: segment.checked ? Theme.face : segment.hovered ? DesktopTokens.raised : "transparent"; border.width: segment.activeFocus ? 2 : 0; border.color: Theme.focus }
                    contentItem: Text {
                        id: label
                        text: typeof segment.modelData === "object" ? segment.modelData.label : String(segment.modelData)
                        color: segment.checked ? Theme.faceText : Theme.textMuted
                        font.family: Theme.bodyFont; font.pixelSize: DesktopTokens.px(13); font.weight: Font.ExtraBold
                        horizontalAlignment: Text.AlignHCenter; verticalAlignment: Text.AlignVCenter
                    }
                    ToolTip.visible: hovered && !enabled && segments.disabledHint !== ""
                    ToolTip.text: segments.disabledHint
                }
            }
        }
    }
    component BitrateSlider: Row {
        id: control
        property alias from: slider.from
        property alias to: slider.to
        property alias stepSize: slider.stepSize
        property alias value: slider.value
        property real trackWidth: DesktopTokens.px(220)
        property string suffix: ""
        property string accessibleName: ""
        signal moved(real value)
        spacing: DesktopTokens.px(16)
        Slider {
            id: slider
            width: control.trackWidth; height: DesktopTokens.px(28)
            leftPadding: 0; rightPadding: 0
            Accessible.name: control.accessibleName
            onMoved: control.moved(value)
            background: Rectangle {
                anchors.verticalCenter: parent.verticalCenter; width: parent.width
                height: DesktopTokens.px(4); radius: height / 2; color: DesktopTokens.raisedStrong
                Rectangle { width: slider.visualPosition * parent.width; height: parent.height; radius: height / 2; color: root.mint }
            }
            handle: Rectangle {
                x: slider.visualPosition * (slider.width - width); anchors.verticalCenter: parent.verticalCenter
                width: DesktopTokens.px(18); height: width; radius: width / 2; color: Theme.label
                border.width: slider.activeFocus ? 2 : 0; border.color: root.mint
            }
        }
        Text {
            width: DesktopTokens.px(72); height: DesktopTokens.px(28)
            text: Math.round(slider.value) + control.suffix
            color: Theme.label; font.family: Theme.monoFont; font.pixelSize: DesktopTokens.px(14); font.weight: Font.Bold
            verticalAlignment: Text.AlignVCenter; horizontalAlignment: Text.AlignRight
        }
    }

    Flow {
        width: parent.width
        spacing: DesktopTokens.px(8)
        Text {
            height: DesktopTokens.px(28); verticalAlignment: Text.AlignVCenter
            text: qsTr("REQUESTED"); color: Theme.textMuted
            font.family: Theme.monoFont; font.pixelSize: DesktopTokens.px(10); font.weight: Font.Bold
            font.letterSpacing: DesktopTokens.px(1)
        }
        Badge { text: root.resolution.replace("x", " × ") }
        Badge { text: Number(root.settings.fps ?? 60) === 0 ? qsTr("Automatic frame rate") : qsTr("%1 FPS").arg(root.settings.fps ?? 60) }
        Badge { text: String(root.settings.codec || "auto").toUpperCase() }
        Badge { text: root.settings.enableHdr === true ? qsTr("HDR requested") : qsTr("SDR") }
        Badge { text: qsTr("%1 Mbps maximum").arg(root.settings.maxBitrateMbps ?? 75) }
    }

    GridLayout {
        width: parent.width
        columns: root.wide ? 2 : 1
        columnSpacing: DesktopTokens.px(24); rowSpacing: DesktopTokens.px(24)
        Rectangle {
            Layout.fillWidth: true; Layout.minimumWidth: 0; Layout.alignment: Qt.AlignTop
            implicitHeight: rows.implicitHeight
            radius: DesktopTokens.px(16); color: root.panelColor; border.color: Theme.seam
            Column {
                id: rows
                width: parent.width
                DesktopSettingsResolution {
                    objectName: "onboardingResolution"
                    width: parent.width
                    items: root.store.resolutionItems()
                    value: root.resolution
                    onSelected: value => root.store.setOnboardingSetting("resolution", value)
                }
                SettingRow {
                    width: parent.width; title: qsTr("Frame rate"); glyph: "speed"
                    description: qsTr("Rates follow your membership and resolution.")
                    Segments {
                        objectName: "onboardingFps"
                        readonly property int current: Number(root.settings.fps ?? 60)
                        options: [60,90,120,144,240].indexOf(current) >= 0 ? [60,90,120,144,240]
                            : [{label: current === 0 ? qsTr("Auto") : String(current), value: current},60,90,120,144,240]
                        optionWidth: 46
                        selectedIndex: options.findIndex(item => Number(optionValue(item)) === current)
                        disabledValues: root.store.unentitledFpsValues(root.resolution)
                        disabledHint: qsTr("Not available on your current membership")
                        onSelected: (index, item) => root.store.setOnboardingSetting("fps", Number(optionValue(item)))
                    }
                }
                SettingRow {
                    id: hdrRow
                    readonly property string status: HdrOutput.supported && !root.store.hdrDecoderAvailable()
                        ? qsTr("HDR requires a supported 10-bit H.265 or AV1 hardware decoder.") : HdrOutput.status
                    width: parent.width; title: qsTr("HDR"); glyph: "sun"
                    description: !HdrOutput.supported ? qsTr("HDR is unavailable on this display.") : status
                    HoverHandler { id: hdrHover; parent: hdrRow }
                    ToolTip.visible: hdrHover.hovered
                    ToolTip.text: status
                    ToolTip.delay: 500
                    DesktopSettingsToggle {
                        objectName: "onboardingHdr"
                        checked: root.settings.enableHdr === true
                        enabled: (HdrOutput.supported && root.store.hdrDecoderAvailable()) || checked
                        opacity: enabled ? 1 : 0.45
                        Accessible.name: qsTr("HDR")
                        Accessible.description: hdrRow.status
                        onValueChangedByUser: value => root.store.setOnboardingSetting("enableHdr", value)
                    }
                }
                SettingRow {
                    width: parent.width; title: qsTr("Codec"); glyph: "chip"
                    description: qsTr("Auto chooses a supported native decoder.")
                    Segments {
                        objectName: "onboardingCodec"
                        options: [{label:qsTr("Auto"),value:"auto"},
                            {label:"AV1",value:"av1",enabled:root.store.codecAvailable("av1")},
                            {label:"H.265",value:"h265",enabled:root.store.codecAvailable("h265")},
                            {label:"H.264",value:"h264",enabled:root.store.codecAvailable("h264")}]
                        optionWidth: 59
                        selectedIndex: options.findIndex(item => item.value === String(root.settings.codec || "auto"))
                        disabledHint: qsTr("Not supported by the detected native decoder")
                        onSelected: (index, item) => root.store.setOnboardingSetting("codec", item.value)
                    }
                }
                SettingRow {
                    id: bitrateRow
                    width: parent.width; title: qsTr("Bitrate"); glyph: "wave"; divider: false
                    description: qsTr("Higher values use more bandwidth.")
                    BitrateSlider {
                        objectName: "onboardingBitrate"; accessibleName: qsTr("Bitrate")
                        trackWidth: Math.min(DesktopTokens.px(220), Math.max(DesktopTokens.px(100), bitrateRow.width - DesktopTokens.px(470)))
                        from: 10; to: 200; stepSize: 5; suffix: qsTr(" Mbps")
                        value: Number(root.settings.maxBitrateMbps ?? 75)
                        onMoved: value => root.store.setOnboardingSetting("maxBitrateMbps", Math.round(value))
                    }
                }
            }
        }
        Column {
            Layout.fillWidth: true; Layout.preferredWidth: DesktopTokens.px(400)
            Layout.maximumWidth: root.wide ? DesktopTokens.px(400) : Infinity
            Layout.minimumWidth: 0; Layout.alignment: Qt.AlignTop
            spacing: DesktopTokens.px(14)
            Text {
                x: DesktopTokens.px(4); text: qsTr("THE STAGE · REQUESTED PICTURE")
                color: Theme.textMuted; font.family: Theme.monoFont
                font.pixelSize: DesktopTokens.px(10); font.weight: Font.Bold; font.letterSpacing: DesktopTokens.px(1)
                lineHeightMode: Text.FixedHeight; lineHeight: DesktopTokens.px(12)
            }
            Rectangle {
                width: parent.width; height: DesktopTokens.px(232)
                radius: DesktopTokens.px(16); border.color: Theme.seam
                gradient: Gradient {
                    GradientStop { position: 0; color: Theme.lightMode ? Theme.glass : "#172B2D" }
                    GradientStop { position: 0.7; color: Theme.lightMode ? Theme.shell : "#0B0F1A" }
                }
                Shape {
                    id: previewOutline
                    anchors.fill: parent; anchors.margins: DesktopTokens.px(12)
                    ShapePath {
                        strokeColor: Theme.seam; strokeWidth: 1; strokeStyle: ShapePath.DashLine
                        dashPattern: [2, 2]; fillColor: "transparent"
                        PathSvg {
                            path: {
                                const w = previewOutline.width - 1
                                const h = previewOutline.height - 1
                                const r = DesktopTokens.px(10)
                                return "M " + r + " 1 H " + (w-r) + " Q " + w + " 1 " + w + " " + r
                                    + " V " + (h-r) + " Q " + w + " " + h + " " + (w-r) + " " + h
                                    + " H " + r + " Q 1 " + h + " 1 " + (h-r) + " V " + r + " Q 1 1 " + r + " 1 Z"
                            }
                        }
                    }
                }
                Row {
                    x: DesktopTokens.px(18); y: DesktopTokens.px(12); spacing: DesktopTokens.px(10)
                    Text {
                        text: root.resolution.split("x").length === 2 ? root.resolution.split("x")[1] + "p" : root.resolution
                        color: Theme.label; font.family: Theme.displayFont; font.pixelSize: DesktopTokens.px(44); font.weight: Font.Black
                    }
                    Text { anchors.bottom: parent.bottom; anchors.bottomMargin: DesktopTokens.px(10); text: root.resolution.replace("x", " × "); color: Theme.textMuted; font.family: Theme.monoFont; font.pixelSize: DesktopTokens.px(10) }
                }
                Column {
                    anchors.right: parent.right; anchors.rightMargin: DesktopTokens.px(18); y: DesktopTokens.px(18)
                    Text { anchors.right: parent.right; text: Number(root.settings.fps ?? 60) === 0 ? qsTr("Auto") : String(root.settings.fps ?? 60); color: root.mint; font.family: Theme.monoFont; font.pixelSize: DesktopTokens.px(28); font.weight: Font.Bold }
                    Text { anchors.right: parent.right; text: qsTr("FPS"); color: Theme.textMuted; font.family: Theme.monoFont; font.pixelSize: DesktopTokens.px(10) }
                }
                Row {
                    x: DesktopTokens.px(18); anchors.bottom: parent.bottom; anchors.bottomMargin: DesktopTokens.px(18); spacing: DesktopTokens.px(6)
                    Badge { text: String(root.settings.codec || "auto").toUpperCase() }
                    Badge { text: root.settings.enableHdr === true ? qsTr("HDR requested") : qsTr("SDR") }
                }
                Text {
                    anchors.right: parent.right; anchors.rightMargin: DesktopTokens.px(18); anchors.bottom: parent.bottom; anchors.bottomMargin: DesktopTokens.px(22)
                    text: qsTr("%1 Mbps").arg(root.settings.maxBitrateMbps ?? 75)
                    color: Theme.label; font.family: Theme.monoFont; font.pixelSize: DesktopTokens.px(14); font.weight: Font.Bold
                }
            }
            Rectangle {
                width: parent.width; implicitHeight: budget.implicitHeight + DesktopTokens.px(32)
                radius: DesktopTokens.px(14); color: root.panelColor; border.color: DesktopTokens.seamSoft
                Column {
                    id: budget
                    x: DesktopTokens.px(18); y: DesktopTokens.px(16); width: parent.width - DesktopTokens.px(36); spacing: DesktopTokens.px(10)
                    RowLayout {
                        width: parent.width
                        Copy { text: qsTr("Bitrate limit"); color: Theme.label; font.pixelSize: DesktopTokens.px(13); font.weight: Font.ExtraBold; Layout.fillWidth: true }
                        Copy { text: qsTr("%1 / 200 Mbps").arg(root.settings.maxBitrateMbps ?? 75); font.family: Theme.monoFont; font.pixelSize: DesktopTokens.px(11) }
                    }
                    Rectangle {
                        width: parent.width; height: DesktopTokens.px(4); radius: height / 2; color: DesktopTokens.raised
                        Rectangle { width: parent.width * Math.max(0, Math.min(1, Number(root.settings.maxBitrateMbps ?? 75) / 200)); height: parent.height; radius: height / 2; color: root.mint }
                    }
                    Copy { width: parent.width; text: qsTr("Requested maximum, not a network test. Actual quality depends on your membership, device and connection.") }
                }
            }
        }
    }
}

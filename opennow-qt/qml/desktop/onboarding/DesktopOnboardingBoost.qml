pragma ComponentBehavior: Bound
import QtQuick
import QtQuick.Controls
import OpenNOW

Column {
    id: root
    required property var store
    readonly property var settings: store.onboardingSettings
    readonly property bool upscalingEnabled: settings.upscaling === (mac ? "metalfx" : "fsr1")
    readonly property bool mac: Qt.platform.os === "osx"
    readonly property color blue: Theme.accentColor("blue")
    readonly property color panelColor: Theme.lightMode ? Theme.glass : "#C70B0F1A"
    spacing: DesktopTokens.px(24)

    component Copy: Text {
        color: Theme.textMuted
        font.family: Theme.bodyFont
        font.pixelSize: DesktopTokens.px(13)
        wrapMode: Text.WordWrap
        lineHeightMode: Text.FixedHeight
        lineHeight: DesktopTokens.px(font.pixelSize >= DesktopTokens.px(20) ? 24 : font.pixelSize <= DesktopTokens.px(12) ? 16 : 18)
    }
    component Eyebrow: Text {
        color: Theme.textMuted
        font.family: Theme.monoFont
        font.pixelSize: DesktopTokens.px(10)
        font.weight: Font.Bold
        font.letterSpacing: DesktopTokens.px(1)
    }
    component Badge: Rectangle {
        property alias text: label.text
        property color ink: Theme.textMuted
        implicitWidth: label.implicitWidth + DesktopTokens.px(14)
        implicitHeight: DesktopTokens.px(24)
        radius: DesktopTokens.px(6)
        color: Qt.rgba(ink.r, ink.g, ink.b, 0.12)
        Eyebrow { id: label; anchors.centerIn: parent; color: parent.ink }
    }
    component Segments: Item {
        id: segments
        property var options: []
        property int selectedIndex: 0
        property int optionWidth: 56
        signal selected(int index, var value)
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
                    objectName: "settingsOption-" + modelData.value
                    width: DesktopTokens.px(segments.optionWidth); height: DesktopTokens.px(30)
                    checkable: true; checked: index === segments.selectedIndex
                    Accessible.name: modelData.label
                    onClicked: segments.selected(index, modelData)
                    background: Rectangle { radius: height / 2; color: segment.checked ? Theme.face : segment.hovered ? DesktopTokens.raised : "transparent"; border.width: segment.activeFocus ? 2 : 0; border.color: Theme.focus }
                    contentItem: Text {
                        text: segment.modelData.label; color: segment.checked ? Theme.faceText : Theme.textMuted
                        font.family: Theme.bodyFont; font.pixelSize: DesktopTokens.px(13); font.weight: Font.ExtraBold
                        horizontalAlignment: Text.AlignHCenter; verticalAlignment: Text.AlignVCenter
                    }
                }
            }
        }
    }
    component TuningSlider: Row {
        id: control
        property alias from: slider.from
        property alias to: slider.to
        property alias stepSize: slider.stepSize
        property alias value: slider.value
        property real trackWidth: DesktopTokens.px(150)
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
                Rectangle { width: slider.visualPosition * parent.width; height: parent.height; radius: height / 2; color: root.blue }
            }
            handle: Rectangle {
                x: slider.visualPosition * (slider.width - width); anchors.verticalCenter: parent.verticalCenter
                width: DesktopTokens.px(16); height: width; radius: width / 2; color: Theme.label
                border.width: slider.activeFocus ? 2 : 0; border.color: root.blue
            }
        }
        Text {
            width: DesktopTokens.px(26); height: DesktopTokens.px(28)
            text: Math.round(slider.value) + control.suffix
            color: Theme.label; font.family: Theme.monoFont; font.pixelSize: DesktopTokens.px(14); font.weight: Font.Bold
            verticalAlignment: Text.AlignVCenter; horizontalAlignment: Text.AlignRight
        }
    }

    component BoostRow: Item {
        id: boostRow
        property string title
        property string description
        property string glyph
        property string badge
        property color badgeInk: Theme.textMuted
        property bool divider: true
        default property alias control: controlSlot.data
        readonly property bool stacked: width < DesktopTokens.px(600)
        implicitHeight: Math.max(DesktopTokens.px(68), labels.implicitHeight + DesktopTokens.px(28))
            + (stacked ? controlSlot.implicitHeight + DesktopTokens.px(12) : 0)
        Rectangle {
            x: DesktopTokens.px(18)
            y: boostRow.stacked ? DesktopTokens.px(16) : (parent.height - height) / 2
            width: DesktopTokens.px(36); height: width
            radius: DesktopTokens.px(10); color: DesktopTokens.raised
            opacity: boostRow.enabled ? 1 : 0.45
            DesktopSettingsIcon { anchors.centerIn: parent; width: DesktopTokens.px(16); height: width; glyph: boostRow.glyph; ink: Theme.label }
        }
        Column {
            id: labels
            x: DesktopTokens.px(68)
            y: boostRow.stacked ? DesktopTokens.px(14) : (parent.height - height) / 2
            width: parent.width - x - DesktopTokens.px(18) - (boostRow.stacked ? 0 : controlSlot.width + DesktopTokens.px(14))
            spacing: DesktopTokens.px(3)
            opacity: boostRow.enabled ? 1 : 0.45
            Flow {
                width: parent.width
                spacing: DesktopTokens.px(8)
                Copy { text: boostRow.title; color: Theme.label; font.pixelSize: DesktopTokens.px(14); font.weight: Font.ExtraBold; height: DesktopTokens.px(22); verticalAlignment: Text.AlignVCenter }
                Badge { visible: boostRow.badge !== ""; text: boostRow.badge; ink: boostRow.badgeInk; implicitHeight: DesktopTokens.px(22) }
            }
            Copy { width: parent.width; text: boostRow.description; font.pixelSize: DesktopTokens.px(12) }
        }
        Row {
            id: controlSlot
            anchors.right: parent.right; anchors.rightMargin: DesktopTokens.px(18)
            y: boostRow.stacked ? labels.y + labels.height + DesktopTokens.px(12) : (parent.height - height) / 2
            spacing: DesktopTokens.px(8)
        }
        Rectangle { visible: boostRow.divider; anchors.bottom: parent.bottom; width: parent.width; height: 1; color: DesktopTokens.seamSoft }
    }

    Rectangle {
        objectName: "onboardingBoostPanel"
        width: Math.min(parent.width, DesktopTokens.px(860))
        anchors.horizontalCenter: parent.horizontalCenter
        implicitHeight: rows.implicitHeight
        radius: DesktopTokens.px(16); color: root.panelColor; border.color: Theme.seam
        Column {
            id: rows
            width: parent.width
            BoostRow {
                width: parent.width; glyph: "bolt"
                title: qsTr("Frame generation"); badge: qsTr("EXPERIMENTAL"); badgeInk: Theme.accentColor("amber")
                description: qsTr("Generates in-between frames once the capability test in Settings → Stream passes on this PC. Needs a fast GPU and a high-refresh display; may add latency and visual artifacts.")
                Segments {
                    objectName: "onboardingFrameGeneration"
                    options: [{label:qsTr("Off"),value:"off"},{label:qsTr("2×"),value:"2x"}]
                    selectedIndex: root.settings.frameGeneration === "2x" ? 1 : 0
                    optionWidth: 56
                    onSelected: (index, item) => root.store.setOnboardingSetting("frameGeneration", item.value)
                }
            }
            BoostRow {
                width: parent.width; glyph: "monitor"
                title: qsTr("Upscaling"); badge: root.mac ? qsTr("macOS ONLY") : qsTr("SDR ONLY"); badgeInk: root.blue
                description: root.mac
                    ? qsTr("MetalFX spatial upscaling for enlarged video. Falls back to normal scaling when MetalFX is unavailable.")
                    : qsTr("FSR 1 sharpens enlarged SDR video on the GPU.")
                Segments {
                    objectName: "onboardingUpscaling"
                    options: [{label:qsTr("Off"),value:"off"},{label:root.mac ? "MetalFX" : "FSR 1",value:root.mac ? "metalfx" : "fsr1"}]
                    optionWidth: 70
                    selectedIndex: root.upscalingEnabled ? 1 : 0
                    onSelected: (index, item) => root.store.setOnboardingSetting("upscaling", item.value)
                }
            }
            Repeater {
                model: [{key:"upscalingSharpness",label:qsTr("Clarity"),glyph:"sun",description:root.mac ? qsTr("Sharpen details before upscaling. 0 disables.") : qsTr("Sharpen details after FSR 1 upscaling. Set to 0 to disable."),maximum:15,fallback:10},
                    ...(root.mac ? [{key:"upscalingDenoise",label:qsTr("Noise reduction"),glyph:"drop",description:qsTr("Smooth noise before upscaling. 0 disables."),maximum:20,fallback:0}] : [])]
                delegate: BoostRow {
                    id: tuningControl
                    required property var modelData
                    width: parent.width; glyph: modelData.glyph
                    title: modelData.label; description: modelData.description
                    enabled: root.upscalingEnabled
                    TuningSlider {
                        objectName: "onboarding-" + tuningControl.modelData.key
                        accessibleName: tuningControl.modelData.label
                        opacity: tuningControl.enabled ? 1 : 0.45
                        trackWidth: Math.max(DesktopTokens.px(70), Math.min(DesktopTokens.px(150), tuningControl.width - DesktopTokens.px(400)))
                        from: 0; to: tuningControl.modelData.maximum; stepSize: 1; suffix: ""
                        value: Number(root.settings[tuningControl.modelData.key] ?? tuningControl.modelData.fallback)
                        onMoved: value => root.store.setOnboardingSetting(tuningControl.modelData.key, Math.round(value))
                    }
                }
            }
            Item {
                width: parent.width
                implicitHeight: Math.max(DesktopTokens.px(52), hint.implicitHeight + DesktopTokens.px(24))
                DesktopSettingsIcon { x: DesktopTokens.px(18); anchors.verticalCenter: parent.verticalCenter; width: DesktopTokens.px(14); height: width; glyph: "info"; ink: Theme.textMuted }
                Copy {
                    id: hint
                    x: DesktopTokens.px(44); anchors.verticalCenter: parent.verticalCenter; width: parent.width - x - DesktopTokens.px(18)
                    font.pixelSize: DesktopTokens.px(12)
                    text: String(root.settings.shortcutToggleStats ?? "Ctrl+N") !== ""
                        ? qsTr("Check the effect mid-game with %1 (stream statistics) and change it from the %2 stream menu.").arg(String(root.settings.shortcutToggleStats ?? "Ctrl+N")).arg("Ctrl+G")
                        : qsTr("Change it mid-game from the %1 stream menu.").arg("Ctrl+G")
                }
            }
        }
    }
}

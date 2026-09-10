pragma ComponentBehavior: Bound
import QtQuick
import QtQuick.Layouts
import OpenNOW

Column {
    id: root
    required property var store
    readonly property var settings: store.onboardingSettings
    readonly property bool metalFx: settings.upscaling === "metalfx"
    spacing: DesktopTokens.px(24)

    GridLayout {
        width: parent.width
        columns: width >= DesktopTokens.px(950) && Qt.platform.os === "osx" ? 2 : 1
        columnSpacing: DesktopTokens.px(24)
        rowSpacing: DesktopTokens.px(24)

        DesktopSettingsPanel {
            Layout.fillWidth: true
            Layout.preferredWidth: DesktopTokens.px(500)
            Layout.minimumWidth: 0
            Layout.alignment: Qt.AlignTop
            padding: DesktopTokens.px(24)
            radius: DesktopTokens.px(16)
            border.color: root.settings.frameGeneration === "2x" ? Theme.accentColor("green") : Theme.seam
            Column {
                width: parent.width
                spacing: DesktopTokens.px(20)
                Text {
                    width: parent.width
                    text: qsTr("EXPERIMENTAL")
                    color: Theme.accentColor("amber")
                    font.family: Theme.monoFont; font.pixelSize: DesktopTokens.monoSize
                    font.weight: Font.Bold; font.letterSpacing: DesktopTokens.px(1)
                }
                Row {
                    width: parent.width
                    spacing: DesktopTokens.px(8)
                    Repeater {
                        model: 8
                        Rectangle {
                            required property int index
                            width: (parent.width - DesktopTokens.px(56)) / 8
                            height: DesktopTokens.px(58)
                            radius: DesktopTokens.px(5)
                            color: index % 2 === 0 ? DesktopTokens.raisedStrong : "transparent"
                            border.color: index % 2 === 0 ? Theme.seam : Theme.accentColor("green")
                            opacity: index % 2 === 0 || root.settings.frameGeneration === "2x" ? 1 : 0.3
                        }
                    }
                }
                Text {
                    width: parent.width; text: qsTr("Frame generation")
                    color: Theme.label; font.family: Theme.displayFont
                    font.pixelSize: DesktopTokens.titleSize; font.weight: Font.Black
                    wrapMode: Text.Wrap
                }
                Text {
                    width: parent.width
                    text: qsTr("Generate intermediate frames on your device. Requires a fast GPU and a high-refresh display; may add latency and visual artifacts.")
                    color: Theme.textMuted; font.family: Theme.bodyFont
                    font.pixelSize: DesktopTokens.bodySize; wrapMode: Text.Wrap; lineHeight: 1.4
                }
                DesktopSettingsSegmented {
                    objectName: "onboardingFrameGeneration"
                    options: [{label:qsTr("Off"),value:"off"},{label:qsTr("2×"),value:"2x"}]
                    selectedIndex: root.settings.frameGeneration === "2x" ? 1 : 0
                    optionWidth: 90
                    onSelected: (index, item) => root.store.setOnboardingSetting("frameGeneration", item.value)
                }
                Text {
                    width: parent.width
                    text: qsTr("Off by default. You can try this later in Stream settings.")
                    color: Theme.textMuted; font.family: Theme.bodyFont
                    font.pixelSize: DesktopTokens.captionSize; wrapMode: Text.Wrap
                }
            }
        }

        DesktopSettingsPanel {
            visible: Qt.platform.os === "osx"
            Layout.fillWidth: true
            Layout.preferredWidth: DesktopTokens.px(500)
            Layout.minimumWidth: 0
            Layout.alignment: Qt.AlignTop
            padding: DesktopTokens.px(24)
            radius: DesktopTokens.px(16)
            border.color: Theme.seam
            Column {
                width: parent.width
                spacing: DesktopTokens.px(20)
                Text {
                    width: parent.width; text: qsTr("macOS ONLY")
                    color: Theme.focus; font.family: Theme.monoFont
                    font.pixelSize: DesktopTokens.monoSize; font.weight: Font.Bold
                }
                Text {
                    width: parent.width; text: qsTr("MetalFX upscaling")
                    color: Theme.label; font.family: Theme.displayFont
                    font.pixelSize: DesktopTokens.titleSize; font.weight: Font.Black
                    wrapMode: Text.Wrap
                }
                Text {
                    width: parent.width
                    text: qsTr("Spatial upscaling for enlarged video. Uses extra GPU time and falls back to normal scaling when MetalFX is unavailable.")
                    color: Theme.textMuted; font.family: Theme.bodyFont
                    font.pixelSize: DesktopTokens.bodySize; wrapMode: Text.Wrap; lineHeight: 1.4
                }
                DesktopSettingsSegmented {
                    objectName: "onboardingUpscaling"
                    options: [{label:qsTr("Off"),value:"off"},{label:"MetalFX",value:"metalfx"}]
                    optionWidth: 90
                    selectedIndex: root.metalFx ? 1 : 0
                    onSelected: (index, item) => root.store.setOnboardingSetting("upscaling", item.value)
                }
                Repeater {
                    model: [{key:"upscalingSharpness",label:qsTr("Clarity"),maximum:15,fallback:10},
                        {key:"upscalingDenoise",label:qsTr("Noise reduction"),maximum:20,fallback:0}]
                    delegate: Column {
                        id: tuningControl
                        required property var modelData
                        width: parent.width; spacing: DesktopTokens.px(8)
                        enabled: root.metalFx
                        opacity: enabled ? 1 : 0.45
                        Text {
                            text: tuningControl.modelData.label
                            color: Theme.label; font.family: Theme.bodyFont
                            font.pixelSize: DesktopTokens.bodySize; font.weight: Font.Bold
                        }
                        DesktopSettingsSlider {
                            objectName: "onboarding-" + tuningControl.modelData.key
                            accessibleName: tuningControl.modelData.label
                            trackWidth: Math.max(DesktopTokens.px(80), parent.width - DesktopTokens.px(110))
                            from: 0; to: tuningControl.modelData.maximum; stepSize: 1; suffix: ""
                            value: Number(root.settings[tuningControl.modelData.key] ?? tuningControl.modelData.fallback)
                            onMoved: value => root.store.setOnboardingSetting(tuningControl.modelData.key, Math.round(value))
                        }
                    }
                }
            }
        }
    }
    Text {
        width: parent.width
        text: qsTr("Boost settings change local presentation, not your membership's stream limits.")
        color: Theme.textMuted; font.family: Theme.bodyFont
        font.pixelSize: DesktopTokens.captionSize; wrapMode: Text.Wrap
    }
}

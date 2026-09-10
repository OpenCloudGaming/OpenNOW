import QtQuick
import QtQuick.Layouts
import OpenNOW

GridLayout {
    id: root
    required property var store
    readonly property var settings: store.onboardingSettings
    readonly property string resolution: String(settings.resolution || "1920x1080")
    readonly property bool wide: width >= DesktopTokens.px(940)
    columns: wide ? 2 : 1
    columnSpacing: DesktopTokens.px(24)
    rowSpacing: DesktopTokens.px(24)

    DesktopSettingsPanel {
        Layout.fillWidth: true
        Layout.alignment: Qt.AlignTop
        Layout.minimumWidth: 0
        paperStyle: true
        border.color: Theme.seam
        radius: DesktopTokens.px(16)

        DesktopSettingsResolution {
            objectName: "onboardingResolution"
            width: parent.width
            items: root.store.resolutionItems()
            value: root.resolution
            onSelected: value => root.store.setOnboardingSetting("resolution", value)
        }
        DesktopSettingsRow {
            width: parent.width; paperStyle: true; glyph: "speed"
            title: qsTr("Frame rate")
            description: qsTr("Available rates follow your membership and selected resolution.")
            DesktopSettingsSegmented {
                objectName: "onboardingFps"
                readonly property int current: Number(root.settings.fps ?? 60)
                options: [60,90,120,144,240].indexOf(current) >= 0 ? [60,90,120,144,240]
                    : [{label: current === 0 ? qsTr("Auto") : String(current), value: current},60,90,120,144,240]
                optionWidth: 44
                selectedIndex: options.findIndex(item => Number(optionValue(item)) === current)
                disabledValues: root.store.unentitledFpsValues(root.resolution)
                disabledHint: qsTr("Not available on your current membership")
                onSelected: (index, item) => root.store.setOnboardingSetting("fps", Number(optionValue(item)))
            }
        }
        DesktopSettingsRow {
            width: parent.width; paperStyle: true; glyph: "sun"
            title: qsTr("HDR")
            description: HdrOutput.supported && !root.store.hdrDecoderAvailable()
                ? qsTr("HDR requires a supported 10-bit H.265 or AV1 hardware decoder.") : HdrOutput.status
            DesktopSettingsToggle {
                objectName: "onboardingHdr"
                checked: root.settings.enableHdr === true
                enabled: (HdrOutput.supported && root.store.hdrDecoderAvailable()) || checked
                opacity: enabled ? 1 : 0.45
                Accessible.name: qsTr("HDR")
                onValueChangedByUser: value => root.store.setOnboardingSetting("enableHdr", value)
            }
        }
        DesktopSettingsRow {
            width: parent.width; paperStyle: true; glyph: "chip"
            title: qsTr("Codec")
            description: qsTr("Auto lets the native streamer choose a supported decoder.")
            DesktopSettingsSegmented {
                objectName: "onboardingCodec"
                options: [{label:qsTr("Auto"),value:"auto"},
                    {label:"AV1",value:"av1",enabled:root.store.codecAvailable("av1")},
                    {label:"H.265",value:"h265",enabled:root.store.codecAvailable("h265")},
                    {label:"H.264",value:"h264",enabled:root.store.codecAvailable("h264")}]
                optionWidth: 56
                selectedIndex: options.findIndex(item => item.value === String(root.settings.codec || "auto"))
                disabledHint: qsTr("Not supported by the detected native decoder")
                onSelected: (index, item) => root.store.setOnboardingSetting("codec", item.value)
            }
        }
        DesktopSettingsRow {
            id: bitrateRow
            width: parent.width; paperStyle: true; glyph: "wave"
            title: qsTr("Bitrate")
            description: qsTr("Maximum requested bitrate. Higher values use more bandwidth.")
            showDivider: false
            DesktopSettingsSlider {
                objectName: "onboardingBitrate"
                accessibleName: qsTr("Bitrate")
                trackWidth: Math.max(DesktopTokens.px(100), Math.min(DesktopTokens.px(180), bitrateRow.width - DesktopTokens.px(180)))
                from: 10; to: 200; stepSize: 5; suffix: qsTr(" Mbps")
                value: Number(root.settings.maxBitrateMbps ?? 75)
                onMoved: value => root.store.setOnboardingSetting("maxBitrateMbps", Math.round(value))
            }
        }
    }

    DesktopSettingsPanel {
        Layout.fillWidth: true
        Layout.preferredWidth: root.wide ? DesktopTokens.px(320) : -1
        Layout.maximumWidth: root.wide ? DesktopTokens.px(360) : Infinity
        Layout.alignment: Qt.AlignTop
        padding: DesktopTokens.px(24)
        radius: DesktopTokens.px(16)
        border.color: Theme.seam
        Column {
            width: parent.width
            spacing: DesktopTokens.px(20)
            Text {
                width: parent.width
                text: qsTr("YOUR REQUESTED PICTURE")
                color: Theme.accentColor("green")
                font.family: Theme.monoFont; font.pixelSize: DesktopTokens.monoSize
                font.weight: Font.Bold; font.letterSpacing: DesktopTokens.px(1)
                wrapMode: Text.Wrap
            }
            Text {
                width: parent.width
                text: root.resolution.replace("x", " × ")
                color: Theme.label
                font.family: Theme.displayFont; font.pixelSize: DesktopTokens.px(34)
                font.weight: Font.Black; wrapMode: Text.Wrap
            }
            Text {
                width: parent.width
                text: Number(root.settings.fps ?? 60) === 0 ? qsTr("Automatic frame rate") : qsTr("%1 FPS").arg(root.settings.fps ?? 60)
                color: Theme.label
                font.family: Theme.monoFont; font.pixelSize: DesktopTokens.titleSize
                wrapMode: Text.Wrap
            }
            Rectangle { width: parent.width; height: 1; color: Theme.seam }
            Text {
                width: parent.width
                text: qsTr("%1 · %2\n%3 Mbps maximum").arg(String(root.settings.codec || "auto").toUpperCase())
                    .arg(root.settings.enableHdr === true ? qsTr("HDR requested") : qsTr("SDR"))
                    .arg(root.settings.maxBitrateMbps ?? 75)
                color: Theme.textMuted
                font.family: Theme.bodyFont; font.pixelSize: DesktopTokens.bodySize
                wrapMode: Text.Wrap; lineHeight: 1.5
            }
            Text {
                width: parent.width
                text: qsTr("These are preferences, not a network test. The actual stream depends on your membership, device and connection.")
                color: Theme.textMuted
                font.family: Theme.bodyFont; font.pixelSize: DesktopTokens.captionSize
                wrapMode: Text.Wrap; lineHeight: 1.4
            }
        }
    }
}

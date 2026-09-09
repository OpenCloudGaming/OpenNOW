import QtQuick
import QtQuick.Controls
import QtQuick.Layouts
import OpenNOW

Column {
    id: page
    required property real availableWidth
    required property var settingsScreen

    width: page.availableWidth; spacing: 20
    DesktopSettingsPanel {
        width: parent.width; paperStyle: true
        DesktopSettingsRow {
            width: parent.width; paperStyle: true; glyph: "monitor"; title: qsTr("Fullscreen when session is ready")
            description: qsTr("Automatically enter fullscreen when your session is ready. F11 toggles fullscreen during play.")
            DesktopSettingsToggle {
                objectName: "autoFullScreenToggle"
                checked: page.settingsScreen.boolSetting("autoFullScreen", true)
                Accessible.name: qsTr("Fullscreen when session is ready")
                onValueChangedByUser: value => page.settingsScreen.setSetting("autoFullScreen", value)
            }
        }
        DesktopSettingsRow {
            width: parent.width; paperStyle: true; glyph: "controller"; title: qsTr("Steam Big Picture mode")
            description: qsTr("Request gamepad-friendly launchers such as Steam Big Picture. Applies to new GeForce NOW sessions only.")
            showDivider: false
            DesktopSettingsToggle {
                objectName: "steamBigPictureToggle"
                checked: page.settingsScreen.boolSetting("steamBigPictureMode", false)
                onValueChangedByUser: value => page.settingsScreen.setSetting("steamBigPictureMode", value)
            }
        }
    }
    DesktopSettingsPanel {
        width: parent.width; paperStyle: true
        DesktopSettingsSection { text: qsTr("PICTURE") }
        DesktopSettingsResolution {
            width: parent.width; items: page.settingsScreen.resolutionItems()
            value: page.settingsScreen.currentResolutionValue()
            onSelected: value => page.settingsScreen.setSetting("resolution", value)
        }
        DesktopSettingsRow {
            width: parent.width; paperStyle: true; glyph: "speed"; title: qsTr("Frame rate"); description: page.settingsScreen.fpsEntitlementNote()
            DesktopSettingsSegmented {
                readonly property string current: Number(page.settingsScreen.valueSetting("fps",60)) === 0 ? "AUTO" : String(page.settingsScreen.valueSetting("fps",60))
                options: ["60","90","120","144","240"].indexOf(current) >= 0 ? ["60","90","120","144","240"] : [current,"60","90","120","144","240"]
                optionWidth: 50; selectedIndex: options.indexOf(current)
                disabledValues: page.settingsScreen.unentitledFpsValues(); disabledHint: page.settingsScreen.fpsLockedHint()
                onSelected: (index,value) => page.settingsScreen.setSetting("fps",value === "AUTO" ? 0 : Number(value))
            }
        }
        DesktopSettingsRow {
            width: parent.width; paperStyle: true; glyph: "speed"; title: qsTr("Frame generation (Experimental)")
            description: qsTr("Targets 120 displayed FPS from a 60 FPS stream. Requires a fast GPU and 120 Hz display; adds latency and artifacts.")
            DesktopSettingsSegmented {
                readonly property string current: String(page.settingsScreen.valueSetting("frameGeneration", "off")) === "2x" ? "2x" : "off"
                options: [{label: qsTr("Off"), value: "off"}, {label: qsTr("2×"), value: "2x"}]
                optionWidth: 64; selectedIndex: options.findIndex(item => item.value === current)
                onSelected: (index,item) => page.settingsScreen.setSetting("frameGeneration", item.value)
            }
        }
        DesktopSettingsRow {
            objectName: "upscalingSettingsRow"
            visible: Qt.platform.os === "osx"
            width: parent.width; paperStyle: true; glyph: "monitor"; title: qsTr("Upscaling")
            description: qsTr("Spatial upscaling for enlarged video. Uses extra GPU time; falls back to normal scaling when MetalFX is unavailable.")
            DesktopSettingsSegmented {
                objectName: "upscalingSelector"
                readonly property string current: String(page.settingsScreen.valueSetting("upscaling", "off")) === "metalfx" ? "metalfx" : "off"
                options: [{label: qsTr("Off"), value: "off"}, {label: "MetalFX", value: "metalfx"}]
                optionWidth: 90; selectedIndex: options.findIndex(item => item.value === current)
                onSelected: (index,item) => page.settingsScreen.setSetting("upscaling", item.value)
            }
        }
        DesktopSettingsRow {
            width: parent.width; paperStyle: true; glyph: "sun"; title: qsTr("HDR")
            description: HdrOutput.supported && !ShellStore.hdrDecoderAvailable()
                ? qsTr("HDR requires a supported 10-bit H.265 or AV1 hardware decoder.") : HdrOutput.status
            DesktopSettingsToggle {
                objectName: "enableHdrToggle"
                checked: page.settingsScreen.boolSetting("enableHdr", false)
                enabled: (HdrOutput.supported && ShellStore.hdrDecoderAvailable()) || checked
                opacity: enabled ? 1 : 0.45
                Accessible.name: qsTr("HDR")
                onValueChangedByUser: value => page.settingsScreen.setSetting("enableHdr", value)
            }
        }
        DesktopSettingsRow {
            width: parent.width; paperStyle: true; glyph: "drop"; title: qsTr("Color depth")
            description: page.settingsScreen.colorQualityFooter(); showDivider: false
            DesktopSettingsSegmented {
                options: page.settingsScreen.colorQualityItems().filter(item => item.value !== "8bit_444" || page.settingsScreen.valueSetting("colorQuality","8bit_420") === "8bit_444").map(item => ({label:item.value === "8bit_420" ? "8-bit" : item.value === "10bit_420" ? "10-bit" : item.value === "8bit_444" ? "8-bit 4:4:4" : "10-bit 4:4:4", value:item.value, enabled:!item.disabled}))
                optionWidth: 85; selectedIndex: options.findIndex(item => item.value === page.settingsScreen.valueSetting("colorQuality","8bit_420"))
                onSelected: (index,item) => page.settingsScreen.setChoice("colorQuality",item.value)
            }
        }
    }
    DesktopSettingsPanel {
        width: parent.width; paperStyle: true
        DesktopSettingsSection { text: qsTr("CONNECTION") }
        DesktopSettingsChoice {
            objectName: "streamBackendChoice"
            width: parent.width; glyph: "chip"; title: qsTr("Video backend")
            description: Qt.platform.os === "windows"
                ? qsTr("Auto uses DX11 hardware decoding. DX12 and Vulkan texture sharing are not supported by the Windows stream view yet. Applies to the next stream.")
                : qsTr("Choose a supported native backend. Applies to the next stream.")
            items: ShellStore.videoBackendItems()
            value: page.settingsScreen.valueSetting("nativeVideoBackend", "auto")
            onSelected: value => page.settingsScreen.setSetting("nativeVideoBackend", value)
        }
        DesktopSettingsRow {
            width: parent.width; paperStyle: true; glyph: "chip"; title: qsTr("Codec")
            description: ShellStore.streamerDetectionMessage
            DesktopSettingsSegmented {
                options: [{label:qsTr("Auto"),value:"auto"},{label:"AV1",value:"av1",enabled:ShellStore.codecAvailable("av1")},{label:"H.265",value:"h265",enabled:ShellStore.codecAvailable("h265")},{label:"H.264",value:"h264",enabled:ShellStore.codecAvailable("h264")}]
                disabledHint: qsTr("Not supported by the detected native decoder")
                optionWidth: 64; selectedIndex: options.findIndex(item => item.value === page.settingsScreen.valueSetting("codec","auto"))
                onSelected: (index,item) => page.settingsScreen.setChoice("codec",item.value)
            }
        }
        DesktopSettingsRow {
            id: bitrateRow
            width: parent.width; paperStyle: true; glyph: "wave"; title: qsTr("Bitrate"); description: qsTr("Maximum requested bitrate")
            DesktopSettingsSlider {
                trackWidth: Math.max(DesktopTokens.px(160), bitrateRow.width - DesktopTokens.px(460)); from: 10; to: 200; stepSize: 5
                value: Number(page.settingsScreen.valueSetting("maxBitrateMbps",75)); suffix: " Mbps"
                onCommitted: value => page.settingsScreen.setSetting("maxBitrateMbps",Math.round(value))
            }
        }
        DesktopSettingsRow {
            width: parent.width; paperStyle: true; glyph: "bolt"; title: qsTr("Reflex low latency")
            description: qsTr("When the game supports it"); showDivider: false
            DesktopSettingsToggle { checked: page.settingsScreen.boolSetting("enableCloudGsync",false); onValueChangedByUser: value => page.settingsScreen.setSetting("enableCloudGsync",value) }
        }
    }
    DesktopSettingsAdvanced { detail: qsTr("Steam Deck identity"); expanded: page.settingsScreen.advancedOpen; onClicked: page.settingsScreen.advancedOpen = !page.settingsScreen.advancedOpen }
    DesktopSettingsDisclosure {
        width: parent.width; expanded: page.settingsScreen.advancedOpen
        sourceComponent: DesktopSettingsPanel {
            width: page.availableWidth; paperStyle: true
            DesktopSettingsRow {
                width: parent.width; paperStyle: true; glyph: "controller"; title: qsTr("Steam Deck identity"); description: qsTr("Unlock Deck resolutions and 90 FPS · refreshes entitlements")
                DesktopSettingsToggle { checked: page.settingsScreen.boolSetting("identifyAsSteamDeck",false); onValueChangedByUser: value => page.settingsScreen.setSetting("identifyAsSteamDeck",value) }
            }
        }
    }
}

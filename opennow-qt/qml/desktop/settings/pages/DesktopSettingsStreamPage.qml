import QtQuick
import OpenNOW

Column {
    id: page
    objectName: "desktopStreamSettings"
    required property real availableWidth
    required property var settingsScreen
    required property Component statsSettingsPageComponent
    property bool statisticsOpen: false

    width: page.availableWidth; spacing: DesktopTokens.px(20)
    DesktopSettingsPanel {
        width: parent.width; paperStyle: true
        DesktopSettingsSection { text: qsTr("STREAM QUALITY") }
        DesktopSettingsResolution {
            width: parent.width; items: page.settingsScreen.resolutionItems()
            value: page.settingsScreen.currentResolutionValue()
            onSelected: value => page.settingsScreen.setSetting("resolution", value)
        }
        DesktopSettingsChoice {
            objectName: "desktopFrameRateControl"
            width: parent.width; glyph: "speed"; title: qsTr("Frame rate")
            description: page.settingsScreen.fpsEntitlementNote()
            readonly property var canonical: ShellStore.canonicalFpsValues().map(value => String(value))
            readonly property string currentRate: Number(page.settingsScreen.valueSetting("fps", 60)) === 0 ? "AUTO" : String(page.settingsScreen.valueSetting("fps", 60))
            items: {
                const values = canonical.indexOf(currentRate) >= 0 ? canonical : [currentRate].concat(canonical)
                const locked = page.settingsScreen.lockedFpsValues().map(value => String(value))
                return values.map(value => ({label: value === "AUTO" ? qsTr("Auto") : qsTr("%1 FPS").arg(value), value: value,
                    disabled: locked.indexOf(value) >= 0,
                    detail: locked.indexOf(value) >= 0 ? page.settingsScreen.fpsLockedHint() : ""}))
            }
            value: currentRate
            onSelected: value => page.settingsScreen.setSetting("fps", value === "AUTO" ? 0 : Number(value))
        }
        DesktopSettingsRow {
            width: parent.width; paperStyle: true; glyph: "wave"; title: qsTr("Bitrate"); description: qsTr("Maximum requested bitrate")
            DesktopSettingsSlider {
                from: 10; to: 200; stepSize: 5
                value: Number(page.settingsScreen.valueSetting("maxBitrateMbps",75)); suffix: " Mbps"
                onCommitted: value => page.settingsScreen.setSetting("maxBitrateMbps",Math.round(value))
            }
        }
        DesktopSettingsRow {
            width: parent.width; paperStyle: true; glyph: "drop"; title: qsTr("Save bandwidth")
            description: qsTr("Lets the server trade resolution and image quality for a steadier frame rate when your connection cannot sustain the selected profile. Off requests no dynamic adjustment. Applies to new sessions.")
            DesktopSettingsToggle {
                objectName: "saveBandwidthToggle"
                checked: page.settingsScreen.boolSetting("saveBandwidth", false)
                Accessible.name: qsTr("Save bandwidth")
                onValueChangedByUser: value => page.settingsScreen.setSetting("saveBandwidth", value)
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
        DesktopSettingsChoice {
            objectName: "colorQualityChoice"
            width: parent.width; glyph: "sun"; title: qsTr("Color quality")
            description: page.settingsScreen.colorQualityFooter(); showDivider: false
            items: ShellStore.settingsOwnerState.colorQualityItems
            value: page.settingsScreen.valueSetting("colorQuality", "8bit_420")
            onSelected: value => page.settingsScreen.setChoice("colorQuality", value)
        }
    }
    DesktopSettingsPanel {
        width: parent.width; paperStyle: true
        DesktopSettingsSection { text: qsTr("IMAGE PROCESSING") }
        DesktopSettingsRow {
            objectName: "upscalingSettingsRow"
            width: parent.width; paperStyle: true; glyph: "monitor"; title: qsTr("Upscaling")
            description: Qt.platform.os === "osx"
                ? qsTr("Spatial upscaling for enlarged video. Uses extra GPU time; falls back to normal scaling when MetalFX is unavailable.")
                : qsTr("FSR 1 upscales enlarged SDR video on the GPU. Uses extra GPU time; HDR and unavailable effects use normal scaling.")
            DesktopSettingsSegmented {
                objectName: "upscalingSelector"
                readonly property string mode: Qt.platform.os === "osx" ? "metalfx" : "fsr1"
                readonly property string current: String(page.settingsScreen.valueSetting("upscaling", "off")) === mode ? mode : "off"
                options: [{label: qsTr("Off"), value: "off"}, {label: Qt.platform.os === "osx" ? "MetalFX" : "FSR 1", value: mode}]
                optionWidth: 90; selectedIndex: options.findIndex(item => item.value === current)
                onSelected: (index,item) => page.settingsScreen.setSetting("upscaling", item.value)
            }
        }
        DesktopSettingsRow {
            objectName: "upscalingSharpnessRow"
            enabled: page.settingsScreen.valueSetting("upscaling", "off") === (Qt.platform.os === "osx" ? "metalfx" : "fsr1")
            visible: enabled
            width: parent.width; paperStyle: true; glyph: "sun"; title: qsTr("Clarity")
            description: Qt.platform.os === "osx"
                ? qsTr("Sharpen details before MetalFX upscaling. Set to 0 to disable.")
                : qsTr("Sharpen details after FSR 1 upscaling. Set to 0 to disable.")
            DesktopSettingsSlider {
                objectName: "upscalingSharpnessSlider"
                accessibleName: qsTr("Clarity")
                from: 0; to: 15; stepSize: 1; suffix: ""
                value: Number(page.settingsScreen.valueSetting("upscalingSharpness", 10))
                onCommitted: value => page.settingsScreen.setSetting("upscalingSharpness", Math.round(value))
            }
        }
        DesktopSettingsRow {
            objectName: "upscalingDenoiseRow"
            enabled: Qt.platform.os === "osx" && page.settingsScreen.valueSetting("upscaling", "off") === "metalfx"
            visible: enabled
            width: parent.width; paperStyle: true; glyph: "drop"; title: qsTr("Noise Reduction")
            description: qsTr("Smooth noise before MetalFX upscaling. Set to 0 to disable.")
            DesktopSettingsSlider {
                objectName: "upscalingDenoiseSlider"
                accessibleName: qsTr("Noise Reduction")
                from: 0; to: 20; stepSize: 1; suffix: ""
                value: Number(page.settingsScreen.valueSetting("upscalingDenoise", 0))
                onCommitted: value => page.settingsScreen.setSetting("upscalingDenoise", Math.round(value))
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
    }
    DesktopSettingsPanel {
        width: parent.width; paperStyle: true
        DesktopSettingsSection { text: qsTr("LATENCY") }
        DesktopSettingsRow {
            width: parent.width; paperStyle: true; glyph: "bolt"; title: qsTr("Reflex low latency")
            description: qsTr("When the game supports it"); showDivider: false
            DesktopSettingsToggle { checked: page.settingsScreen.boolSetting("enableCloudGsync",false); onValueChangedByUser: value => page.settingsScreen.setSetting("enableCloudGsync",value) }
        }
    }
    DesktopSettingsPanel {
        width: parent.width; paperStyle: true
        DesktopSettingsSection { text: qsTr("SESSION") }
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
            DesktopSettingsToggle {
                objectName: "steamBigPictureToggle"
                checked: page.settingsScreen.boolSetting("steamBigPictureMode", false)
                onValueChangedByUser: value => page.settingsScreen.setSetting("steamBigPictureMode", value)
            }
        }
        DesktopSettingsRow {
            width: parent.width; paperStyle: true; glyph: "controller"; title: qsTr("Persistent in-game settings")
            description: qsTr("Keep your in-game graphics settings between sessions for supported games and memberships. Applies to new sessions.")
            DesktopSettingsToggle {
                objectName: "persistentInGameSettingsToggle"
                checked: page.settingsScreen.boolSetting("enablePersistingInGameSettings", true)
                Accessible.name: qsTr("Persistent in-game settings")
                onValueChangedByUser: value => page.settingsScreen.setSetting("enablePersistingInGameSettings", value)
            }
        }
        DesktopSettingsRow {
            width: parent.width; paperStyle: true; glyph: "info"
            title: qsTr("Background stream reminder")
            description: qsTr("Request taskbar or dock attention every 5 minutes while a stream runs in the background. Availability depends on your desktop. Does not prevent AFK timeouts.")
            showDivider: false
            DesktopSettingsToggle {
                objectName: "backgroundStreamReminderToggle"
                checked: page.settingsScreen.valueSetting("backgroundStreamReminder", false) === true
                onValueChangedByUser: value => page.settingsScreen.setSetting("backgroundStreamReminder", value)
            }
        }
    }
    DesktopSettingsPanel {
        width: parent.width; paperStyle: true
        DesktopSettingsRow {
            objectName: "statisticsOverlaySection"
            width: parent.width; paperStyle: true; glyph: "speed"; title: qsTr("Statistics overlay")
            expandable: true; expanded: page.statisticsOpen; showDivider: false
            onExpansionRequested: page.statisticsOpen = !page.statisticsOpen
        }
    }
    DesktopSettingsDisclosure {
        objectName: "streamStatsDisclosure"
        width: parent.width; expanded: page.statisticsOpen
        sourceComponent: page.statsSettingsPageComponent
    }
    DesktopSettingsAdvanced {
        detail: qsTr("Graphics processor · Decoder · Steam Deck identity")
        expanded: page.settingsScreen.advancedOpen
        onClicked: page.settingsScreen.advancedOpen = !page.settingsScreen.advancedOpen
    }
    DesktopSettingsDisclosure {
        width: parent.width; expanded: page.settingsScreen.advancedOpen
        sourceComponent: DesktopSettingsPanel {
            width: page.availableWidth; paperStyle: true
            DesktopSettingsChoice {
                objectName: "graphicsProcessorSelector"
                visible: GraphicsDevices.selectorVisible
                width: parent.width
                title: qsTr("Graphics processor")
                description: GraphicsDevices.savedDeviceUnavailable
                    ? qsTr("Saved GPU unavailable; using Automatic. Changes apply after restarting OpenNOW.")
                    : qsTr("Uses the same GPU for decoding and display. Changes apply after restarting OpenNOW.")
                glyph: "monitor"
                items: GraphicsDevices.choices
                maximumColumns: 2
                readonly property string preferredId: String(page.settingsScreen.valueSetting("windowsGpuDeviceId", ""))
                value: items.some(item => item.value === preferredId && !item.disabled) ? preferredId : ""
                onSelected: value => ShellStore.setSetting("windowsGpuDeviceId", value)
            }
            DesktopSettingsChoice {
                objectName: "streamBackendChoice"
                width: parent.width; glyph: "chip"; title: qsTr("Video backend")
                description: Qt.platform.os === "windows"
                    ? qsTr("Auto uses DX11 hardware decoding. DX12 and Vulkan texture sharing are not supported by the Windows stream view yet. Applies to the next stream.")
                    : qsTr("Choose a supported decoder. Auto never falls back to software. Applies to the next stream.")
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
            DesktopSettingsHevcHelp {
                width: parent.width
                runtimeReady: ShellStore.nativeRuntimeReady
                capabilities: ShellStore.nativeRuntimeCapabilities
                onOpenStoreRequested: url => Qt.openUrlExternally(url)
            }
            DesktopSettingsRow {
                width: parent.width; paperStyle: true; glyph: "controller"; title: qsTr("Steam Deck identity"); description: qsTr("Unlock Deck resolutions and 90 FPS · refreshes entitlements")
                DesktopSettingsToggle { checked: page.settingsScreen.boolSetting("identifyAsSteamDeck",false); onValueChangedByUser: value => page.settingsScreen.setSetting("identifyAsSteamDeck",value) }
            }
        }
    }
}

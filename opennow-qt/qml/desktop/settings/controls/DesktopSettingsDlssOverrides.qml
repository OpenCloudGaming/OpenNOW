import QtQuick
import QtQuick.Controls
import OpenNOW

// Cloud DLSS overrides (Option B): outcome tiles that each set a complete, valid
// override, a Next session summary of the wire values, and a Fine-tune disclosure
// grouped by feature. The core normalizes and sends the stored object unchanged.
Column {
    id: root
    objectName: "dlssOverridesSettings"
    required property var settingsScreen
    property bool fineTuneOpen: false
    property bool customChosen: false
    property bool modelsCustom: false
    spacing: DesktopTokens.px(12)

    readonly property var defaults: ({srMode: 0, srModel: 0, srScale: 100, rrModel: 0, fgMode: 0, fgModel: 0, fgMultiplier: 0})
    readonly property var overrides: Object.assign({}, defaults, ShellStore.settings.dlssOverrides || {})
    readonly property var availability: ShellStore.dlssOverrideAvailability
    readonly property bool srEntitled: ShellStore.dlssCapability("superResolution").entitled === true
    readonly property bool fgEntitled: ShellStore.dlssCapability("frameGeneration").entitled === true
    readonly property bool rrEntitled: ShellStore.dlssCapability("rayReconstruction").entitled === true
    readonly property bool anyEntitled: srEntitled || fgEntitled || rrEntitled
    readonly property var multipliers: ShellStore.dlssFrameGenerationMultipliers
    readonly property bool dynamicEntitled: ShellStore.dlssDynamicFrameGeneration
    readonly property int lowestMultiplier: multipliers.length ? multipliers[0] : 3
    readonly property int highestMultiplier: multipliers.length ? multipliers[multipliers.length - 1] : 3
    readonly property var upsellFeatures: [
        {key: "superResolution", label: qsTr("Super Resolution")},
        {key: "frameGeneration", label: qsTr("Frame Generation")},
        {key: "rayReconstruction", label: qsTr("Ray Reconstruction")}
    ].filter(feature => ShellStore.dlssCapability(feature.key).entitled !== true && ShellStore.dlssCapability(feature.key).upsell === true)

    readonly property var srModes: [
        {value: 0, label: qsTr("In-game"), scale: 100},
        {value: 5, label: qsTr("DLAA"), scale: 100},
        {value: 3, label: qsTr("Quality"), scale: 67},
        {value: 2, label: qsTr("Balanced"), scale: 58},
        {value: 1, label: qsTr("Performance"), scale: 50},
        {value: 4, label: qsTr("Ultra Performance"), scale: 33},
        {value: 6, label: qsTr("Custom"), scale: 75}
    ]
    readonly property var srModels: [0, 1, 2, 3, 4, 5, 6, 7, 11, 12, 13, 14]
    readonly property var rrModels: [0, 1, 5, 7]
    readonly property var fgModels: [0, 1, 2, 3]

    readonly property var tiles: [
        {key: "inGame", glyph: "monitor", label: qsTr("In-game"), detail: qsTr("Each game's own DLSS"), enabled: true},
        {key: "sharpest", glyph: "image", label: qsTr("Sharpest"),
            detail: srEntitled ? qsTr("DLAA · native resolution") : qsTr("Needs Super Resolution"), enabled: srEntitled},
        {key: "balanced", glyph: "star", label: qsTr("Balanced"),
            detail: dynamicEntitled ? qsTr("Balanced · up to %1×").arg(lowestMultiplier) : qsTr("Needs Frame Generation"), enabled: dynamicEntitled},
        {key: "smoothest", glyph: "speed", label: qsTr("Smoothest"),
            detail: dynamicEntitled ? qsTr("Performance · up to %1×").arg(highestMultiplier) : qsTr("Needs Frame Generation"), enabled: dynamicEntitled},
        {key: "custom", glyph: "sliders", label: qsTr("Custom"), detail: qsTr("Your own mix"), enabled: anyEntitled}
    ]
    readonly property string selectedTile: {
        if (customChosen)
            return "custom"
        const match = tiles.find(tile => tile.key !== "custom" && tile.enabled && sameOverrides(preset(tile.key), overrides))
        return match ? match.key : "custom"
    }
    readonly property string modelPreset: {
        const models = [srEntitled ? overrides.srModel : null, fgEntitled ? overrides.fgModel : null,
                        rrEntitled ? overrides.rrModel : null].filter(model => model !== null)
        if (modelsCustom || !models.length)
            return modelsCustom ? "custom" : "inGame"
        if (models.every(model => model === 0)) return "inGame"
        if (models.every(model => model === 1)) return "recommended"
        return "custom"
    }

    function sameOverrides(left, right) {
        return Object.keys(defaults).every(key => Number(left[key]) === Number(right[key]))
    }

    function srScaleFor(mode) {
        const entry = srModes.find(item => item.value === mode)
        return entry ? entry.scale : 100
    }

    function preset(key) {
        const sr = (mode) => srEntitled ? {srModel: 1, srMode: mode, srScale: srScaleFor(mode)} : {srModel: 0, srMode: 0, srScale: 100}
        const rr = {rrModel: rrEntitled ? 1 : 0}
        const fg = (multiplier) => dynamicEntitled ? {fgModel: 1, fgMode: 1, fgMultiplier: multiplier} : {fgModel: 0, fgMode: 0, fgMultiplier: 0}
        if (key === "sharpest") return Object.assign({}, defaults, sr(5), rr)
        if (key === "balanced") return Object.assign({}, defaults, sr(2), rr, fg(lowestMultiplier))
        if (key === "smoothest") return Object.assign({}, defaults, sr(1), rr, fg(highestMultiplier))
        return Object.assign({}, defaults)
    }

    function write(changes) {
        root.settingsScreen.setSetting("dlssOverrides", Object.assign({}, overrides, changes))
    }

    function chooseTile(key) {
        if (key === "custom") {
            customChosen = true
            fineTuneOpen = true
            return
        }
        customChosen = false
        modelsCustom = false
        root.settingsScreen.setSetting("dlssOverrides", preset(key))
    }

    function chooseModelPreset(value) {
        modelsCustom = value === "custom"
        if (value === "custom")
            return
        const model = value === "recommended" ? 1 : 0
        const changes = {}
        if (srEntitled) Object.assign(changes, model ? {srModel: 1} : {srModel: 0, srMode: 0, srScale: 100})
        if (fgEntitled) Object.assign(changes, model ? {fgModel: 1} : {fgModel: 0, fgMode: 0, fgMultiplier: 0})
        if (rrEntitled) changes.rrModel = model
        write(changes)
    }

    function setSrModel(model) {
        write(model === 0 ? {srModel: 0, srMode: 0, srScale: 100} : {srModel: model})
    }

    function setSrMode(mode) {
        const changes = {srMode: mode, srScale: srScaleFor(mode)}
        if (mode !== 0) {
            if (overrides.srModel === 0) changes.srModel = 1
            if (rrEntitled && overrides.rrModel === 0) changes.rrModel = 1
        }
        write(changes)
    }

    function setFgModel(model) {
        write(model === 0 ? {fgModel: 0, fgMode: 0, fgMultiplier: 0} : {fgModel: model})
    }

    function setFgMode(mode) {
        if (mode === 1 && !dynamicEntitled)
            return
        const changes = {fgMode: mode, fgMultiplier: mode === 1
            ? (multipliers.indexOf(overrides.fgMultiplier) >= 0 ? overrides.fgMultiplier : lowestMultiplier) : 0}
        if (mode !== 0 && overrides.fgModel === 0) changes.fgModel = 1
        write(changes)
    }

    function modelLabel(value) {
        if (value === 0) return qsTr("In-game")
        if (value === 1) return qsTr("Recommended")
        return qsTr("Preset %1").arg(String.fromCharCode(63 + value))
    }

    function modelOptions(values) {
        return values.map(value => ({label: value > 1 ? String.fromCharCode(63 + value) : modelLabel(value), value: value}))
    }

    function srModeLabel(mode) {
        const entry = srModes.find(item => item.value === mode)
        return entry ? entry.label : qsTr("In-game")
    }

    function nextSessionSummary() {
        const sr = overrides.srMode === 0 ? qsTr("In-game") : srModeLabel(overrides.srMode) + " " + overrides.srScale + "%"
        const fg = overrides.fgMode === 1 ? qsTr("Dynamic up to %1×").arg(overrides.fgMultiplier) : qsTr("In-game")
        return "SR " + sr + "  ·  FG " + fg + "  ·  RR " + modelLabel(overrides.rrModel)
    }

    function statusDescription() {
        if (!availability)
            return ShellStore.signedIn ? qsTr("Loading your membership entitlements…") : qsTr("Sign in to use DLSS overrides.")
        if (!anyEntitled && !upsellFeatures.length)
            return qsTr("DLSS overrides aren't available on your membership yet.")
        return qsTr("Pick how supported games look and feel. Runs on the cloud rig and applies when your next session starts.")
    }

    DesktopSettingsPanel {
        width: parent.width; paperStyle: true
        DesktopSettingsSection {
            text: qsTr("DLSS OVERRIDES")
            description: root.statusDescription()
            DesktopSettingsButton {
                objectName: "dlssResetButton"
                text: qsTr("Reset to in-game")
                compact: true
                enabled: !root.sameOverrides(root.overrides, root.defaults)
                opacity: enabled ? 1 : 0.45
                onClicked: root.chooseTile("inGame")
            }
        }
        Item {
            width: parent.width
            implicitHeight: tileFlow.implicitHeight + DesktopTokens.px(16)
            Flow {
                id: tileFlow
                x: DesktopTokens.settingsInset
                width: parent.width - DesktopTokens.settingsInset * 2
                spacing: DesktopTokens.px(8)
                readonly property int columns: Math.max(1, Math.min(3, Math.floor((width + spacing) / (DesktopTokens.px(220) + spacing))))
                Repeater {
                    model: root.tiles
                    delegate: AbstractButton {
                        id: tile
                        required property var modelData
                        readonly property bool chosen: root.selectedTile === modelData.key
                        objectName: "dlssTile-" + modelData.key
                        width: Math.floor((tileFlow.width - (tileFlow.columns - 1) * tileFlow.spacing) / tileFlow.columns)
                        height: Math.max(DesktopTokens.px(76), tileCopy.implicitHeight + DesktopTokens.px(28))
                        enabled: modelData.enabled
                        opacity: enabled ? 1 : 0.45
                        hoverEnabled: true
                        checkable: false
                        Accessible.role: Accessible.RadioButton
                        Accessible.name: modelData.label + ", " + modelData.detail
                        Accessible.checked: chosen
                        onClicked: root.chooseTile(modelData.key)
                        Keys.onReturnPressed: event => { tile.clicked(); event.accepted = true }
                        Keys.onEnterPressed: event => { tile.clicked(); event.accepted = true }
                        background: Rectangle {
                            radius: DesktopTokens.px(12)
                            color: tile.chosen ? Theme.focus : tile.hovered ? DesktopTokens.raisedStrong : DesktopTokens.raised
                            border.width: tile.activeFocus ? 2 : 1
                            border.color: tile.activeFocus ? Theme.focus : DesktopTokens.seamSoft
                        }
                        DesktopSettingsIcon {
                            id: tileIcon
                            x: DesktopTokens.px(14); anchors.verticalCenter: parent.verticalCenter
                            width: DesktopTokens.px(20); height: width
                            glyph: tile.modelData.glyph
                            ink: tile.chosen ? Theme.focusText : Theme.label
                        }
                        Column {
                            id: tileCopy
                            anchors.verticalCenter: parent.verticalCenter
                            x: tileIcon.x + tileIcon.width + DesktopTokens.px(12)
                            width: parent.width - x - DesktopTokens.px(10)
                            spacing: DesktopTokens.px(2)
                            Text {
                                width: parent.width; text: tile.modelData.label; wrapMode: Text.WordWrap
                                color: tile.chosen ? Theme.focusText : Theme.label
                                font.family: Theme.bodyFont; font.pixelSize: DesktopTokens.captionSize; font.weight: Font.ExtraBold
                            }
                            Text {
                                width: parent.width; text: tile.modelData.detail; wrapMode: Text.WordWrap
                                color: tile.chosen ? Theme.focusText : Theme.textMuted
                                font.family: Theme.bodyFont; font.pixelSize: DesktopTokens.px(12)
                            }
                        }
                    }
                }
            }
        }
        DesktopSettingsRow {
            objectName: "dlssUpsellRow"
            visible: root.upsellFeatures.length > 0
            width: parent.width; paperStyle: true; glyph: "crown"
            title: root.upsellFeatures.length === 1
                ? qsTr("%1 isn't in your membership").arg(root.upsellFeatures[0].label)
                : qsTr("Some DLSS overrides aren't in your membership")
            description: qsTr("Not included: %1. A membership with these overrides unlocks them.")
                .arg(root.upsellFeatures.map(feature => feature.label).join(", "))
            DesktopSettingsButton {
                text: qsTr("See memberships")
                onClicked: Qt.openUrlExternally("https://www.nvidia.com/geforce-now/memberships/")
            }
        }
        DesktopSettingsRow {
            objectName: "dlssNextSessionRow"
            visible: root.anyEntitled
            width: parent.width; paperStyle: true; glyph: "clock"
            title: qsTr("Next session")
            description: qsTr("What supported games get when your next session starts")
            value: root.nextSessionSummary().toUpperCase()
        }
        DesktopSettingsRow {
            objectName: "dlssFineTuneRow"
            visible: root.anyEntitled
            width: parent.width; paperStyle: true; glyph: "sliders"
            title: qsTr("Fine-tune")
            description: qsTr("Model preset, per-model pickers, render resolution and multiplier")
            expandable: true; expanded: root.fineTuneOpen; showDivider: false
            onExpansionRequested: root.fineTuneOpen = !root.fineTuneOpen
        }
    }

    DesktopSettingsDisclosure {
        objectName: "dlssFineTuneDisclosure"
        width: parent.width
        expanded: root.fineTuneOpen && root.anyEntitled
        sourceComponent: DesktopSettingsPanel {
            width: root.width; paperStyle: true
            DesktopSettingsRow {
                width: parent.width; paperStyle: true; glyph: "grid"
                title: qsTr("Model preset")
                description: root.modelPreset === "custom" ? qsTr("Pick a model for each feature")
                    : root.modelPreset === "recommended" ? qsTr("NVIDIA's recommended model for each feature")
                    : qsTr("Each game keeps its own DLSS models")
                DesktopSettingsSegmented {
                    objectName: "dlssModelPresetSelector"
                    options: [{label: qsTr("In-game"), value: "inGame"}, {label: qsTr("Recommended"), value: "recommended"},
                              {label: qsTr("Custom"), value: "custom"}]
                    optionWidth: 104; selectedIndex: options.findIndex(item => item.value === root.modelPreset)
                    onSelected: (index, item) => root.chooseModelPreset(item.value)
                }
            }

            DesktopSettingsSection { visible: root.srEntitled; text: qsTr("SUPER RESOLUTION") }
            DesktopSettingsChoice {
                objectName: "dlssSrModelChoice"
                visible: root.srEntitled && root.modelPreset === "custom"
                width: parent.width; glyph: "image"; title: qsTr("Super Resolution model")
                description: qsTr("Presets A–F and J–M, or let NVIDIA pick per game")
                items: root.srModels.map(value => ({kind: "choice", label: root.modelLabel(value), value: value}))
                value: root.overrides.srModel
                onSelected: value => root.setSrModel(Number(value))
            }
            DesktopSettingsChoice {
                objectName: "dlssSrModeChoice"
                visible: root.srEntitled
                width: parent.width; glyph: "monitor"; title: qsTr("DLSS Super Resolution")
                description: root.overrides.srMode === 0 ? qsTr("The game's own upscaling setting applies")
                    : qsTr("Renders at %1% of the stream resolution and upscales with AI").arg(root.overrides.srScale)
                items: root.srModes.map(mode => ({kind: "choice", label: mode.label, value: mode.value,
                    detail: mode.value === 0 ? qsTr("Game decides") : mode.value === 6 ? qsTr("33–100%") : mode.scale + "%"}))
                value: root.overrides.srMode
                onSelected: value => root.setSrMode(Number(value))
            }
            DesktopSettingsRow {
                objectName: "dlssInputResolutionRow"
                visible: root.srEntitled && root.overrides.srMode === 6
                width: parent.width; paperStyle: true; glyph: "sliders"
                title: qsTr("Input resolution")
                description: qsTr("Share of the stream resolution the game renders before upscaling")
                DesktopSettingsSlider {
                    objectName: "dlssInputResolutionSlider"
                    accessibleName: qsTr("Input resolution")
                    from: 33; to: 100; stepSize: 1; suffix: "%"
                    value: Number(root.overrides.srScale)
                    onCommitted: value => root.write({srScale: Math.round(value)})
                }
            }

            DesktopSettingsSection { visible: root.fgEntitled; text: qsTr("FRAME GENERATION") }
            DesktopSettingsRow {
                visible: root.fgEntitled && root.modelPreset === "custom"
                width: parent.width; paperStyle: true; glyph: "speed"
                title: qsTr("Frame Generation model")
                description: qsTr("Used when Frame Generation is Dynamic")
                DesktopSettingsSegmented {
                    objectName: "dlssFgModelSelector"
                    options: root.modelOptions(root.fgModels)
                    optionWidth: 96; selectedIndex: options.findIndex(item => item.value === root.overrides.fgModel)
                    onSelected: (index, item) => root.setFgModel(item.value)
                }
            }
            DesktopSettingsRow {
                visible: root.fgEntitled
                width: parent.width; paperStyle: true; glyph: "speed"
                title: qsTr("DLSS Frame Generation")
                description: root.overrides.fgMode === 1 ? qsTr("Adds frames only when needed to reach your frame rate")
                    : qsTr("The game's own frame generation setting applies")
                DesktopSettingsSegmented {
                    objectName: "dlssFgModeSelector"
                    options: [{label: qsTr("In-game"), value: 0}, {label: qsTr("Dynamic"), value: 1, enabled: root.dynamicEntitled}]
                    optionWidth: 96; selectedIndex: options.findIndex(item => item.value === root.overrides.fgMode)
                    onSelected: (index, item) => root.setFgMode(item.value)
                }
            }
            DesktopSettingsRow {
                visible: root.fgEntitled && root.overrides.fgMode === 1
                width: parent.width; paperStyle: true; glyph: "arrows"
                title: qsTr("Frame multiplier")
                description: root.highestMultiplier < 6
                    ? qsTr("Up to %1× with your membership on this cloud rig").arg(root.highestMultiplier)
                    : qsTr("Generated frames per rendered frame, at most")
                DesktopSettingsSegmented {
                    objectName: "dlssMultiplierSelector"
                    options: [3, 4, 5, 6].map(value => ({label: qsTr("Up to %1×").arg(value), value: value}))
                    disabledValues: [3, 4, 5, 6].filter(value => root.multipliers.indexOf(value) < 0)
                    disabledHint: qsTr("Not included with your membership on this cloud rig")
                    optionWidth: 84; selectedIndex: options.findIndex(item => item.value === root.overrides.fgMultiplier)
                    onSelected: (index, item) => root.write({fgMultiplier: item.value})
                }
            }

            DesktopSettingsSection { visible: root.rrEntitled; text: qsTr("RAY RECONSTRUCTION") }
            DesktopSettingsRow {
                visible: root.rrEntitled
                width: parent.width; paperStyle: true; glyph: "sun"; showDivider: false
                title: qsTr("DLSS Ray Reconstruction")
                description: root.modelPreset === "custom" ? qsTr("Ray-traced games only")
                    : qsTr("Follows the model preset · ray-traced games only")
                value: root.modelPreset === "custom" ? "" : root.modelLabel(root.overrides.rrModel).toUpperCase()
                DesktopSettingsSegmented {
                    objectName: "dlssRrModelSelector"
                    visible: root.modelPreset === "custom"
                    options: root.modelOptions(root.rrModels)
                    optionWidth: 96; selectedIndex: options.findIndex(item => item.value === root.overrides.rrModel)
                    onSelected: (index, item) => root.write({rrModel: item.value})
                }
            }
        }
    }
}

import QtQuick
import QtQuick.Controls
import QtQuick.Dialogs
import OpenNOW

FocusScope {
    id: root
    objectName: "consoleSettingsScreen"
    property int initialSection: 1
    property bool initialDropdownOpen: false
    property int selectedSection: initialSection
    property bool dropdownOpen: initialDropdownOpen
    property bool dropdownPresented: initialDropdownOpen
    property bool proxyEditorOpen: false
    property string proxyEditorMessage: ""
    property bool shortcutEditorOpen: false
    property string shortcutEditorKey: ""
    property string shortcutEditorTitle: ""
    property string shortcutEditorMessage: ""
    property string dropdownTitle: qsTr("Choose a value")
    property string dropdownKey: ""
    property var dropdownLabels: []
    property var dropdownValues: []
    property var dropdownDisabledValues: []
    property string warningKind: ""
    property string presentedWarningKind: ""
    onWarningKindChanged: if (warningKind !== "") presentedWarningKind = warningKind
    readonly property bool warningOpen: warningKind !== ""
    property string pluginSheetId: ""
    property string pluginWarningId: ""
    readonly property var pluginStore: ShellStore.pluginOwnerState
    readonly property var sourceStore: ShellStore.sourceOwnerState
    readonly property var pluginSheetPlugin: pluginStore.pluginById(pluginSheetId)
    readonly property bool pluginSheetOpen: pluginSheetId !== ""
    readonly property bool pluginPreviewOpen: pluginStore.previewSourceId !== ""
    readonly property bool sheetOpen: dropdownOpen || warningOpen || proxyEditorOpen || shortcutEditorOpen
        || pluginSheetOpen || pluginPreviewOpen
    property var rows: settingsModel()
    property int rowCount: 0
    property bool restoringRows: false
    property string focusedRowKey: ""
    readonly property real dropdownPanelHeight: Math.max(height, 1080) - 240
    readonly property var sections: [
        {name:qsTr("Account"), icon:"settings-account.svg", color:Theme.violet},
        {name:qsTr("Streaming"), icon:"settings-streaming.svg", color:Theme.focus},
        {name:qsTr("Video & display"), icon:"settings-video.svg", color:Theme.yellow},
        {name:qsTr("Input & controllers"), icon:"settings-input.svg", color:Theme.mint},
        {name:qsTr("Network"), icon:"settings-network.svg", color:Theme.coral},
        {name:qsTr("Themes"), icon:"settings-themes.svg", color:Theme.face},
        {name:qsTr("Advanced"), icon:"settings-advanced.svg", color:"#252A35"},
        {name:qsTr("Recording"), icon:"settings-video.svg", color:Theme.coral},
        {name:qsTr("Plugins"), icon:"settings-plugins.svg", color:Theme.violet}
    ]
    DesktopSettingsShortcutBinding { id: shortcutBinding }

    function titleCase(value) {
        const words = String(value || "").split("-").join(" ").split("_").join(" ").split(" ")
        for (let index = 0; index < words.length; ++index) {
            if (words[index].length > 0)
                words[index] = words[index][0].toUpperCase() + words[index].slice(1)
        }
        return words.join(" ")
    }

    function choice(title, description, key, values, labels, control, disabledValues) {
        const current = key === "controllerInputSource" ? ControllerInput.inputControllerId : ShellStore.settings[key]
        const index = values.indexOf(current)
        return {t:title, d:description, v:index >= 0 ? labels[index] : key === "controllerInputSource" ? qsTr("Selected controller disconnected") : key === "windowsGpuDeviceId" ? qsTr("Automatic") : root.titleCase(current), key:key, values:values, labels:labels, control:control || "dropdown", disabledValues:disabledValues || []}
    }

    function descriptorChoice(title, description, key, items) {
        const row = choice(title, description, key, items.map(item => item.value),
            items.map(item => item.label), "dropdown", items.filter(item => item.disabled).map(item => item.value))
        row.details = items.map(item => item.detail || "")
        return row
    }

    function toggle(title, description, key, onLabel, offLabel) {
        return {t:title, d:description, v:Boolean(ShellStore.settings[key]) ? (onLabel || qsTr("On")) : (offLabel || qsTr("Off")), key:key, toggle:true, control:"toggle"}
    }

    function shortcut(title, description, key) {
        return {t:title, d:description, v:shortcutBinding.value(key) || qsTr("Not set"), key:key, action:"shortcut-editor", shortcut:true}
    }

    function aspectForResolution(value) {
        const parts = String(value || "").split("x")
        if (parts.length !== 2)
            return ""
        const ratio = Number(parts[0]) / Math.max(1, Number(parts[1]))
        if (Math.abs(ratio - 16 / 9) < 0.05) return "16:9"
        if (Math.abs(ratio - 16 / 10) < 0.05) return "16:10"
        if (Math.abs(ratio - 21 / 9) < 0.08) return "21:9"
        if (Math.abs(ratio - 32 / 9) < 0.08) return "32:9"
        if (Math.abs(ratio - 4 / 3) < 0.05) return "4:3"
        return ""
    }

    function resolutionRowData() {
        const current = String(ShellStore.settings.resolution || "")
        const options = []
        let group = ""
        for (const item of ShellStore.resolutionItems()) {
            if (item.kind === "heading") {
                group = item.label
                continue
            }
            options.push({label:item.label, value:item.value, detail:item.detail || "", disabled:item.disabled === true, group:group})
        }
        if (current !== "" && !options.some(option => option.value === current))
            options.unshift({label:current.replace("x", "×"), value:current, detail:qsTr("Current resolution"), disabled:false, group:qsTr("CURRENT")})
        return options
    }

    function resolutionLabel(value) {
        const option = resolutionRowData().find(item => item.value === value)
        return option ? option.label + " · " + option.detail : String(value || "").replace("x", "×")
    }

    function openInitialDropdown() {
        const rows = settingsModel()
        for (let index = 0; index < rows.length; ++index) {
            if (rows[index].values) {
                settingsList.currentIndex = index
                openChoices(rows[index])
                return
            }
        }
    }

    function fpsChoices() {
        return ShellStore.canonicalFpsValues()
    }

    function fpsLockedValues() {
        return ShellStore.lockedFpsValues(String(ShellStore.settings.resolution || ""))
    }

    function fpsNote() {
        if (!ShellStore.subscription)
            return ShellStore.signedIn
                ? qsTr("Loading your membership entitlements…")
                : qsTr("Sign in — only entitled rates stay selectable")
        const entitled = ShellStore.entitledFpsForResolution(String(ShellStore.settings.resolution || ""))
        const tier = ShellStore.subscription.membershipTier
            ? String(ShellStore.subscription.membershipTier).toUpperCase()
            : qsTr("Membership")
        if (entitled.length === 0)
            return qsTr("Only rates your membership entitles are selectable")
        const resolution = String(ShellStore.settings.resolution || "")
        const selectable = ShellStore.selectableFpsValues(resolution)
        const top = selectable.length ? selectable[selectable.length - 1] : entitled[entitled.length - 1]
        const note = qsTr("Only rates your membership entitles are selectable · %1 up to %2 FPS")
            .arg(tier).arg(top)
        const reason = ShellStore.lockedFpsReason()
        return top < entitled[entitled.length - 1] && reason !== "" ? note + " · " + reason : note
    }

    function captureShortcut(event) {
        event.accepted = true
        if (event.isAutoRepeat)
            return
        if (event.key === Qt.Key_Escape) {
            shortcutEditorOpen = false
            return
        }
        const result = shortcutBinding.validate(shortcutEditorKey, event)
        if (result.error) {
            shortcutEditorMessage = result.error
            return
        }
        ShellStore.setSetting(shortcutEditorKey, result.chord)
        shortcutEditorOpen = false
    }

    function proxyDisplay(value) {
        const raw = String(value || "")
        if (raw === "")
            return "Not set"
        return raw.replace(/\/\/[^@/]+@/, "//••••@")
    }

    function proxyLooksValid(value) {
        const raw = String(value || "").trim()
        if (raw === "")
            return true
        return /^(?:(?:https?|socks4|socks5):\/\/)?(?:[^\s/@]+(?::[^\s/@]*)?@)?[^\s/:]+:\d{1,5}\/?$/i.test(raw)
    }

    function settingsModel() {
        return sectionRows().map(row => {
            if (!row || !row.values || row.control === "slider" || row.control === "colors")
                return row
            const fixedSheet = ["selectedSourceId", "resolution", "region", "controllerInputSource", "windowsGpuDeviceId",
                "gameLanguage", "keyboardLayout", "appLanguage", "colorQuality"].indexOf(row.key) >= 0
            const shortDropdown = row.control === "dropdown" && row.values.length >= 2 && row.values.length <= 6
                && !(row.details && row.details.some(detail => detail))
            const longSegments = row.control === "segments" && row.values.length > 4
            if (fixedSheet || !(shortDropdown || longSegments))
                return row
            return Object.assign({}, row, {control:"cycler"})
        })
    }

    function pluginStateLabel(plugin) {
        if (pluginStore.busyId === plugin.id)
            return qsTr("Working…")
        if (plugin.state === "ready")
            return qsTr("Running")
        if (plugin.state === "starting")
            return qsTr("Starting…")
        if (plugin.state === "failed")
            return qsTr("Failed")
        return qsTr("Off")
    }

    function pluginSummary(plugin) {
        const parts = [qsTr("Version %1").arg(String(plugin.version || "")), String(plugin.publisher || ""),
            plugin.builtin === true ? qsTr("Built in") : qsTr("Community")].filter(part => part !== "")
        const error = plugin.lastError ? String(plugin.lastError.message || plugin.lastError.code || "") : ""
        return parts.join(" · ") + (error !== "" ? "\n" + error : "")
    }

    function pluginRows() {
        if (!pluginStore.available)
            return [{t:qsTr("Plugins are unavailable"), d:ShellStore.ready ? qsTr("This OpenNOW core does not support plugins.")
                : qsTr("Plugins appear when OpenNOW finishes starting."), info:true}]
        const browse = sourceStore.available && sourceStore.playableSources.length > 1 ? [{
            t:qsTr("Browse with"), d:qsTr("The service Home and Library show. A running session keeps its own service."),
            v:sourceStore.selectedSource ? String(sourceStore.selectedSource.name || sourceStore.selectedSourceId) : "",
            key:"selectedSourceId", values:sourceStore.playableSources.map(source => source.id),
            labels:sourceStore.playableSources.map(source => String(source.name || source.id)),
            control:"dropdown", plainText:true
        }] : []
        const rows = browse
        for (const plugin of pluginStore.plugins) {
            rows.push({t:String(plugin.name || plugin.id), d:root.pluginSummary(plugin), v:root.pluginStateLabel(plugin),
                action:"plugin", pluginId:plugin.id, plainText:true, danger:plugin.state === "failed"})
            for (const domain of ["provider", "stream"])
                rows.push(...root.sourceSettingRows(plugin.id, domain))
        }
        if (pluginStore.error !== "")
            rows.unshift({t:qsTr("Plugins need attention"), d:pluginStore.error, info:true, plainText:true})
        rows.push({t:qsTr("Install plugins in Desktop mode"),
            d:qsTr("Installing a plugin means choosing its package file and reading a security warning. Switch to Desktop mode to install one."),
            info:true})
        return rows
    }

    function sourceSettingKey(domain, sourceId, key) {
        return "source-setting:" + JSON.stringify([domain, sourceId, key])
    }

    function sourceSettingTarget(rowKey) {
        if (String(rowKey).indexOf("source-setting:") !== 0)
            return null
        const parts = JSON.parse(String(rowKey).slice(15))
        const view = (parts[0] === "stream" ? sourceStore.streamSettingsViews : sourceStore.settingsViews)[parts[1]]
        const definition = view ? view.settings.find(item => item.key === parts[2]) || null : null
        return definition ? {domain: parts[0], sourceId: parts[1], definition: definition} : null
    }

    function numericSettingValues(control, current) {
        const min = Number(control.min)
        const max = Number(control.max)
        const step = Number(control.step || 1)
        const decimals = (String(step).split(".")[1] || "").length
        const count = Math.floor((max - min) / step + 1e-9)
        const stride = Math.max(1, Math.ceil(count / 24))
        const values = []
        for (let index = 0; index <= count; index += stride)
            values.push(Number((min + index * step).toFixed(decimals)))
        if (values[values.length - 1] !== max)
            values.push(max)
        if (!values.some(value => Math.abs(value - current) < step / 2))
            values.push(current)
        return values.sort((left, right) => left - right)
    }

    function sourceSettingRows(sourceId, domain) {
        const source = sourceId !== sourceStore.gfnId ? sourceStore.sourceById(sourceId) : null
        const view = source && source.enabled === true
            ? (domain === "stream" ? sourceStore.streamSettingsViews : sourceStore.settingsViews)[sourceId] : null
        const detail = domain === "stream" ? qsTr("Stream quality for this service") : String(source ? source.name || source.id : "")
        return (view ? view.settings : []).map(definition => {
            const kind = definition.control.kind
            const key = root.sourceSettingKey(domain, sourceId, definition.key)
            const base = {t:String(definition.label), d:detail, key:key, sourceSetting:true, plainText:true}
            if (kind === "boolean")
                return Object.assign(base, {toggle:true, toggleState:definition.value.value === true})
            if (kind === "choice") {
                const choices = definition.control.choices || []
                const index = choices.findIndex(choice => choice.value === definition.value.value)
                return Object.assign(base, {control:"cycler", values:choices.map(choice => choice.value),
                    labels:choices.map(choice => String(choice.label)), selectedIndex:index,
                    v:index >= 0 ? String(choices[index].label) : String(definition.value.value)})
            }
            if (kind === "integer" || kind === "number") {
                const current = Number(definition.value.value)
                const values = root.numericSettingValues(definition.control, current)
                const min = Number(definition.control.min)
                const max = Number(definition.control.max)
                return Object.assign(base, {control:"slider", values:values, labels:values.map(String),
                    selectedIndex:values.findIndex(value => Math.abs(value - current) < 1e-9), v:String(current),
                    sliderPercent:max > min ? (current - min) / (max - min) : 0})
            }
            return Object.assign(base, {info:true, v:String(definition.value.value)})
        })
    }

    function loadSourceSettings() {
        for (const source of sourceStore.sources)
            if (source.id !== sourceStore.gfnId && source.enabled === true)
                sourceStore.loadSettings(source.id)
    }

    onSelectedSectionChanged: if (selectedSection === 8) loadSourceSettings()

    function pluginSheetOptions() {
        const plugin = root.pluginSheetPlugin
        if (!plugin)
            return []
        const options = [{label:plugin.enabled === true ? qsTr("Turn off") : qsTr("Turn on"), value:"toggle",
            detail:plugin.required === true ? qsTr("Required by OpenNOW") : "", disabled:plugin.required === true || pluginStore.busyId !== ""}]
        options.push({label:qsTr("Browse catalog"), value:"browse",
            detail:pluginStore.catalogAvailable && pluginStore.catalogReady(plugin) ? "" : qsTr("Turn it on first"),
            disabled:!pluginStore.catalogAvailable || !pluginStore.catalogReady(plugin)})
        const source = plugin.id !== sourceStore.gfnId ? sourceStore.sourceById(plugin.id) : null
        const auth = source ? sourceStore.authState(source.id) : null
        if (auth && auth.state === "signed-in")
            options.push({label:qsTr("Sign out"), value:"source-sign-out",
                detail:String(auth.account && auth.account.name || ""), disabled:false})
        if (plugin.builtin !== true && plugin.required !== true)
            options.push({label:qsTr("Remove…"), value:"remove", detail:"", disabled:pluginStore.busyId !== ""})
        return options
    }

    function pluginSheetDescription() {
        const plugin = root.pluginSheetPlugin
        if (!plugin)
            return ""
        const lines = [plugin.trust === "builtin" ? qsTr("Built into OpenNOW.")
            : qsTr("Unsigned native code · publisher not verified"), root.pluginSummary(plugin)]
        if (String(plugin.description || "") !== "")
            lines.push(String(plugin.description))
        return lines.join("\n")
    }

    function openPluginSheet(id) {
        if (!pluginStore.pluginById(id))
            return
        root.pluginSheetId = id
        if (id !== sourceStore.gfnId)
            sourceStore.loadSettings(id)
        pluginSheet.focusedIndex = 0
        pluginSheet.syncFocus()
        pluginSheet.forceActiveFocus()
    }

    function closePluginSheet() {
        root.pluginSheetId = ""
        Qt.callLater(() => { if (!root.sheetOpen) settingsList.forceActiveFocus() })
    }

    function choosePluginOption(index) {
        const plugin = root.pluginSheetPlugin
        const option = pluginSheet.options[index]
        if (!plugin || !option || option.disabled)
            return
        root.closePluginSheet()
        if (option.value === "toggle") {
            pluginStore.setEnabled(plugin.id, plugin.enabled !== true)
        } else if (option.value === "browse") {
            if (pluginStore.openPreview(plugin.id))
                Qt.callLater(() => pluginPreviewSearch.forceActiveFocus())
        } else if (option.value === "source-sign-out") {
            sourceStore.signOut(plugin.id)
        } else if (option.value === "remove") {
            root.pluginWarningId = plugin.id
            root.openWarning("plugin-remove")
        }
    }

    function closePluginPreview() {
        pluginStore.closePreview()
        Qt.callLater(() => { if (!root.sheetOpen) settingsList.forceActiveFocus() })
    }

    function sectionRows() {
        const settings = ShellStore.settings || ({})
        if (root.selectedSection === 8)
            return root.pluginRows()
        if (root.selectedSection === 0) {
            const user = ShellStore.authSession && ShellStore.authSession.user ? ShellStore.authSession.user : ({})
            const accountName = String(user.displayName || qsTr("OpenNOW profile"))
            const membership = ShellStore.subscription && ShellStore.subscription.membershipTier
                ? String(ShellStore.subscription.membershipTier) : String(user.membershipTier || "—")
            return [
                {t:"Profile", control:"profile", height:120, initial:accountName.slice(0,1).toUpperCase(), name:accountName, tier:membership.toUpperCase(), subtitle:ShellStore.signedIn ? qsTr("NVIDIA account · signed in on this PC") : qsTr("Connect securely with NVIDIA"), meta:ShellStore.sessionPersistence === "os-credential-store" ? qsTr("Protected by the operating system credential store") : qsTr("Session-only profile"), v:ShellStore.signedIn ? qsTr("Manage on nvidia.com") : qsTr("Sign in"), route:ShellStore.signedIn ? "accounts" : "sign-in"},
                {t:"Profiles", d:"Each profile has its own My games shelf and settings", v:qsTr("%1 saved").arg(ShellStore.savedAccounts.length), route:"accounts"},
                {t:"Profile PIN", d:"Ask for a 4-digit PIN when switching to this profile", v:"Set up", route:"profile-pin"},
                toggle(qsTr("Persistent in-game settings"), qsTr("Keep your in-game graphics settings between sessions for supported games and memberships. Applies to new sessions."), "enablePersistingInGameSettings"),
                toggle("Discord Rich Presence", "Show what you're playing on Discord", "discordRichPresence"),
                {t:qsTr("Usage & bug reports · Experimental"), d:qsTr("Send usage statistics and error reports with logs to the developer, identified by your GeForce NOW username or e-mail"), v:ShellStore.bugReports.enabled ? qsTr("On") : qsTr("Off"), action:"automatic-bug-reports"},
                {t:"Sign out", d:"Removes the NVIDIA token from this PC; My games stay", v:"Sign out of NVIDIA", action:"sign-out", danger:true},
                {t:"Game accounts", d:"Steam, Epic, Ubisoft and Xbox", v:qsTr("%1 detected").arg(ShellStore.gameAccounts.length), route:"game-accounts"},
                {t:"Persistent storage", d:ShellStore.subscription && ShellStore.subscription.storageAddon ? (ShellStore.subscription.storageAddon.regionName || "Cloud storage active") : "Manage cloud storage locations", v:"Open", route:"persistent-storage"}
            ]
        }
        if (root.selectedSection === 1) {
            const codecValues = ["auto", "av1", "h264", "h265"]
            const codecLabels = ["Auto", "AV1", "H.264", "H.265"]
            const codecDisabled = ShellStore.codecsDisabledByProfile()
            const capsDisabled = codecValues.filter(value => value !== "auto" && !ShellStore.codecAvailable(value))
            const disabledCodecs = codecDisabled.concat(capsDisabled.filter(value => codecDisabled.indexOf(value) < 0))
            const frameGeneration = String(settings.frameGeneration || "off") === "2x"
            const hdrAvailable = HdrOutput.supported && ShellStore.hdrDecoderAvailable()
            const hdrTierOk = ShellStore.tenBitAllowedByMembership()
            const hdrDescription = !hdrTierOk ? qsTr("HDR10 requires a Performance or Ultimate membership.")
                : HdrOutput.supported && !ShellStore.hdrDecoderAvailable()
                ? qsTr("HDR requires a supported 10-bit H.265 or AV1 hardware decoder.") : HdrOutput.status
            return [
                {t:"Codec", d:"Auto prefers AV1, then H.265, then H.264", v:root.titleCase(settings.codec || "auto"), key:"codec", values:codecValues, labels:codecLabels, segmentLabels:codecLabels, control:"segments", selectedIndex:codecValues.indexOf(String(settings.codec || "auto")), disabledValues:disabledCodecs},
                choice("Fallback codec", "Used when the preferred codec isn't offered by the rig", "fallbackCodec", ["auto","h264","h265"], ["Auto","H.264","H.265"], "dropdown", disabledCodecs),
                descriptorChoice(qsTr("Color quality"), ShellStore.settingsOwnerState.colorDescription, "colorQuality", ShellStore.settingsOwnerState.colorQualityItems),
                {t:qsTr("HDR"), d:hdrDescription, v:Boolean(settings.enableHdr) ? qsTr("On") : qsTr("Off"), key:"enableHdr", values:[false,true], labels:[qsTr("Off"),qsTr("On")], control:"segments", selectedIndex:Boolean(settings.enableHdr) ? 1 : 0, disabledValues:(hdrAvailable && hdrTierOk) ? [] : [true]},
                {t:"Max bitrate", d:"Maximum requested stream bitrate", v:Number(settings.maxBitrateMbps || 75) + " Mbps", key:"maxBitrateMbps", values:[0.22,1,5,10,25,50,75,100,150,200], labels:["0.22 Mbps","1 Mbps","5 Mbps","10 Mbps","25 Mbps","50 Mbps","75 Mbps","100 Mbps","150 Mbps","200 Mbps"], control:"slider", sliderPercent:Number(settings.maxBitrateMbps || 75) / 200},
                toggle(qsTr("Save bandwidth"), qsTr("Lets the server trade resolution and image quality for a steadier frame rate when your connection cannot sustain the selected profile. Off requests no dynamic adjustment. Applies to new sessions."), "saveBandwidth"),
                {t:qsTr("Frame generation (Experimental)"), d:qsTr("Targets 120 displayed FPS from a 60 FPS stream. Requires a fast GPU and 120 Hz display; adds latency and artifacts."), v:frameGeneration ? qsTr("2×") : qsTr("Off"), key:"frameGeneration", values:["off","2x"], labels:[qsTr("Off"),qsTr("2×")], control:"segments", selectedIndex:frameGeneration ? 1 : 0},
                choice(qsTr("Upscaling"), Qt.platform.os === "osx"
                    ? qsTr("Spatial upscaling for enlarged video. Uses extra GPU time; falls back to normal scaling when MetalFX is unavailable.")
                    : qsTr("FSR 1 upscales enlarged SDR video on the GPU. Uses extra GPU time; HDR and unavailable effects use normal scaling."),
                    "upscaling", ["off", Qt.platform.os === "osx" ? "metalfx" : "fsr1"], [qsTr("Off"), Qt.platform.os === "osx" ? "MetalFX" : "FSR 1"], "segments"),
                ...[
                    {key:"upscalingSharpness", title:qsTr("Clarity"), description:Qt.platform.os === "osx" ? qsTr("Sharpen details before MetalFX upscaling. Set to 0 to disable.") : qsTr("Sharpen details after FSR 1 upscaling. Set to 0 to disable."), maximum:15, fallback:10},
                    ...(Qt.platform.os === "osx" ? [{key:"upscalingDenoise", title:qsTr("Noise Reduction"), description:qsTr("Smooth noise before MetalFX upscaling. Set to 0 to disable."), maximum:20, fallback:0}] : [])
                ].map(setting => {
                    const value = Number(settings[setting.key] ?? setting.fallback)
                    const values = Array.from({length:setting.maximum + 1}, (_, index) => index)
                    return {t:setting.title, d:setting.description, v:String(value), key:setting.key,
                        values:values, labels:values.map(String), control:"slider", sliderPercent:value / setting.maximum,
                        info:settings.upscaling !== (Qt.platform.os === "osx" ? "metalfx" : "fsr1")}
                }),
                toggle("Cloud G-Sync", "Variable refresh on G-Sync and FreeSync displays", "enableCloudGsync"),
                toggle(qsTr("Stats overlay on launch"), qsTr("Show stream statistics when a session starts"), "showStatsOnLaunch"),
                choice("Stats overlay position", "FPS, RTT, loss and bitrate readout", "statsOverlayPosition", ["top-right","top-left","bottom-right","bottom-left"], ["Top-right","Top-left","Bottom-right","Bottom-left"])
            ]
        }
        if (root.selectedSection === 2) {
            const resolutionOptions = resolutionRowData()
            const frameRates = fpsChoices()
            const shader = settings.videoShader || ({enabled:false})
            const shaderValues = [
                {enabled:false,sharpen:40,saturation:100,contrast:100,brightness:100,vibrance:0,filmGrain:0},
                {enabled:true,sharpen:55,saturation:100,contrast:100,brightness:100,vibrance:0,filmGrain:0},
                {enabled:true,sharpen:65,saturation:108,contrast:104,brightness:100,vibrance:12,filmGrain:0},
                {enabled:true,sharpen:20,saturation:88,contrast:112,brightness:96,vibrance:-5,filmGrain:22}
            ]
            const shaderIndex = shader.enabled ? (Number(shader.filmGrain || 0) > 0 ? 3 : Number(shader.vibrance || 0) > 0 ? 2 : 1) : 0
            return [
                ...(GraphicsDevices.selectorVisible ? [choice(qsTr("Graphics processor"),
                    GraphicsDevices.savedDeviceUnavailable
                        ? qsTr("Saved GPU unavailable; using the first GPU that can hardware-decode. Changes apply after restarting OpenNOW.")
                        : qsTr("Automatic uses the first GPU that can hardware-decode and lists each GPU's codecs. The same GPU decodes and displays. Changes apply after restarting OpenNOW."),
                    "windowsGpuDeviceId", GraphicsDevices.choices.map(item => item.value),
                    GraphicsDevices.choices.map(item => item.detail ? item.label + " — " + item.detail : item.label), "dropdown",
                    GraphicsDevices.choices.filter(item => item.disabled).map(item => item.value))] : []),
                toggle(qsTr("Steam Big Picture mode"), qsTr("Request gamepad-friendly launchers such as Steam Big Picture. Applies to new GeForce NOW sessions only."), "steamBigPictureMode"),
                {t:"Display", d:"The Qt stream surface uses the current display", v:"Monitor 1 · current display", info:true},
                {t:qsTr("Resolution"), d:qsTr("Exact stream size · up / down to browse, A to pick"), key:"resolution",
                    v:resolutionLabel(String(settings.resolution || "")), values:resolutionOptions.map(option => option.value),
                    labels:resolutionOptions.map(option => option.label), details:resolutionOptions.map(option => option.detail),
                    groups:resolutionOptions.map(option => option.group), control:"dropdown",
                    disabledValues:resolutionOptions.filter(option => option.disabled).map(option => option.value)},
                choice("Frame rate", root.fpsNote(), "fps", frameRates, frameRates.map(value => String(value)), "segments", root.fpsLockedValues()),
                toggle(qsTr("Fullscreen when session is ready"), qsTr("Automatically enter fullscreen when your session is ready. F11 toggles fullscreen during play."), "autoFullScreen"),
                {t:"Video shader", d:"Post-process on this device after decode", v:["Off","Sharpen","FidelityFX","CRT"][shaderIndex], key:"videoShader", values:shaderValues, labels:["Off","Sharpen","FidelityFX","CRT"], control:"segments", selectedIndex:shaderIndex},
                choice("Cursor", qsTr("Lock the pointer to the game window"), "nativeCursorOverlay", [true,false], ["Lock to window","Free"], "segments"),
            ]
        }
        if (root.selectedSection === 3) {
            const rows = []
            const controllerCards = []
            for (let index = 0; index < Math.min(4, ControllerInput.controllers.length); ++index) {
                const controller = ControllerInput.controllers[index]
                controllerCards.push({slot:controller.slot, name:controller.name, connected:true, battery:controller.batteryPercent >= 0 ? controller.batteryPercent + "%" : qsTr("Ready")})
            }
            while (controllerCards.length < 4)
                controllerCards.push({slot:controllerCards.length + 1, name:qsTr("Controller %1").arg(controllerCards.length + 1), connected:false, battery:""})
            rows.push({t:"Controllers", control:"controllers", height:131, controllers:controllerCards, route:"joining"})
            rows.push({t:"Button glyphs", d:"Detected automatically from the active controller", v:"Auto", info:true})
            rows.push(choice(qsTr("Controller input source"),
                qsTr("Choose one device as Player 1 if a controller appears twice. Selection lasts until app restart; select again after reconnecting."),
                "controllerInputSource", [0].concat(ControllerInput.availableControllers.map(controller => Number(controller.instanceId))),
                [qsTr("All controllers (multiplayer)")].concat(ControllerInput.availableControllers.map(controller => qsTr("Device %1 · %2").arg(controller.slot).arg(controller.name)))))
            rows.push(toggle("Gyroscope", "Forward motion data to the rig", "enableGyroscopeControls"))
            rows.push(toggle(qsTr("Clipboard paste"), qsTr("Paste local text into the stream with Ctrl+V (Command+V on macOS). Up to 64 KiB per paste. No automatic clipboard sync."), "clipboardPaste"))
            for (const setting of [
                {key:"controllerLeftStickDeadzone", title:qsTr("Left stick dead zone"), description:qsTr("Ignore stick drift during gameplay. Default: 5%. The remaining travel is rescaled to full range."), maximum:50, fallback:5},
                {key:"controllerRightStickDeadzone", title:qsTr("Right stick dead zone"), description:qsTr("Ignore stick drift during gameplay. Default: 5%. Set to 0% to leave dead zones to the game."), maximum:50, fallback:5},
                {key:"controllerVibrationIntensity", title:qsTr("Controller vibration"), description:qsTr("Scale game vibration on supported controllers. Set to 0% to disable."), maximum:100, fallback:100}
            ]) {
                const value = Number(settings[setting.key] ?? setting.fallback)
                const values = Array.from({length:setting.maximum + 1}, (_, index) => index)
                rows.push({t:setting.title, d:setting.description, v:value + "%", key:setting.key,
                    values:values, labels:values.map(value => value + "%"), control:"slider", sliderPercent:value / setting.maximum})
            }
            rows.push({t:"Mouse sensitivity", d:"Acceleration off · raw input", v:Number(settings.mouseSensitivity || 1).toFixed(1) + "×", key:"mouseSensitivity", values:[0.5,0.75,1,1.25,1.5], labels:["0.5×","0.75×","1.0×","1.25×","1.5×"], control:"slider", sliderPercent:Number(settings.mouseSensitivity || 1) / 1.5})
            rows.push(descriptorChoice(qsTr("Game language"), ShellStore.settingsOwnerState.gameLanguageDescription,
                "gameLanguage", ShellStore.settingsOwnerState.gameLanguageItems))
            rows.push({t:qsTr("Game language metadata"), d:ShellStore.settingsOwnerState.languageStatusText,
                v:qsTr("Retry"), action:"retry-languages", info:!ShellStore.settingsOwnerState.ready || ShellStore.settingsOwnerState.languageState === "loading"})
            rows.push(descriptorChoice(qsTr("Keyboard layout"), ShellStore.settingsOwnerState.keyboardLayoutDescription,
                "keyboardLayout", ShellStore.keyboardLayoutItems))
            rows.push({t:"Shortcuts", d:qsTr("Keyboard shortcuts for stream controls. Shows your current bindings."), v:"Edit shortcuts", key:"shortcutToggleStats", action:"shortcut-editor"})
            rows.push(choice(qsTr("Microphone"), ShellStore.microphoneCaptureSupported ? ShellStore.microphoneDescription : qsTr("Microphone capture is unavailable in this build."),
                "microphoneMode", ["disabled", "voice-activity"], [qsTr("Disabled"), qsTr("Open microphone")], "segments",
                ShellStore.microphoneCaptureSupported ? [] : ["voice-activity"]))
            rows.push(shortcut("Toggle stats", "Cycle the Qt stream statistics overlay", "shortcutToggleStats"))
            rows.push(shortcut("Toggle pointer lock", "Capture or release the mouse on the Qt stream surface", "shortcutTogglePointerLock"))
            rows.push(shortcut("Toggle fullscreen", "Switch the Qt application surface between fullscreen and windowed", "shortcutToggleFullscreen"))
            rows.push(shortcut("Stop stream", "End the active GeForce NOW session", "shortcutStopStream"))
            rows.push(shortcut("Toggle anti-AFK", "Enable or disable the session activity helper", "shortcutToggleAntiAfk"))
            rows.push(shortcut("Screenshot", "Save the current decoded frame", "shortcutScreenshot"))
            rows.push(shortcut(qsTr("Toggle recording"), qsTr("Start or stop a source-quality recording during a stream."), "shortcutToggleRecording"))
            rows.push(shortcut(qsTr("Save replay clip"), qsTr("Save the buffered video and audio. Requires the replay buffer to be enabled for this session."), "shortcutSaveClip"))
            return rows
        }
        if (root.selectedSection === 4) {
            const regionValues = [""]
            const regionLabels = ["Automatic"]
            for (let index = 0; index < ShellStore.regions.length; ++index) {
                regionValues.push(ShellStore.regions[index].url)
                const measured = ShellStore.regionPingResults[ShellStore.regions[index].url]
                regionLabels.push(ShellStore.regions[index].name
                    + (measured === null || measured === undefined ? "" : " · " + measured + " ms"))
            }
            return [
                choice("Region", ShellStore.regions.length ? qsTr("%1 streaming regions discovered").arg(ShellStore.regions.length) : "Sign in to discover available regions", "region", regionValues, regionLabels),
                {t:"Proxy address", d:"HTTP(S), SOCKS4 or SOCKS5; credentials stay in the protected local settings file", v:root.proxyDisplay(settings.sessionProxyUrl), action:"proxy-url"},
                toggle("Session proxy", "Use the configured community session proxy", "sessionProxyEnabled"),
                toggle("L4S", "Request low-latency scalable throughput when available", "enableL4S"),
                toggle("Network test", "Measure this zone's UDP payload reachability before streaming · selected zones only", "networkTest"),
                toggle("Steam Deck identity", "Unlock Deck resolutions and 90 FPS · refreshes entitlements", "identifyAsSteamDeck"),
                {t:"Refresh regions", d:ShellStore.regionsVpcId ? qsTr("Service region %1").arg(ShellStore.regionsVpcId) : "Query the authenticated NVIDIA region service", v:ShellStore.regionsRequestId === "" ? "Run" : "Running…", action:"refresh-regions"}
            ]
        }
        if (root.selectedSection === 5) {
            return [
                choice("Theme", "Auto follows the system at sunset", "appTheme", ["auto","dark","light"], ["Auto","Midnight","Light"], "segments"),
                {t:"Accent colour", d:"Focus ring, progress and active states", v:root.titleCase(settings.appAccentColor || "blue"), key:"appAccentColor", values:["violet","blue","amber","green","rose","coral","white"], labels:["Violet","Sky","Amber","Mint","Rose","Coral","White"], colors:["violet","blue","amber","green","rose","coral","white"].map(value => Theme.accentColor(value, Theme.lightMode)), control:"colors"},
                choice("Backdrop", "What sits behind the glass", "themePack", ["nocturne","aurora","kraft","phosphor"], ["Aurora gradient","Nocturne","Console room","Off"], "segments"),
                toggle("Translucent glass", "Blur the backdrop through panels · off is faster on iGPUs", "translucentUI"),
                choice("Tile style", "Shape of game tiles on My games", "posterSizeScale", [0.9,1.05,1.25], ["Compact","Soft","Round"], "segments"),
                toggle("Tile labels", "Show the game name under each tile", "showTileLabels"),
                toggle("Reduced motion", "Remove decorative motion without delaying actions", "reducedMotion"),
                {t:"UI sounds", d:"Play the Game Mode startup sound", v:ShellStore.settings.uiSoundsEnabled !== false ? qsTr("On") : qsTr("Off"), key:"uiSoundsEnabled", toggle:true, control:"toggle", toggleState:ShellStore.settings.uiSoundsEnabled !== false},
                toggle("Console mode", "Bigger 10-foot layout, profile picker on start, controller-only navigation", "launchInConsoleMode"),
                {t:"Theme store", d:"Browse controller-first palettes from the Paper V3 collection", v:root.titleCase(settings.themePack || "default"), route:"theme-store"},
                descriptorChoice(qsTr("Interface language"), ShellStore.settingsOwnerState.interfaceLanguageDescription,
                    "appLanguage", ShellStore.settingsOwnerState.interfaceLanguageItems),
                toggle("Anti-AFK indicator", "Show an in-session badge while anti-AFK pulses are enabled", "showAntiAfkIndicator"),
                choice("Anti-AFK reminder", "Repeat the activation reminder when the persistent indicator is hidden", "antiAfkReminderEveryMinutes", [0,5,10,15,30,60], ["Off","Every 5 minutes","Every 10 minutes","Every 15 minutes","Every 30 minutes","Every hour"]),
                choice("Anti-AFK reminder duration", "How long a reminder remains visible", "antiAfkReminderDurationSeconds", [2,3,5,8,10], ["2 seconds","3 seconds","5 seconds","8 seconds","10 seconds"]),
                toggle("Session clock", "Briefly show elapsed play time during a stream", "sessionCounterEnabled"),
                choice("Session clock interval", "How often elapsed play time returns", "sessionClockShowEveryMinutes", [0,15,30,45,60], ["Start only","Every 15 minutes","Every 30 minutes","Every 45 minutes","Every hour"]),
                choice("Session clock duration", "How long elapsed play time remains visible", "sessionClockShowDurationSeconds", [5,10,15,30,60], ["5 seconds","10 seconds","15 seconds","30 seconds","60 seconds"]),
                toggle("Session report", "Show performance and recovery results after a session ends", "showSessionReport")
            ]
        }
        if (root.selectedSection === 7) {
            return [
                {t:qsTr("Resolution, frame rate and quality"), d:qsTr("Capture follows the incoming stream resolution, frame rate and quality."), v:qsTr("Stream settings"), action:"stream-settings"},
                {t:qsTr("Source-quality capture"), d:qsTr("Independent downscaling needs re-encoding, unavailable in low-overhead mode."), info:true},
                {t:qsTr("Recording format"), d:qsTr("Source video and game audio in a Matroska (.mkv) file. No extra video encoder runs while you play."), v:"MKV", info:true},
                {t:qsTr("Save location"), d:ShellStore.mediaRootPath ? ShellStore.mediaRootPath + "/Recordings" : qsTr("Pictures/OpenNOW/Recordings"), v:qsTr("Open folder"), action:"recordings-folder"},
                toggle(qsTr("Enable replay buffer"), qsTr("Off by default. Enabling takes effect next session; disabling clears the buffer immediately."), "replayBufferEnabled"),
                {t:qsTr("Replay duration"), d:qsTr("Target clip length. Memory limits and source keyframes may shorten clips or require waiting for a new keyframe. Changes apply next session."), key:"replayBufferSeconds", v:qsTr("%1 seconds").arg(settings.replayBufferSeconds || 30), values:[15,30,60,120], labels:[15,30,60,120].map(value => qsTr("%1 seconds").arg(value))},
                {t:qsTr("Replay memory limit"), d:qsTr("Maximum memory for buffered media. Higher stream bitrates fill it sooner. Changes take effect next session."), key:"replayBufferMemoryMiB", v:qsTr("%1 MiB").arg(settings.replayBufferMemoryMiB || 256), values:[64,128,256,512], labels:[64,128,256,512].map(value => qsTr("%1 MiB").arg(value))},
                shortcut(qsTr("Toggle recording"), qsTr("Start or stop a source-quality recording during a stream."), "shortcutToggleRecording"),
                shortcut(qsTr("Save replay clip"), qsTr("Save the buffered video and audio. Requires the replay buffer to be enabled for this session."), "shortcutSaveClip")
            ]
        }
        return [
            {t:qsTr("Recording"), d:qsTr("Capture, replay, shortcuts"), v:qsTr("Open"), action:"recording-settings"},
            {t:"Anti-AFK", d:"Nudge the session so GeForce NOW doesn't end it while idle", v:ShellStore.antiAfkEnabled ? "On" : "Off", control:"toggle", toggleState:ShellStore.antiAfkEnabled, action:"anti-afk"},
            choice(qsTr("Microphone"), ShellStore.microphoneCaptureSupported ? ShellStore.microphoneDescription : qsTr("Microphone capture is unavailable in this build."),
                "microphoneMode", ["disabled", "voice-activity"], [qsTr("Disabled"), qsTr("Open microphone")], "segments",
                ShellStore.microphoneCaptureSupported ? [] : ["voice-activity"]),
            choice("Updates", qsTr("OpenNOW %1 · signed update feed").arg(ShellStore.updaterState.currentVersion || ""), "updateChannel", ["stable","nightly"], ["Stable","Nightly"], "segments"),
            toggle(qsTr("Automatically check for updates"), qsTr("Check every six hours while no streaming session is active."), "autoCheckForUpdates"),
            toggle(qsTr("Automatically download updates"), qsTr("Download verified updates while idle. Installation always requires your confirmation."), "autoDownloadUpdates"),
            {t:"Reset all settings", d:"Keeps your account and My games", v:"Reset to defaults", action:"reset", danger:true}
        ]
    }

    property var dropdownDetails: []
    property var dropdownGroups: []

    function openChoices(row) {
        dropdownCloseTimer.stop()
        dropdownTitle = row.t
        dropdownKey = row.key
        dropdownLabels = row.labels
        dropdownValues = row.values
        dropdownDisabledValues = row.disabledValues || []
        dropdownDetails = row.details || []
        dropdownGroups = row.groups || []
        const options = root.choiceSheetOptions()
        choiceSheet.options = options
        choiceSheet.currentIndex = root.choiceSheetCurrentIndex(options)
        dropdownPresented = true
        if (dropdownOpen) {
            choiceSheet.syncFocus()
            choiceSheet.forceActiveFocus()
        }
        dropdownOpen = true
    }

    function closeDropdown() {
        if (!dropdownPresented)
            return
        initialDropdownOpen = false
        dropdownOpen = false
        if (!root.sheetOpen)
            settingsList.forceActiveFocus()
        dropdownCloseTimer.restart()
    }

    function dropdownChoiceSelected(index) {
        const target = root.sourceSettingTarget(root.dropdownKey)
        const current = target ? target.definition.value.value
            : root.dropdownKey === "controllerInputSource" ? ControllerInput.inputControllerId
            : root.dropdownKey === "selectedSourceId" ? sourceStore.selectedSourceId : ShellStore.settings[root.dropdownKey]
        const candidate = root.dropdownValues[index]
        if (typeof current === "object" || typeof candidate === "object")
            return JSON.stringify(current) === JSON.stringify(candidate)
        return current === candidate
    }

    function dropdownChoiceDisabled(index) {
        return root.dropdownDisabledValues.indexOf(root.dropdownValues[index]) >= 0
    }

    function commitDropdownChoice(index) {
        if (index < 0 || index >= root.dropdownValues.length || root.dropdownChoiceDisabled(index))
            return
        const key = root.dropdownKey
        const value = root.dropdownValues[index]
        const currentQuality = String(ShellStore.settings.colorQuality || "8bit_420")
        root.applyChoice(key, value)
        root.closeDropdown()
        if (key === "colorQuality")
            root.notifyTenBitSelection(currentQuality, value)
    }

    function applyChoice(key, value) {
        const target = root.sourceSettingTarget(key)
        if (target) {
            sourceStore.setSetting(target.sourceId, target.definition.key,
                {kind: target.definition.control.kind, value: value}, target.domain)
            return
        }
        if (key === "selectedSourceId")
            sourceStore.select(value)
        else if (key === "controllerInputSource")
            ControllerInput.inputControllerId = Number(value)
        else
            ShellStore.setSetting(key, value)
        if (key === "resolution")
            ShellStore.clampFpsToEntitlement()
    }

    function sameValue(left, right) {
        if (typeof left === "object" || typeof right === "object")
            return JSON.stringify(left) === JSON.stringify(right)
        return left === right
    }

    function stepRow(row, delta) {
        if (!row || row.info || !row.values || ["segments", "slider", "cycler", "colors"].indexOf(row.control) < 0)
            return false
        const values = row.values
        const disabled = row.disabledValues || []
        const current = row.key === "controllerInputSource" ? ControllerInput.inputControllerId : ShellStore.settings[row.key]
        let index = row.selectedIndex !== undefined ? Number(row.selectedIndex)
            : values.findIndex(value => root.sameValue(value, current))
        if (index < 0 && row.control === "slider") {
            let nearest = 0
            for (let candidate = 1; candidate < values.length; ++candidate) {
                if (Math.abs(Number(values[candidate]) - Number(current)) < Math.abs(Number(values[nearest]) - Number(current)))
                    nearest = candidate
            }
            index = nearest
        }
        let next = index
        do {
            next += delta
        } while (next >= 0 && next < values.length && disabled.indexOf(values[next]) >= 0)
        if (next < 0 || next >= values.length || next === index)
            return true
        const previousQuality = String(ShellStore.settings.colorQuality || "8bit_420")
        root.applyChoice(row.key, values[next])
        if (row.key === "colorQuality")
            root.notifyTenBitSelection(previousQuality, values[next])
        return true
    }

    function notifyTenBitSelection(previous, value) {
        if (previous === value || ["10bit_420", "10bit_444"].indexOf(value) < 0
                || ShellStore.settings.suppressTenBitWarning === true)
            return
        settingsWarning.checked = false
        root.warningKind = "ten-bit"
    }

    function openWarning(kind) {
        settingsWarning.checked = false
        root.warningKind = kind
    }

    function closeWarning() {
        if (root.warningKind === "plugin-remove")
            root.pluginWarningId = ""
        root.warningKind = ""
        Qt.callLater(() => { if (!root.sheetOpen) settingsList.forceActiveFocus() })
    }

    function warningSafe() {
        if (root.warningKind === "ten-bit" && settingsWarning.checked) {
            ShellStore.applySetting("suppressTenBitWarning", true)
            ShellStore.setSetting("suppressTenBitWarning", true)
        }
        root.closeWarning()
    }

    function warningAction() {
        if (root.warningKind === "sign-out")
            ShellStore.logout()
        else if (root.warningKind === "reset")
            ShellStore.resetSettings()
        else if (root.warningKind === "plugin-remove")
            pluginStore.uninstall(root.pluginWarningId)
        root.closeWarning()
    }

    function choiceSheetOptions() {
        return root.dropdownLabels.map((label, index) => ({
            label:root.dropdownKey === "selectedSourceId" ? String(label) : I18n.source(String(label), I18n.revision),
            value:root.dropdownValues[index],
            detail:root.dropdownDetails[index] || (root.dropdownChoiceDisabled(index) ? qsTr("Unavailable") : ""),
            disabled:root.dropdownChoiceDisabled(index),
            group:root.dropdownGroups[index] || ""
        }))
    }

    function choiceSheetCurrentIndex(options) {
        for (let index = 0; index < root.dropdownValues.length; ++index)
            if (root.dropdownChoiceSelected(index)) return index
        return -1
    }

    function chooseFromSheet(index) {
        root.commitDropdownChoice(index)
    }

    function sectionMeta() {
        if (root.selectedSection === 0)
            return ShellStore.subscription && ShellStore.subscription.membershipTier
                ? String(ShellStore.subscription.membershipTier).toUpperCase() : ""
        if (root.selectedSection === 3)
            return qsTr("%1 CONTROLLERS CONNECTED").arg(ControllerInput.controllers.length)
        if (root.selectedSection === 4)
            return ShellStore.regions.length ? qsTr("%1 REGIONS DISCOVERED").arg(ShellStore.regions.length) : ""
        if (root.selectedSection === 6)
            return ShellStore.updaterState.currentVersion ? qsTr("OPENNOW %1").arg(ShellStore.updaterState.currentVersion) : ""
        if (root.selectedSection === 8)
            return pluginStore.available ? qsTr("%1 INSTALLED").arg(pluginStore.plugins.length) : ""
        return ""
    }

    function activate(row) {
        if (!row || row.info)
            return
        if (row.sourceSetting && row.toggle) {
            root.applyChoice(row.key, !row.toggleState)
            return
        }
        if (row.action === "retry-languages") {
            ShellStore.settingsOwnerState.ensureGameLanguages(true)
        } else if (row.route) {
            AppController.navigate(row.route)
        } else if (row.toggle) {
            ShellStore.setSetting(row.key, row.toggleState !== undefined ? !row.toggleState : !Boolean(ShellStore.settings[row.key]))
        } else if (row.values) {
            openChoices(row)
        } else if (row.action === "recording-settings") {
            root.selectedSection = 7
        } else if (row.action === "stream-settings") {
            root.selectedSection = 1
        } else if (row.action === "recordings-folder") {
            if (ShellStore.mediaRootPath)
                AppController.openLocalPath(ShellStore.mediaRootPath + "/Recordings", false)
            else
                ShellStore.refreshMedia()
        } else if (row.action === "refresh-regions") {
            ShellStore.refreshRegions()
        } else if (row.action === "refresh-streamer-capabilities") {
            ShellStore.refreshStreamerDetection()
        } else if (row.action === "ping-regions") {
            ShellStore.pingRegions()
        } else if (row.action === "proxy-url") {
            proxyField.text = String(ShellStore.settings.sessionProxyUrl || "")
            proxyEditorMessage = ""
            proxyEditorOpen = true
        } else if (row.action === "shortcut-editor") {
            shortcutEditorKey = row.key
            shortcutEditorTitle = row.t
            shortcutEditorMessage = qsTr("Press a new shortcut or clear this binding. Escape cancels.")
            shortcutEditorOpen = true
        } else if (row.action === "select-streamer") {
            streamerExecutableDialog.open()
        } else if (row.action === "automatic-bug-reports") {
            ShellStore.bugReports.setEnabled(!ShellStore.bugReports.enabled, "settings")
        } else if (row.action === "sign-out") {
            root.openWarning("sign-out")
        } else if (row.action === "anti-afk") {
            ShellStore.antiAfkEnabled = !ShellStore.antiAfkEnabled
        } else if (row.action === "reset") {
            root.openWarning("reset")
        } else if (row.action === "plugin") {
            root.openPluginSheet(row.pluginId)
        }
    }

    FileDialog {
        id: streamerExecutableDialog
        title: qsTr("Select OpenNOW native streamer")
        fileMode: FileDialog.OpenFile
        nameFilters: Qt.platform.os === "windows"
            ? [qsTr("Applications (*.exe)"), qsTr("All files (*)")]
            : [qsTr("All files (*)")]
        onAccepted: {
            const path = AppController.normalizeNativeStreamerExecutable(selectedFile)
            if (path)
                ShellStore.setSetting("nativeStreamerExecutablePath", path)
            else
                ShellStore.lastError = qsTr("Select an executable native streamer file")
            settingsList.forceActiveFocus()
        }
        onRejected: settingsList.forceActiveFocus()
    }

    function rowFocusKey() { return "settings-rows-" + root.selectedSection }

    function rowIdentity(row) {
        return row ? root.selectedSection + "|" + String(row.key || row.action || row.route || "") + "|" + String(row.t || "") : ""
    }

    function restoreRowFocus() {
        const count = root.rows.length
        if (count === 0) {
            settingsList.currentIndex = -1
            return
        }
        const keyed = root.rows.findIndex(row => root.rowIdentity(row) === root.focusedRowKey)
        const target = keyed >= 0 ? keyed : Math.min(ShellStore.focusIndex(root.rowFocusKey()), count - 1)
        if (settingsList.currentIndex !== target)
            settingsList.currentIndex = target
        if (keyed < 0)
            root.focusedRowKey = root.rowIdentity(root.rows[target])
        ShellStore.rememberFocus(root.rowFocusKey(), target)
    }

    function syncRows() {
        root.restoringRows = true
        root.rowCount = root.rows.length
        root.restoreRowFocus()
        root.restoringRows = false
    }

    onRowsChanged: syncRows()

    onDropdownOpenChanged: {
        if (dropdownOpen) {
            dropdownCloseTimer.stop()
            dropdownPresented = true
        }
    }
    onProxyEditorOpenChanged: {
        if (proxyEditorOpen)
            Qt.callLater(proxyField.forceActiveFocus)
        else
            Qt.callLater(() => { if (!root.sheetOpen) settingsList.forceActiveFocus() })
    }
    onShortcutEditorOpenChanged: {
        if (shortcutEditorOpen)
            Qt.callLater(shortcutCapture.forceActiveFocus)
        else
            Qt.callLater(() => { if (!root.sheetOpen) settingsList.forceActiveFocus() })
    }
    Component.onCompleted: {
        if (root.selectedSection === 8)
            root.loadSourceSettings()
        root.syncRows()
        if (AppController.route === "settings")
            root.selectedSection = Math.max(0, Math.min(root.sections.length - 1, ShellStore.focusIndex("settings-section")))
    }

    Connections {
        target: ShellStore
        function onSubscriptionChanged() { ShellStore.clampFpsToEntitlement() }
    }

    Timer {
        interval: 250
        running: root.initialDropdownOpen
        repeat: false
        onTriggered: root.openInitialDropdown()
    }
    Timer {
        id: dropdownCloseTimer
        interval: Theme.overlayDuration
        repeat: false
        onTriggered: {
            root.dropdownPresented = false
            if (!root.sheetOpen)
                settingsList.forceActiveFocus()
        }
    }

    ScreenBackground { tint: "#17233B" }

    GlassPanel {
        objectName: "consoleSettingsSectionsPanel"
        x: 96; y: 124; width: 360; height: root.height - 288; panelRadius: 40
        ListView {
            id: sectionList
            objectName: "consoleSettingsSections"
            anchors.fill: parent; anchors.margins: 23; spacing: 6; clip: false; focus: false
            interactive: false
            keyNavigationEnabled: true
            keyNavigationWraps: false
            KeyNavigation.right: settingsList
            model: root.sections.length; currentIndex: root.selectedSection
            onCurrentIndexChanged: if (currentIndex >= 0) {
                ShellStore.rememberFocus("settings-section", currentIndex)
                if (activeFocus) { root.selectedSection = currentIndex; root.closeDropdown() }
            }
            onActiveFocusChanged: if (activeFocus && currentIndex !== root.selectedSection) currentIndex = root.selectedSection
            Keys.onReturnPressed: settingsList.forceActiveFocus()
            Keys.onEnterPressed: settingsList.forceActiveFocus()
            delegate: ItemDelegate {
                id: sectionItem
                required property int index
                readonly property var modelData: root.sections[index] || ({})
                readonly property bool selected: root.selectedSection === sectionItem.index
                width: sectionList.width; height: 58; focusPolicy: Qt.StrongFocus; padding: 0
                Accessible.name: I18n.source(modelData.name, I18n.revision)
                onClicked: { root.selectedSection = sectionItem.index; root.closeDropdown() }
                highlighted: ListView.isCurrentItem
                background: Item {
                    Rectangle {
                        anchors.fill: parent; anchors.margins: -5; radius: 34
                        color: "transparent"; border.width: 5
                        border.color: Qt.rgba(Theme.focus.r, Theme.focus.g, Theme.focus.b, 0.5)
                        visible: sectionItem.activeFocus
                    }
                    Rectangle {
                        anchors.fill: parent; radius: 29
                        color: sectionItem.selected ? Theme.face : "transparent"
                        border.width: sectionItem.activeFocus && !sectionItem.selected ? 3 : 0
                        border.color: Theme.face
                        Behavior on color { ColorAnimation { duration: Theme.focusDuration } }
                    }
                }
                contentItem: Item {
                    Row {
                        x: 16
                        anchors.verticalCenter: parent.verticalCenter
                        spacing: 14
                        Rectangle {
                            width: 32; height: 32; radius: 11; color: sectionItem.modelData.color
                            anchors.verticalCenter: parent.verticalCenter
                            Image {
                                anchors.centerIn: parent
                                width: sectionItem.modelData.icon === "settings-input.svg" ? 22 : 20; height: width
                                source: "qrc:/qt/qml/OpenNOW/res/icons/" + sectionItem.modelData.icon
                                sourceSize: Qt.size(width * 2, height * 2)
                            }
                        }
                        Text {
                            anchors.verticalCenter: parent.verticalCenter
                            text: I18n.source(sectionItem.modelData.name, I18n.revision)
                            color: sectionItem.selected ? Theme.faceText : Theme.label
                            font.family: Theme.displayFont; font.pixelSize: 19; font.weight: Font.Black
                        }
                    }
                }
            }
        }
    }

    GlassPanel {
        objectName: "consoleSettingsRowsPanel"
        x: 480; y: 124; width: 1344; height: root.height - 288; panelRadius: 40
        Item {
            id: rowsHeader
            x: 57; y: 31
            width: parent.width - 114
            height: 40
            Text {
                id: rowsHeading
                objectName: "consoleSettingsHeading"
                anchors.left: parent.left
                height: parent.height
                verticalAlignment: Text.AlignVCenter
                text: I18n.source(root.sections[root.selectedSection].name, I18n.revision)
                color: Theme.label
                font.family: Theme.displayFont; font.pixelSize: 32; font.weight: Font.Black; font.letterSpacing: -0.3
            }
            Text {
                anchors.right: parent.right
                anchors.baseline: rowsHeading.baseline
                text: root.sectionMeta()
                color: Theme.textMuted
                font.family: Theme.monoFont; font.pixelSize: 13; font.weight: Font.Bold; font.letterSpacing: 1.3
            }
        }
        ListView {
            id: settingsList
            objectName: "consoleSettingsList"
            anchors.fill: parent
            anchors.leftMargin: 25; anchors.rightMargin: 25
            anchors.topMargin: 79; anchors.bottomMargin: 18
            leftMargin: 10; rightMargin: 10; topMargin: 10; bottomMargin: 10
            spacing: 4; clip: true; keyNavigationWraps: false
            highlightMoveDuration: AppController.reducedMotion ? 0 : 260
            highlightRangeMode: ListView.ApplyRange
            preferredHighlightBegin: 80
            preferredHighlightEnd: height - 120
            focus: true
            model: root.rowCount
            onCurrentIndexChanged: if (currentIndex >= 0 && !root.restoringRows) {
                ShellStore.rememberFocus(root.rowFocusKey(), currentIndex)
                root.focusedRowKey = root.rowIdentity(root.rows[currentIndex])
            }
            delegate: SettingRow {
                id: settingRow
                required property int index
                readonly property var modelData: root.rows[index] || ({})
                width: ListView.view.width - 20
                rowData: modelData
                currentItem: ListView.isCurrentItem
                ringVisible: ListView.isCurrentItem && settingsList.activeFocus
                parked: ListView.isCurrentItem && root.sheetOpen
                onClicked: {
                    settingsList.currentIndex = settingRow.index
                    root.activate(settingRow.modelData)
                }
            }
            Keys.onPressed: event => {
                const row = currentIndex >= 0 ? root.rows[currentIndex] : null
                if (event.key === Qt.Key_Left || event.key === Qt.Key_Right) {
                    const delta = event.key === Qt.Key_Left ? -1 : 1
                    if (!root.stepRow(row, delta) && delta < 0)
                        sectionList.forceActiveFocus()
                    event.accepted = true
                } else if (event.key === Qt.Key_Return || event.key === Qt.Key_Enter) {
                    if (!event.isAutoRepeat && currentItem) currentItem.clicked()
                    event.accepted = true
                } else if (event.key === Qt.Key_Escape || event.key === Qt.Key_Back) {
                    sectionList.forceActiveFocus()
                    event.accepted = true
                }
            }
        }
        Rectangle {
            anchors.left: parent.left; anchors.right: parent.right; anchors.bottom: parent.bottom
            anchors.margins: 1
            height: 64
            radius: 40
            visible: !settingsList.atYEnd
            gradient: Gradient {
                GradientStop { position: 0; color: "transparent" }
                GradientStop { position: 1; color: Qt.rgba(Theme.shell.r, Theme.shell.g, Theme.shell.b, 0.9) }
            }
        }
    }

    AppChrome {
        anchors.fill: parent
        title: qsTr("Settings")
        currentRoute: "settings"
        leftHints: [{glyph:"B", label: settingsList.activeFocus ? qsTr("Sections") : qsTr("Back")}]
        rightHints: [{glyph:"A", label: qsTr("Select")}]
        onRouteRequested: route => AppController.navigate(route)
    }

    ConsoleChoiceSheet {
        id: choiceSheet
        objectName: "consoleSettingsChoiceSheet"
        opened: root.dropdownOpen
        textFormat: root.dropdownKey === "selectedSourceId" ? Text.PlainText : Text.AutoText
        eyebrow: I18n.source(root.sections[root.selectedSection].name, I18n.revision)
        title: I18n.source(root.dropdownTitle, I18n.revision)
        description: root.dropdownKey === "fps" ? root.fpsNote() : ""
        chooseText: qsTr("Choose")
        dismissText: qsTr("Cancel")
        onChosen: index => root.chooseFromSheet(index)
        onDismissed: root.closeDropdown()
    }

    ConsoleWarningSheet {
        id: settingsWarning
        objectName: "consoleSettingsWarning"
        opened: root.warningOpen
        danger: root.presentedWarningKind !== "ten-bit"
        textFormat: root.presentedWarningKind === "plugin-remove" ? Text.PlainText : Text.AutoText
        eyebrow: root.presentedWarningKind === "ten-bit" ? qsTr("Saved · Color quality")
            : root.presentedWarningKind === "sign-out" ? qsTr("Sign out")
            : root.presentedWarningKind === "plugin-remove" ? qsTr("Plugins") : qsTr("Reset")
        title: root.presentedWarningKind === "ten-bit" ? qsTr("10-bit color")
            : root.presentedWarningKind === "sign-out" ? qsTr("Sign out of NVIDIA?")
            : root.presentedWarningKind === "plugin-remove" ? qsTr("Remove this plugin?") : qsTr("Reset all settings?")
        message: root.presentedWarningKind === "ten-bit"
            ? qsTr("10-bit color may cause stuttering on some systems. If you notice stuttering, switch back to 8-bit.")
            : root.presentedWarningKind === "sign-out"
            ? qsTr("OpenNOW removes the NVIDIA token from this PC. My games stay.")
            : root.presentedWarningKind === "plugin-remove"
            ? qsTr("OpenNOW removes %1 and the plugin's data from this PC. To use it again, install its package file again in Desktop mode.")
                .arg(pluginStore.pluginById(root.pluginWarningId) ? String(pluginStore.pluginById(root.pluginWarningId).name || root.pluginWarningId) : root.pluginWarningId)
            : qsTr("Every setting on this PC returns to its default. Your account and My games stay.")
        detail: root.presentedWarningKind === "ten-bit" ? qsTr("Your choice is already saved. Closing this keeps it.") : ""
        safeText: root.presentedWarningKind === "ten-bit" ? qsTr("Got it")
            : root.presentedWarningKind === "sign-out" ? qsTr("Stay signed in")
            : root.presentedWarningKind === "plugin-remove" ? qsTr("Keep plugin") : qsTr("Keep my settings")
        actionText: root.presentedWarningKind === "sign-out" ? qsTr("Sign out")
            : root.presentedWarningKind === "reset" ? qsTr("Reset to defaults")
            : root.presentedWarningKind === "plugin-remove" ? qsTr("Remove plugin") : ""
        checkboxText: root.presentedWarningKind === "ten-bit" ? qsTr("Don't notify me again") : ""
        safeButtonObjectName: root.presentedWarningKind === "ten-bit" ? "tenBitWarningDismiss" : ""
        checkboxObjectName: root.presentedWarningKind === "ten-bit" ? "tenBitWarningDontNotify" : ""
        onSafeRequested: root.warningSafe()
        onActionRequested: root.warningAction()
    }

    FocusScope {
        id: proxyEditor
        anchors.fill: parent
        visible: proxyFrame.present
        enabled: root.proxyEditorOpen
        z: 220
        Keys.onPressed: event => {
            if (!root.proxyEditorOpen)
                return
            if (event.key === Qt.Key_Escape || event.key === Qt.Key_Back) {
                root.proxyEditorOpen = false
            } else if (event.key === Qt.Key_Down || event.key === Qt.Key_Tab) {
                if (proxyField.activeFocus) proxySave.forceActiveFocus()
                else if (proxySave.activeFocus) proxyCancel.forceActiveFocus()
                else proxyField.forceActiveFocus()
            } else if (event.key === Qt.Key_Up || event.key === Qt.Key_Backtab) {
                if (proxyCancel.activeFocus) proxySave.forceActiveFocus()
                else proxyField.forceActiveFocus()
            }
            event.accepted = true
        }

        function save() {
            if (!root.proxyLooksValid(proxyField.text)) {
                root.proxyEditorMessage = qsTr("Include a host and port, for example proxy.example.com:8080.")
                Accessible.announce(root.proxyEditorMessage, Accessible.Assertive)
                return
            }
            ShellStore.setSetting("sessionProxyUrl", proxyField.text.trim())
            root.proxyEditorOpen = false
        }

        ConsoleSheetFrame {
            id: proxyFrame
            opened: root.proxyEditorOpen
            toneColor: Theme.focus
            onScrimClicked: root.proxyEditorOpen = false
            Column {
                width: parent.width
                spacing: 18
                Text { text: qsTr("NETWORK"); color: Theme.textMuted; font.family: Theme.monoFont; font.pixelSize: 14; font.weight: Font.Bold; font.letterSpacing: 2 }
                Text { text: qsTr("Session proxy"); color: Theme.label; font.family: Theme.displayFont; font.pixelSize: 44; font.weight: Font.Black; font.letterSpacing: -0.9 }
                Text { width: parent.width; text: qsTr("Enter host:port or an explicit http, https, socks4 or socks5 URL."); color: Theme.textMuted; font.family: Theme.bodyFont; font.pixelSize: 20; font.weight: Font.DemiBold; wrapMode: Text.WordWrap }
                TextField {
                    id: proxyField
                    width: parent.width; height: 68
                    leftPadding: 22; rightPadding: 22
                    placeholderText: qsTr("proxy.example.com:8080")
                    color: Theme.label; placeholderTextColor: Theme.textMuted
                    font.family: Theme.monoFont; font.pixelSize: 18
                    selectByMouse: true
                    inputMethodHints: Qt.ImhNoPredictiveText | Qt.ImhSensitiveData
                    Accessible.name: qsTr("Proxy address")
                    background: Rectangle {
                        radius: 22
                        color: Qt.rgba(Theme.face.r, Theme.face.g, Theme.face.b, 0.06)
                        border.color: proxyField.activeFocus ? Theme.face : Theme.seam
                        border.width: proxyField.activeFocus ? 3 : 1
                    }
                    Keys.onReturnPressed: proxyEditor.save()
                    Keys.onEnterPressed: proxyEditor.save()
                }
                Text {
                    width: parent.width
                    visible: text !== ""
                    text: I18n.source(root.proxyEditorMessage, I18n.revision)
                    color: Theme.coral
                    font.family: Theme.bodyFont; font.pixelSize: 17; font.weight: Font.Bold
                    wrapMode: Text.WordWrap
                }
            }
            Column {
                anchors.bottom: parent.bottom
                width: parent.width
                spacing: 12
                ConsoleActionButton { id: proxySave; width: parent.width; height: 76; primary: true; glyph: "A"; text: qsTr("Save"); onClicked: proxyEditor.save() }
                ConsoleActionButton { id: proxyCancel; width: parent.width; height: 72; glyph: "B"; text: qsTr("Cancel"); onClicked: root.proxyEditorOpen = false }
            }
        }
    }

    FocusScope {
        id: shortcutEditor
        anchors.fill: parent
        visible: shortcutFrame.present
        enabled: root.shortcutEditorOpen
        z: 230
        ConsoleSheetFrame {
            id: shortcutFrame
            opened: root.shortcutEditorOpen
            toneColor: Theme.focus
            onScrimClicked: root.shortcutEditorOpen = false
            FocusScope {
                id: shortcutCapture
                anchors.fill: parent
                focus: root.shortcutEditorOpen
                Keys.onShortcutOverride: event => { event.accepted = true }
                Keys.onPressed: event => root.captureShortcut(event)
                Column {
                    width: parent.width
                    spacing: 18
                    Text { text: qsTr("SHORTCUT"); color: Theme.textMuted; font.family: Theme.monoFont; font.pixelSize: 14; font.weight: Font.Bold; font.letterSpacing: 2 }
                    Text { width: parent.width; text: I18n.source(root.shortcutEditorTitle, I18n.revision); color: Theme.label; font.family: Theme.displayFont; font.pixelSize: 44; font.weight: Font.Black; font.letterSpacing: -0.9; wrapMode: Text.WordWrap }
                    Rectangle {
                        width: parent.width; height: 96; radius: 24
                        color: Qt.rgba(Theme.face.r, Theme.face.g, Theme.face.b, 0.06)
                        border.color: shortcutCapture.activeFocus ? Theme.face : Theme.seam
                        border.width: shortcutCapture.activeFocus ? 3 : 1
                        Text { anchors.centerIn: parent; text: qsTr("Press a key combination…"); color: Theme.label; font.family: Theme.monoFont; font.pixelSize: 22; font.weight: Font.Bold }
                    }
                    Text { width: parent.width; text: I18n.source(root.shortcutEditorMessage, I18n.revision); color: Theme.textMuted; font.family: Theme.bodyFont; font.pixelSize: 18; font.weight: Font.DemiBold; wrapMode: Text.WordWrap }
                }
                Column {
                    anchors.bottom: parent.bottom
                    width: parent.width
                    spacing: 12
                    ConsoleActionButton { width: parent.width; height: 72; focusPolicy: Qt.NoFocus; glyph: "B"; text: qsTr("Cancel"); onClicked: root.shortcutEditorOpen = false }
                    ConsoleActionButton {
                        width: parent.width; height: 72; focusPolicy: Qt.NoFocus; text: qsTr("Clear shortcut")
                        onClicked: {
                            ShellStore.setSetting(root.shortcutEditorKey, "")
                            root.shortcutEditorOpen = false
                        }
                    }
                }
            }
        }
    }

    ConsoleChoiceSheet {
        id: pluginSheet
        objectName: "consolePluginSheet"
        opened: root.pluginSheetOpen
        textFormat: Text.PlainText
        eyebrow: qsTr("Plugins")
        title: root.pluginSheetPlugin ? String(root.pluginSheetPlugin.name || root.pluginSheetId) : ""
        description: root.pluginSheetDescription()
        options: root.pluginSheetOptions()
        currentIndex: -1
        chooseText: qsTr("Select")
        dismissText: qsTr("Back")
        onChosen: index => root.choosePluginOption(index)
        onDismissed: root.closePluginSheet()
    }

    FocusScope {
        id: pluginPreview
        objectName: "consolePluginPreview"
        anchors.fill: parent
        visible: pluginPreviewFrame.present
        enabled: root.pluginPreviewOpen
        z: 240
        Keys.onPressed: event => {
            if (!root.pluginPreviewOpen)
                return
            if (event.key === Qt.Key_Escape || event.key === Qt.Key_Back) {
                root.closePluginPreview()
            } else if (event.key === Qt.Key_Down || event.key === Qt.Key_Tab) {
                if (pluginPreviewSearch.activeFocus) pluginPreviewList.forceActiveFocus()
                else if (pluginPreviewList.activeFocus && pluginPreviewList.currentIndex < pluginPreviewList.count - 1) pluginPreviewList.incrementCurrentIndex()
                else if (pluginPreviewList.activeFocus && pluginPreviewMore.visible) pluginPreviewMore.forceActiveFocus()
                else pluginPreviewClose.forceActiveFocus()
            } else if (event.key === Qt.Key_Up || event.key === Qt.Key_Backtab) {
                if (pluginPreviewClose.activeFocus && pluginPreviewMore.visible) pluginPreviewMore.forceActiveFocus()
                else if (pluginPreviewClose.activeFocus || pluginPreviewMore.activeFocus) pluginPreviewList.forceActiveFocus()
                else if (pluginPreviewList.activeFocus && pluginPreviewList.currentIndex > 0) pluginPreviewList.decrementCurrentIndex()
                else pluginPreviewSearch.forceActiveFocus()
            } else {
                return
            }
            event.accepted = true
        }

        ConsoleSheetFrame {
            id: pluginPreviewFrame
            opened: root.pluginPreviewOpen
            toneColor: Theme.violet
            onScrimClicked: root.closePluginPreview()
            Column {
                id: pluginPreviewHeader
                width: parent.width
                spacing: 14
                Text { text: qsTr("CATALOG PREVIEW"); color: Theme.textMuted; font.family: Theme.monoFont; font.pixelSize: 14; font.weight: Font.Bold; font.letterSpacing: 2 }
                Text {
                    width: parent.width
                    text: root.pluginStore.previewPlugin ? String(root.pluginStore.previewPlugin.name || root.pluginStore.previewSourceId) : ""
                    textFormat: Text.PlainText
                    color: Theme.label; font.family: Theme.displayFont; font.pixelSize: 44; font.weight: Font.Black; font.letterSpacing: -0.9
                    elide: Text.ElideRight
                }
                Text {
                    width: parent.width
                    text: qsTr("Read-only titles from this plugin. They can't be played from OpenNOW.")
                    color: Theme.textMuted; font.family: Theme.bodyFont; font.pixelSize: 18; font.weight: Font.DemiBold; wrapMode: Text.WordWrap
                }
                TextField {
                    id: pluginPreviewSearch
                    objectName: "consolePluginPreviewSearch"
                    width: parent.width; height: 64
                    leftPadding: 22; rightPadding: 22
                    placeholderText: qsTr("Search this catalog")
                    maximumLength: 512
                    color: Theme.label; placeholderTextColor: Theme.textMuted
                    font.family: Theme.bodyFont; font.pixelSize: 19
                    Accessible.name: qsTr("Search this catalog")
                    background: Rectangle {
                        radius: 22
                        color: Qt.rgba(Theme.face.r, Theme.face.g, Theme.face.b, 0.06)
                        border.color: pluginPreviewSearch.activeFocus ? Theme.face : Theme.seam
                        border.width: pluginPreviewSearch.activeFocus ? 3 : 1
                    }
                    Keys.onReturnPressed: root.pluginStore.searchPreview(text)
                    Keys.onEnterPressed: root.pluginStore.searchPreview(text)
                }
            }
            ListView {
                id: pluginPreviewList
                objectName: "consolePluginPreviewList"
                anchors.top: pluginPreviewHeader.bottom; anchors.topMargin: 14
                anchors.bottom: pluginPreviewFooter.top; anchors.bottomMargin: 14
                width: parent.width
                clip: true
                spacing: 4
                model: root.pluginStore.previewItems
                highlightMoveDuration: AppController.reducedMotion ? 0 : 160
                keyNavigationEnabled: false
                delegate: Rectangle {
                    required property var modelData
                    required property int index
                    width: pluginPreviewList.width
                    height: 56
                    radius: 18
                    color: ListView.isCurrentItem && pluginPreviewList.activeFocus
                        ? Qt.rgba(Theme.face.r, Theme.face.g, Theme.face.b, 0.12) : "transparent"
                    border.width: ListView.isCurrentItem && pluginPreviewList.activeFocus ? 3 : 0
                    border.color: Theme.face
                    Text {
                        x: 20; width: parent.width - 40
                        anchors.verticalCenter: parent.verticalCenter
                        text: modelData.title
                        textFormat: Text.PlainText
                        elide: Text.ElideRight
                        color: Theme.label; font.family: Theme.bodyFont; font.pixelSize: 19; font.weight: Font.Bold
                    }
                }
            }
            Column {
                id: pluginPreviewFooter
                anchors.bottom: parent.bottom
                width: parent.width
                spacing: 12
                Text {
                    objectName: "consolePluginPreviewStatus"
                    width: parent.width
                    text: root.pluginStore.previewError !== "" ? root.pluginStore.previewError
                    : root.pluginStore.previewWaiting ? qsTr("The plugin is restarting. Titles reload when it's ready.")
                        : root.pluginStore.previewLoading ? qsTr("Loading titles…")
                        : root.pluginStore.previewSummary
                    textFormat: Text.PlainText
                    color: root.pluginStore.previewError !== "" ? Theme.coral : Theme.textMuted
                    font.family: Theme.bodyFont; font.pixelSize: 17; font.weight: Font.Bold
                    wrapMode: Text.WordWrap
                }
                ConsoleActionButton {
                    id: pluginPreviewMore
                    objectName: "consolePluginPreviewMore"
                    width: parent.width; height: 72
                    visible: root.pluginStore.previewNextCursor !== null
                    enabled: !root.pluginStore.previewLoading
                    glyph: "A"; text: qsTr("Load more")
                    onClicked: root.pluginStore.loadMorePreview()
                }
                ConsoleActionButton {
                    id: pluginPreviewClose
                    objectName: "consolePluginPreviewClose"
                    width: parent.width; height: 72
                    glyph: "B"; text: qsTr("Close")
                    onClicked: root.closePluginPreview()
                }
            }
        }
    }
}

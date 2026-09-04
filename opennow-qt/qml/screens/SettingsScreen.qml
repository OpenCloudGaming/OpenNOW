import QtQuick
import QtQuick.Controls
import QtQuick.Dialogs
import OpenNOW

FocusScope {
    id: root
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
    property string dropdownTitle: "Choose a value"
    property string dropdownKey: ""
    property var dropdownLabels: []
    property var dropdownValues: []
    property var dropdownDisabledValues: []
    property int resolutionMenuCurrentIndex: 1
    readonly property real dropdownPanelY: dropdownKey === "resolution" ? 320
        : Math.max(110, Math.min(height - dropdownPanelHeight - 110,
            96 + 33 + (settingsList.currentItem ? settingsList.currentItem.y : 0) + 58))
    readonly property real dropdownPanelHeight: dropdownKey === "resolution"
        ? 499 : Math.min(499, 89 + dropdownLabels.length * 40)
    readonly property var sections: [
        {name:"Account", icon:"settings-account.svg", color:Theme.violet},
        {name:"Streaming", icon:"settings-streaming.svg", color:Theme.focus},
        {name:"Video & display", icon:"settings-video.svg", color:Theme.yellow},
        {name:"Input & controllers", icon:"settings-input.svg", color:Theme.mint},
        {name:"Network", icon:"settings-network.svg", color:Theme.coral},
        {name:"Themes", icon:"settings-themes.svg", color:Theme.face},
        {name:"Advanced", icon:"settings-advanced.svg", color:"#252A35"}
    ]
    readonly property var shortcutKeys: [
        "shortcutToggleStats", "shortcutTogglePointerLock", "shortcutToggleFullscreen",
        "shortcutStopStream", "shortcutToggleAntiAfk", "shortcutScreenshot",
        "shortcutToggleRecording"
    ]

    function titleCase(value) {
        const words = String(value || "").split("-").join(" ").split("_").join(" ").split(" ")
        for (let index = 0; index < words.length; ++index) {
            if (words[index].length > 0)
                words[index] = words[index][0].toUpperCase() + words[index].slice(1)
        }
        return words.join(" ")
    }

    function choice(title, description, key, values, labels, control, disabledValues) {
        const current = ShellStore.settings[key]
        const index = values.indexOf(current)
        return {t:title, d:description, v:index >= 0 ? labels[index] : root.titleCase(current), key:key, values:values, labels:labels, control:control || "dropdown", disabledValues:disabledValues || []}
    }

    function toggle(title, description, key, onLabel, offLabel) {
        return {t:title, d:description, v:Boolean(ShellStore.settings[key]) ? (onLabel || "On") : (offLabel || "Off"), key:key, toggle:true, control:"toggle"}
    }

    function shortcut(title, description, key) {
        return {t:title, d:description, v:String(ShellStore.settings[key] || "Not set"), key:key, action:"shortcut-editor"}
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

    function resolutionChoices() {
        const entries = {}
        const defaults = [
            "1280x720", "1920x1080", "2560x1440", "3840x2160",
            "1280x800", "1440x900", "1680x1050",
            "1920x1200", "2560x1600", "3840x2400",
            "2560x1080", "3440x1440", "5120x1440"
        ]
        for (let index = 0; index < defaults.length; ++index)
            entries[defaults[index]] = defaults[index]
        const entitled = ShellStore.subscription && ShellStore.subscription.entitledResolutions
            ? ShellStore.subscription.entitledResolutions : []
        for (let index = 0; index < entitled.length; ++index) {
            const width = Number(entitled[index].width || 0)
            const height = Number(entitled[index].height || 0)
            if (width >= 640 && height >= 480)
                entries[width + "x" + height] = width + "x" + height
        }
        const values = Object.keys(entries)
        values.sort((left, right) => {
            const leftParts = left.split("x")
            const rightParts = right.split("x")
            return Number(leftParts[0]) * Number(leftParts[1]) - Number(rightParts[0]) * Number(rightParts[1])
        })
        return values
    }

    function resolutionDisplayLabel(value) {
        const parts = String(value || "").split("x")
        if (parts.length !== 2)
            return String(value || "")
        const height = Number(parts[1])
        const aspect = aspectForResolution(value)
        let name = height >= 2160 && aspect === "16:9" ? "4K" : height + "p"
        if (aspect === "21:9")
            name = "UW " + height + "p"
        else if (aspect === "32:9")
            name = "Super Ultrawide"
        return name + (aspect.length ? " (" + aspect + ")" : "")
            + " · " + parts[0] + "×" + parts[1]
    }

    function resolutionLabels(values) {
        return values.map(value => resolutionDisplayLabel(value))
    }

    function resolutionDropdownItems() {
        return [
            {kind:"heading", label:"16:9 STANDARD", height:24},
            {kind:"choice", label:"720p", detail:"1280×720", values:["1280x720"], height:38},
            {kind:"choice", label:"1080p", detail:"1920×1080", values:["1920x1080"], height:38},
            {kind:"choice", label:"1440p", detail:"2560×1440 · up to 120", values:["2560x1440"], height:38},
            {kind:"choice", label:"4K", detail:"3840×2160 · up to 120", values:["3840x2160"], height:38},
            {kind:"heading", label:"16:10 WIDESCREEN", height:28},
            {kind:"choice", label:"720p · WXGA · WSXGA", detail:"1280×800 · 1440×900 · 1680×1050", values:["1280x800","1440x900","1680x1050"], height:38},
            {kind:"choice", label:"1200p · 1600p · 4K", detail:"1920×1200 · 2560×1600 · 3840×2400", values:["1920x1200","2560x1600","3840x2400"], height:38},
            {kind:"heading", label:"21:9 ULTRAWIDE", height:28},
            {kind:"choice", label:"UW 1080p · UW 1440p", detail:"2560×1080 · 3440×1440", values:["2560x1080","3440x1440"], height:38},
            {kind:"heading", label:"32:9 SUPER ULTRAWIDE", height:28},
            {kind:"choice", label:"Super Ultrawide", detail:"5120×1440", values:["5120x1440"], height:38}
        ]
    }

    function resolutionItemSelected(item) {
        return Boolean(item && item.values
            && item.values.indexOf(String(ShellStore.settings.resolution || "")) >= 0
        )
    }

    function prepareResolutionMenu() {
        const items = resolutionDropdownItems()
        resolutionMenuCurrentIndex = 1
        for (let index = 0; index < items.length; ++index) {
            if (items[index].kind === "choice" && resolutionItemSelected(items[index])) {
                resolutionMenuCurrentIndex = index
                return
            }
        }
    }

    function moveResolutionMenu(delta) {
        const items = resolutionDropdownItems()
        let next = resolutionMenuCurrentIndex
        do {
            next += delta
            if (next < 0)
                next = items.length - 1
            else if (next >= items.length)
                next = 0
        } while (items[next].kind !== "choice")
        resolutionMenuCurrentIndex = next
    }

    function chooseResolutionItem(item) {
        if (!item || item.kind !== "choice")
            return
        const current = String(ShellStore.settings.resolution || "")
        let candidate = item.values.indexOf(current) >= 0 ? current : ""
        for (let index = 0; candidate === "" && index < item.values.length; ++index) {
            if (dropdownValues.indexOf(item.values[index]) >= 0)
                candidate = item.values[index]
        }
        if (candidate === "")
            candidate = item.values[0]
        ShellStore.setSetting("resolution", candidate)
        ShellStore.clampFpsToEntitlement()
        closeDropdown()
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
        // Full canonical list; unentitled rates are locked via
        // fpsLockedValues() instead of hidden, so members can see what a
        // higher tier unlocks. Falls back to everything enabled offline.
        return ShellStore.canonicalFpsValues()
    }

    function fpsLockedValues() {
        return ShellStore.unentitledFpsValues(String(ShellStore.settings.resolution || ""))
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
        return qsTr("Only rates your membership entitles are selectable · %1 up to %2 FPS")
            .arg(tier).arg(entitled[entitled.length - 1])
    }

    function shortcutKeyName(key) {
        if (key >= Qt.Key_F1 && key <= Qt.Key_F24)
            return "F" + String(key - Qt.Key_F1 + 1)
        if (key >= Qt.Key_A && key <= Qt.Key_Z)
            return String.fromCharCode(65 + key - Qt.Key_A)
        if (key >= Qt.Key_0 && key <= Qt.Key_9)
            return String.fromCharCode(48 + key - Qt.Key_0)
        switch (key) {
        case Qt.Key_Return:
        case Qt.Key_Enter: return "Enter"
        case Qt.Key_Backspace: return "Backspace"
        case Qt.Key_Tab: return "Tab"
        case Qt.Key_Space: return "Space"
        case Qt.Key_Left: return "Left"
        case Qt.Key_Up: return "Up"
        case Qt.Key_Right: return "Right"
        case Qt.Key_Down: return "Down"
        case Qt.Key_Insert: return "Insert"
        case Qt.Key_Delete: return "Delete"
        case Qt.Key_Home: return "Home"
        case Qt.Key_End: return "End"
        case Qt.Key_PageUp: return "PageUp"
        case Qt.Key_PageDown: return "PageDown"
        case Qt.Key_Print: return "PrintScreen"
        case Qt.Key_Pause: return "Pause"
        default: return ""
        }
    }

    function captureShortcut(event) {
        event.accepted = true
        if (event.isAutoRepeat)
            return
        if (event.key === Qt.Key_Escape) {
            shortcutEditorOpen = false
            return
        }
        const keyName = shortcutKeyName(event.key)
        if (keyName === "") {
            shortcutEditorMessage = qsTr("Press a letter, number, function key, or navigation key.")
            return
        }
        const modifiers = []
        if (event.modifiers & Qt.ControlModifier)
            modifiers.push("Ctrl")
        if (event.modifiers & Qt.ShiftModifier)
            modifiers.push("Shift")
        if (event.modifiers & Qt.AltModifier)
            modifiers.push("Alt")
        if (event.modifiers & Qt.MetaModifier)
            modifiers.push("Meta")
        if ((keyName.length === 1) && modifiers.length === 0) {
            shortcutEditorMessage = qsTr("Add Ctrl, Shift, Alt, or Meta to letter and number shortcuts.")
            return
        }
        modifiers.push(keyName)
        const chord = modifiers.join("+")
        if (chord.toLowerCase() === "ctrl+g") {
            shortcutEditorMessage = qsTr("Ctrl+G is reserved for the in-stream Guide.")
            return
        }
        for (let index = 0; index < shortcutKeys.length; ++index) {
            const otherKey = shortcutKeys[index]
            if (otherKey !== shortcutEditorKey && String(ShellStore.settings[otherKey] || "").toLowerCase() === chord.toLowerCase()) {
                shortcutEditorMessage = qsTr("That shortcut is already assigned.")
                return
            }
        }
        ShellStore.setSetting(shortcutEditorKey, chord)
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
        const settings = ShellStore.settings || ({})
        if (root.selectedSection === 0) {
            const user = ShellStore.authSession && ShellStore.authSession.user ? ShellStore.authSession.user : ({})
            const accountName = String(user.displayName || qsTr("OpenNOW profile"))
            const membership = ShellStore.subscription && ShellStore.subscription.membershipTier
                ? String(ShellStore.subscription.membershipTier) : String(user.membershipTier || "—")
            return [
                {t:"Profile", control:"profile", height:121, initial:accountName.slice(0,1).toUpperCase(), name:accountName, tier:membership.toUpperCase(), subtitle:ShellStore.signedIn ? qsTr("NVIDIA account · signed in on this PC") : qsTr("Connect securely with NVIDIA"), meta:ShellStore.sessionPersistence === "os-credential-store" ? qsTr("Protected by the operating system credential store") : qsTr("Session-only profile"), v:ShellStore.signedIn ? qsTr("Manage on nvidia.com") : qsTr("Sign in"), route:ShellStore.signedIn ? "accounts" : "sign-in"},
                {t:"Profiles", d:"Each profile has its own My games shelf and settings", v:qsTr("%1 saved").arg(ShellStore.savedAccounts.length), route:"accounts"},
                {t:"Profile PIN", d:"Ask for a 4-digit PIN when switching to this profile", v:"Set up", route:"profile-pin"},
                toggle("Cloud saves", "Sync Steam / Epic / Ubisoft saves before each session", "enablePersistingInGameSettings"),
                toggle("Discord Rich Presence", "Show what you're playing on Discord", "discordRichPresence"),
                choice("Error reporting", "Send anonymous crash reports to help fix OpenNOW", "errorReportingConsent", ["denied","granted"], ["Off","Anonymous"], "segments"),
                {t:"Sign out", d:"Removes the NVIDIA token from this PC; My games stay", v:"Sign out of NVIDIA", action:"sign-out", danger:true},
                {t:"Game accounts", d:"Steam, Epic, Ubisoft and Xbox", v:qsTr("%1 detected").arg(ShellStore.gameAccounts.length), route:"game-accounts"},
                {t:"Persistent storage", d:ShellStore.subscription && ShellStore.subscription.storageAddon ? (ShellStore.subscription.storageAddon.regionName || "Cloud storage active") : "Manage cloud storage locations", v:"Open", route:"persistent-storage"}
            ]
        }
        if (root.selectedSection === 1) {
            const codecValues = ShellStore.availableCodecValues()
            const codecLabels = codecValues.map(value => value === "auto" ? "Auto" : value === "h264" ? "H.264" : value === "h265" ? "H.265" : String(value).toUpperCase())
            const selectedCodec = String(settings.codec || "auto").toLowerCase()
            const colorDisabled = selectedCodec === "h264"
                ? ["8bit_444", "10bit_420", "10bit_444"]
                : selectedCodec === "av1" ? ["8bit_444", "10bit_444"] : []
            const interpolation = settings.frameInterpolation || ({enabled:false, factor:2, quality:480})
            return [
                {t:"Codec", d:"Auto prefers AV1, then H.264, then H.265", v:root.titleCase(settings.codec || "auto"), key:"codec", values:codecValues, labels:codecLabels, segmentLabels:["Auto","AV1","H.264","H.265"], control:"segments", selectedIndex:["auto","av1","h264","h265"].indexOf(String(settings.codec || "auto"))},
                choice("Fallback codec", "Used when the preferred codec isn't offered by the rig", "fallbackCodec", codecValues, codecLabels),
                choice("Color quality", "10-bit needs H.265 or AV1; 4:4:4 needs H.265", "colorQuality", ["8bit_420","8bit_444","10bit_420","10bit_444"], ["8-bit, YUV 4:2:0","8-bit, YUV 4:4:4","10-bit, YUV 4:2:0","10-bit, YUV 4:4:4"], "segments", colorDisabled),
                {t:"Max bitrate", d:"Maximum requested stream bitrate", v:Number(settings.maxBitrateMbps || 75) + " Mbps", key:"maxBitrateMbps", values:[25,50,75,100,150,200], labels:["25 Mbps","50 Mbps","75 Mbps","100 Mbps","150 Mbps","200 Mbps"], control:"slider", sliderPercent:Number(settings.maxBitrateMbps || 75) / 106},
                {t:"Frame generation", d:"Interpolates after decode — 60 stream FPS presents at 120", v:interpolation.enabled ? Number(interpolation.factor) + "×" : "Off", key:"frameInterpolation", values:[{enabled:false,factor:2,quality:480},{enabled:true,factor:2,quality:480},{enabled:true,factor:3,quality:480}], labels:["Off","2×","3×"], control:"segments", selectedIndex:interpolation.enabled ? (Number(interpolation.factor) === 3 ? 2 : 1) : 0},
                toggle("Cloud G-Sync", "Variable refresh on G-Sync and FreeSync displays", "enableCloudGsync"),
                toggle("Stats overlay on launch", "Ctrl+N toggles it in-game", "showStatsOnLaunch"),
                choice("Stats overlay position", "FPS, RTT, loss and bitrate readout", "statsOverlayPosition", ["top-right","top-left","bottom-right","bottom-left"], ["Top-right","Top-left","Bottom-right","Bottom-left"])
            ]
        }
        if (root.selectedSection === 2) {
            const resolutions = resolutionChoices()
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
                {t:"Display", d:"The Qt stream surface uses the current display", v:"Monitor 1 · current display", info:true},
                choice("Resolution", "Exact stream size · up / down to browse, A to pick", "resolution", resolutions, resolutionLabels(resolutions)),
                choice("Frame rate", root.fpsNote(), "fps", frameRates, frameRates.map(value => String(value)), "segments", root.fpsLockedValues()),
                toggle("Fullscreen on launch", "F11 toggles in-game", "autoFullScreen"),
                {t:"Video shader", d:"Post-process on this device after decode", v:["Off","Sharpen","FidelityFX","CRT"][shaderIndex], key:"videoShader", values:shaderValues, labels:["Off","Sharpen","FidelityFX","CRT"], control:"segments", selectedIndex:shaderIndex},
                choice("Cursor", "Lock the pointer to the game window · F8", "nativeCursorOverlay", [true,false], ["Lock to window","Free"], "segments"),
            ]
        }
        if (root.selectedSection === 3) {
            const rows = []
            const controllerCards = []
            for (let index = 0; index < Math.min(2, ControllerInput.controllers.length); ++index) {
                const controller = ControllerInput.controllers[index]
                controllerCards.push({slot:controller.slot, name:controller.name, connected:true, battery:controller.batteryPercent >= 0 ? controller.batteryPercent + "%" : qsTr("Ready")})
            }
            while (controllerCards.length < 2)
                controllerCards.push({slot:controllerCards.length + 1, name:qsTr("Controller %1").arg(controllerCards.length + 1), connected:false, battery:""})
            rows.push({t:"Controllers", control:"controllers", height:105, controllers:controllerCards, route:"joining"})
            rows.push({t:"Button glyphs", d:"Detected automatically from the active controller", v:"Auto", info:true})
            rows.push(toggle("Steam controller compatibility", "Expose the pad as an Xbox controller for picky games", "steamControllerCompatibilityMode"))
            rows.push(toggle("Gyroscope", "Forward motion data to the rig", "enableGyroscopeControls"))
            rows.push({t:"Mouse sensitivity", d:"Acceleration off · raw input", v:Number(settings.mouseSensitivity || 1).toFixed(1) + "×", key:"mouseSensitivity", values:[0.5,0.75,1,1.25,1.5], labels:["0.5×","0.75×","1.0×","1.25×","1.5×"], control:"slider", sliderPercent:Number(settings.mouseSensitivity || 1) / 1.5})
            rows.push(choice("Keyboard layout", "Physical key mapping requested from GeForce NOW", "keyboardLayout",
                ["en-US","en-GB","tr-TR","de-DE","fr-FR","es-ES","es-MX","it-IT","pt-PT","pt-BR","pl-PL","da-DK","nb-NO","sv-SE","fi-FI","ru-RU","ja-JP","ko-KR","zh-CN","zh-TW"],
                ["English (US)","English (UK)","Turkish Q","German","French","Spanish","Spanish (Latin America)","Italian","Portuguese (Portugal)","Portuguese (Brazil)","Polish","Danish","Norwegian","Swedish","Finnish","Russian","Japanese","Korean","Chinese (Simplified)","Chinese (Traditional)"]))
            rows.push(choice("Game language", "Requested from the game when it supports it", "gameLanguage", ["en_US","en_GB","de_DE","fr_FR","es_ES","it_IT","pt_BR","ja_JP","ko_KR"], ["English (US)","English (UK)","Deutsch","Français","Español","Italiano","Português (BR)","日本語","한국어"]))
            rows.push({t:"Shortcuts", d:"Stats Ctrl+N · Pointer lock F8 · Fullscreen F11 · Screenshot Ctrl+F11", v:"Edit shortcuts", key:"shortcutToggleStats", action:"shortcut-editor"})
            rows.push({t:"Microphone", d:"Microphone upstream is unavailable for NVST sessions", v:"Unavailable", info:true})
            rows.push(shortcut("Toggle stats", "Cycle the Qt stream statistics overlay", "shortcutToggleStats"))
            rows.push(shortcut("Toggle pointer lock", "Capture or release the mouse on the Qt stream surface", "shortcutTogglePointerLock"))
            rows.push(shortcut("Toggle fullscreen", "Switch the Qt application surface between fullscreen and windowed", "shortcutToggleFullscreen"))
            rows.push(shortcut("Stop stream", "End the active GeForce NOW session", "shortcutStopStream"))
            rows.push(shortcut("Toggle anti-AFK", "Enable or disable the session activity helper", "shortcutToggleAntiAfk"))
            rows.push(shortcut("Screenshot", "Save the current decoded frame", "shortcutScreenshot"))
            rows.push(shortcut("Toggle recording", "Start or stop lossless stream capture", "shortcutToggleRecording"))
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
                toggle("Steam Deck identity", "Unlock Deck resolutions and 90 FPS · refreshes entitlements", "identifyAsSteamDeck"),
                {t:"Refresh regions", d:ShellStore.regionsVpcId ? qsTr("Service region %1").arg(ShellStore.regionsVpcId) : "Query the authenticated NVIDIA region service", v:ShellStore.regionsRequestId === "" ? "Run" : "Running…", action:"refresh-regions"}
            ]
        }
        if (root.selectedSection === 5) {
            return [
                choice("Theme", "Auto follows the system at sunset", "appTheme", ["auto","dark","light"], ["Auto","Midnight","Light"], "segments"),
                {t:"Accent colour", d:"Focus ring, progress and active states", v:root.titleCase(settings.appAccentColor || "blue"), key:"appAccentColor", values:["violet","blue","amber","green","rose","coral","white"], labels:["Violet","Sky","Amber","Mint","Rose","Coral","White"], colors:[Theme.violet,Theme.focus,Theme.yellow,Theme.mint,"#FF8A9A",Theme.coral,Theme.face], control:"colors"},
                choice("Backdrop", "What sits behind the glass", "themePack", ["nocturne","aurora","kraft","phosphor"], ["Aurora gradient","Nocturne","Console room","Off"], "segments"),
                toggle("Translucent glass", "Blur the backdrop through panels · off is faster on iGPUs", "translucentUI"),
                choice("Tile style", "Shape of game tiles on My games", "posterSizeScale", [0.9,1.05,1.25], ["Compact","Soft","Round"], "segments"),
                toggle("Tile labels", "Show the game name under each tile", "showTileLabels"),
                toggle("Reduced motion", "Remove decorative motion without delaying actions", "reducedMotion"),
                toggle("Console mode", "Bigger 10-foot layout, profile picker on start, controller-only navigation", "launchInConsoleMode"),
                {t:"Theme store", d:"Browse controller-first palettes from the Paper V3 collection", v:root.titleCase(settings.themePack || "default"), route:"theme-store"},
                choice("Language", "Changes the OpenNOW interface; untranslated text falls back to English", "appLanguage",
                    ["system","de","en","es","fr","ja","ko","nl","pl","ro","ru","tr","zh"],
                    ["System","Deutsch","English","Español","Français","日本語","한국어","Nederlands","Polski","Română","Русский","Türkçe","中文"]),
                toggle("Anti-AFK indicator", "Show an in-session badge while anti-AFK pulses are enabled", "showAntiAfkIndicator"),
                choice("Anti-AFK reminder", "Repeat the activation reminder when the persistent indicator is hidden", "antiAfkReminderEveryMinutes", [0,5,10,15,30,60], ["Off","Every 5 minutes","Every 10 minutes","Every 15 minutes","Every 30 minutes","Every hour"]),
                choice("Anti-AFK reminder duration", "How long a reminder remains visible", "antiAfkReminderDurationSeconds", [2,3,5,8,10], ["2 seconds","3 seconds","5 seconds","8 seconds","10 seconds"]),
                toggle("Session clock", "Briefly show elapsed play time during a stream", "sessionCounterEnabled"),
                choice("Session clock interval", "How often elapsed play time returns", "sessionClockShowEveryMinutes", [0,15,30,45,60], ["Start only","Every 15 minutes","Every 30 minutes","Every 45 minutes","Every hour"]),
                choice("Session clock duration", "How long elapsed play time remains visible", "sessionClockShowDurationSeconds", [5,10,15,30,60], ["5 seconds","10 seconds","15 seconds","30 seconds","60 seconds"]),
                toggle("Session report", "Show performance and recovery results after a session ends", "showSessionReport")
            ]
        }
        return [
            choice("Recording", "F12 clips · saved to ~/Videos/OpenNOW", "recordingResolution", ["720p","1080p","1440p"], ["720p · 30 FPS","1080p · 60 FPS","1440p · 60 FPS"], "segments"),
            {t:"Anti-AFK", d:"Nudge the session so GeForce NOW doesn't end it while idle", v:ShellStore.antiAfkEnabled ? "On" : "Off", control:"toggle", toggleState:ShellStore.antiAfkEnabled, action:"anti-afk"},
            {t:"Microphone", d:"Microphone upstream is unavailable for NVST sessions", v:"Unavailable", info:true},
            choice("Updates", qsTr("OpenNOW %1 · signed update feed").arg(ShellStore.updaterState.currentVersion || ""), "updateChannel", ["stable","nightly"], ["Stable","Nightly"], "segments"),
            {t:"Reset all settings", d:"Keeps your account and My games", v:"Reset to defaults", action:"reset", danger:true}
        ]
    }

    function openChoices(row) {
        dropdownCloseTimer.stop()
        dropdownTitle = row.t
        dropdownKey = row.key
        dropdownLabels = row.labels
        dropdownValues = row.values
        dropdownDisabledValues = row.disabledValues || []
        if (row.key === "resolution")
            prepareResolutionMenu()
        dropdownPresented = true
        dropdownOpen = true
    }

    function closeDropdown() {
        if (!dropdownPresented)
            return
        initialDropdownOpen = false
        dropdownOpen = false
        dropdownCloseTimer.restart()
    }

    function dropdownChoiceSelected(index) {
        const current = ShellStore.settings[root.dropdownKey]
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
        ShellStore.setSetting(key, value)
        if (key === "codec") {
            const codec = String(value).toLowerCase()
            if (codec === "h264" && currentQuality !== "8bit_420")
                ShellStore.setSetting("colorQuality", "8bit_420")
            else if (codec === "av1" && currentQuality.indexOf("444") >= 0)
                ShellStore.setSetting("colorQuality", currentQuality.replace("444", "420"))
        }
        root.closeDropdown()
    }

    function activate(row) {
        if (!row || row.info)
            return
        if (row.route) {
            AppController.navigate(row.route)
        } else if (row.toggle) {
            ShellStore.setSetting(row.key, !Boolean(ShellStore.settings[row.key]))
        } else if (row.values) {
            openChoices(row)
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
            shortcutEditorMessage = qsTr("Press the new shortcut. Escape cancels.")
            shortcutEditorOpen = true
        } else if (row.action === "select-streamer") {
            streamerExecutableDialog.open()
        } else if (row.action === "telemetry") {
            ShellStore.setSetting("errorReportingConsent", ShellStore.settings.errorReportingConsent === "granted" ? "denied" : "granted")
        } else if (row.action === "sign-out") {
            ShellStore.logout()
        } else if (row.action === "anti-afk") {
            ShellStore.antiAfkEnabled = !ShellStore.antiAfkEnabled
        } else if (row.action === "reset") {
            ShellStore.resetSettings()
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

    onDropdownOpenChanged: {
        if (dropdownOpen) {
            dropdownCloseTimer.stop()
            dropdownPresented = true
            if (dropdownKey === "resolution")
                Qt.callLater(resolutionMenuFocus.forceActiveFocus)
            else
                Qt.callLater(dropdownList.forceActiveFocus)
        }
    }
    onProxyEditorOpenChanged: {
        if (proxyEditorOpen)
            Qt.callLater(proxyField.forceActiveFocus)
        else
            settingsList.forceActiveFocus()
    }
    onShortcutEditorOpenChanged: {
        if (shortcutEditorOpen)
            Qt.callLater(shortcutCapture.forceActiveFocus)
        else
            settingsList.forceActiveFocus()
    }
    onSelectedSectionChanged: Qt.callLater(() => {
        settingsList.currentIndex = settingsList.count
            ? Math.min(ShellStore.focusIndex(root.rowFocusKey()), settingsList.count - 1)
            : -1
    })
    Component.onCompleted: {
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
            settingsList.forceActiveFocus()
        }
    }

    ScreenBackground { tint: "#17233B" }
    GlassPanel {
        x: 88; y: 96; width: 360; height: 848; panelRadius: 44
        ListView {
            id: sectionList
            anchors.fill: parent; anchors.margins: 24; spacing: 8; clip: true; focus: false
            KeyNavigation.right: settingsList
            model: root.sections; currentIndex: root.selectedSection
            onCurrentIndexChanged: if (currentIndex >= 0) {
                ShellStore.rememberFocus("settings-section", currentIndex)
                if (activeFocus) { root.selectedSection = currentIndex; root.closeDropdown() }
            }
            delegate: ItemDelegate {
                required property var modelData; required property int index
                width: sectionList.width; height: 56; focusPolicy: Qt.StrongFocus
                Accessible.name: I18n.source(modelData.name, I18n.revision)
                onClicked: { root.selectedSection = index; root.closeDropdown() }
                highlighted: ListView.isCurrentItem
                background: Rectangle { radius: 28; color: root.selectedSection === index ? Theme.face : "transparent"; border.color: parent.activeFocus ? Theme.focus : "transparent"; border.width: parent.activeFocus ? 3 : 0 }
                contentItem: Row {
                    spacing: 12
                    Rectangle { width: 30; height: 30; radius: 10; color: modelData.color
                        Image { anchors.centerIn: parent; width: modelData.name === "Input & controllers" ? 20 : 18; height: width; source: "qrc:/qt/qml/OpenNOW/res/icons/" + modelData.icon; sourceSize: Qt.size(width, height) }
                    }
                    Text { anchors.verticalCenter: parent.verticalCenter; text: I18n.source(modelData.name, I18n.revision); color: root.selectedSection === index ? Theme.faceText : Theme.label; font.family: Theme.bodyFont; font.pixelSize: 17; font.weight: Font.ExtraBold }
                }
            }
        }
    }

    GlassPanel {
        x: 472; y: 96; width: 1360; height: 848; panelRadius: 44
        Item {
            anchors.fill: parent; anchors.margins: 33
            ListView {
                id: settingsList
                anchors.fill: parent
                spacing: 0; clip: true; keyNavigationWraps: false
                focus: true
                KeyNavigation.left: sectionList
                model: root.settingsModel()
                Component.onCompleted: currentIndex = count ? Math.min(ShellStore.focusIndex(root.rowFocusKey()), count - 1) : -1
                onCountChanged: if (count > 0) currentIndex = Math.min(ShellStore.focusIndex(root.rowFocusKey()), count - 1)
                onCurrentIndexChanged: if (currentIndex >= 0) ShellStore.rememberFocus(root.rowFocusKey(), currentIndex)
                delegate: SettingRow {
                    required property var modelData
                    width: ListView.view.width
                    rowData: modelData
                    currentItem: ListView.isCurrentItem
                    onClicked: root.activate(modelData)
                }
                Keys.onReturnPressed: if (currentItem) currentItem.clicked()
                Keys.onEnterPressed: if (currentItem) currentItem.clicked()
            }
        }
    }

    Rectangle {
        visible: root.dropdownPresented
        anchors.fill: parent
        color: root.dropdownOpen ? Qt.rgba(0, 0, 0, 0.12) : "transparent"
        z: 20
        Behavior on color { ColorAnimation { duration: Theme.overlayDuration } }
        MouseArea { anchors.fill: parent; onClicked: root.closeDropdown() }
    }
    Rectangle {
        visible: root.dropdownPresented
        x: 1307
        y: root.dropdownPanelY + 12
        width: 500
        height: root.dropdownPanelHeight
        radius: 30
        color: Qt.rgba(0, 0, 0, 0.38)
        z: 20.5
        opacity: root.dropdownOpen ? 1 : 0
        scale: root.dropdownOpen ? 1 : 0.96
        transformOrigin: Item.TopRight
        Behavior on opacity { NumberAnimation { duration: Theme.overlayDuration; easing.type: Easing.OutCubic } }
        Behavior on scale { NumberAnimation { duration: Theme.overlayDuration; easing.type: Easing.OutCubic } }
    }
    GlassPanel {
        visible: root.dropdownPresented
        z: 21
        x: 1299
        y: root.dropdownPanelY
        width: 500
        height: root.dropdownPanelHeight
        panelRadius: 28
        strong: true
        color: "#10131C"
        opacity: root.dropdownOpen ? 1 : 0
        scale: root.dropdownOpen ? 1 : 0.96
        transformOrigin: Item.TopRight
        Behavior on opacity { NumberAnimation { duration: Theme.overlayDuration; easing.type: Easing.OutCubic } }
        Behavior on scale { NumberAnimation { duration: Theme.overlayDuration; easing.type: Easing.OutCubic } }

        FocusScope {
            id: resolutionMenuFocus
            visible: root.dropdownKey === "resolution"
            anchors.fill: parent
            focus: visible
            Keys.onPressed: event => {
                if (event.key === Qt.Key_Up)
                    root.moveResolutionMenu(-1)
                else if (event.key === Qt.Key_Down)
                    root.moveResolutionMenu(1)
                else if (event.key === Qt.Key_Return || event.key === Qt.Key_Enter || event.key === Qt.Key_Space)
                    root.chooseResolutionItem(root.resolutionDropdownItems()[root.resolutionMenuCurrentIndex])
                else if (event.key === Qt.Key_Escape || event.key === Qt.Key_Back)
                    root.closeDropdown()
                else return
                event.accepted = true
            }

            Column {
                x: 12
                y: 12
                width: parent.width - 24
                spacing: 2

                Repeater {
                    model: root.resolutionDropdownItems()
                    ItemDelegate {
                        id: resolutionItem
                        required property var modelData
                        required property int index
                        width: parent.width
                        height: modelData.height
                        padding: 0
                        enabled: modelData.kind === "choice"
                        focusPolicy: Qt.NoFocus
                        highlighted: modelData.kind === "choice"
                            && root.resolutionMenuCurrentIndex === index
                        Accessible.name: modelData.label
                        Accessible.description: modelData.detail || ""
                        onClicked: {
                            root.resolutionMenuCurrentIndex = index
                            root.chooseResolutionItem(modelData)
                        }

                        background: Rectangle {
                            radius: 19
                            color: resolutionItem.highlighted ? Theme.face : "transparent"
                            border.color: resolutionItem.highlighted ? Theme.focus : "transparent"
                            border.width: resolutionItem.highlighted ? 3 : 0
                            Behavior on color {
                                ColorAnimation { duration: Theme.focusDuration }
                            }
                        }

                        contentItem: Item {
                            Text {
                                visible: modelData.kind === "heading"
                                x: 12
                                anchors.verticalCenter: parent.verticalCenter
                                text: modelData.label
                                color: Theme.textMuted
                                font.family: Theme.bodyFont
                                font.pixelSize: 11
                                font.weight: Font.Black
                                font.letterSpacing: 0.88
                            }
                            Row {
                                visible: modelData.kind === "choice"
                                x: 12
                                anchors.verticalCenter: parent.verticalCenter
                                spacing: 8
                                Text {
                                    visible: root.resolutionItemSelected(modelData)
                                    anchors.verticalCenter: parent.verticalCenter
                                    text: "✓"
                                    color: resolutionItem.highlighted ? Theme.faceText : Theme.label
                                    font.family: Theme.bodyFont
                                    font.pixelSize: 16
                                    font.weight: Font.Black
                                }
                                Text {
                                    anchors.verticalCenter: parent.verticalCenter
                                    text: modelData.label
                                    color: resolutionItem.highlighted ? Theme.faceText : Theme.label
                                    font.family: Theme.bodyFont
                                    font.pixelSize: 15
                                    font.weight: Font.Black
                                }
                            }
                            Text {
                                visible: modelData.kind === "choice"
                                anchors.right: parent.right
                                anchors.rightMargin: 12
                                anchors.verticalCenter: parent.verticalCenter
                                text: modelData.detail || ""
                                color: resolutionItem.highlighted
                                    ? Qt.rgba(0.043, 0.059, 0.102, 0.60) : Theme.textMuted
                                font.family: Theme.bodyFont
                                font.pixelSize: 13
                                font.weight: Font.Bold
                            }
                        }
                    }
                }

                Item {
                    width: parent.width
                    height: 37
                    Rectangle {
                        anchors.top: parent.top
                        width: parent.width
                        height: 1
                        color: Theme.seam
                    }
                    Row {
                        x: 12
                        anchors.verticalCenter: parent.verticalCenter
                        anchors.verticalCenterOffset: 4
                        spacing: 16
                        ControllerGlyph { glyph: "A"; label: qsTr("Pick"); glyphSize: 22 }
                        ControllerGlyph { glyph: "B"; label: qsTr("Cancel"); glyphSize: 22 }
                    }
                }
            }
        }

        Item {
            visible: root.dropdownKey !== "resolution"
            anchors.fill: parent
            Text {
                x: 24
                y: 16
                text: I18n.source(root.dropdownTitle, I18n.revision).toUpperCase()
                color: Theme.textMuted
                font.family: Theme.bodyFont
                font.pixelSize: 11
                font.weight: Font.Black
                font.letterSpacing: 0.88
            }
            ListView {
                id: dropdownList
                x: 12
                y: 42
                width: parent.width - 24
                height: parent.height - 91
                spacing: 2
                clip: true
                keyNavigationWraps: true
                model: root.dropdownLabels
                delegate: ItemDelegate {
                    id: dropdownItem
                    required property string modelData
                    required property int index
                    width: ListView.view.width
                    height: 38
                    padding: 0
                    enabled: !root.dropdownChoiceDisabled(index)
                    highlighted: ListView.isCurrentItem && enabled
                    opacity: enabled ? 1 : 0.42
                    onClicked: root.commitDropdownChoice(index)
                    background: Rectangle {
                        radius: 19
                        color: dropdownItem.highlighted ? Theme.face : "transparent"
                        border.color: dropdownItem.highlighted ? Theme.focus : "transparent"
                        border.width: dropdownItem.highlighted ? 3 : 0
                    }
                    contentItem: Item {
                        Text {
                            visible: root.dropdownChoiceSelected(index)
                            x: 12
                            anchors.verticalCenter: parent.verticalCenter
                            text: "✓"
                            color: dropdownItem.highlighted ? Theme.faceText : Theme.label
                            font.family: Theme.bodyFont
                            font.pixelSize: 16
                            font.weight: Font.Black
                        }
                        Text {
                            x: root.dropdownChoiceSelected(index) ? 38 : 12
                            anchors.verticalCenter: parent.verticalCenter
                            width: parent.width - x - (disabledReason.visible ? disabledReason.width + 18 : 12)
                            text: I18n.source(modelData, I18n.revision)
                            color: dropdownItem.highlighted ? Theme.faceText : Theme.label
                            font.family: Theme.bodyFont
                            font.pixelSize: 15
                            font.weight: Font.Black
                            elide: Text.ElideRight
                        }
                        Text {
                            id: disabledReason
                            visible: root.dropdownChoiceDisabled(index)
                            anchors.right: parent.right
                            anchors.rightMargin: 12
                            anchors.verticalCenter: parent.verticalCenter
                            text: String(root.dropdownValues[index]).indexOf("444") >= 0
                                ? qsTr("H.265") : qsTr("H.265 / AV1")
                            color: Theme.textMuted
                            font.family: Theme.bodyFont
                            font.pixelSize: 12
                            font.weight: Font.Bold
                        }
                    }
                }
                Keys.onReturnPressed: root.commitDropdownChoice(currentIndex)
                Keys.onEnterPressed: root.commitDropdownChoice(currentIndex)
                Keys.onEscapePressed: root.closeDropdown()
            }

            Rectangle {
                x: 12
                y: parent.height - 49
                width: parent.width - 24
                height: 1
                color: Theme.seam
            }
            Row {
                x: 24
                y: parent.height - 39
                spacing: 16
                ControllerGlyph { glyph: "A"; label: qsTr("Pick"); glyphSize: 22 }
                ControllerGlyph { glyph: "B"; label: qsTr("Cancel"); glyphSize: 22 }
            }
        }
    }
    Rectangle {
        visible: root.proxyEditorOpen; anchors.fill: parent; color: Qt.rgba(0, 0, 0, 0.42); z: 30
        MouseArea { anchors.fill: parent; onClicked: root.proxyEditorOpen = false }
    }
    GlassPanel {
        visible: root.proxyEditorOpen
        z: 31
        anchors.centerIn: parent
        width: 620
        height: 260
        panelRadius: 30
        strong: true
        Column {
            anchors.fill: parent; anchors.margins: 26; spacing: 14
            Text { text: qsTr("Session proxy"); color: Theme.label; font.family: Theme.displayFont; font.pixelSize: 24; font.weight: Font.Black }
            Text { width: parent.width; text: qsTr("Enter host:port or an explicit http, https, socks4 or socks5 URL."); color: Theme.textMuted; font.family: Theme.bodyFont; font.pixelSize: 14; wrapMode: Text.WordWrap }
            TextField {
                id: proxyField
                width: parent.width; height: 52
                placeholderText: qsTr("proxy.example.com:8080")
                color: Theme.label; placeholderTextColor: Theme.textMuted
                font.family: Theme.monoFont; font.pixelSize: 14
                selectByMouse: true
                inputMethodHints: Qt.ImhNoPredictiveText | Qt.ImhSensitiveData
                Accessible.name: qsTr("Proxy address")
                background: Rectangle { radius: 16; color: Theme.glass; border.color: proxyField.activeFocus ? Theme.focus : Theme.seam; border.width: proxyField.activeFocus ? 3 : 1 }
                Keys.onEscapePressed: root.proxyEditorOpen = false
            }
            Row {
                spacing: 12
                Text { width: 260; anchors.verticalCenter: parent.verticalCenter; text: I18n.source(root.proxyEditorMessage, I18n.revision); color: Theme.coral; font.family: Theme.bodyFont; font.pixelSize: 12; wrapMode: Text.WordWrap }
                GlassButton { width: 130; height: 44; text: qsTr("Cancel"); glyph: "B"; onClicked: root.proxyEditorOpen = false }
                GlassButton {
                    width: 150; height: 44; text: qsTr("Save"); glyph: "A"; primary: true
                    onClicked: {
                        if (!root.proxyLooksValid(proxyField.text)) {
                            root.proxyEditorMessage = "Include a host and port, for example proxy.example.com:8080."
                            Accessible.announce(root.proxyEditorMessage, Accessible.Assertive)
                            return
                        }
                        ShellStore.setSetting("sessionProxyUrl", proxyField.text.trim())
                        root.proxyEditorOpen = false
                    }
                }
            }
        }
        scale: visible ? 1 : 0.94
        opacity: visible ? 1 : 0
        Behavior on scale { NumberAnimation { duration: Theme.overlayDuration; easing.type: Easing.OutBack } }
        Behavior on opacity { NumberAnimation { duration: Theme.overlayDuration; easing.type: Easing.OutCubic } }
    }
    Rectangle {
        visible: root.shortcutEditorOpen; anchors.fill: parent; color: Qt.rgba(0, 0, 0, 0.42); z: 40
        MouseArea { anchors.fill: parent; onClicked: root.shortcutEditorOpen = false }
    }
    GlassPanel {
        visible: root.shortcutEditorOpen
        z: 41
        anchors.centerIn: parent
        width: 560
        height: 244
        panelRadius: 30
        strong: true
        FocusScope {
            id: shortcutCapture
            anchors.fill: parent
            focus: root.shortcutEditorOpen
            Keys.onPressed: event => root.captureShortcut(event)
            Column {
                anchors.fill: parent; anchors.margins: 28; spacing: 16
                Text { text: I18n.source(root.shortcutEditorTitle, I18n.revision); color: Theme.label; font.family: Theme.displayFont; font.pixelSize: 25; font.weight: Font.Black }
                Rectangle {
                    width: parent.width; height: 66; radius: 20; color: Theme.glass
                    border.color: shortcutCapture.activeFocus ? Theme.focus : Theme.seam
                    border.width: shortcutCapture.activeFocus ? 3 : 1
                    Text { anchors.centerIn: parent; text: qsTr("Press a key combination…"); color: Theme.label; font.family: Theme.monoFont; font.pixelSize: 18; font.weight: Font.Bold }
                }
                Row {
                    spacing: 14
                    Text { width: 350; anchors.verticalCenter: parent.verticalCenter; text: I18n.source(root.shortcutEditorMessage, I18n.revision); color: Theme.textMuted; font.family: Theme.bodyFont; font.pixelSize: 13; wrapMode: Text.WordWrap }
                    GlassButton { width: 130; height: 44; text: qsTr("Cancel"); glyph: "B"; onClicked: root.shortcutEditorOpen = false }
                }
            }
        }
        scale: visible ? 1 : 0.94
        opacity: visible ? 1 : 0
        Behavior on scale { NumberAnimation { duration: Theme.overlayDuration; easing.type: Easing.OutBack } }
        Behavior on opacity { NumberAnimation { duration: Theme.overlayDuration; easing.type: Easing.OutCubic } }
    }
    AppChrome { anchors.fill: parent; title: qsTr("Settings  ·  ") + I18n.source(root.sections[root.selectedSection].name, I18n.revision); currentRoute: "settings"; onRouteRequested: route => AppController.navigate(route) }
}
